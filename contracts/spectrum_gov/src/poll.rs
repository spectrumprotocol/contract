use cosmwasm_std::{
    attr, to_binary, Api, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Uint128, WasmMsg,
};
use spectrum_protocol::common::OrderBy;
use spectrum_protocol::gov::{
    PollExecuteMsg, PollInfo, PollStatus, PollsResponse, VoteOption, VoterInfo, VotersResponse,
};

use crate::stake::{reconcile_balance, validate_minted};
use crate::state::{
    account_store, poll_indexer_store, poll_store, poll_voter_store, read_config, read_poll,
    read_poll_voter, read_poll_voters, read_polls, read_state, state_store, Poll,
};
use cw20::Cw20ExecuteMsg;
use std::ops::Mul;
use classic_bindings::TerraQuery;
use classic_terraswap::querier::query_token_balance;

/// create a new poll
#[allow(clippy::too_many_arguments)]
pub fn poll_start(
    deps: DepsMut<TerraQuery>,
    env: Env,
    proposer: String,
    deposit_amount: Uint128,
    title: String,
    description: String,
    link: Option<String>,
    execute_msgs: Vec<PollExecuteMsg>,
) -> StdResult<Response> {
    validate_title(&title)?;
    validate_description(&description)?;
    validate_link(&link)?;

    let config = read_config(deps.storage)?;
    if deposit_amount < config.proposal_deposit {
        return Err(StdError::generic_err(format!(
            "Must deposit more than {} token",
            config.proposal_deposit
        )));
    }

    let mut state = state_store(deps.storage).load()?;
    let poll_id = state.poll_count + 1;

    // Increase poll count & total deposit amount
    state.poll_count += 1;
    state.poll_deposit += deposit_amount;

    let new_poll = Poll {
        id: poll_id,
        creator: deps.api.addr_canonicalize(&proposer)?,
        status: PollStatus::in_progress,
        yes_votes: Uint128::zero(),
        no_votes: Uint128::zero(),
        end_height: env.block.height + config.voting_period,
        title,
        description,
        link,
        execute_msgs,
        deposit_amount,
        total_balance_at_end_poll: None,
    };

    poll_store(deps.storage).save(&poll_id.to_be_bytes(), &new_poll)?;
    poll_indexer_store(deps.storage, &PollStatus::in_progress)
        .save(&poll_id.to_be_bytes(), &true)?;

    state_store(deps.storage).save(&state)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "create_poll"),
        attr("creator", deps.api.addr_humanize(&new_poll.creator)?),
        attr("poll_id", poll_id.to_string()),
        attr("end_height", new_poll.end_height.to_string()),
    ]))
}

const MIN_TITLE_LENGTH: usize = 4;
const MAX_TITLE_LENGTH: usize = 64;
const MIN_DESC_LENGTH: usize = 4;
const MAX_DESC_LENGTH: usize = 256;
const MIN_LINK_LENGTH: usize = 12;
const MAX_LINK_LENGTH: usize = 128;

/// validate_title returns an error if the title is invalid
fn validate_title(title: &str) -> StdResult<()> {
    if title.len() < MIN_TITLE_LENGTH {
        Err(StdError::generic_err("Title too short"))
    } else if title.len() > MAX_TITLE_LENGTH {
        Err(StdError::generic_err("Title too long"))
    } else {
        Ok(())
    }
}

/// validate_description returns an error if the description is invalid
fn validate_description(description: &str) -> StdResult<()> {
    if description.len() < MIN_DESC_LENGTH {
        Err(StdError::generic_err("Description too short"))
    } else if description.len() > MAX_DESC_LENGTH {
        Err(StdError::generic_err("Description too long"))
    } else {
        Ok(())
    }
}

/// validate_link returns an error if the link is invalid
fn validate_link(link: &Option<String>) -> StdResult<()> {
    if let Some(link) = link {
        if link.len() < MIN_LINK_LENGTH {
            Err(StdError::generic_err("Link too short"))
        } else if link.len() > MAX_LINK_LENGTH {
            Err(StdError::generic_err("Link too long"))
        } else {
            Ok(())
        }
    } else {
        Ok(())
    }
}

pub fn poll_vote(
    deps: DepsMut<TerraQuery>,
    env: Env,
    info: MessageInfo,
    poll_id: u64,
    vote: VoteOption,
    amount: Uint128,
) -> StdResult<Response> {
    let sender_address_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;
    if poll_id == 0 || state.poll_count < poll_id {
        return Err(StdError::generic_err("Poll does not exist"));
    }

    let mut a_poll = poll_store(deps.storage).load(&poll_id.to_be_bytes())?;
    if a_poll.status != PollStatus::in_progress || env.block.height > a_poll.end_height {
        return Err(StdError::generic_err("Poll is not in progress"));
    }

    // Check the voter already has a vote on the poll
    if read_poll_voter(deps.storage, poll_id, &sender_address_raw).is_ok() {
        return Err(StdError::generic_err("User has already voted."));
    }

    // reconcile
    reconcile_balance(&deps.as_ref(), &mut state, &config, Uint128::zero())?;

    let key = sender_address_raw.as_slice();
    let mut account = account_store(deps.storage).load(key)?;

    // convert share to amount
    if account.calc_total_balance(&state)? < amount {
        return Err(StdError::generic_err(
            "User does not have enough staked tokens.",
        ));
    }

    // update tally info
    if VoteOption::yes == vote {
        a_poll.yes_votes += amount;
    } else {
        a_poll.no_votes += amount;
    }

    let vote_info = VoterInfo {
        vote,
        balance: amount,
    };
    account.locked_balance.push((poll_id, vote_info.clone()));
    account_store(deps.storage).save(key, &account)?;

    // store poll voter && and update poll data
    poll_voter_store(deps.storage, poll_id).save(sender_address_raw.as_slice(), &vote_info)?;
    poll_store(deps.storage).save(&poll_id.to_be_bytes(), &a_poll)?;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "cast_vote"),
        attr("poll_id", poll_id.to_string()),
        attr("amount", amount),
        attr("voter", info.sender),
        attr("vote_option", vote_info.vote.to_string()),
    ]))
}

/*
 * Ends a poll.
 */
pub fn poll_end(deps: DepsMut<TerraQuery>, env: Env, poll_id: u64) -> StdResult<Response> {
    let mut a_poll = poll_store(deps.storage).load(&poll_id.to_be_bytes())?;

    if a_poll.status != PollStatus::in_progress {
        return Err(StdError::generic_err("Poll is not in progress"));
    }

    let no = a_poll.no_votes.u128();
    let yes = a_poll.yes_votes.u128();

    let all_votes = yes + no;

    let mut messages: Vec<CosmosMsg> = vec![];
    let config = read_config(deps.storage)?;
    let mut state = state_store(deps.storage).load()?;

    let staked = query_token_balance(
        &deps.querier,
        deps.api.addr_humanize(&config.spec_token)?,
        deps.api.addr_humanize(&state.contract_addr)?,
    )?
        .checked_sub(state.poll_deposit)?
        .checked_sub(state.vault_balances)?;

    let quorum = if staked.is_zero() {
        Decimal::zero()
    } else {
        Decimal::from_ratio(all_votes, staked)
    };

    if a_poll.end_height > env.block.height
        && !staked.is_zero()
        && Decimal::from_ratio(yes, staked) < config.threshold
        && Decimal::from_ratio(no, staked) < config.threshold
    {
        return Err(StdError::generic_err("Voting period has not expired"));
    }

    let (passed, rejected_reason) = if quorum.is_zero() || quorum < config.quorum {
        // Quorum: More than quorum of the total staked tokens at the end of the voting
        // period need to have participated in the vote.
        (false, "Quorum not reached")
    } else if Decimal::from_ratio(yes, all_votes) < config.threshold {
        (false, "Threshold not reached")
    } else {
        //Threshold: More than 50% of the tokens that participated in the vote
        // (after excluding “Abstain” votes) need to have voted in favor of the proposal (“Yes”).
        (true, "")
    };

    if !a_poll.deposit_amount.is_zero() {
        // mint must be calculated before distribute poll deposit
        if !passed {
            validate_minted(&state, &config, env.block.height)?;
        }
        let return_amount = if passed || a_poll.execute_msgs.is_empty() {
            a_poll.deposit_amount
        } else if quorum.is_zero() {
            Uint128::zero()
        } else if quorum < config.quorum {
            a_poll
                .deposit_amount
                .multiply_ratio(yes, staked.mul(config.quorum))
        } else {
            a_poll.deposit_amount.multiply_ratio(yes, all_votes)
        };
        if !return_amount.is_zero() {
            // refunds deposit only when pass
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.addr_humanize(&config.spec_token)?.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: deps.api.addr_humanize(&a_poll.creator)?.to_string(),
                    amount: return_amount,
                })?,
            }))
        }
    }

    // Decrease total deposit amount
    state.poll_deposit = state.poll_deposit.checked_sub(a_poll.deposit_amount)?;
    state_store(deps.storage).save(&state)?;

    // Update poll status
    a_poll.status = if passed {
        PollStatus::passed
    } else {
        PollStatus::rejected
    };
    a_poll.total_balance_at_end_poll = Some(staked);
    if env.block.height < a_poll.end_height {
        a_poll.end_height = env.block.height;
    }
    poll_store(deps.storage).save(&poll_id.to_be_bytes(), &a_poll)?;

    // Update poll indexer
    poll_indexer_store(deps.storage, &PollStatus::in_progress).remove(&a_poll.id.to_be_bytes());
    poll_indexer_store(deps.storage, &a_poll.status).save(&a_poll.id.to_be_bytes(), &true)?;

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "end_poll"),
        attr("poll_id", poll_id.to_string()),
        attr("rejected_reason", rejected_reason),
        attr("passed", passed.to_string()),
    ]))
}

/*
 * Execute a msg of passed poll.
 */
pub fn poll_execute(deps: DepsMut<TerraQuery>, env: Env, poll_id: u64) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let mut a_poll = poll_store(deps.storage).load(&poll_id.to_be_bytes())?;

    if a_poll.status != PollStatus::passed {
        return Err(StdError::generic_err("Poll is not in passed status"));
    }

    if a_poll.end_height + config.effective_delay > env.block.height {
        return Err(StdError::generic_err("Effective delay has not expired"));
    }

    if a_poll.execute_msgs.is_empty() {
        return Err(StdError::generic_err("The poll does not have execute_data"));
    }

    poll_indexer_store(deps.storage, &PollStatus::passed).remove(&poll_id.to_be_bytes());
    poll_indexer_store(deps.storage, &PollStatus::executed).save(&poll_id.to_be_bytes(), &true)?;

    a_poll.status = PollStatus::executed;
    poll_store(deps.storage).save(&poll_id.to_be_bytes(), &a_poll)?;
    let messages: Vec<CosmosMsg> = a_poll.execute_msgs.into_iter().map(match_msg).collect();
    Ok(Box::new(Response::new())
        .add_messages(messages)
        .add_attributes(vec![
            attr("action", "execute_poll"),
            attr("poll_id", poll_id.to_string()),
        ]))
}

fn match_msg(msg: PollExecuteMsg) -> CosmosMsg {
    match msg {
        PollExecuteMsg::execute { contract, msg } => CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract,
            msg: Binary(msg.into_bytes()),
            funds: vec![],
        }),
    }
}

/// ExpirePoll is used to make the poll as expired state for querying purpose
pub fn poll_expire(deps: DepsMut<TerraQuery>, env: Env, poll_id: u64) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let mut a_poll = poll_store(deps.storage).load(&poll_id.to_be_bytes())?;

    if a_poll.status != PollStatus::passed {
        return Err(StdError::generic_err("Poll is not in passed status"));
    }

    if a_poll.execute_msgs.is_empty() {
        return Err(StdError::generic_err(
            "Cannot make a text proposal to expired state",
        ));
    }

    if a_poll.end_height + config.expiration_period > env.block.height {
        return Err(StdError::generic_err("Expire height has not been reached"));
    }

    poll_indexer_store(deps.storage, &PollStatus::passed).remove(&poll_id.to_be_bytes());
    poll_indexer_store(deps.storage, &PollStatus::expired).save(&poll_id.to_be_bytes(), &true)?;

    a_poll.status = PollStatus::expired;
    poll_store(deps.storage).save(&poll_id.to_be_bytes(), &a_poll)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "expire_poll"),
        attr("poll_id", poll_id.to_string()),
    ]))
}

fn map_poll(poll: Poll, api: &dyn Api) -> StdResult<PollInfo> {
    Ok(PollInfo {
        id: poll.id,
        creator: api.addr_humanize(&poll.creator).unwrap().to_string(),
        status: poll.status.clone(),
        end_height: poll.end_height,
        title: poll.title,
        description: poll.description,
        link: poll.link,
        deposit_amount: poll.deposit_amount,
        execute_msgs: poll.execute_msgs,
        yes_votes: poll.yes_votes,
        no_votes: poll.no_votes,
        total_balance_at_end_poll: poll.total_balance_at_end_poll,
    })
}

pub fn query_poll(deps: Deps<TerraQuery>, poll_id: u64) -> StdResult<PollInfo> {
    let poll = read_poll(deps.storage, &poll_id.to_be_bytes())?;
    if poll.is_none() {
        return Err(StdError::generic_err("Poll does not exist"));
    }
    map_poll(poll.unwrap(), deps.api)
}

pub fn query_polls(
    deps: Deps<TerraQuery>,
    filter: Option<PollStatus>,
    start_after: Option<u64>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<PollsResponse> {
    let polls = read_polls(deps.storage, filter, start_after, limit, order_by)?;
    let poll_responses: StdResult<Vec<PollInfo>> = polls
        .into_iter()
        .map(|poll| map_poll(poll, deps.api))
        .collect();

    Ok(PollsResponse {
        polls: poll_responses?,
    })
}

pub fn query_voters(
    deps: Deps<TerraQuery>,
    poll_id: u64,
    start_after: Option<String>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<VotersResponse> {
    let poll = match read_poll(deps.storage, &poll_id.to_be_bytes())? {
        Some(poll) => Some(poll),
        None => return Err(StdError::generic_err("Poll does not exist")),
    }
    .unwrap();

    let voters = if poll.status != PollStatus::in_progress {
        vec![]
    } else {
        read_poll_voters(
            deps.storage,
            poll_id,
            match start_after {
                Some(sa) => Some(deps.api.addr_canonicalize(&sa)?),
                None => None,
            },
            limit,
            order_by,
        )?
    };

    let voters_response: StdResult<Vec<(String, VoterInfo)>> = voters
        .into_iter()
        .map(|voter_info| {
            Ok((
                deps.api.addr_humanize(&voter_info.0)?.to_string(),
                VoterInfo {
                    vote: voter_info.1.vote,
                    balance: voter_info.1.balance,
                },
            ))
        })
        .collect();

    Ok(VotersResponse {
        voters: voters_response?,
    })
}
