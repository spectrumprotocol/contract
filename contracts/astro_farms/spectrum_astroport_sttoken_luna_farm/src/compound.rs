use cosmwasm_std::{attr, to_binary, Attribute, CanonicalAddr, Coin, CosmosMsg, DepsMut, Env, MessageInfo, QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery, Decimal, QuerierWrapper, Fraction, Addr};

use crate::{
    bond::deposit_farm_share,
    querier::{query_astroport_pending_token, query_astroport_pool_balance, astroport_router_simulate_swap},
    state::{read_config, state_store},
};

use cw20::Cw20ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store, read_state, Config, PoolInfo};
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::PairType;
use astroport::generator::{
    Cw20HookMsg as AstroportCw20HookMsg, ExecuteMsg as AstroportExecuteMsg
};
use astroport::pair::{Cw20HookMsg as AstroportPairCw20HookMsg, Cw20HookMsg, ExecuteMsg as AstroportPairExecuteMsg, PoolResponse, QueryMsg as AstroportPairQueryMsg};
use astroport::router::{SwapOperation, ExecuteMsg as AstroportRouterExecuteMsg};
use astroport::querier::{query_token_balance, simulate};
use moneymarket::market::ExecuteMsg as MoneyMarketExecuteMsg;
use spectrum_protocol::anchor_farm::ExecuteMsg;
use spectrum_protocol::farm_helper::{deduct_tax, get_swap_amount, get_swap_amount_astroport, U256};
use spectrum_protocol::gov_proxy::Cw20HookMsg as GovProxyCw20HookMsg;
use spectrum_protocol::gov::{ExecuteMsg as GovExecuteMsg};
use crate::bond::deposit_farm2_share;
use uint::construct_uint;

construct_uint! {
    pub struct U256(4);
}

// ASTRO -> UST -> LUNA need to process first to see how much LUNA still needed
// weLDO -> stLUNA -(optimal swap)> LUNA -> UST

pub fn compound(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    threshold_compound_astro: Uint128,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    if config.controller != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let farm_token = deps.api.addr_humanize(&config.farm_token)?;
    let stluna_token = deps.api.addr_humanize(&config.stluna_token)?;
    let weldo_token =  deps.api.addr_humanize(&config.weldo_token)?;
    let astro_token = deps.api.addr_humanize(&config.astro_token)?;
    let xastro_proxy = deps.api.addr_humanize(&config.xastro_proxy)?;
    let astro_ust_pair_contract = deps.api.addr_humanize(&config.astro_ust_pair_contract)?;
    let pair_contract = deps.api.addr_humanize(&config.pair_contract)?;
    let astroport_router = deps.api.addr_humanize(&config.astroport_router)?;
    let stluna_weldo_pair_contract = deps.api.addr_humanize(&config.stluna_weldo_pair_contract)?;
    let stluna_uluna_pair_contract = deps.api.addr_humanize(&config.stluna_uluna_pair_contract)?;
    let uluna_uusd_pair_contract = deps.api.addr_humanize(&config.uluna_uusd_pair_contract)?;


    let uluna = "uluna".to_string();
    let uusd = "uusd".to_string();


    let mut pool_info = pool_info_read(deps.storage).load(config.farm_token.as_slice())?;

    // This get pending (ASTRO), and pending proxy rewards
    let pending_token_response = query_astroport_pending_token(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator
    )?;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    let mut total_weldo_token_swap_amount = Uint128::zero();
    let mut total_weldo_token_stake_amount = Uint128::zero();
    let mut total_weldo_token_commission = Uint128::zero();
    let mut total_astro_token_swap_amount = Uint128::zero();
    let mut total_astro_token_stake_amount = Uint128::zero();
    let mut total_astro_token_commission = Uint128::zero();
    let mut compound_amount = Uint128::zero();
    let mut compound_amount_astro = Uint128::zero();

    let mut attributes: Vec<Attribute> = vec![];
    let community_fee = config.community_fee;
    let platform_fee = config.platform_fee;
    let controller_fee = config.controller_fee;
    let total_fee = community_fee + platform_fee + controller_fee;

    let reward = query_token_balance(&deps.querier, weldo_token.clone(), env.contract.address.clone())? + pending_token_response.pending_on_proxy.unwrap_or_else(Uint128::zero);
    let reward_astro = query_token_balance(&deps.querier, astro_token.clone(), env.contract.address.clone())? + pending_token_response.pending;

    // calculate auto-compound, auto-stake, and commission in astro token
    let mut state = read_state(deps.storage)?;
    if !reward_astro.is_zero() && !lp_balance.is_zero() && reward_astro > threshold_compound_astro {
        let commission_astro = reward_astro * total_fee;
        let astro_amount = reward_astro.checked_sub(commission_astro)?;
        // add commission to total swap amount
        total_astro_token_commission += commission_astro;
        total_astro_token_swap_amount += commission_astro;

        let auto_bond_amount_astro = lp_balance.checked_sub(pool_info.total_stake_bond_amount)?;
        compound_amount_astro = astro_amount.multiply_ratio(auto_bond_amount_astro, lp_balance);
        let stake_amount_astro = astro_amount.checked_sub(compound_amount_astro)?;

        attributes.push(attr("commission_astro", commission_astro));
        attributes.push(attr("compound_amount_astro", compound_amount_astro));
        attributes.push(attr("stake_amount_astro", stake_amount_astro));

        total_astro_token_stake_amount += stake_amount_astro;

        deposit_farm_share(
            deps.as_ref(),
            &env,
            &mut state,
            &mut pool_info,
            &config,
            total_astro_token_stake_amount,
        )?;
    }

    // calculate auto-compound, auto-stake, and commission in farm token
    if !reward.is_zero() && !lp_balance.is_zero() {
        let commission = reward * total_fee;
        let weldo_token_amount = reward.checked_sub(commission)?;
        // add commission to total swap amount
        total_weldo_token_commission += commission;
        total_weldo_token_swap_amount += reward;

        let auto_bond_amount = lp_balance.checked_sub(pool_info.total_stake_bond_amount)?;
        compound_amount = weldo_token_amount.multiply_ratio(auto_bond_amount, lp_balance);
        let stake_amount = weldo_token_amount.checked_sub(compound_amount)?;

        attributes.push(attr("commission", commission));
        attributes.push(attr("compound_amount", compound_amount));
        attributes.push(attr("stake_amount", stake_amount));

        total_weldo_token_stake_amount += stake_amount;

        deposit_farm2_share(
            deps.as_ref(),
            &env,
            &mut state,
            &mut pool_info,
            &config,
            total_weldo_token_stake_amount,
        )?;
    }
    state_store(deps.storage).save(&state)?;
    pool_info_store(deps.storage).save(config.farm_token.as_slice(), &pool_info)?;

    // swap all
    total_astro_token_swap_amount += compound_amount_astro;
    let (mut total_ust_reinvest_amount_astro, total_ust_commission_amount_astro) = if !total_astro_token_swap_amount.is_zero() {

        // find ASTRO swap rate
        let astro_asset = Asset {
            info: AssetInfo::Token {
                contract_addr: astro_token.clone(),
            },
            amount: total_astro_token_swap_amount,
        };
        let astro_swap_rate = simulate(&deps.querier, astro_ust_pair_contract.clone(), &astro_asset)?;

        let total_ust_return_amount_astro = deduct_tax(
            &deps.querier,
            astro_swap_rate.return_amount,
            uusd.clone(),
        )?;
        attributes.push(attr("total_ust_return_amount_astro", total_ust_return_amount_astro));

        let total_ust_commission_amount_astro = total_ust_return_amount_astro
            .multiply_ratio(total_astro_token_commission, total_astro_token_swap_amount);

        let total_ust_reinvest_amount_astro = total_ust_return_amount_astro.checked_sub(total_ust_commission_amount_astro)?;

        (total_ust_reinvest_amount_astro, total_ust_commission_amount_astro)
    } else {
        (Uint128::zero(), Uint128::zero())
    };

    // find weLDO to stLuna swap rate
    let weldo_asset = Asset {
        info: AssetInfo::Token {
            contract_addr: weldo_token.clone(),
        },
        amount: total_weldo_token_swap_amount,
    };
    let weldo_token_swap_rate = simulate(&deps.querier, stluna_weldo_pair_contract.clone(), &weldo_asset)?;

    let total_weldo_stluna_return_amount = weldo_token_swap_rate.amount;
    attributes.push(attr("total_weldo_stluna_return_amount", total_weldo_stluna_return_amount));

    let total_stluna_commission_amount = if total_weldo_token_swap_amount != Uint128::zero() {
        total_weldo_stluna_return_amount.multiply_ratio(total_weldo_token_commission, total_weldo_token_swap_amount)
    } else {
        Uint128::zero()
    };

    let stluna_ust_swap_rate = astroport_router_simulate_swap(deps.as_ref(),
            total_stluna_commission_amount,
        vec![
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::Token { contract_addr: stluna_token.clone() },
                ask_asset_info: AssetInfo::NativeToken { denom: uluna.clone() }
            },
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::NativeToken { denom: uluna.clone() },
                ask_asset_info: AssetInfo::NativeToken { denom: uusd.clone() },
            }
        ],
        &config.astroport_router
    )?;

    let ust_commission_amount = deduct_tax(
        &deps.querier,
        stluna_ust_swap_rate.amount,
        uusd.clone(),
    )?;

    let total_stluna_reinvest_amount =
        total_weldo_stluna_return_amount.checked_sub(total_stluna_commission_amount)?;


    // TODO total_stluna_reinvest_amount stluna --best swap--> uluna. Need to get how much uluna can be swapped from ASTRO -> uusd -> uluna first
    // cases to be handled
    // ASTRO = 0 or uluna from ASTRO < stluna area THEN swap stluna (from weldo) to uluna
    // ASTRO -> uluna and uluna area more than stluna THEN swap uluna (from ASTRO) to stluna

    let uusd_from_astro_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: uusd.clone(),
        },
        amount: total_ust_reinvest_amount_astro,
    };
    let uluna_from_ust_from_astro_swap_rate = simulate(&deps.querier, uluna_uusd_pair_contract.clone(), &uusd_from_astro_asset)?;

    let uluna_from_ust_from_astro = deduct_tax(
        &deps.querier,
        uluna_from_ust_from_astro_swap_rate.return_amount,
        uluna.clone(),
    )?;

    let uluna_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: uluna.clone(),
        },
        amount: uluna_from_ust_from_astro,
    };

    // TODO uluna_from_ust_from_astro input to best swap rate

    let mut messages: Vec<CosmosMsg> = vec![];

    let manual_claim_pending_token = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.addr_humanize(&config.astroport_generator)?.to_string(),
        funds: vec![],
        msg: to_binary(&AstroportExecuteMsg::Withdraw {
            lp_token: deps.api.addr_humanize(&pool_info.staking_token)?,
            amount: Uint128::zero(),
        })?,
    });
    messages.push(manual_claim_pending_token);

    if !total_weldo_token_swap_amount.is_zero() {
        // TODO is this still needed?
        // let ust_amount = deps.querier.query_balance(env.contract.address.clone(), "uusd")?.amount;
        // if ust_amount < total_weldo_stluna_return_amount {
        //
        // }
        let swap_weldo_token_to_stluna: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: weldo_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: stluna_weldo_pair_contract.to_string(),
                amount: total_weldo_token_swap_amount,
                msg: to_binary(&AstroportPairCw20HookMsg::Swap {
                    max_spread: Some(Decimal::percent(50)),
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_weldo_token_to_stluna);
        // TODO swap 8% stluna to uluna to uusd
        let swap_stluna_to_ust: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: stluna_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: astroport_router.to_string(),
                amount: total_stluna_commission_amount,
                msg: to_binary(&AstroportRouterExecuteMsg::ExecuteSwapOperations {
                    operations: vec![
                        SwapOperation::AstroSwap {
                            offer_asset_info: AssetInfo::Token { contract_addr: stluna_token.clone() },
                            ask_asset_info: AssetInfo::NativeToken { denom: uluna.clone() },
                        },
                        SwapOperation::AstroSwap {
                            offer_asset_info: AssetInfo::NativeToken { denom: uluna.clone() },
                            ask_asset_info: AssetInfo::NativeToken { denom: uusd.clone() },
                        },
                    ],
                    minimum_receive: None,
                    to: None,
                    max_spread: Some(Decimal::percent(50))
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_stluna_to_ust);

    }

    if !total_astro_token_swap_amount.is_zero() {
        //swap 100% astro to uusd
        let swap_astro_to_uusd: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: astro_ust_pair_contract.to_string(),
                amount: total_astro_token_swap_amount,
                msg: to_binary(&AstroportPairCw20HookMsg::Swap {
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_astro_to_uusd);
        //swap 92% uusd to uluna
        let uusd_from_astro_exclude_commission = Asset {
            info: AssetInfo::NativeToken {
                denom: uusd.clone(),
            },
            amount: total_ust_reinvest_amount_astro,
        };
        let swap_uusd_to_uluna: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: uluna_uusd_pair_contract.to_string(),
            msg: to_binary(&AstroportPairExecuteMsg::Swap {
                to: None,
                max_spread: Some(Decimal::percent(50)),
                belief_price: None,
                offer_asset: uusd_from_astro_exclude_commission
            })?,
            funds: vec![Coin { denom: uusd.clone(), amount: total_ust_for_swap_farm_token.clone() }]
        });
        messages.push(swap_uusd_to_uluna);
    }


    let sttoken_asset_info = AssetInfo::Token {
        contract_addr: stluna_token.clone(),
    };
    let uluna_asset_info = AssetInfo::NativeToken {
        denom: uluna.clone(),
    };
    // swap 92% stluna (from weldo) to uluna and uluna (from ASTRO) stluna right amount with optimal swap
    swap(&deps.querier,
         total_stluna_reinvest_amount,
         uluna_from_ust_from_astro,
         sttoken_asset_info,
         uluna_asset_info,
        None,
        Decimal::percent(50),
        stluna_uluna_pair_contract,
         &mut messages
    );




    if let Some(gov_proxy) = config.gov_proxy {
        if !total_weldo_token_stake_amount.is_zero() {
            let stake_farm_token = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: farm_token.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: deps.api.addr_humanize(&gov_proxy)?.to_string(),
                    amount: total_weldo_token_stake_amount,
                    msg: to_binary(&GovProxyCw20HookMsg::Stake {})?,
                })?,
            });
            messages.push(stake_farm_token);
        }
    }

    if !total_astro_token_stake_amount.is_zero() {
        let stake_astro_token = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: astro_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: xastro_proxy.to_string(),
                amount: total_astro_token_stake_amount,
                msg: to_binary(&GovProxyCw20HookMsg::Stake {})?,
            })?,
        });
        messages.push(stake_astro_token);
    }

    if !provide_farm_token.is_zero() {
        let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: farm_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender: pair_contract.to_string(),
                amount: provide_farm_token,
                expires: None,
            })?,
            funds: vec![],
        });
        messages.push(increase_allowance);

        //TODO logic for amount, get return from swap?
        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pair_contract.to_string(),
            msg: to_binary(&AstroportPairExecuteMsg::ProvideLiquidity {
                assets: [
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: farm_token.clone(),
                        },
                        amount: provide_farm_token,
                    },
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: uusd.clone(),
                        },
                        amount: net_reinvest_stluna,
                    },
                ],
                slippage_tolerance: None,
                receiver: None,
                auto_stake: Some(true),
            })?,
            funds: vec![Coin {
                denom: uusd.clone(),
                amount: net_reinvest_stluna,
            }],
        });
        messages.push(provide_liquidity);

        // let stake = CosmosMsg::Wasm(WasmMsg::Execute {
        //     contract_addr: env.contract.address.to_string(),
        //     msg: to_binary(&ExecuteMsg::stake {
        //         asset_token: farm_token.to_string(),
        //     })?,
        //     funds: vec![],
        // });
        // messages.push(stake);
    }

    let total_ust_commission_amount = ust_commission_amount + total_ust_commission_amount_astro;
    if !total_ust_commission_amount.is_zero() {
        let net_commission_amount = deduct_tax(
            &deps.querier,
            total_ust_commission_amount,
            uusd.clone(),
        )?;

        let mut state = read_state(deps.storage)?;
        state.earning += net_commission_amount;
        state_store(deps.storage).save(&state)?;

        attributes.push(attr("net_commission", net_commission_amount));

        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.anchor_market)?.to_string(),
            msg: to_binary(&MoneyMarketExecuteMsg::DepositStable {})?,
            funds: vec![Coin {
                denom: uusd.clone(),
                amount: net_commission_amount,
            }],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
            msg: to_binary(&GovExecuteMsg::mint {})?,
            funds: vec![],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::send_fee {})?,
            funds: vec![],
        }));
    }

    attributes.push(attr("action", "compound"));
    attributes.push(attr("asset_token", &farm_token));
    attributes.push(attr("provide_farm_token", provide_farm_token));
    attributes.push(attr("provide_ust_amount", net_reinvest_stluna));

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes))
}

fn compute_provide(
    pool: &PoolResponse,
    asset: &Asset,
) -> Uint128 {
    let (asset_a_amount, asset_b_amount) = if pool.assets[0].info == asset.info {
        (pool.assets[0].amount, pool.assets[1].amount)
    } else {
        (pool.assets[1].amount, pool.assets[0].amount)
    };

    asset.amount.multiply_ratio(asset_b_amount, asset_a_amount)
}

pub fn compute_provide_after_swap(
    pool: &PoolResponse,
    offer: &Asset,
    return_amt: Uint128,
    ask_reinvest_amt: Uint128,
) -> StdResult<Uint128> {
    let (offer_amount, ask_amount) = if pool.assets[0].info == offer.info {
        (pool.assets[0].amount, pool.assets[1].amount)
    } else {
        (pool.assets[1].amount, pool.assets[0].amount)
    };

    let offer_amount = offer_amount + offer.amount;
    let ask_amount = ask_amount.checked_sub(return_amt)?;

    Ok(ask_reinvest_amt.multiply_ratio(offer_amount, ask_amount))
}

pub fn stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset_token: String,
) -> StdResult<Response> {
    // only anchor farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config: Config = read_config(deps.storage)?;
    let astroport_generator = deps.api.addr_humanize(&config.astroport_generator)?;
    let asset_token_raw: CanonicalAddr = deps.api.addr_canonicalize(&asset_token)?;
    let pool_info: PoolInfo = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let staking_token = deps.api.addr_humanize(&pool_info.staking_token)?;

    let amount = query_token_balance(&deps.querier, staking_token.clone(), env.contract.address)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: astroport_generator.to_string(),
                amount,
                msg: to_binary(&AstroportCw20HookMsg::Deposit {})?,
            })?,
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("asset_token", asset_token),
            attr("staking_token", staking_token),
            attr("amount", amount),
        ]))
}

pub fn send_fee(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> StdResult<Response> {

    // only farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config = read_config(deps.storage)?;
    let aust_token = deps.api.addr_humanize(&config.aust_token)?;
    let spectrum_gov = deps.api.addr_humanize(&config.spectrum_gov)?;

    let aust_balance = query_token_balance(&deps.querier, aust_token.clone(), env.contract.address.clone())?;

    let mut messages: Vec<CosmosMsg> = vec![];
    let thousand = Uint128::from(1000u64);
    let total_fee = config.community_fee + config.controller_fee + config.platform_fee;
    let community_amount = aust_balance.multiply_ratio(thousand * config.community_fee, thousand * total_fee);
    if !community_amount.is_zero() {
        let transfer_community_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: spectrum_gov.to_string(),
                amount: community_amount,
            })?,
            funds: vec![],
        });
        messages.push(transfer_community_fee);
    }

    let platform_amount = aust_balance.multiply_ratio(thousand * config.platform_fee, thousand * total_fee);
    if !platform_amount.is_zero() {
        let stake_platform_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: deps.api.addr_humanize(&config.platform)?.to_string(),
                amount: platform_amount,
            })?,
            funds: vec![],
        });
        messages.push(stake_platform_fee);
    }

    let controller_amount = aust_balance.checked_sub(community_amount + platform_amount)?;
    if !controller_amount.is_zero() {
        let stake_controller_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: deps.api.addr_humanize(&config.controller)?.to_string(),
                amount: controller_amount,
            })?,
            funds: vec![],
        });
        messages.push(stake_controller_fee);
    }

    // remaining UST > 100, swap all to farm token, in case ASTRO provides more than farm
    let ust_amount = deps.querier.query_balance(env.contract.address, "uusd")?.amount;
    if ust_amount >= Uint128::from(100_000000u128) {
        let ust_after_tax = deduct_tax(&deps.querier, ust_amount, "uusd".to_string())?;
        let swap_ust: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.pair_contract)?.to_string(),
            msg: to_binary(&AstroportPairExecuteMsg::Swap {
                max_spread: None,
                belief_price: None,
                to: None,
                offer_asset: Asset {
                    info: AssetInfo::NativeToken { denom: "uusd".to_string() },
                    amount: ust_after_tax,
                },
            })?,
            funds: vec![
                Coin { denom: "uusd".to_string(), amount: ust_after_tax },
            ],
        });
        messages.push(swap_ust);
    }

    Ok(Response::new()
        .add_messages(messages))
}

/// Query the Astroport pool, parse response, and return the following 3-tuple:
/// 1. depth of the primary asset
/// 2. depth of the secondary asset
/// 3. total supply of the share token
fn query_pool(
    pair_contract: String,
    querier: &QuerierWrapper,
    primary_asset_info: &AssetInfo,
    secondary_asset_info: &AssetInfo,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    let response: PoolResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract,
        msg: to_binary(&AstroportPairQueryMsg::Pool {})?,
    }))?;

    let primary_asset_depth = response
        .assets
        .iter()
        .find(|asset| &asset.info == primary_asset_info)
        .ok_or_else(|| StdError::generic_err("Cannot find primary asset in pool response"))?
        .amount;

    let secondary_asset_depth = response
        .assets
        .iter()
        .find(|asset| &asset.info == secondary_asset_info)
        .ok_or_else(|| StdError::generic_err("Cannot find secondary asset in pool response"))?
        .amount;

    Ok((primary_asset_depth, secondary_asset_depth, response.total_share))
}

/// @notice Generate msg for swapping specified asset
fn swap_msg(pair_contract: String, asset: &Asset, belief_price: Option<Decimal>, max_spread: Option<Decimal>, to: Option<String>) -> StdResult<CosmosMsg> {
    let wasm_msg = match &asset.info {
        AssetInfo::Cw20 {
            contract_addr,
        } => WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pair_contract.to_string(),
                amount: asset.amount,
                msg: to_binary(&AstroportPairCw20HookMsg::Swap {
                    belief_price,
                    max_spread,
                    to,
                })?,
            })?,
            funds: vec![],
        },

        AssetInfo::Native {
            denom,
        } => WasmMsg::Execute {
            contract_addr: pair_contract,
            msg: to_binary(&AstroportPairExecuteMsg::Swap {
                offer_asset: asset.clone().into(),
                belief_price,
                max_spread,
                to: None,
            })?,
            funds: vec![Coin {
                denom: denom.clone(),
                amount: asset.amount,
            }],
        },
    };

    Ok(CosmosMsg::Wasm(wasm_msg))
}

fn get_swap_amount(
    amount_a: U256,
    amount_b: U256,
    pool_a: U256,
    pool_b: U256,
) -> Uint128 {
    let pool_ax = amount_a + pool_a;
    let pool_bx = amount_b + pool_b;
    let area_ax = pool_ax * pool_b;
    let area_bx = pool_bx * pool_a;

    let a = U256::from(9) * area_ax + U256::from(3988000) * area_bx;
    let b = U256::from(3) * area_ax + area_ax.integer_sqrt() * a.integer_sqrt();
    let result = b / U256::from(2000) / pool_bx - pool_a;

    result.as_u128().into()
}

fn swap(
    querier: &QuerierWrapper,
    provide_a_amount: Uint128,
    provide_b_amount: Uint128,
    asset_info_a: AssetInfo,
    asset_info_b: AssetInfo,
    belief_price: Option<Decimal>,
    max_spread: Decimal,
    pair_contract: Addr,
    messages: &mut Vec<CosmosMsg>,
) -> StdResult<(Uint128, Uint128)> {
    let (pool_a_amount, pool_b_amount, _) =
        query_pool(pair_contract.to_string(), querier, &asset_info_a, &asset_info_b)?;
    let provide_a_amount = U256::from(provide_a_amount.u128());
    let provide_b_amount = U256::from(provide_b_amount.u128());
    let pool_a_amount = U256::from(pool_a_amount.u128());
    let pool_b_amount = U256::from(pool_b_amount.u128());
    let provide_a_area = provide_a_amount * pool_b_amount;
    let provide_b_area = provide_b_amount * pool_a_amount;
    let mut swap_amount_a = Uint128::zero();
    let mut swap_amount_b = Uint128::zero();

    #[allow(clippy::comparison_chain)]
    if provide_a_area > provide_b_area {
        swap_amount_a =
            get_swap_amount(provide_a_amount, provide_b_amount, pool_a_amount, pool_b_amount);
        if !swap_amount_a.is_zero() {
            let swap_asset = Asset::new(&asset_info_a, swap_amount_a)
                .with_tax_info(querier)?
                .deduct_tax()?;
            let return_amount =
                simulate(querier, pair_contract, &swap_asset.asset)
                .map_or(Uint128::zero(), |it| it.return_amount);
            if !return_amount.is_zero() {
                messages.push(swap_msg(
                    pair_contract.to_string(),
                    &swap_asset.asset,
                    belief_price,
                    Some(max_spread),
                    None,
                )?);
            }
        }
    } else if provide_a_area < provide_b_area {
        swap_amount_b =
            get_swap_amount(provide_b_amount, provide_a_amount, pool_b_amount, pool_a_amount);
        if !swap_amount_b.is_zero() {
            let swap_asset = Asset::new(asset_info_b, swap_amount_b)
                .with_tax_info(querier)?
                .deduct_tax()?;
            let return_amount =
                simulate(querier, pair_contract,&swap_asset.asset)
                .map_or(Uint128::zero(), |it| it.return_amount);
            if !return_amount.is_zero() {
                messages.push(swap_msg(
                    pair_contract.to_string(),
                    &swap_asset.asset,
                    belief_price.unwrap_or_else(None).inv(),
                    Some(max_spread),
                    None,
                )?);
            }
        }
    };

    Ok((swap_amount_a, swap_amount_b))
}
