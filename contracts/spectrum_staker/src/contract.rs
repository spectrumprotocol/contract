use crate::state::{config_store, read_config, Config};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use spectrum_protocol::mirror_farm::Cw20HookMsg;
use spectrum_protocol::staker::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::ExecuteMsg as PairExecuteMsg;
use terraswap::querier::{query_pair_info, query_token_balance};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    config_store(deps.storage).save(&Config {
        terraswap_factory: deps.api.addr_canonicalize(&msg.terraswap_factory)?,
    })?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> StdResult<Response> {
    match msg {
        ExecuteMsg::bond {
            contract,
            assets,
            slippage_tolerance,
            compound_rate,
        } => bond(
            deps,
            env,
            info,
            contract,
            assets,
            slippage_tolerance,
            compound_rate,
        ),
        ExecuteMsg::bond_hook {
            contract,
            asset_token,
            staking_token,
            staker_addr,
            compound_rate,
        } => bond_hook(
            deps,
            env,
            info,
            contract,
            asset_token,
            staking_token,
            staker_addr,
            compound_rate,
        ),
    }
}

fn bond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    assets: [Asset; 2],
    slippage_tolerance: Option<Decimal>,
    compound_rate: Option<Decimal>,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let terraswap_factory = deps
        .api
        .addr_humanize(&config.terraswap_factory)?
        .to_string();

    let mut native_asset_op: Option<Asset> = None;
    let mut token_info_op: Option<(String, Uint128)> = None;
    for asset in assets.iter() {
        match asset.info.clone() {
            AssetInfo::Token { contract_addr } => {
                token_info_op = Some((contract_addr, asset.amount))
            }
            AssetInfo::NativeToken { .. } => {
                asset.assert_sent_native_token_balance(&info)?;
                native_asset_op = Some(asset.clone())
            }
        }
    }

    // will fail if one of them is missing
    let native_asset = match native_asset_op {
        Some(v) => v,
        None => return Err(StdError::generic_err("Missing native asset")),
    };
    let (token_addr, token_amount) = match token_info_op {
        Some(v) => v,
        None => return Err(StdError::generic_err("Missing token asset")),
    };

    // query pair info to obtain pair contract address
    let asset_infos = [assets[0].info.clone(), assets[1].info.clone()];
    let terraswap_pair = query_pair_info(
        &deps.querier,
        deps.api.addr_validate(&terraswap_factory)?,
        &asset_infos,
    )?;

    // compute tax
    let tax_amount = native_asset.compute_tax(&deps.querier)?;
    let native_asset = Asset {
        amount: native_asset.amount.checked_sub(tax_amount)?,
        info: native_asset.info,
    };

    // 1. Transfer token asset to staking contract
    // 2. Increase allowance of token for pair contract
    // 3. Provide liquidity
    // 4. Execute staking hook, will stake in the name of the sender

    Ok(Response::new()
        .add_messages(vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_addr.clone(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount: token_amount,
                })?,
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_addr.clone(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: terraswap_pair.contract_addr.clone(),
                    amount: token_amount,
                    expires: None,
                })?,
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: terraswap_pair.contract_addr,
                msg: to_binary(&PairExecuteMsg::ProvideLiquidity {
                    assets: if let AssetInfo::NativeToken { .. } = assets[0].info.clone() {
                        [native_asset.clone(), assets[1].clone()]
                    } else {
                        [assets[0].clone(), native_asset.clone()]
                    },
                    slippage_tolerance,
                    receiver: None,
                })?,
                funds: vec![Coin {
                    denom: native_asset.info.to_string(),
                    amount: native_asset.amount,
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::bond_hook {
                    contract,
                    asset_token: token_addr.clone(),
                    staking_token: terraswap_pair.liquidity_token,
                    staker_addr: Some(info.sender.to_string()),
                    compound_rate,
                })?,
                funds: vec![],
            }),
        ])
        .add_attributes(vec![
            attr("action", "bond"),
            attr("asset_token", token_addr),
            attr("tax_amount", tax_amount.to_string()),
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
    staker_addr: Option<String>,
    compound_rate: Option<Decimal>,
) -> StdResult<Response> {

    // stake all lp tokens received, compare with staking token amount before liquidity provision was executed
    let amount_to_stake = query_token_balance(
        &deps.querier,
        deps.api.addr_validate(&staking_token)?,
        env.contract.address,
    )?;

    Ok(
        Response::new().add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token,
            msg: to_binary(&Cw20ExecuteMsg::Send {
                amount: amount_to_stake,
                contract,
                msg: to_binary(&Cw20HookMsg::bond {
                    asset_token,
                    staker_addr: Some(staker_addr.unwrap_or_else(|| info.sender.to_string())),
                    compound_rate,
                })?,
            })?,
            funds: vec![],
        })]),
    )
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    Err(StdError::generic_err("query not support"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
