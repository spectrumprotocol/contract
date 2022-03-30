#![allow(non_camel_case_types)]

use cosmwasm_std::{Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use terraswap::asset::AssetInfo;
use crate::state::{Burn, HubType, StakeCredit};

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: String,
    pub spectrum_token: String,
    pub spectrum_gov: String,
    pub platform: String,
    pub controller: String,
    pub community_fee: Decimal,
    pub platform_fee: Decimal,
    pub controller_fee: Decimal,
    pub deposit_fee: Decimal,
    pub anchor_market: String,
    pub aust_token: String,
    pub max_unbond_count: u32,
    pub burn_period: u64,
    pub ust_pair_contract: String,
    pub oracle: String,
    pub credits: Vec<StakeCredit>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteMsg {
    bond {
        staker_addr: Option<String>,
    },
    // Update config
    update_config {
        owner: Option<String>,
        controller: Option<String>,
        community_fee: Option<Decimal>,
        platform_fee: Option<Decimal>,
        controller_fee: Option<Decimal>,
        deposit_fee: Option<Decimal>,
        max_unbond_count: Option<u32>,
        burn_period: Option<u64>,
        credits: Option<Vec<StakeCredit>>,
    },
    // Unbond luna
    unbond {
        amount: Uint128,
    },
    claim_unbond {},
    // Withdraw rewards
    withdraw {
        // If the asset token is not given, then all rewards are withdrawn
        spec_amount: Option<Uint128>,
    },
    register_hub {
        token: String,
        hub_address: String,
        hub_type: HubType,
    },
    burn {
        amount: Uint128,
        swap_operations: Vec<SwapOperation>,
        min_profit: Option<Decimal>,
    },
    collect {},
    collect_hook {
        prev_balance: Uint128,
        total_input_amount: Uint128,
    },
    collect_fee {},
    send_fee {
        deposit_fee_ratio: Decimal,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SwapOperation {
    pub to_asset_info: AssetInfo,
    pub pair_address: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    config {}, // get config
    // get deposited balances
    reward_info {
        staker_addr: String,
    },
    unbond {
        staker_addr: String,
    },
    state {},
    hubs {},
    burns {},
    simulate_collect {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    pub staker_addr: String,
    pub reward_infos: Vec<RewardInfoResponseItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    pub asset_token: String,
    pub spec_share_index: Decimal,
    pub bond_amount: Uint128,
    pub spec_share: Uint128,
    pub bond_share: Uint128,
    pub pending_spec_reward: Uint128,
    pub deposit_amount: Uint128,
    pub deposit_time: u64,
    pub unbonding_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SimulateCollectResponse {
    pub burnable: Uint128,
    pub total_bond_amount: Uint128,
    pub prev_unbonded_index: Uint128,
    pub unbonded_index: Uint128,
    pub can_collect: bool,
    pub remaining_burns: Vec<Burn>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
}
