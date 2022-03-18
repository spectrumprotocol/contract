use crate::asset::AssetInfo;
use crate::factory::PairType;
use cosmwasm_std::{Addr, Binary, Decimal, Uint128, Uint64};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes the parameters used for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Address that can change contract settings
    pub owner: String,
    /// Address of factory contract
    pub factory: String,
    /// Address that can set active generators and their alloc points
    pub generator_controller: Option<String>,
    /// Address of guardian
    pub guardian: Option<String>,
    /// ASTRO token contract address
    pub astro_token: String,
    /// Amount of ASTRO distributed per block among all pairs
    pub tokens_per_block: Uint128,
    /// Start block for distributing ASTRO
    pub start_block: Uint64,
    /// Dual rewards proxy contracts allowed to interact with the generator
    pub allowed_reward_proxies: Vec<String>,
    /// The ASTRO vesting contract that drips ASTRO rewards
    pub vesting_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Update the address of the ASTRO vesting contract
    /// ## Executor
    /// Only the owner can execute it.
    UpdateConfig {
        /// The new vesting contract address
        vesting_contract: Option<String>,
        /// The new generator controller contract address
        generator_controller: Option<String>,
        /// The new generator guardian
        guardian: Option<String>,
    },
    /// Setup generators with their respective allocation points.
    /// ## Executor
    /// Only the owner or generator controller can execute this.
    SetupPools {
        /// The list of pools with allocation point.
        pools: Vec<(String, Uint64)>,
    },
    /// Update the given pool's ASTRO allocation slice
    /// ## Executor
    /// Only the owner or generator controller can execute this.
    UpdatePool {
        /// The address of the LP token contract address whose allocation we change
        lp_token: String,
        /// This flag determines whether the pool gets 3rd party token rewards
        has_asset_rewards: bool,
    },
    /// Update rewards and return it to user.
    ClaimRewards {
        /// the LP token contract address
        lp_tokens: Vec<String>,
    },
    /// Withdraw LP tokens from the Generator
    Withdraw {
        /// The address of the LP token to withdraw
        lp_token: String,
        /// The amount to withdraw
        amount: Uint128,
    },
    /// Withdraw LP tokens from the Generator without withdrawing outstanding rewards
    EmergencyWithdraw {
        /// The address of the LP token to withdraw
        lp_token: String,
    },
    /// Allowed reward proxy contracts that can interact with the Generator
    SetAllowedRewardProxies {
        /// The full list of allowed proxy contracts
        proxies: Vec<String>,
    },
    /// Sends orphan proxy rewards (which were left behind after emergency withdrawals) to another address
    SendOrphanProxyReward {
        /// The transfer recipient
        recipient: String,
        /// The address of the LP token contract for which we send orphaned rewards
        lp_token: String,
    },
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// Set a new amount of ASTRO to distribute per block
    /// ## Executor
    /// Only the owner can execute this.
    SetTokensPerBlock {
        /// The new amount of ASTRO to distro per block
        amount: Uint128,
    },
    /// Creates a request to change contract ownership
    /// ## Executor
    /// Only the current owner can execute this.
    ProposeNewOwner {
        /// The newly proposed owner
        owner: String,
        /// The validity period of the proposal to change the contract owner
        expires_in: u64,
    },
    /// Removes a request to change contract ownership
    /// ## Executor
    /// Only the current owner can execute this
    DropOwnershipProposal {},
    /// Claims contract ownership
    /// ## Executor
    /// Only the newly proposed owner can execute this
    ClaimOwnership {},
    /// Add or remove a proxy contract that can interact with the Generator
    UpdateAllowedProxies {
        /// Allowed proxy contract
        add: Option<Vec<String>>,
        /// Proxy contracts to remove
        remove: Option<Vec<String>>,
    },
    /// Sets a new proxy contract for a specific generator
    /// Sets a proxy for the pool
    /// ## Executor
    /// Only the current owner or generator controller can execute this
    MoveToProxy { lp_token: String, proxy: String },
    /// Add or remove token to blocked list
    UpdateTokensBlockedlist {
        /// Tokens to add
        add: Option<Vec<AssetInfo>>,
        /// Tokens to remove
        remove: Option<Vec<AssetInfo>>,
    },
    /// Sets the allocation point to zero for the specified pool
    DeactivatePool { lp_token: String },
    /// Sets the allocation point to zero for each pool by the pair type
    DeactivatePools { pair_types: Vec<PairType> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns the length of the array that contains all the active pool generators
    ActivePoolLength {},
    /// PoolLength returns the length of the array that contains all the instantiated pool generators
    PoolLength {},
    /// Deposit returns the LP token amount deposited in a specific generator
    Deposit { lp_token: String, user: String },
    /// PendingToken returns the amount of rewards that can be claimed by an account that deposited a specific LP token in a generator
    PendingToken { lp_token: String, user: String },
    /// Config returns the main contract parameters
    Config {},
    /// RewardInfo returns reward information for a specified LP token
    RewardInfo { lp_token: String },
    /// OrphanProxyRewards returns orphaned reward information for the specified LP token
    OrphanProxyRewards { lp_token: String },
    /// PoolInfo returns information about a pool associated with the specified LP token alongside
    /// the total pending amount of ASTRO and proxy rewards claimable by generator stakers (for that LP token)
    PoolInfo { lp_token: String },
    /// SimulateFutureReward returns the amount of ASTRO that will be distributed until a future block and for a specific generator
    SimulateFutureReward { lp_token: String, future_block: u64 },
    /// Returns a list of stakers for a specific generator
    PoolStakers {
        lp_token: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns the blocked list of tokens
    BlockedListTokens {},
}

/// This structure holds the response returned when querying the total length of the array that keeps track of instantiated generators
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolLengthResponse {
    pub length: usize,
}

/// This structure holds the response returned when querying the amount of pending rewards that can be withdrawn from a 3rd party
/// rewards contract
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PendingTokenResponse {
    /// The amount of pending ASTRO
    pub pending: Uint128,
    /// The amount of pending 3rd party reward tokens
    pub pending_on_proxy: Option<Uint128>,
}

/// This structure describes the main information of pool
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    /// Accumulated amount of reward per share unit. Used for reward calculations
    pub last_reward_block: Uint64,
    pub accumulated_rewards_per_share: Decimal,
    /// the reward proxy contract
    pub reward_proxy: Option<Addr>,
    pub accumulated_proxy_rewards_per_share: Decimal,
    /// for calculation of new proxy rewards
    pub proxy_reward_balance_before_update: Uint128,
    /// the orphan proxy rewards which are left by emergency withdrawals
    pub orphan_proxy_rewards: Uint128,
    /// The pool has assets giving additional rewards
    pub has_asset_rewards: bool,
}

/// This structure holds the response returned when querying for the token addresses used to reward a specific generator
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    /// The address of the base reward token
    pub base_reward_token: Addr,
    /// The address of the 3rd party reward token
    pub proxy_reward_token: Option<Addr>,
}

/// This structure holds the response returned when querying for a pool's information
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfoResponse {
    /// The slice of ASTRO that this pool's generator gets per block
    pub alloc_point: Uint64,
    /// Amount of ASTRO tokens being distributed per block to this LP pool
    pub astro_tokens_per_block: Uint128,
    /// The last block when token emissions were snapshotted (distributed)
    pub last_reward_block: u64,
    /// Current block number. Useful for computing APRs off-chain
    pub current_block: u64,
    /// Total amount of ASTRO rewards already accumulated per LP token staked
    pub accumulated_rewards_per_share: Decimal,
    /// Pending amount of total ASTRO rewards which are claimable by stakers right now
    pub pending_astro_rewards: Uint128,
    /// The address of the 3rd party reward proxy contract
    pub reward_proxy: Option<Addr>,
    /// Pending amount of total proxy rewards which are claimable by stakers right now
    pub pending_proxy_rewards: Option<Uint128>,
    /// Total amount of 3rd party token rewards already accumulated per LP token staked
    pub accumulated_proxy_rewards_per_share: Decimal,
    /// Reward balance for the dual rewards proxy before updating accrued rewards
    pub proxy_reward_balance_before_update: Uint128,
    /// The amount of orphan proxy rewards which are left behind by emergency withdrawals and not yet transferred out
    pub orphan_proxy_rewards: Uint128,
    /// Total amount of lp tokens staked in the pool's generator
    pub lp_supply: Uint128,
}

/// This structure holds the response returned when querying the contract for general parameters
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// the Factory address
    pub factory: Addr,
    /// contract address which can only set active generators and their alloc points
    pub generator_controller: Option<Addr>,
    /// ASTRO token contract address
    pub astro_token: Addr,
    /// Total amount of ASTRO distributed per block
    pub tokens_per_block: Uint128,
    /// Sum of total allocation points across all active generators
    pub total_alloc_point: Uint64,
    /// Start block for ASTRO incentives
    pub start_block: Uint64,
    /// List of 3rd party reward proxies allowed to interact with the Generator contract
    pub allowed_reward_proxies: Vec<Addr>,
    /// The ASTRO vesting contract address
    pub vesting_contract: Addr,
    /// The list of active pools with allocation points
    pub active_pools: Vec<(Addr, Uint64)>,
    /// The blocked list of tokens
    pub blocked_list_tokens: Vec<AssetInfo>,
    /// The guardian address
    pub guardian: Option<Addr>,
}

/// This structure describes a migration message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
    pub params: Binary,
}

/// This structure describes custom hooks for the CW20.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Deposit performs a token deposit on behalf of the message sender.
    Deposit {},
    /// DepositFor performs a token deposit on behalf of another address that's not the message sender.
    DepositFor(Addr),
}

/// This structure holds the parameters used to return information about a staked in
/// a specific generator.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug, Default)]
pub struct StakerResponse {
    // The staker's address
    pub account: String,
    // The amount that the staker currently has in the generator
    pub amount: Uint128,
}
