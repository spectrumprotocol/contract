#![allow(non_camel_case_types)]
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{from_binary, from_slice, to_binary, Coin, ContractResult, Decimal, OwnedDeps, Querier, QuerierResult, QueryRequest, SystemError, SystemResult, Uint128, WasmQuery, BankQuery, BalanceResponse};
use std::collections::HashMap;
use terra_cosmwasm::{TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{PoolResponse, SimulationResponse};

use spectrum_protocol::gov::BalanceResponse as SpecBalanceResponse;
use crate::contract::PairInfo;

/// mock_dependencies is a drop-in replacement for cosmwasm_std::testing::mock_dependencies
/// this uses our CustomQuerier.
pub fn mock_dependencies(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let contract_addr = MOCK_CONTRACT_ADDR.to_string();
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(MockQuerier::new(&[(&contract_addr, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

pub struct WasmMockQuerier {
    base: MockQuerier<TerraQueryWrapper>,
    token_querier: TokenQuerier,
    tax_querier: TaxQuerier,
    terraswap_factory_querier: TerraswapFactoryQuerier,
    terraswap_pair_querier: TerraswapPairQuerier,
}

#[derive(Clone, Default)]
pub struct TokenQuerier {
    // this lets us iterate over all pairs that match the first string
    balances: HashMap<String, HashMap<String, Uint128>>,
    balance_percent: u128,
}

impl TokenQuerier {
    #![allow(dead_code)]
    pub fn new(balances: &[(&String, &[(&String, &Uint128)])], balance_percent: u128) -> Self {
        TokenQuerier {
            balances: balances_to_map(balances),
            balance_percent,
        }
    }
}

pub(crate) fn balances_to_map(
    balances: &[(&String, &[(&String, &Uint128)])],
) -> HashMap<String, HashMap<String, Uint128>> {
    let mut balances_map: HashMap<String, HashMap<String, Uint128>> = HashMap::new();
    for (contract_addr, balances) in balances.iter() {
        let mut contract_balances_map: HashMap<String, Uint128> = HashMap::new();
        for (addr, balance) in balances.iter() {
            contract_balances_map.insert(addr.to_string(), **balance);
        }

        balances_map.insert(contract_addr.to_string(), contract_balances_map);
    }
    balances_map
}

#[derive(Clone, Default)]
pub struct TaxQuerier {
    rate: Decimal,
    // this lets us iterate over all pairs that match the first string
    caps: HashMap<String, Uint128>,
}

impl TaxQuerier {
    #![allow(dead_code)]
    pub fn new(rate: Decimal, caps: &[(&String, &Uint128)]) -> Self {
        TaxQuerier {
            rate,
            caps: caps_to_map(caps),
        }
    }
}

pub(crate) fn caps_to_map(caps: &[(&String, &Uint128)]) -> HashMap<String, Uint128> {
    let mut owner_map: HashMap<String, Uint128> = HashMap::new();
    for (denom, cap) in caps.iter() {
        owner_map.insert(denom.to_string(), **cap);
    }
    owner_map
}

#[derive(Clone, Default)]
pub struct TerraswapFactoryQuerier {
    pairs: HashMap<String, PairInfo>,
}

#[derive(Clone, Default)]
pub struct TerraswapPairQuerier {
    pools: HashMap<String, PoolResponse>,
}

impl TerraswapFactoryQuerier {
    pub fn new(pairs: &[(&String, &PairInfo)]) -> Self {
        TerraswapFactoryQuerier {
            pairs: pairs_to_map(pairs),
        }
    }
}

impl TerraswapPairQuerier {
    pub fn new(pools: &[(&String, &PoolResponse)]) -> Self {
        TerraswapPairQuerier {
            pools: pools_to_map(pools),
        }
    }
}

pub(crate) fn pairs_to_map(pairs: &[(&String, &PairInfo)]) -> HashMap<String, PairInfo> {
    let mut pairs_map: HashMap<String, PairInfo> = HashMap::new();
    for (key, pair) in pairs.iter() {
        pairs_map.insert(key.to_string(), (*pair).clone());
    }
    pairs_map
}

pub(crate) fn pools_to_map(pools: &[(&String, &PoolResponse)]) -> HashMap<String, PoolResponse> {
    let mut pools_map: HashMap<String, PoolResponse> = HashMap::new();
    for (key, pool) in pools.iter() {
        pools_map.insert(key.to_string(), (*pool).clone());
    }
    pools_map
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<TerraQueryWrapper> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.execute_query(&request)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum MockQueryMsg {
    balance {
        address: String,
    },
    Pair {
        asset_infos: [AssetInfo; 2],
    },
    Simulation {
        offer_asset: Asset,
    },
    Pool {},
}

impl WasmMockQuerier {
    pub fn execute_query(&self, request: &QueryRequest<TerraQueryWrapper>) -> QuerierResult {
        match &request {
            QueryRequest::Bank(BankQuery::Balance { address, denom }) => {
                let amount = self.read_token_balance(&denom.to_string(), address.clone());
                SystemResult::Ok(ContractResult::from(to_binary(&BalanceResponse {
                    amount: Coin { denom: denom.clone(), amount }
                })))
            },
            QueryRequest::Custom(TerraQueryWrapper { route, query_data }) => {
                if &TerraRoute::Treasury == route {
                    match query_data {
                        TerraQuery::TaxRate {} => {
                            let res = TaxRateResponse {
                                rate: self.tax_querier.rate,
                            };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        TerraQuery::TaxCap { denom } => {
                            let cap = self
                                .tax_querier
                                .caps
                                .get(denom)
                                .copied()
                                .unwrap_or_default();
                            let res = TaxCapResponse { cap };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                } else {
                    panic!("DO NOT ENTER HERE")
                }
            }
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                match from_binary(msg).unwrap() {
                    MockQueryMsg::balance { address } => {
                        let balance = self.read_token_balance(contract_addr, address);
                        SystemResult::Ok(ContractResult::from(to_binary(&SpecBalanceResponse {
                            balance,
                            share: balance
                                .multiply_ratio(100u128, self.token_querier.balance_percent),
                            locked_balance: vec![],
                            pools: vec![],
                        })))
                    },
                    MockQueryMsg::Pair { asset_infos } => {
                        let key = asset_infos[0].to_string() + asset_infos[1].to_string().as_str();
                        match self.terraswap_factory_querier.pairs.get(&key) {
                            Some(v) => SystemResult::Ok(ContractResult::from(to_binary(&v))),
                            None => {
                                let key = asset_infos[1].to_string() + asset_infos[0].to_string().as_str();
                                match self.terraswap_factory_querier.pairs.get(&key) {
                                    Some(v) => SystemResult::Ok(ContractResult::from(to_binary(&v))),
                                    None => SystemResult::Err(SystemError::InvalidRequest {
                                        error: "No pair info exists".to_string(),
                                        request: msg.as_slice().into(),
                                    }),
                                }
                            },
                        }
                    },
                    MockQueryMsg::Simulation { offer_asset } => {
                        let commission_amount = offer_asset.amount.multiply_ratio(3u128, 1000u128);
                        let return_amount = offer_asset.amount.checked_sub(commission_amount);
                        match return_amount {
                            Ok(amount) => SystemResult::Ok(ContractResult::from(to_binary(
                                &SimulationResponse {
                                    return_amount: amount,
                                    commission_amount,
                                    spread_amount: Uint128::from(100u128),
                                },
                            ))),
                            Err(_e) => SystemResult::Err(SystemError::Unknown {}),
                        }
                    },
                    MockQueryMsg::Pool {} => {
                        match self.terraswap_pair_querier.pools.get(contract_addr) {
                            Some(v) => SystemResult::Ok(ContractResult::from(to_binary(&v))),
                            None => SystemResult::Err(SystemError::InvalidRequest {
                                error: "No pool info exists".to_string(),
                                request: msg.as_slice().into(),
                            }),
                        }
                    },
                }
            }
            _ => self.base.handle_query(request),
        }
    }
}

impl WasmMockQuerier {
    #![allow(dead_code)]
    pub fn new(base: MockQuerier<TerraQueryWrapper>) -> Self {
        WasmMockQuerier {
            base,
            token_querier: TokenQuerier::default(),
            tax_querier: TaxQuerier::default(),
            terraswap_factory_querier: TerraswapFactoryQuerier::default(),
            terraswap_pair_querier: TerraswapPairQuerier::default(),
        }
    }

    pub fn with_balance_percent(&mut self, balance_percent: u128) {
        self.token_querier.balance_percent = balance_percent;
    }

    // configure the mint whitelist mock querier
    pub fn with_token_balances(&mut self, balances: &[(&String, &[(&String, &Uint128)])]) {
        self.token_querier = TokenQuerier::new(balances, self.token_querier.balance_percent);
    }

    pub fn read_token_balance(&self, contract_addr: &str, address: String) -> Uint128 {
        let balances: &HashMap<String, Uint128> =
            match self.token_querier.balances.get(contract_addr) {
                Some(balances) => balances,
                None => return Uint128::zero(),
            };

        match balances.get(&address) {
            Some(v) => *v,
            None => Uint128::zero(),
        }
    }

    // configure the token owner mock querier
    pub fn with_tax(&mut self, rate: Decimal, caps: &[(&String, &Uint128)]) {
        self.tax_querier = TaxQuerier::new(rate, caps);
    }

    // configure the terraswap factory
    pub fn with_terraswap_factory(&mut self, pairs: &[(&String, &PairInfo)]) {
        self.terraswap_factory_querier = TerraswapFactoryQuerier::new(pairs);
    }

    // configure the terraswap pair
    pub fn with_terraswap_pairs(&mut self, pools: &[(&String, &PoolResponse)]) {
        self.terraswap_pair_querier = TerraswapPairQuerier::new(pools);
    }
}
