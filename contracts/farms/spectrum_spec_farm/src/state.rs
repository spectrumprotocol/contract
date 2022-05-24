use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Decimal, Order, StdResult, Storage, Uint128};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read, Bucket, ReadonlyBucket, Singleton,
};

static KEY_CONFIG: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub spectrum_token: CanonicalAddr,
    pub spectrum_gov: CanonicalAddr,
}

pub fn config_store(storage: &mut dyn Storage) -> Singleton<Config> {
    singleton(storage, KEY_CONFIG)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}

static KEY_STATE: &[u8] = b"state";

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
pub struct State {
    pub contract_addr: CanonicalAddr,
    pub previous_spec_share: Uint128,
    pub spec_share_index: Decimal, // per weight
    pub total_weight: u32,
}

pub fn state_store(storage: &mut dyn Storage) -> Singleton<State> {
    singleton(storage, KEY_STATE)
}

pub fn read_state(storage: &dyn Storage) -> StdResult<State> {
    singleton_read(storage, KEY_STATE).load()
}

static PREFIX_POOL_INFO: &[u8] = b"pool_info";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    pub staking_token: CanonicalAddr,
    pub total_bond_amount: Uint128,
    pub weight: u32,
    pub state_spec_share_index: Decimal,
    pub spec_share_index: Decimal, // per bond amount
}

pub fn pool_info_store(storage: &mut dyn Storage) -> Bucket<PoolInfo> {
    bucket(storage, PREFIX_POOL_INFO)
}

pub fn pool_info_read(storage: &dyn Storage) -> ReadonlyBucket<PoolInfo> {
    bucket_read(storage, PREFIX_POOL_INFO)
}

static PREFIX_REWARD: &[u8] = b"reward";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfo {
    pub spec_share_index: Decimal,
    pub bond_amount: Uint128,
    pub spec_share: Uint128,
}

fn encode_length(namespace: &[u8]) -> [u8; 2] {
    if namespace.len() > 0xFFFF {
        panic!("only supports namespaces up to length 0xFFFF")
    }
    let length_bytes = (namespace.len() as u32).to_be_bytes();
    [length_bytes[2], length_bytes[3]]
}

fn calc_range_start_addr(start_after: Option<CanonicalAddr>) -> Option<Vec<u8>> {
    start_after.map(|addr| {
        let slice = addr.as_slice();
        let mut out = Vec::with_capacity(slice.len() + 6);
        out.extend_from_slice(&encode_length(slice));
        out.extend_from_slice(slice);
        out.push(255);
        out.push(255);
        out.push(255);
        out.push(255);
        out
    })
}

pub fn query_rewards(
    storage: &dyn Storage,
    start_after: Option<CanonicalAddr>,
    limit: Option<u32>,
) -> StdResult<Vec<(CanonicalAddr, CanonicalAddr, RewardInfo)>> {
    let limit = limit.unwrap_or(32) as usize;
    let start = calc_range_start_addr(start_after);
    bucket_read::<RewardInfo>(storage, PREFIX_REWARD)
        .range(start.as_deref(), None, Order::Ascending)
        .take(limit)
        .map(|it| {
            let (key, item) = it?;
            let (left, right) = key.split_at(2);
            let mut len_bytes: [u8; 2] = Default::default();
            len_bytes.copy_from_slice(left);
            let len = u16::from_be_bytes(len_bytes) as usize;
            let (addr1, addr2) = right.split_at(len);
            Ok((CanonicalAddr::from(addr1), CanonicalAddr::from(addr2), item))
        })
        .collect()
}

pub fn rewards_store<'a>(
    storage: &'a mut dyn Storage,
    owner: &CanonicalAddr,
) -> Bucket<'a, RewardInfo> {
    Bucket::multilevel(storage, &[PREFIX_REWARD, owner.as_slice()])
}

pub fn rewards_read<'a>(
    storage: &'a dyn Storage,
    owner: &CanonicalAddr,
) -> ReadonlyBucket<'a, RewardInfo> {
    ReadonlyBucket::multilevel(storage, &[PREFIX_REWARD, owner.as_slice()])
}
