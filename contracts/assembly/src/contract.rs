use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, wasm_execute, Addr, Binary, CosmosMsg, Decimal,
    Deps, DepsMut, Env, IbcQuery, ListChannelsResponse, MessageInfo, Order, QuerierWrapper,
    QueryRequest, Response, StdResult, Uint128, Uint64, WasmMsg,
};
use cw2::{get_contract_version, set_contract_version};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_storage_plus::Bound;
use std::str::FromStr;

use astroport_governance::assembly::{
    helpers::validate_links, Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, Proposal,
    ProposalListResponse, ProposalMessage, ProposalStatus, ProposalVoteOption,
    ProposalVotesResponse, QueryMsg, UpdateConfig,
};

use astroport::xastro_token::QueryMsg as XAstroTokenQueryMsg;
use astroport_governance::builder_unlock::msg::{
    AllocationResponse, QueryMsg as BuilderUnlockQueryMsg, StateResponse,
};
use astroport_governance::voting_escrow::{QueryMsg as VotingEscrowQueryMsg, VotingPowerResponse};

use crate::error::ContractError;
use crate::migration::{migrate_config, migrate_proposals, MigrateMsg};
use crate::state::{CONFIG, PROPOSALS, PROPOSAL_COUNT};

// Contract name and version used for migration.
const CONTRACT_NAME: &str = "astro-assembly";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Default pagination constants
const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 30;

/// ## Description
/// Creates a new contract with the specified parameters in the `msg` variable.
/// Returns a [`Response`] with the specified attributes if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`]
///
/// * **_info** is an object of type [`MessageInfo`]
///
/// * **msg**  is a message of type [`InstantiateMsg`] which contains the parameters used for creating a contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    if msg.whitelisted_links.is_empty() {
        return Err(ContractError::WhitelistEmpty {});
    }

    validate_links(&msg.whitelisted_links)?;

    let mut config = Config {
        xastro_token_addr: deps.api.addr_validate(&msg.xastro_token_addr)?,
        vxastro_token_addr: None,
        ibc_controller: None,
        builder_unlock_addr: deps.api.addr_validate(&msg.builder_unlock_addr)?,
        proposal_voting_period: msg.proposal_voting_period,
        proposal_effective_delay: msg.proposal_effective_delay,
        proposal_expiration_period: msg.proposal_expiration_period,
        proposal_required_deposit: msg.proposal_required_deposit,
        proposal_required_quorum: Decimal::from_str(&msg.proposal_required_quorum)?,
        proposal_required_threshold: Decimal::from_str(&msg.proposal_required_threshold)?,
        whitelisted_links: msg.whitelisted_links,
    };

    if let Some(vxastro_token_addr) = msg.vxastro_token_addr {
        config.vxastro_token_addr = Some(deps.api.addr_validate(&vxastro_token_addr)?);
    }

    if let Some(ibc_controller) = msg.ibc_controller {
        config.ibc_controller = Some(deps.api.addr_validate(&ibc_controller)?);
    }

    config.validate()?;

    CONFIG.save(deps.storage, &config)?;

    PROPOSAL_COUNT.save(deps.storage, &Uint64::zero())?;

    Ok(Response::default())
}

/// ## Description
/// Exposes all the execute functions available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`ExecuteMsg`].
///
/// ## Execute messages
/// * **ExecuteMsg::Receive(cw20_msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::CastVote { proposal_id, vote }** Cast a vote on a specific proposal.
///
/// * **ExecuteMsg::EndProposal { proposal_id }** Sets the status of an expired/finalized proposal.
///
/// * **ExecuteMsg::ExecuteProposal { proposal_id }** Executes a successful proposal.
///
/// * **ExecuteMsg::RemoveCompletedProposal { proposal_id }** Removes a finalized proposal from the proposal list.
///
/// * **ExecuteMsg::UpdateConfig(config)** Updates the contract configuration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(cw20_msg) => receive_cw20(deps, env, info, cw20_msg),
        ExecuteMsg::CastVote { proposal_id, vote } => cast_vote(deps, env, info, proposal_id, vote),
        ExecuteMsg::EndProposal { proposal_id } => end_proposal(deps, env, info, proposal_id),
        ExecuteMsg::ExecuteProposal { proposal_id } => {
            execute_proposal(deps, env, info, proposal_id)
        }
        ExecuteMsg::CheckMessages { messages } => check_messages(env, messages),
        ExecuteMsg::CheckMessagesPassed {} => Err(ContractError::MessagesCheckPassed {}),
        ExecuteMsg::RemoveCompletedProposal { proposal_id } => {
            remove_completed_proposal(deps, env, info, proposal_id)
        }
        ExecuteMsg::UpdateConfig(config) => update_config(deps, env, info, config),
        ExecuteMsg::IBCProposalCompleted {
            proposal_id,
            status,
        } => update_ibc_proposal_status(deps, info, proposal_id, status),
    }
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then a [`ContractError`] is returned,
/// otherwise the function returns a [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **cw20_msg** is an object of type [`Cw20ReceiveMsg`]. This is the CW20 message to process.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::SubmitProposal {
            title,
            description,
            link,
            messages,
            ibc_channel,
        } => submit_proposal(
            deps,
            env,
            info,
            Addr::unchecked(cw20_msg.sender),
            cw20_msg.amount,
            title,
            description,
            link,
            messages,
            ibc_channel,
        ),
    }
}

/// ## Description
/// Submit a brand new proposal and locks some xASTRO as an anti-spam mechanism.
/// Returns [`ContractError`] on failure, otherwise returns a [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **sender** is an object of type [`Addr`]. Proposal submitter.
///
/// * **deposit_amount** is an object of type [`Uint128`]. This is the amount of xASTRO to deposit in order to submit the proposal.
///
/// * **title** is an object of type [`String`]. Proposal title.
///
/// * **description** is an object of type [`String`]. Proposal description.
///
/// * **link** is an object of type [`Option<String>`]. Proposal link.
///
/// * **messages** is an object of type [`Option<Vec<ProposalMessage>>`]. Executable messages (actions to perform if the proposal passes).
#[allow(clippy::too_many_arguments)]
pub fn submit_proposal(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    deposit_amount: Uint128,
    title: String,
    description: String,
    link: Option<String>,
    messages: Option<Vec<ProposalMessage>>,
    ibc_channel: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.xastro_token_addr {
        return Err(ContractError::Unauthorized {});
    }

    if deposit_amount < config.proposal_required_deposit {
        return Err(ContractError::InsufficientDeposit {});
    }

    // Update the proposal count
    let count = PROPOSAL_COUNT.update(deps.storage, |c| -> StdResult<_> {
        Ok(c.checked_add(Uint64::new(1))?)
    })?;

    // Check that controller exists and it supports this channel
    if let Some(ibc_channel) = &ibc_channel {
        if let Some(ibc_controller) = &config.ibc_controller {
            check_controller_supports_channel(deps.querier, ibc_controller, ibc_channel)?;
        } else {
            return Err(ContractError::MissingIBCController {});
        }
    }

    let proposal = Proposal {
        proposal_id: count,
        submitter: sender.clone(),
        status: ProposalStatus::Active,
        for_power: Uint128::zero(),
        against_power: Uint128::zero(),
        for_voters: Vec::new(),
        against_voters: Vec::new(),
        start_block: env.block.height,
        start_time: env.block.time.seconds(),
        end_block: env.block.height + config.proposal_voting_period,
        title,
        description,
        link,
        messages,
        deposit_amount,
        ibc_channel,
    };

    proposal.validate(config.whitelisted_links)?;

    PROPOSALS.save(deps.storage, count.u64(), &proposal)?;

    Ok(Response::new()
        .add_attribute("action", "submit_proposal")
        .add_attribute("submitter", sender)
        .add_attribute("proposal_id", count.to_string())
        .add_attribute(
            "proposal_end_height",
            (env.block.height + config.proposal_voting_period).to_string(),
        ))
}

/// ## Description
/// Cast a vote on a proposal.
/// Returns [`ContractError`] on failure, otherwise returns a [`Response`] with the specified
/// attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **proposal_id** is the identifier of the proposal.
///
/// * **vote_option** is an object of type [`ProposalVoteOption`]. Contains the vote option.
pub fn cast_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote_option: ProposalVoteOption,
) -> Result<Response, ContractError> {
    let mut proposal = PROPOSALS.load(deps.storage, proposal_id)?;

    if proposal.status != ProposalStatus::Active {
        return Err(ContractError::ProposalNotActive {});
    }

    if proposal.submitter == info.sender {
        return Err(ContractError::Unauthorized {});
    }

    if env.block.height > proposal.end_block {
        return Err(ContractError::VotingPeriodEnded {});
    }

    if proposal.for_voters.contains(&info.sender) || proposal.against_voters.contains(&info.sender)
    {
        return Err(ContractError::UserAlreadyVoted {});
    }

    let voting_power = calc_voting_power(deps.as_ref(), info.sender.to_string(), &proposal)?;

    if voting_power.is_zero() {
        return Err(ContractError::NoVotingPower {});
    }

    match vote_option {
        ProposalVoteOption::For => {
            proposal.for_power = proposal.for_power.checked_add(voting_power)?;
            proposal.for_voters.push(info.sender.clone());
        }
        ProposalVoteOption::Against => {
            proposal.against_power = proposal.against_power.checked_add(voting_power)?;
            proposal.against_voters.push(info.sender.clone());
        }
    };

    PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

    Ok(Response::new()
        .add_attribute("action", "cast_vote")
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("voter", &info.sender)
        .add_attribute("vote", vote_option.to_string())
        .add_attribute("voting_power", voting_power))
}

/// ## Description
/// Ends proposal voting and sets the proposal status.
/// Returns a [`ContractError`] on failure, otherwise returns a [`Response`] with the specified
/// attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **proposal_id** is a parameter of type `u64`. This is the proposal identifier.
pub fn end_proposal(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let mut proposal = PROPOSALS.load(deps.storage, proposal_id)?;

    if proposal.status != ProposalStatus::Active {
        return Err(ContractError::ProposalNotActive {});
    }

    if env.block.height <= proposal.end_block {
        return Err(ContractError::VotingPeriodNotEnded {});
    }

    let config = CONFIG.load(deps.storage)?;

    let for_votes = proposal.for_power;
    let against_votes = proposal.against_power;
    let total_votes = for_votes + against_votes;

    let total_voting_power = calc_total_voting_power_at(deps.as_ref(), &proposal)?;

    let mut proposal_quorum: Decimal = Decimal::zero();
    let mut proposal_threshold: Decimal = Decimal::zero();

    if !total_voting_power.is_zero() {
        proposal_quorum = Decimal::from_ratio(total_votes, total_voting_power);
    }

    if !total_votes.is_zero() {
        proposal_threshold = Decimal::from_ratio(for_votes, total_votes);
    }

    // Determine the proposal result
    proposal.status = if proposal_quorum >= config.proposal_required_quorum
        && proposal_threshold > config.proposal_required_threshold
    {
        ProposalStatus::Passed
    } else {
        ProposalStatus::Rejected
    };

    PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

    let response = Response::new()
        .add_attributes(vec![
            attr("action", "end_proposal"),
            attr("proposal_id", proposal_id.to_string()),
            attr("proposal_result", proposal.status.to_string()),
        ])
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.xastro_token_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: proposal.submitter.to_string(),
                amount: proposal.deposit_amount,
            })?,
            funds: vec![],
        }));

    Ok(response)
}

/// ## Description
/// Executes a successful proposal.
/// Returns [`ContractError`] on failure, otherwise returns a [`Response`] with the specified
/// attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **proposal_id** is a parameter of type `u64`. This is the proposal identifier.
pub fn execute_proposal(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let mut proposal = PROPOSALS.load(deps.storage, proposal_id)?;

    if proposal.status != ProposalStatus::Passed {
        return Err(ContractError::ProposalNotPassed {});
    }

    let config = CONFIG.load(deps.storage)?;

    if env.block.height < (proposal.end_block + config.proposal_effective_delay) {
        return Err(ContractError::ProposalDelayNotEnded {});
    }

    if env.block.height
        > (proposal.end_block + config.proposal_effective_delay + config.proposal_expiration_period)
    {
        return Err(ContractError::ExecuteProposalExpired {});
    }

    let messages;
    if let Some(channel) = &proposal.ibc_channel {
        let config = CONFIG.load(deps.storage)?;

        if proposal.messages.is_some() {
            proposal.status = ProposalStatus::InProgress;
        } else {
            proposal.status = ProposalStatus::Executed;
        }
        PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

        messages = match proposal.messages {
            Some(mut messages) => {
                messages.sort_by(|a, b| a.order.cmp(&b.order));

                vec![CosmosMsg::Wasm(wasm_execute(
                    &config
                        .ibc_controller
                        .ok_or(ContractError::MissingIBCController {})?,
                    &ibc_controller_package::ExecuteMsg::IbcExecuteProposal {
                        channel_id: channel.to_string(),
                        proposal_id,
                        messages,
                    },
                    vec![],
                )?)]
            }
            None => vec![],
        };
    } else {
        proposal.status = ProposalStatus::Executed;
        PROPOSALS.save(deps.storage, proposal_id, &proposal)?;

        messages = match proposal.messages {
            Some(mut messages) => {
                messages.sort_by(|a, b| a.order.cmp(&b.order));
                messages.into_iter().map(|message| message.msg).collect()
            }
            None => vec![],
        };
    }

    Ok(Response::new()
        .add_attribute("action", "execute_proposal")
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_messages(messages))
}

/// ## Description
/// Checks that proposal messages are correct.
/// Returns [`ContractError`] on failure, otherwise returns a [`Response`] with the specified
/// attributes if the operation was successful.
/// ## Params
/// * **env** is an object of type [`Env`].
///
/// * **messages** is a vector of [`ProposalMessage`].
pub fn check_messages(
    env: Env,
    mut messages: Vec<ProposalMessage>,
) -> Result<Response, ContractError> {
    messages.sort_by(|a, b| a.order.cmp(&b.order));
    let mut messages: Vec<_> = messages.into_iter().map(|message| message.msg).collect();
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::CheckMessagesPassed {})?,
        funds: vec![],
    }));

    Ok(Response::new()
        .add_attribute("action", "check_messages")
        .add_messages(messages))
}

/// ## Description
/// Removes an expired or rejected proposal from the general proposal list.
/// Returns [`ContractError`] on failure, otherwise returns a [`Response`] with the specified
/// attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **proposal_id** is a parameter of type `u64`. This is the proposal identifier.
pub fn remove_completed_proposal(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let mut proposal = PROPOSALS.load(deps.storage, proposal_id)?;

    if env.block.height
        > (proposal.end_block + config.proposal_effective_delay + config.proposal_expiration_period)
    {
        proposal.status = ProposalStatus::Expired;
    }

    if proposal.status != ProposalStatus::Expired && proposal.status != ProposalStatus::Rejected {
        return Err(ContractError::ProposalNotCompleted {});
    }

    PROPOSALS.remove(deps.storage, proposal_id);

    Ok(Response::new()
        .add_attribute("action", "remove_completed_proposal")
        .add_attribute("proposal_id", proposal_id.to_string()))
}

/// ## Description
/// Updates Assembly contract parameters.
/// Returns [`ContractError`] on failure, otherwise returns a [`Response`] with the specified
/// attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **updated_config** is an object of type [`UpdateConfig`]. This is the new contract configuration.
pub fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    updated_config: UpdateConfig,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // Only the Assembly is allowed to update its own parameters (through a successful proposal)
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(ibc_controller) = updated_config.ibc_controller {
        config.ibc_controller = Some(deps.api.addr_validate(&ibc_controller)?)
    }

    if let Some(builder_unlock_addr) = updated_config.builder_unlock_addr {
        config.builder_unlock_addr = deps.api.addr_validate(&builder_unlock_addr)?;
    }

    if let Some(proposal_voting_period) = updated_config.proposal_voting_period {
        config.proposal_voting_period = proposal_voting_period;
    }

    if let Some(proposal_effective_delay) = updated_config.proposal_effective_delay {
        config.proposal_effective_delay = proposal_effective_delay;
    }

    if let Some(proposal_expiration_period) = updated_config.proposal_expiration_period {
        config.proposal_expiration_period = proposal_expiration_period;
    }

    if let Some(proposal_required_deposit) = updated_config.proposal_required_deposit {
        config.proposal_required_deposit = Uint128::from(proposal_required_deposit);
    }

    if let Some(proposal_required_quorum) = updated_config.proposal_required_quorum {
        config.proposal_required_quorum = Decimal::from_str(&proposal_required_quorum)?;
    }

    if let Some(proposal_required_threshold) = updated_config.proposal_required_threshold {
        config.proposal_required_threshold = Decimal::from_str(&proposal_required_threshold)?;
    }

    if let Some(whitelist_add) = updated_config.whitelist_add {
        validate_links(&whitelist_add)?;

        config.whitelisted_links.append(
            &mut whitelist_add
                .into_iter()
                .filter(|link| !config.whitelisted_links.contains(link))
                .collect(),
        );
    }

    if let Some(whitelist_remove) = updated_config.whitelist_remove {
        config
            .whitelisted_links
            .retain(|link| !whitelist_remove.contains(link));

        if config.whitelisted_links.is_empty() {
            return Err(ContractError::WhitelistEmpty {});
        }
    }

    config.validate()?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

/// ## Description
/// Updates proposal status InProgress -> Executed or Failed. Intended to be called in the end of
/// the ibc execution cycle via ibc-controller. Only ibc controller is able to call this function.
/// ## Params
/// * **deps** cosmwasm dependencies,
///
/// * **info** message info,
///
/// * **id** proposal's id,
///
/// * **status** a new proposal status reported by ibc controller.
fn update_ibc_proposal_status(
    deps: DepsMut,
    info: MessageInfo,
    id: u64,
    new_status: ProposalStatus,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if Some(info.sender) == config.ibc_controller {
        let mut proposal = PROPOSALS.load(deps.storage, id)?;

        if proposal.status != ProposalStatus::InProgress {
            return Err(ContractError::NotUpdatedProposalStatus {});
        }

        match new_status {
            ProposalStatus::Executed {} | ProposalStatus::Failed {} => {
                proposal.status = new_status;
                PROPOSALS.save(deps.storage, id, &proposal)?;
                Ok(Response::new().add_attribute("action", "ibc_proposal_completed"))
            }
            _ => Err(ContractError::InvalidIBCProposalStatus(
                new_status.to_string(),
            )),
        }
    } else {
        Err(ContractError::InvalidIBCController {})
    }
}

/// ## Description
/// Expose available contract queries.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns core contract settings stored in the [`Config`] structure.
///
/// * **QueryMsg::Proposals { start, limit }** Returns a [`ProposalListResponse`] according to the specified input parameters.
///
/// * **QueryMsg::Proposal { proposal_id }** Returns a [`Proposal`] according to the specified `proposal_id`.
///
/// * **QueryMsg::ProposalVotes { proposal_id }** Returns proposal vote counts that are stored in the [`ProposalVotesResponse`] structure.
///
/// * **QueryMsg::UserVotingPower { user, proposal_id }** Returns user voting power for a specific proposal.
///
/// * **QueryMsg::TotalVotingPower { proposal_id }** Returns total voting power for a specific proposal.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::Proposals { start, limit } => to_binary(&query_proposals(deps, start, limit)?),
        QueryMsg::Proposal { proposal_id } => to_binary(&query_proposal(deps, proposal_id)?),
        QueryMsg::ProposalVotes { proposal_id } => {
            to_binary(&query_proposal_votes(deps, proposal_id)?)
        }
        QueryMsg::UserVotingPower { user, proposal_id } => {
            let proposal = PROPOSALS.load(deps.storage, proposal_id)?;

            deps.api.addr_validate(&user)?;

            to_binary(&calc_voting_power(deps, user, &proposal)?)
        }
        QueryMsg::TotalVotingPower { proposal_id } => {
            let proposal = PROPOSALS.load(deps.storage, proposal_id)?;
            to_binary(&calc_total_voting_power_at(deps, &proposal)?)
        }
    }
}

/// ## Description
/// Returns the contract configuration stored in the [`Config`] structure.
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config)
}

/// ## Description
/// Returns the current proposal list.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **start_after** is an [`Option`] type. Specifies the proposal list index to start reading from.
///
/// * **limit** is a [`Option`] type. Specifies the number of items to read.
pub fn query_proposals(
    deps: Deps,
    start: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let proposal_count = PROPOSAL_COUNT.load(deps.storage)?;

    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start.map(Bound::inclusive);

    let proposals_list: StdResult<Vec<_>> = PROPOSALS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (_k, v) = item?;
            Ok(v)
        })
        .collect();

    Ok(ProposalListResponse {
        proposal_count,
        proposal_list: proposals_list?,
    })
}

/// ## Description
/// Returns proposal information stored in the [`Proposal`] structure.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **proposal_id** is a parameter of type `u64`. This is the proposal identifier.
pub fn query_proposal(deps: Deps, proposal_id: u64) -> StdResult<Proposal> {
    let proposal = PROPOSALS.load(deps.storage, proposal_id)?;
    Ok(proposal)
}

/// ## Description
/// Returns proposal votes stored in the [`ProposalVotesResponse`] structure.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **proposal_id** is a parameter of type `u64`. This is the proposal identifier.
pub fn query_proposal_votes(deps: Deps, proposal_id: u64) -> StdResult<ProposalVotesResponse> {
    let proposal = PROPOSALS.load(deps.storage, proposal_id)?;

    Ok(ProposalVotesResponse {
        proposal_id,
        for_power: proposal.for_power,
        against_power: proposal.against_power,
    })
}

/// ## Description
/// Calculates an address' voting power at the specified block.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **sender** is an object of type [`String`]. This is the address whose voting power we calculate.
///
/// * **proposal** is an object of type [`Proposal`]. This is the proposal for which we want to compute the `sender` (voter) voting power.
pub fn calc_voting_power(deps: Deps, sender: String, proposal: &Proposal) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    // This is the address' xASTRO balance at the previous block (proposal.start_block - 1).
    // We use the previous block because it always has an up-to-date checkpoint.
    // BalanceAt will always return the balance information in the previous block,
    // so we don't subtract one block from proposal.start_block.
    let xastro_amount: BalanceResponse = deps.querier.query_wasm_smart(
        config.xastro_token_addr,
        &XAstroTokenQueryMsg::BalanceAt {
            address: sender.clone(),
            block: proposal.start_block,
        },
    )?;

    let mut total = xastro_amount.balance;

    let locked_amount: AllocationResponse = deps.querier.query_wasm_smart(
        config.builder_unlock_addr,
        &BuilderUnlockQueryMsg::Allocation {
            account: sender.clone(),
        },
    )?;

    if !locked_amount.params.amount.is_zero() {
        total = total
            .checked_add(locked_amount.params.amount)?
            .checked_sub(locked_amount.status.astro_withdrawn)?;
    }

    if let Some(vxastro_token_addr) = config.vxastro_token_addr {
        let vxastro_amount: VotingPowerResponse = deps.querier.query_wasm_smart(
            &vxastro_token_addr,
            &VotingEscrowQueryMsg::UserVotingPowerAt {
                user: sender.clone(),
                time: proposal.start_time - 1,
            },
        )?;

        if !vxastro_amount.voting_power.is_zero() {
            total = total.checked_add(vxastro_amount.voting_power)?;
        }

        let locked_xastro: Uint128 = deps.querier.query_wasm_smart(
            vxastro_token_addr,
            &VotingEscrowQueryMsg::UserDepositAtHeight {
                user: sender,
                height: proposal.start_block,
            },
        )?;

        total = total.checked_add(locked_xastro)?;
    }

    Ok(total)
}

/// ## Description
/// Calculates the total voting power at a specified block (that is relevant for a specific proposal).
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **proposal** is an object of type [`Proposal`]. This is the proposal for which we calculate the total voting power.
pub fn calc_total_voting_power_at(deps: Deps, proposal: &Proposal) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    // This is the address' xASTRO balance at the previous block (proposal.start_block - 1).
    // We use the previous block because it always has an up-to-date checkpoint.
    let mut total: Uint128 = deps.querier.query_wasm_smart(
        &config.xastro_token_addr,
        &XAstroTokenQueryMsg::TotalSupplyAt {
            block: proposal.start_block - 1,
        },
    )?;

    // Total amount of ASTRO locked in the initial builder's unlock schedule
    let builder_state: StateResponse = deps
        .querier
        .query_wasm_smart(config.builder_unlock_addr, &BuilderUnlockQueryMsg::State {})?;

    if !builder_state.remaining_astro_tokens.is_zero() {
        total = total.checked_add(builder_state.remaining_astro_tokens)?;
    }

    if let Some(vxastro_token_addr) = config.vxastro_token_addr {
        // Total vxASTRO voting power
        let vxastro: VotingPowerResponse = deps.querier.query_wasm_smart(
            &vxastro_token_addr,
            &VotingEscrowQueryMsg::TotalVotingPowerAt {
                time: proposal.start_time - 1,
            },
        )?;
        if !vxastro.voting_power.is_zero() {
            total = total.checked_add(vxastro.voting_power)?;
        }
    }

    Ok(total)
}

/// ## Description
/// Checks that controller supports given IBC-channel.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **ibc_controller** is an ibc controller contract address.
///
/// * **given_channel** is an IBC channel id the function needs to check.
pub fn check_controller_supports_channel(
    querier: QuerierWrapper,
    ibc_controller: &Addr,
    given_channel: &String,
) -> Result<(), ContractError> {
    let port_id = Some(format!("wasm.{}", ibc_controller));
    let ListChannelsResponse { channels } =
        querier.query(&QueryRequest::Ibc(IbcQuery::ListChannels { port_id }))?;
    channels
        .iter()
        .find(|channel| &channel.endpoint.channel_id == given_channel)
        .map(|_| ())
        .ok_or_else(|| ContractError::InvalidChannel(given_channel.to_string()))
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(mut deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astro-assembly" => match contract_version.version.as_ref() {
            "1.1.0" => {
                migrate_config(&mut deps, &msg)?;

                migrate_proposals(deps.storage)?;
            }
            "1.2.0" => {}
            "1.2.1" => {}
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
