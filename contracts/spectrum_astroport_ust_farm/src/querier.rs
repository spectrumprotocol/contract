use astroport::generator::{
    PendingTokenResponse, QueryMsg as AstroportQueryMsg,
};
use astroport::staking::{ Cw20HookMsg as AstroportCw20HookMsg };
use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, Uint128, WasmQuery, Addr};
use spectrum_protocol::gov_proxy::{QueryMsg as GovProxyQueryMsg, StakerInfoGovResponse};

pub fn query_astroport_pending_token(
    deps: Deps,
    lp_token: &CanonicalAddr,
    staker: &Addr,
    astroport_generator: &CanonicalAddr,
) -> StdResult<PendingTokenResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(astroport_generator)?.to_string(),
        msg: to_binary(&AstroportQueryMsg::PendingToken {
            lp_token: deps.api.addr_humanize(lp_token)?,
            user: staker.clone(),
        })?,
    }))
}

pub fn query_astroport_pool_balance(
    deps: Deps,
    lp_token: &CanonicalAddr,
    staker: &Addr,
    astroport_generator: &CanonicalAddr,
) -> StdResult<Uint128> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(astroport_generator)?.to_string(),
        msg: to_binary(&AstroportQueryMsg::Deposit {
            lp_token: deps.api.addr_humanize(lp_token)?,
            user: staker.clone(),
        })?,
    }))
}

pub fn query_farm_gov_balance(
    deps: Deps,
    gov_proxy: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<StakerInfoGovResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(gov_proxy)?.to_string(),
        msg: to_binary(&GovProxyQueryMsg::StakerInfo {
            staker_addr: staker.to_string(),
        })?,
    }))
}
