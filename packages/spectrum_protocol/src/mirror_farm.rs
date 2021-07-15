use cosmwasm_std::{Decimal, HumanAddr, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: HumanAddr,
    pub terraswap_factory: HumanAddr,
    pub spectrum_token: HumanAddr,
    pub spectrum_gov: HumanAddr,
    pub mirror_token: HumanAddr,
    pub mirror_staking: HumanAddr,
    pub mirror_gov: HumanAddr,
    pub platform: Option<HumanAddr>,
    pub controller: Option<HumanAddr>,
    pub base_denom: String,
    pub community_fee: Decimal,
    pub platform_fee: Decimal,
    pub controller_fee: Decimal,
    pub deposit_fee: Decimal,
    pub lock_start: u64,
    pub lock_end: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum HandleMsg {
    receive(Cw20ReceiveMsg), // Bond lp token
    // Update config
    update_config {
        owner: Option<HumanAddr>,
        platform: Option<HumanAddr>,
        controller: Option<HumanAddr>,
        community_fee: Option<Decimal>,
        platform_fee: Option<Decimal>,
        controller_fee: Option<Decimal>,
        deposit_fee: Option<Decimal>,
        lock_start: Option<u64>,
        lock_end: Option<u64>,
    },
    // Unbond lp token
    unbond {
        asset_token: HumanAddr,
        amount: Uint128,
    },
    register_asset {
        asset_token: HumanAddr,
        staking_token: HumanAddr,
        weight: u32,
        auto_compound: bool,
    },
    // Withdraw rewards
    withdraw {
        // If the asset token is not given, then all rewards are withdrawn
        asset_token: Option<HumanAddr>,
    },
    harvest_all {},
    re_invest {
        asset_token: HumanAddr,
    },
    stake {
        asset_token: HumanAddr,
    },
    cast_vote_to_mirror {
        poll_id: u64,
        amount: Uint128,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Cw20HookMsg {
    bond {
        staker_addr: Option<HumanAddr>,
        asset_token: HumanAddr,
        compound_rate: Option<Decimal>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    config {}, // get config
    // get all vault settings
    pools {},
    // get deposited balances
    reward_info {
        staker_addr: HumanAddr,
        asset_token: Option<HumanAddr>,
        height: u64,
    },
    state {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolsResponse {
    pub pools: Vec<PoolItem>,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolItem {
    pub asset_token: HumanAddr,
    pub staking_token: HumanAddr,
    pub total_auto_bond_share: Uint128, // share auto bond
    pub total_stake_bond_share: Uint128,
    pub total_stake_bond_amount: Uint128, // amount stake
    pub weight: u32,
    pub auto_compound: bool,
    pub farm_share: Uint128, // MIR share
    pub state_spec_share_index: Decimal,
    pub farm_share_index: Decimal,       // per stake bond share
    pub stake_spec_share_index: Decimal, // per stake bond share
    pub auto_spec_share_index: Decimal,  // per auto bond share
    pub reinvest_allowance: Uint128,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    pub staker_addr: HumanAddr,
    pub reward_infos: Vec<RewardInfoResponseItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    pub asset_token: HumanAddr,
    pub farm_share_index: Decimal,
    pub auto_spec_share_index: Decimal,
    pub stake_spec_share_index: Decimal,
    pub bond_amount: Uint128,
    pub auto_bond_amount: Uint128,
    pub stake_bond_amount: Uint128,
    pub farm_share: Uint128,
    pub spec_share: Uint128,
    pub auto_bond_share: Uint128,
    pub stake_bond_share: Uint128,
    pub pending_farm_reward: Uint128,
    pub pending_spec_reward: Uint128,
    pub accum_spec_share: Uint128,
    pub locked_spec_share: Uint128,
    pub locked_spec_reward: Uint128,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct StateInfo {
    pub previous_spec_share: Uint128,
    pub spec_share_index: Decimal, // per weight
    pub total_farm_share: Uint128,
    pub total_weight: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
