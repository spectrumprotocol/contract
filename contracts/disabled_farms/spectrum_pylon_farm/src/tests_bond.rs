use crate::bond::deposit_farm_share;
use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::{pool_info_read, pool_info_store, read_config, read_state, state_store};
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{from_binary, to_binary, CosmosMsg, Decimal, OwnedDeps, Uint128, WasmMsg};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use pylon_token::staking::ExecuteMsg as PylonStakingExecuteMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum_protocol::gov::ExecuteMsg as GovExecuteMsg;
use spectrum_protocol::pylon_farm::{
    ConfigInfo, Cw20HookMsg, ExecuteMsg, PoolItem, PoolsResponse, QueryMsg, StateInfo,
};
use std::fmt::Debug;

const SPEC_GOV: &str = "SPEC_GOV";
const SPEC_TOKEN: &str = "spec_token";
const MINE_GOV: &str = "mine_gov";
const MINE_TOKEN: &str = "mine_token";
const MINE_STAKING: &str = "mine_staking";
const TEST_CREATOR: &str = "creator";
const USER1: &str = "user1";
const USER2: &str = "user2";
const MINE_LP: &str = "mine_lp";
const SPY_TOKEN: &str = "spy_token";
const SPY_LP: &str = "spy_lp";
const ANC_MARKET: &str = "anc_market";
const AUST_TOKEN: &str = "aust_token";
const MINE_POOL: &str = "mine_pool";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    pub staker_addr: String,
    pub reward_infos: Vec<RewardInfoResponseItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    pub asset_token: String,
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
}

#[test]
fn test() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance_percent(100);

    let _ = test_config(&mut deps);
    test_register_asset(&mut deps);
    test_bond(&mut deps);
    // test_deposit_fee(&mut deps);
    // test_staked_reward(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test init & read config & read state
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: TEST_CREATOR.to_string(),
        spectrum_gov: SPEC_GOV.to_string(),
        spectrum_token: SPEC_TOKEN.to_string(),
        pylon_gov: MINE_GOV.to_string(),
        pylon_token: MINE_TOKEN.to_string(),
        pylon_staking: MINE_STAKING.to_string(),
        platform: TEST_CREATOR.to_string(),
        controller: TEST_CREATOR.to_string(),
        base_denom: "uusd".to_string(),
        community_fee: Decimal::zero(),
        platform_fee: Decimal::zero(),
        controller_fee: Decimal::zero(),
        deposit_fee: Decimal::zero(),
        anchor_market: ANC_MARKET.to_string(),
        aust_token: AUST_TOKEN.to_string(),
        pair_contract: MINE_POOL.to_string(),
    };

    // success init
    let res = instantiate(deps.as_mut(), env.clone(), info, config.clone());
    assert!(res.is_ok());

    // read config
    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, config);

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            previous_spec_share: Uint128::zero(),
            total_farm_share: Uint128::zero(),
            total_weight: 0u32,
            spec_share_index: Decimal::zero(),
            earning: Uint128::zero(),
            total_farm_amount: Uint128::zero(),
        }
    );

    // alter config, validate owner
    let info = mock_info(SPEC_GOV, &[]);
    let msg = ExecuteMsg::update_config {
        owner: Some(SPEC_GOV.to_string()),
        controller: None,
        community_fee: None,
        platform_fee: None,
        controller_fee: None,
        deposit_fee: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // success
    let info = mock_info(TEST_CREATOR, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    config.owner = SPEC_GOV.to_string();
    assert_eq!(res, config);

    config
}

fn test_register_asset(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // no permission
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::register_asset {
        asset_token: MINE_TOKEN.to_string(),
        staking_token: MINE_LP.to_string(),
        weight: 1u32,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // success
    let info = mock_info(SPEC_GOV, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // query pool info
    let msg = QueryMsg::pools {};
    let res: PoolsResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        PoolsResponse {
            pools: vec![PoolItem {
                asset_token: MINE_TOKEN.to_string(),
                staking_token: MINE_LP.to_string(),
                weight: 1u32,
                farm_share: Uint128::zero(),
                state_spec_share_index: Decimal::zero(),
                stake_spec_share_index: Decimal::zero(),
                auto_spec_share_index: Decimal::zero(),
                farm_share_index: Decimal::zero(),
                total_stake_bond_amount: Uint128::zero(),
                total_stake_bond_share: Uint128::zero(),
                total_auto_bond_share: Uint128::zero(),
            }]
        }
    );

    // register again should fail
    let msg = ExecuteMsg::register_asset {
        asset_token: SPY_TOKEN.to_string(),
        staking_token: SPY_LP.to_string(),
        weight: 1u32,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(res.total_weight, 1u32);
}

fn test_bond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // bond err
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(10000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: MINE_TOKEN.to_string(),
            compound_rate: Some(Decimal::percent(60)),
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // bond success user1 1000 MINE-LP
    let info = mock_info(MINE_LP, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let deps_ref = deps.as_ref();
    let config = read_config(deps_ref.storage).unwrap();
    let mut state = read_state(deps_ref.storage).unwrap();
    let mut pool_info = pool_info_read(deps_ref.storage)
        .load(config.pylon_token.as_slice())
        .unwrap();
    deposit_farm_share(
        &mut state,
        &mut pool_info,
        Uint128::from(500u128),
    )
    .unwrap();
    state_store(deps.as_mut().storage).save(&state).unwrap();
    pool_info_store(deps.as_mut().storage)
        .save(config.pylon_token.as_slice(), &pool_info)
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &MINE_STAKING.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(10000u128))],
        ),
        (
            &MINE_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
        (
            &MINE_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(500u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(2700u128))],
        ),
    ]);

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: MINE_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(500u128),    // pending farm reward shouldn't increase
            pending_spec_reward: Uint128::from(2700u128),
            bond_amount: Uint128::from(10000u128),
            auto_bond_amount: Uint128::from(6000u128),
            stake_bond_amount: Uint128::from(4000u128),
            farm_share_index: Decimal::zero(),
            auto_spec_share_index: Decimal::zero(),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::from(500u128),
            spec_share: Uint128::from(2700u128),
            auto_bond_share: Uint128::from(6000u128),
            stake_bond_share: Uint128::from(4000u128),
        },]
    );

    // unbond 3000 MINE-LP
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::unbond {
        asset_token: MINE_TOKEN.to_string(),
        amount: Uint128::from(3000u128),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap()
            .messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_STAKING.to_string(),
                funds: vec![],
                msg: to_binary(&PylonStakingExecuteMsg::Unbond {
                    amount: Uint128::from(3000u128),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_LP.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(3000u128),
                })
                .unwrap(),
            }),
        ]
    );

    // withdraw rewards
    let msg = ExecuteMsg::withdraw { asset_token: None, spec_amount: None, farm_amount: None };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap()
            .messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_GOV.to_string(),
                funds: vec![],
                msg: to_binary(&GovExecuteMsg::withdraw {
                    amount: Some(Uint128::from(2700u128)),
                    days: None,
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(2700u128),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(500u128),
                })
                .unwrap(),
            }),
        ]
    );

    deps.querier.with_token_balances(&[
        (
            &MINE_STAKING.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(7000u128))],
        ),
        (
            &MINE_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(0u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(0u128))],
        ),
    ]);

    // query balance for user2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.reward_infos, vec![]);

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: MINE_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(0u128),
            pending_spec_reward: Uint128::from(0u128),
            bond_amount: Uint128::from(7000u128),
            auto_bond_amount: Uint128::from(4200u128),
            stake_bond_amount: Uint128::from(2800u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(0u128),
            spec_share: Uint128::from(0u128),
            auto_bond_share: Uint128::from(4200u128),
            stake_bond_share: Uint128::from(2800u128),
        },]
    );

    // bond user2 5000 MINE-LP auto-stake
    let info = mock_info(MINE_LP, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER2.to_string(),
        amount: Uint128::from(5000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: MINE_TOKEN.to_string(),
            compound_rate: None,
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let deps_ref = deps.as_ref();
    let mut state = read_state(deps_ref.storage).unwrap();
    let mut pool_info = pool_info_read(deps_ref.storage)
        .load(config.pylon_token.as_slice())
        .unwrap();
    deposit_farm_share(
        &mut state,
        &mut pool_info,
        Uint128::from(10000u128),
    )
    .unwrap();
    state_store(deps.as_mut().storage).save(&state).unwrap();
    pool_info_store(deps.as_mut().storage)
        .save(config.pylon_token.as_slice(), &pool_info)
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &MINE_STAKING.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12000u128))],
        ),
        (
            &MINE_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(5000u128))],
        ),
        (
            &MINE_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(10000u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
    ]);

    /*
        USER1 7000 (auto 4200, stake 2800)
        USER2 5000 (auto 0, stake 5000)
        Total lp 12000
        Total farm share 7800
        Farm share +10000
        USER1 Farm share = 28/78 * 10000 = 3589
        USER2 Farm share = 50/78 * 10000 = 6410
        Farm reward 5000
        USER1 Farm reward = 28/78 * 5000 = 1794
        USER2 Farm reward = 50/78 * 5000 = 3205
        SPEC reward +1000
        USER1 SPEC reward ~ 582
        USER2 SPEC reward ~ 416
    */

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: MINE_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(3589u128),
            pending_spec_reward: Uint128::from(582u128),
            bond_amount: Uint128::from(7000u128),
            auto_bond_amount: Uint128::from(4200u128),
            stake_bond_amount: Uint128::from(2800u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(3589u128),
            spec_share: Uint128::from(582u128),
            auto_bond_share: Uint128::from(4200u128),
            stake_bond_share: Uint128::from(2800u128),
        },]
    );

    // query balance for user2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: MINE_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(6410u128),
            pending_spec_reward: Uint128::from(416u128),
            bond_amount: Uint128::from(5000u128),
            auto_bond_amount: Uint128::from(0u128),
            stake_bond_amount: Uint128::from(5000u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(6410u128),
            spec_share: Uint128::from(416u128),
            auto_bond_share: Uint128::from(0u128),
            stake_bond_share: Uint128::from(5000u128),
        },]
    );
}

// fn test_staked_reward(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
//     // unbond user1
//     let info = mock_info(USER1, &[]);
//     let msg = ExecuteMsg::unbond {
//         asset_token: MIR_TOKEN.to_string(),
//         amount: Uint128::from(13200u128),
//     };
//     let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
//     assert!(res.is_ok());
//     assert_eq!(
//         res.unwrap().messages,
//         [
//             CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: MIR_STAKING.to_string(),
//                 funds: vec![],
//                 msg: to_binary(&MirrorStakingExecuteMsg::unbond {
//                     amount: Uint128::from(13200u128),
//                     asset_token: MIR_TOKEN.to_string(),
//                 })
//                 .unwrap(),
//             }),
//             CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: MIR_LP.to_string(),
//                 funds: vec![],
//                 msg: to_binary(&Cw20ExecuteMsg::Transfer {
//                     recipient: USER1.to_string(),
//                     amount: Uint128::from(13200u128),
//                 })
//                 .unwrap(),
//             }),
//         ]
//     );

//     // withdraw for user2
//     let info = mock_info(USER2, &[]);
//     let msg = ExecuteMsg::withdraw { asset_token: None };
//     let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
//     assert!(res.is_ok());
//     assert_eq!(
//         res.unwrap().messages,
//         vec![
//             CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: SPEC_GOV.to_string(),
//                 funds: vec![],
//                 msg: to_binary(&GovExecuteMsg::withdraw {
//                     amount: Some(Uint128::from(4700u128)),
//                 })
//                 .unwrap(),
//             }),
//             CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: SPEC_TOKEN.to_string(),
//                 funds: vec![],
//                 msg: to_binary(&Cw20ExecuteMsg::Transfer {
//                     recipient: USER2.to_string(),
//                     amount: Uint128::from(4700u128),
//                 })
//                 .unwrap(),
//             }),
//             CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: MIR_GOV.to_string(),
//                 funds: vec![],
//                 msg: to_binary(&MirrorGovExecuteMsg::WithdrawVotingTokens {
//                     amount: Some(Uint128::from(7300u128)),
//                 })
//                 .unwrap(),
//             }),
//             CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: MIR_TOKEN.to_string(),
//                 funds: vec![],
//                 msg: to_binary(&Cw20ExecuteMsg::Transfer {
//                     recipient: USER2.to_string(),
//                     amount: Uint128::from(7300u128),
//                 })
//                 .unwrap(),
//             }),
//         ]
//     );
//     deps.querier.with_balance_percent(120);
//     deps.querier.with_token_balances(&[
//         (
//             &MIR_STAKING.to_string(),
//             &[
//                 (&MIR_TOKEN.to_string(), &Uint128::from(90000u128)),
//                 (&sPY_TOKEN.to_string(), &Uint128::from(72000u128)),
//             ],
//         ),
//         (
//             &MIR_GOV.to_string(),
//             &[(
//                 &MOCK_CONTRACT_ADDR.to_string(),
//                 &Uint128::from(9200u128),
//             )],
//         ),
//         (
//             &SPEC_GOV.to_string(),
//             &[(
//                 &MOCK_CONTRACT_ADDR.to_string(),
//                 &Uint128::from(24600u128), //+9000 +20%
//             )],
//         ),
//     ]);

//     // query balance1 (still earn gov income even there is no bond)
//     let msg = QueryMsg::reward_info {
//         staker_addr: USER1.to_string(),
//         asset_token: None,
//         height: 0u64,
//     };
//     let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
//     assert_eq!(
//         res.reward_infos,
//         vec![
//             RewardInfoResponseItem {
//                 asset_token: MIR_TOKEN.to_string(),
//                 pending_farm_reward: Uint128::from(3330u128), //+33%
//                 pending_spec_reward: Uint128::from(4678u128), //+20%
//                 bond_amount: Uint128::from(0u128),
//                 auto_bond_amount: Uint128::from(0u128),
//                 stake_bond_amount: Uint128::from(0u128),
//                 accum_spec_share: Uint128::from(4799u128),
//             },
//             RewardInfoResponseItem {
//                 asset_token: SPY_TOKEN.to_string(),
//                 pending_farm_reward: Uint128::from(5866u128), //+33%
//                 pending_spec_reward: Uint128::from(10080u128), //+800+20%
//                 bond_amount: Uint128::from(9600u128),
//                 auto_bond_amount: Uint128::from(7200u128),
//                 stake_bond_amount: Uint128::from(2400u128),
//                 accum_spec_share: Uint128::from(10200u128),
//             },
//         ]
//     );

//     // query balance2
//     let msg = QueryMsg::reward_info {
//         staker_addr: USER2.to_string(),
//         asset_token: None,
//         height: 0u64,
//     };
//     let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
//     assert_eq!(
//         res.reward_infos,
//         vec![
//             RewardInfoResponseItem {
//                 asset_token: MIR_TOKEN.to_string(),
//                 pending_farm_reward: Uint128::from(0u128),
//                 pending_spec_reward: Uint128::from(240u128), //+200+20%
//                 bond_amount: Uint128::from(6000u128),
//                 auto_bond_amount: Uint128::from(0u128),
//                 stake_bond_amount: Uint128::from(6000u128),
//                 accum_spec_share: Uint128::from(1700u128),
//             },
//             RewardInfoResponseItem {
//                 asset_token: SPY_TOKEN.to_string(),
//                 pending_farm_reward: Uint128::from(0u128),
//                 pending_spec_reward: Uint128::from(480u128), //+400+20%
//                 bond_amount: Uint128::from(4800u128),
//                 auto_bond_amount: Uint128::from(0u128),
//                 stake_bond_amount: Uint128::from(4800u128),
//                 accum_spec_share: Uint128::from(3600u128),
//             },
//         ]
//     );

//     // query balance3
//     let msg = QueryMsg::reward_info {
//         staker_addr: USER3.to_string(),
//         asset_token: None,
//         height: 0u64,
//     };
//     let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
//     assert_eq!(
//         res.reward_infos,
//         vec![
//             RewardInfoResponseItem {
//                 asset_token: MIR_TOKEN.to_string(),
//                 pending_farm_reward: Uint128::zero(),
//                 pending_spec_reward: Uint128::from(3358u128), //+2799+20%
//                 bond_amount: Uint128::from(84000u128),
//                 auto_bond_amount: Uint128::from(45600u128),
//                 stake_bond_amount: Uint128::from(38400u128),
//                 accum_spec_share: Uint128::from(2799u128),
//             },
//             RewardInfoResponseItem {
//                 asset_token: SPY_TOKEN.to_string(),
//                 pending_farm_reward: Uint128::zero(),
//                 pending_spec_reward: Uint128::from(5760u128), //+4800+20%
//                 bond_amount: Uint128::from(57600u128),
//                 auto_bond_amount: Uint128::from(28800u128),
//                 stake_bond_amount: Uint128::from(28800u128),
//                 accum_spec_share: Uint128::from(4800u128),
//             },
//         ]
//     );
// }
