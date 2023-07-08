use classic_bindings::TerraQuery;
use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Addr};
use anchor_token::gov::{QueryMsg, StakerResponse};

pub fn query_anchor_gov(
    deps: Deps<TerraQuery>,
    anchor_gov: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<StakerResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(anchor_gov)?.to_string(),
        msg: to_binary(&QueryMsg::Staker {
            address: staker.to_string()
        })?,
    }))
}
