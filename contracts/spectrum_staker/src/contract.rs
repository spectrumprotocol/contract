#![allow(clippy::assign_op_pattern)]
#![allow(clippy::ptr_offset_with_cast)]

use std::collections::HashSet;
use std::iter::FromIterator;

use crate::state::{config_store, read_config, Config};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{attr, from_binary, to_binary, BankMsg, Binary, CanonicalAddr, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg, QueryRequest, WasmQuery, Addr, QuerierWrapper};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use spectrum_protocol::mirror_farm::Cw20HookMsg as MirrorCw20HookMsg;
use spectrum_protocol::staker::{ConfigInfo, Cw20HookMsg, ExecuteMsg, MigrateMsg, QueryMsg, SimulateZapToBondResponse};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{Cw20HookMsg as PairCw20HookMsg, ExecuteMsg as PairExecuteMsg, PoolResponse, QueryMsg as PairQueryMsg, SimulationResponse};
use terraswap::querier::{query_balance, query_token_balance, simulate};
use terraswap::factory::{QueryMsg as FactoryQueryMsg};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use uint::construct_uint;
use spectrum_protocol::staker_single_asset::SwapOperation;

construct_uint! {
	pub struct U256(4);
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PairType {
    /// XYK pair type
    Xyk {},
    /// Stable pair type
    Stable {},
    /// Custom pair type
    Custom(String),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PairInfo {
    /// the type of asset infos available in [`AssetInfo`]
    pub asset_infos: [AssetInfo; 2],
    /// pair contract address
    pub contract_addr: Addr,
    /// pair liquidity token
    pub liquidity_token: Addr,
    /// the type of pair available in [`PairType`]
    pub pair_type: Option<PairType>,
}

// max slippage tolerance is 0.5
fn validate_slippage(slippage_tolerance: Decimal) -> StdResult<()> {
    if slippage_tolerance > Decimal::percent(50) {
        Err(StdError::generic_err("Slippage tolerance must be 0 to 0.5"))
    } else {
        Ok(())
    }
}

// validate contract with allowlist
fn validate_contract(contract: CanonicalAddr, allowlist: &HashSet<CanonicalAddr>) -> StdResult<()> {
    if !allowlist.contains(&contract) {
        Err(StdError::generic_err("not allowed"))
    } else {
        Ok(())
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ConfigInfo,
) -> StdResult<Response> {
    let allowlist = msg
        .allowlist
        .into_iter()
        .map(|w| deps.api.addr_canonicalize(&w))
        .collect::<StdResult<Vec<CanonicalAddr>>>()?;

    config_store(deps.storage).save(&Config {
        owner: deps.api.addr_canonicalize(&msg.owner)?,
        terraswap_factory: deps.api.addr_canonicalize(&msg.terraswap_factory)?,
        allowlist: HashSet::from_iter(allowlist),
    })?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::bond {
            contract,
            assets,
            slippage_tolerance,
            compound_rate,
            staker_addr,
            asset_token,
        } => bond(
            deps,
            env,
            info,
            contract,
            assets,
            slippage_tolerance,
            compound_rate,
            staker_addr,
            asset_token,
        ),
        ExecuteMsg::bond_hook {
            contract,
            asset_token,
            staking_token,
            staker_addr,
            prev_staking_token_amount,
            compound_rate,
        } => bond_hook(
            deps,
            env,
            info,
            contract,
            asset_token,
            staking_token,
            staker_addr,
            prev_staking_token_amount,
            compound_rate,
        ),
        ExecuteMsg::zap_to_bond {
            contract,
            provide_asset,
            pair_asset,
            pair_asset_b,
            belief_price,
            belief_price_b,
            max_spread,
            compound_rate,
            asset_token,
            skip_stable_swap,
            swap_hints,
        } => zap_to_bond(
            deps,
            env,
            info,
            contract,
            provide_asset,
            pair_asset,
            pair_asset_b,
            belief_price,
            belief_price_b,
            max_spread,
            compound_rate,
            asset_token,
            skip_stable_swap,
            swap_hints,
        ),
        ExecuteMsg::update_config {
            insert_allowlist,
            remove_allowlist,
        } => update_config(deps, info, insert_allowlist, remove_allowlist),
        ExecuteMsg::zap_to_unbond_hook {
            staker_addr,
            prev_target_asset,
            prev_asset_a,
            prev_asset_b,
            belief_price_a,
            belief_price_b,
            max_spread,
            swap_hints,
        } => zap_to_unbond_hook(
            deps,
            env,
            info,
            staker_addr,
            prev_target_asset,
            prev_asset_a,
            prev_asset_b,
            belief_price_a,
            belief_price_b,
            max_spread,
            swap_hints,
        ),
    }
}

fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::zap_to_unbond {
            sell_asset,
            sell_asset_b,
            target_asset,
            belief_price,
            belief_price_b,
            max_spread,
            swap_hints,
        }) => zap_to_unbond(
            deps,
            env,
            info,
            cw20_msg.sender,
            cw20_msg.amount,
            sell_asset,
            sell_asset_b,
            target_asset,
            belief_price,
            belief_price_b,
            max_spread,
            swap_hints,
        ),
        Err(_) => Err(StdError::generic_err("data should be given")),
    }
}

#[allow(clippy::too_many_arguments)]
fn bond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    assets: [Asset; 2],
    slippage_tolerance: Decimal,
    compound_rate: Option<Decimal>,
    staker_addr: Option<String>,
    asset_token: Option<String>,
) -> StdResult<Response> {
    validate_slippage(slippage_tolerance)?;

    let config = read_config(deps.storage)?;
    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    let contract_raw = deps.api.addr_canonicalize(contract.as_str())?;

    validate_contract(contract_raw, &config.allowlist)?;

    let mut funds: Vec<Coin> = vec![];
    let mut provide_assets: Vec<Asset> = vec![];
    let mut default_asset_token: Option<String> = None;
    for asset in assets.iter() {
        match asset.info.clone() {
            AssetInfo::NativeToken { denom } => {
                if info.sender != env.contract.address {
                    asset.assert_sent_native_token_balance(&info)?;
                }
                let coin = asset.deduct_tax(&deps.querier)?;
                let provide_asset = Asset {
                    info: asset.info.clone(),
                    amount: coin.amount,
                };
                if !coin.amount.is_zero() {
                    funds.push(coin);
                }
                provide_assets.push(provide_asset);
                if default_asset_token.is_none() {
                    default_asset_token = Some(denom);
                }
            },
            AssetInfo::Token { contract_addr } => {
                provide_assets.push(asset.clone());
                default_asset_token = Some(contract_addr);
            },
        }
    }

    // query pair info to obtain pair contract address
    let asset_infos = [assets[0].info.clone(), assets[1].info.clone()];
    let terraswap_pair = query_pair_info(&deps.querier, terraswap_factory, &asset_infos)?;

    // get current lp token amount to later compute the received amount
    let prev_staking_token_amount = query_token_balance(
        &deps.querier,
        terraswap_pair.liquidity_token.clone(),
        env.contract.address.clone(),
    )?;

    // 1. Transfer token asset to staking contract
    // 2. Increase allowance of token for pair contract
    // 3. Provide liquidity
    // 4. Execute staking hook, will stake in the name of the sender

    let staker = staker_addr.unwrap_or_else(|| info.sender.to_string());

    let mut messages: Vec<CosmosMsg> = vec![];
    if !assets[0].amount.is_zero() {
        if let AssetInfo::Token { contract_addr } = &assets[0].info {
            if info.sender != env.contract.address {
                messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.clone(),
                    msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                        owner: staker.clone(),
                        recipient: env.contract.address.to_string(),
                        amount: assets[0].amount,
                    })?,
                    funds: vec![],
                }));
            }
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.clone(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: terraswap_pair.contract_addr.to_string(),
                    amount: assets[0].amount,
                    expires: None,
                })?,
                funds: vec![],
            }));
        }
    }

    if !assets[1].amount.is_zero() {
        if let AssetInfo::Token { contract_addr } = &assets[1].info {
            if info.sender != env.contract.address {
                messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.clone(),
                    msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                        owner: staker.clone(),
                        recipient: env.contract.address.to_string(),
                        amount: assets[1].amount,
                    })?,
                    funds: vec![],
                }));
            }
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.clone(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: terraswap_pair.contract_addr.to_string(),
                    amount: assets[1].amount,
                    expires: None,
                })?,
                funds: vec![],
            }));
        }
    }

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: terraswap_pair.contract_addr.to_string(),
        msg: to_binary(&PairExecuteMsg::ProvideLiquidity {
            assets: [provide_assets[0].clone(), provide_assets[1].clone()],
            slippage_tolerance: Some(slippage_tolerance),
            receiver: None,
        })?,
        funds,
    }));
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::bond_hook {
            contract,
            asset_token: asset_token.unwrap_or_else(|| default_asset_token.unwrap()),
            staking_token: terraswap_pair.liquidity_token.to_string(),
            staker_addr: staker,
            prev_staking_token_amount,
            compound_rate,
        })?,
        funds: vec![],
    }));

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "bond"),
        attr("asset_token_a", assets[0].info.to_string()),
        attr("asset_token_b", assets[1].info.to_string()),
    ]))
}

#[allow(clippy::too_many_arguments)]
fn bond_hook(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    asset_token: String,
    staking_token: String,
    staker_addr: String,
    prev_staking_token_amount: Uint128,
    compound_rate: Option<Decimal>,
) -> StdResult<Response> {
    // only can be called by itself
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }

    // stake all lp tokens received, compare with staking token amount before liquidity provision was executed
    let current_staking_token_amount = query_token_balance(
        &deps.querier,
        deps.api.addr_validate(&staking_token)?,
        env.contract.address,
    )?;
    let amount_to_stake = current_staking_token_amount.checked_sub(prev_staking_token_amount)?;

    Ok(
        Response::new().add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token,
            msg: to_binary(&Cw20ExecuteMsg::Send {
                amount: amount_to_stake,
                contract,
                msg: to_binary(&MirrorCw20HookMsg::bond {
                    asset_token,
                    staker_addr: Some(staker_addr),
                    compound_rate,
                })?,
            })?,
            funds: vec![],
        })]),
    )
}

fn query_pair_info(
    querier: &QuerierWrapper,
    factory_contract: Addr,
    asset_infos: &[AssetInfo; 2],
) -> StdResult<PairInfo> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: factory_contract.to_string(),
        msg: to_binary(&FactoryQueryMsg::Pair {
            asset_infos: asset_infos.clone(),
        })?,
    }))
}

pub(crate) fn compute_swap_amount(
    amount_a: Uint128,
    amount_b: Uint128,
    pool_a: Uint128,
    pool_b: Uint128,
) -> Uint128 {
    let amount_a = U256::from(amount_a.u128());
    let amount_b = U256::from(amount_b.u128());
    let pool_a = U256::from(pool_a.u128());
    let pool_b = U256::from(pool_b.u128());

    let pool_ax = amount_a + pool_a;
    let pool_bx = amount_b + pool_b;
    let area_ax = pool_ax * pool_b;
    let area_bx = pool_bx * pool_a;

    let a = U256::from(9) * area_ax + U256::from(3988000) * area_bx;
    let b = U256::from(3) * area_ax + area_ax.integer_sqrt() * a.integer_sqrt();
    let result = b / U256::from(2000) / pool_bx - pool_a;

    result.as_u128().into()
}

fn get_swap_amount(
    pool: &PoolResponse,
    asset: &Asset,
    pair_type: Option<PairType>,
    skip_stable_swap: bool,
) -> Uint128 {
    if let Some(PairType::Stable {}) = pair_type {
        if skip_stable_swap {
            Uint128::zero()
        } else {
            asset.amount.multiply_ratio(10000u128, 19995u128)
        }
    } else if pool.assets[0].info == asset.info {
        compute_swap_amount(asset.amount, Uint128::zero(), pool.assets[0].amount, pool.assets[1].amount)
    } else {
        compute_swap_amount(asset.amount, Uint128::zero(), pool.assets[1].amount, pool.assets[0].amount)
    }
}

fn apply_pool(
    pool: &mut PoolResponse,
    swap_asset: &Asset,
    return_amount: Uint128,
) {
    if pool.assets[0].info == swap_asset.info {
        pool.assets[0].amount += swap_asset.amount;
        pool.assets[1].amount -= return_amount;
    } else {
        pool.assets[1].amount += swap_asset.amount;
        pool.assets[0].amount -= return_amount;
    }
}

fn create_swap_msg(
    asset_info: AssetInfo,
    contract: String,
    amount: Uint128,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<String>,
) -> StdResult<CosmosMsg> {
    Ok(match asset_info {
        AssetInfo::Token { contract_addr } => CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract,
                amount,
                msg: to_binary(&PairCw20HookMsg::Swap {
                    belief_price,
                    max_spread,
                    to,
                })?
            })?,
            funds: vec![],
        }),
        AssetInfo::NativeToken { denom } => CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract,
            msg: to_binary(&PairExecuteMsg::Swap {
                offer_asset: Asset {
                    info: AssetInfo::NativeToken { denom: denom.clone() },
                    amount,
                },
                belief_price,
                max_spread,
                to,
            })?,
            funds: vec![
                Coin { denom, amount }
            ],
        }),
    })
}

#[allow(clippy::too_many_arguments)]
fn zap_to_bond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    provide_asset: Asset,
    pair_asset_a: AssetInfo,
    pair_asset_b: Option<AssetInfo>,
    belief_price_a: Option<Decimal>,
    belief_price_b: Option<Decimal>,
    max_spread: Decimal,
    compound_rate: Option<Decimal>,
    asset_token: Option<String>,
    skip_stable_swap: Option<bool>,
    swap_hints: Option<Vec<SwapOperation>>,
) -> StdResult<Response> {
    validate_slippage(max_spread)?;
    provide_asset.assert_sent_native_token_balance(&info)?;

    let config = read_config(deps.storage)?;
    let contract_raw = deps.api.addr_canonicalize(contract.as_str())?;

    validate_contract(contract_raw, &config.allowlist)?;

    if !provide_asset.is_native_token() {
        return Err(StdError::generic_err("not support provide_asset as token"));
    }

    let mut messages: Vec<CosmosMsg> = vec![];
    compute_zap_to_bond(
        deps.as_ref(),
        env,
        &config,
        contract,
        provide_asset.clone(),
        pair_asset_a.clone(),
        pair_asset_b.clone(),
        belief_price_a,
        belief_price_b,
        Some(max_spread),
        compound_rate,
        asset_token,
        Some(info.sender.to_string()),
        false,
        skip_stable_swap.unwrap_or_default(),
        swap_hints,
        &mut messages,
    )?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![
            attr("action", "zap_to_bond"),
            attr("asset_token_a", pair_asset_a.to_string()),
            attr("asset_token_b", pair_asset_b.unwrap_or_else(|| provide_asset.info.clone()).to_string()),
            attr("provide_amount", provide_asset.amount),
        ]))
}

fn do_swap(
    querier: &QuerierWrapper,
    swaps: Vec<SwapOperation>,
    offer_amount: Uint128,
    max_spread: Option<Decimal>,
    to: Option<String>,
    messages: &mut Vec<CosmosMsg>,
) -> StdResult<SimulationResponse> {
    let mut i = 0usize;
    let len = swaps.len();
    let mut amount = offer_amount;
    let mut commission_amount = Uint128::zero();
    for swap in swaps {
        i += 1;
        let simulate = simulate(
            querier,
            Addr::unchecked(swap.pair_contract.clone()),
            &Asset {
                info: swap.asset_info.clone(),
                amount,
            })?;
        messages.push(
            create_swap_msg(
                swap.asset_info,
                swap.pair_contract,
                amount,
                swap.belief_price,
                max_spread,
                if i < len { None } else { to.clone() },
            )?,
        );
        amount = simulate.return_amount;
        commission_amount = simulate.commission_amount;
    }

    Ok(SimulationResponse {
        return_amount: amount,
        commission_amount,
        spread_amount: Uint128::zero(),
    })
}

#[allow(clippy::too_many_arguments)]
fn compute_zap_to_bond(
    deps: Deps,
    env: Env,
    config: &Config,
    contract: String,
    provide_asset: Asset,
    pair_asset_a: AssetInfo,
    pair_asset_b: Option<AssetInfo>,
    belief_price_a: Option<Decimal>,
    belief_price_b: Option<Decimal>,
    max_spread: Option<Decimal>,
    compound_rate: Option<Decimal>,
    asset_token: Option<String>,
    staker_addr: Option<String>,
    simulation_mode: bool,
    skip_stable_swap: bool,
    swap_hints: Option<Vec<SwapOperation>>,
    messages: &mut Vec<CosmosMsg>,
) -> StdResult<Option<SimulateZapToBondResponse>> {

    if let Some(pair_asset_b) = pair_asset_b {
        let ust_swap_asset = Asset {
            info: provide_asset.info.clone(),
            amount: provide_asset.amount,
        };
        let tax_amount = ust_swap_asset.compute_tax(&deps.querier)?;
        let offer_amount = provide_asset.amount.checked_sub(tax_amount)?;

        // swap ust -> A
        let swaps = match swap_hints {
            None => {
                let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
                let asset_pair_a = [provide_asset.info.clone(), pair_asset_a.clone()];
                let terraswap_pair_a = query_pair_info(&deps.querier, terraswap_factory, &asset_pair_a)?;

                vec![SwapOperation {
                    pair_contract: terraswap_pair_a.contract_addr.to_string(),
                    asset_info: provide_asset.info.clone(),
                    belief_price: belief_price_a,
                }]
            },
            Some(swaps) => swaps
        };
        let simulate_a = do_swap(
            &deps.querier,
            swaps,
            offer_amount,
            max_spread,
            None,
            messages,
        )?;

        let asset_token = asset_token.unwrap_or_else(|| pair_asset_b.to_string());
        let res = compute_zap_to_bond(
            deps,
            env,
            config,
            contract,
            Asset {
                info: pair_asset_a,
                amount: simulate_a.return_amount,
            },
            pair_asset_b,
            None,
            belief_price_b,
            None,
            max_spread,
            compound_rate,
            Some(asset_token),
            staker_addr,
            simulation_mode,
            skip_stable_swap,
            None,
            messages,
        )?;

        if let Some(res) = res {
            let price_a = Decimal::from_ratio(offer_amount, simulate_a.return_amount + simulate_a.commission_amount);
            Ok(Some(SimulateZapToBondResponse {
                lp_amount: res.lp_amount,
                belief_price: price_a,
                belief_price_b: Some(res.belief_price),
                swap_ust: provide_asset.amount,
                receive_a: simulate_a.return_amount,
                swap_a: Some(res.swap_ust),
                provide_a: res.provide_b,
                provide_b: res.provide_a,
            }))
        } else {
            Ok(None)
        }
    } else {
        let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
        let asset_pair_a = [provide_asset.info.clone(), pair_asset_a.clone()];
        let terraswap_pair_a = query_pair_info(&deps.querier, terraswap_factory, &asset_pair_a)?;

        let mut pool: PoolResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: terraswap_pair_a.contract_addr.to_string(),
            msg: to_binary(&PairQueryMsg::Pool {})?,
        }))?;
        let swap_amount = get_swap_amount(&pool, &provide_asset, terraswap_pair_a.pair_type.clone(), skip_stable_swap);
        let bond_asset = Asset {
            info: provide_asset.info.clone(),
            amount: provide_asset.amount.checked_sub(swap_amount)?,
        };
        let (amount_a, price_a) = if !swap_amount.is_zero() {
            let swap_asset = Asset {
                info: provide_asset.info.clone(),
                amount: swap_amount,
            };
            let tax_amount = swap_asset.compute_tax(&deps.querier)?;
            let swap_asset = Asset {
                info: provide_asset.info,
                amount: swap_amount.checked_sub(tax_amount)?,
            };

            // swap ust -> A
            let simulate_a = simulate(
                &deps.querier,
                terraswap_pair_a.contract_addr.clone(),
                &swap_asset)?;
            apply_pool(&mut pool, &swap_asset, simulate_a.return_amount);
            let price_a = Decimal::from_ratio(swap_asset.amount, simulate_a.return_amount + simulate_a.commission_amount);
            let amount_a = simulate_a.return_amount;
            messages.push(
                create_swap_msg(
                    swap_asset.info.clone(),
                    terraswap_pair_a.contract_addr.to_string(),
                    swap_asset.amount,
                    belief_price_a,
                    max_spread,
                    None,
                )?,
            );
            (amount_a, price_a)
        } else {
            (Uint128::zero(), Decimal::zero())
        };

        let assets = [Asset {
            info: pair_asset_a,
            amount: amount_a,
        }, bond_asset.clone()];
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::bond {
                contract,
                assets: assets.clone(),
                slippage_tolerance: max_spread.unwrap_or_else(|| Decimal::percent(50)),
                compound_rate,
                staker_addr,
                asset_token,
            })?,
            funds: vec![],
        }));

        if simulation_mode {
            let (pool_a, pool_b) = if pool.assets[0].info.clone() == assets[0].info {
                (pool.assets[0].amount, pool.assets[1].amount)
            } else {
                (pool.assets[1].amount, pool.assets[0].amount)
            };
            let lp_amount = std::cmp::min(
                assets[0].amount.multiply_ratio(pool.total_share, pool_a),
                assets[1].amount.multiply_ratio(pool.total_share, pool_b),
            );
            Ok(Some(SimulateZapToBondResponse {
                lp_amount,
                belief_price: price_a,
                belief_price_b: None,
                swap_ust: swap_amount,
                receive_a: amount_a,
                swap_a: None,
                provide_a: amount_a,
                provide_b: bond_asset.amount,
            }))
        } else {
            Ok(None)
        }

    }

}

fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    insert_allowlist: Option<Vec<String>>,
    remove_allowlist: Option<Vec<String>>,
) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;

    if deps.api.addr_canonicalize(info.sender.as_str())? != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(add_allowlist) = insert_allowlist {
        for contract in add_allowlist.iter() {
            config.allowlist.insert(deps.api.addr_canonicalize(contract)?);
        }
    }

    if let Some(remove_allowlist) = remove_allowlist {
        for contract in remove_allowlist.iter() {
            config.allowlist.remove(&deps.api.addr_canonicalize(contract)?);
        }
    }

    config_store(deps.storage).save(&config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

fn get_balance(
    deps: &Deps,
    account_addr: Addr,
    asset_info: AssetInfo,
) -> StdResult<Uint128> {
    match asset_info {
        AssetInfo::Token { contract_addr } => query_token_balance(
            &deps.querier,
            deps.api.addr_validate(&contract_addr)?,
            account_addr,
        ),
        AssetInfo::NativeToken { denom } => query_balance(&deps.querier, account_addr, denom),
    }
}

#[allow(clippy::too_many_arguments)]
fn zap_to_unbond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    staker_addr: String,
    amount: Uint128,
    sell_asset_a: AssetInfo,
    sell_asset_b: Option<AssetInfo>,
    target_asset: AssetInfo,
    belief_price_a: Option<Decimal>,
    belief_price_b: Option<Decimal>,
    max_spread: Decimal,
    swap_hints: Option<Vec<SwapOperation>>,
) -> StdResult<Response> {
    validate_slippage(max_spread)?;

    let denom = match target_asset.clone() {
        AssetInfo::NativeToken { denom } => denom,
        _ => return Err(StdError::generic_err("not support target_asset as token")),
    };

    let config = read_config(deps.storage)?;
    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    let asset_infos = if let Some(asset_info) = sell_asset_b.clone() {
        [sell_asset_a.clone(), asset_info]
    } else {
        [target_asset.clone(), sell_asset_a.clone()]
    };
    let terraswap_pair = query_pair_info(&deps.querier, terraswap_factory, &asset_infos)?;

    if terraswap_pair.liquidity_token != info.sender {
        return Err(StdError::generic_err("invalid lp token"));
    }

    let current_denom_amount = query_balance(&deps.querier, env.contract.address.clone(), denom)?;
    let current_token_a_amount = get_balance(&deps.as_ref(), env.contract.address.clone(), sell_asset_a.clone())?;
    let current_token_b_asset = match sell_asset_b {
        Some(sell_asset_b) => {
            let current_token_b_amount = get_balance(&deps.as_ref(), env.contract.address.clone(), sell_asset_b.clone())?;
            Some(Asset {
                info: sell_asset_b,
                amount: current_token_b_amount,
            })
        },
        None => None,
    };

    Ok(Response::new().add_messages(vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: terraswap_pair.liquidity_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                amount,
                contract: terraswap_pair.contract_addr.to_string(),
                msg: to_binary(&PairCw20HookMsg::WithdrawLiquidity {})?,
            })?,
            funds: vec![],
        }),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::zap_to_unbond_hook {
                staker_addr,
                prev_target_asset: Asset {
                    amount: current_denom_amount,
                    info: target_asset,
                },
                prev_asset_a: Asset {
                    amount: current_token_a_amount,
                    info: sell_asset_a,
                },
                prev_asset_b: current_token_b_asset,
                belief_price_a,
                belief_price_b,
                max_spread,
                swap_hints,
            })?,
            funds: vec![],
        }),
    ]))
}

#[allow(clippy::too_many_arguments)]
fn zap_to_unbond_hook(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    staker_addr: String,
    prev_target_asset: Asset,
    prev_asset_a: Asset,
    prev_asset_b: Option<Asset>,
    belief_price_a: Option<Decimal>,
    belief_price_b: Option<Decimal>,
    max_spread: Decimal,
    swap_hints: Option<Vec<SwapOperation>>,
) -> StdResult<Response> {
    // only can be called by itself
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;
    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    if let Some(prev_asset_b) = prev_asset_b {
        let asset_token_b = match prev_asset_b.info.clone() {
            AssetInfo::Token { contract_addr } => contract_addr,
            _ => return Err(StdError::generic_err("not support sell_asset as native coin")),
        };
        let asset_infos = [prev_asset_b.info.clone(), prev_asset_a.info.clone()];
        let terraswap_pair = query_pair_info(&deps.querier, terraswap_factory, &asset_infos)?;
        let current_token_b_amount = query_token_balance(
            &deps.querier,
            deps.api.addr_validate(&asset_token_b)?,
            env.contract.address.clone())?;

        Ok(Response::new().add_messages(vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: asset_token_b,
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: terraswap_pair.contract_addr.to_string(),
                    amount: current_token_b_amount.checked_sub(prev_asset_b.amount)?,
                    msg: to_binary(&PairCw20HookMsg::Swap {
                        to: None,
                        belief_price: belief_price_b,
                        max_spread: Some(max_spread),
                    })?,
                })?,
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::zap_to_unbond_hook {
                    staker_addr,
                    prev_target_asset,
                    prev_asset_a,
                    prev_asset_b: None,
                    belief_price_a,
                    belief_price_b: None,
                    max_spread,
                    swap_hints,
                })?,
                funds: vec![],
            }),
        ]))
    } else {
        let denom = match prev_target_asset.info.clone() {
            AssetInfo::NativeToken { denom } => denom,
            _ => return Err(StdError::generic_err("not support target_asset as token")),
        };
        let current_token_a_amount = get_balance(
            &deps.as_ref(),
            env.contract.address.clone(),
            prev_asset_a.info.clone(),
        )?;
        let current_denom_amount = deps
            .querier
            .query_balance(env.contract.address.to_string(), denom.clone())?
            .amount;

        let transfer_asset = Asset {
            info: prev_target_asset.info.clone(),
            amount: current_denom_amount.checked_sub(prev_target_asset.amount)?,
        };
        let mut messages: Vec<CosmosMsg> = vec![];
        if !transfer_asset.amount.is_zero() {
            let tax_amount = transfer_asset.compute_tax(&deps.querier)?;
            messages.push(CosmosMsg::Bank(BankMsg::Send {
                to_address: staker_addr.clone(),
                amount: vec![Coin {
                    denom,
                    amount: transfer_asset.amount.checked_sub(tax_amount)?,
                }],
            }));
        }

        let offer_amount = current_token_a_amount.checked_sub(prev_asset_a.amount)?;

        let swaps = match swap_hints {
            None => {
                let asset_infos = [prev_target_asset.info, prev_asset_a.info.clone()];
                let terraswap_pair = query_pair_info(&deps.querier, terraswap_factory, &asset_infos)?;
                vec![SwapOperation {
                    pair_contract: terraswap_pair.contract_addr.to_string(),
                    asset_info: prev_asset_a.info,
                    belief_price: belief_price_a,
                }]
            }
            Some(swaps) => swaps,
        };
        do_swap(
            &deps.querier,
            swaps,
            offer_amount,
            Some(max_spread),
            Some(staker_addr),
            &mut messages,
        )?;

        Ok(Response::new().add_messages(messages))
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::simulate_zap_to_bond {
            provide_asset,
            pair_asset,
            pair_asset_b,
            swap_hints,
        } => to_binary(&simulate_zap_to_bond(
            deps,
            env,
            provide_asset,
            pair_asset,
            pair_asset_b,
            swap_hints,
        )?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigInfo> {
    let config = read_config(deps.storage)?;
    let resp = ConfigInfo {
        owner: deps.api.addr_humanize(&config.owner)?.to_string(),
        terraswap_factory: deps
            .api
            .addr_humanize(&config.terraswap_factory)?
            .to_string(),
        allowlist: config
            .allowlist
            .into_iter()
            .map(|w| deps.api.addr_humanize(&w).map(|addr| addr.to_string()))
            .collect::<StdResult<Vec<String>>>()?,
    };

    Ok(resp)
}

fn simulate_zap_to_bond(
    deps: Deps,
    env: Env,
    provide_asset: Asset,
    pair_asset_a: AssetInfo,
    pair_asset_b: Option<AssetInfo>,
    swap_hints: Option<Vec<SwapOperation>>,
) -> StdResult<SimulateZapToBondResponse> {
    let config = read_config(deps.storage)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    let res = compute_zap_to_bond(
        deps,
        env,
        &config,
        "".to_string(),
        provide_asset,
        pair_asset_a,
        pair_asset_b,
        None,
        None,
        None,
        None,
        None,
        None,
        true,
        false,
        swap_hints,
        &mut messages,
    )?;

    Ok(res.unwrap())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
