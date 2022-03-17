use cosmwasm_std::{Decimal, QuerierWrapper, QueryRequest, StdResult, Timestamp, to_binary, Uint128, WasmQuery};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StaderCw20HookMsg {
    QueueUndelegate {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StaderExecuteMsg {
    WithdrawFundsToWallet {
        batch_id: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StaderQueryMsg {
    State {},
    BatchUndelegation {
        batch_id: u64,
    },
    GetUserUndelegationRecords {
        user_addr: String,
        start_after: Option<u64>,
        limit: Option<u64>,
    }, // return shares & undelegation list.
    GetUserUndelegationInfo {
        user_addr: String,
        batch_id: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StaderState {
    pub exchange_rate: Decimal, // shares to token value. 1 share = (ExchangeRate) tokens.
    pub current_undelegation_batch_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryBatchUndelegationResponse {
    pub batch: Option<BatchUndelegationRecord>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BatchUndelegationRecord {
    pub undelegated_tokens: Uint128,
    pub create_time: Timestamp,
    pub est_release_time: Option<Timestamp>,
    pub reconciled: bool,
    pub undelegation_er: Decimal,
    pub undelegated_stake: Uint128,
    pub unbonding_slashing_ratio: Decimal, // If Unbonding slashing happens during the 21 day period.
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetFundsClaimRecord {
    pub user_withdrawal_amount: Uint128,
    pub protocol_fee: Uint128,
    pub undelegated_tokens: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UndelegationInfo {
    pub batch_id: u64,
    pub token_amount: Uint128, // Shares undelegated
}

pub fn query_stader_state(querier: &QuerierWrapper, contract_addr: String) -> StdResult<StaderState> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr,
        msg: to_binary(&StaderQueryMsg::State {})?,
    }))
}
