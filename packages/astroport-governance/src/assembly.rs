use cosmwasm_std::{Addr, CosmosMsg, Decimal, Uint128, Uint64};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter, Result};

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Address of xASTRO token
    pub xastro_token_addr: String,
    /// Address of staking contract
    pub staking_addr: String,
    /// Proposal voting period
    pub proposal_voting_period: u64,
    /// Proposal effective delay
    pub proposal_effective_delay: u64,
    /// Proposal expiration period
    pub proposal_expiration_period: u64,
    /// Proposal required deposit
    pub proposal_required_deposit: u128,
    /// Proposal required quorum
    pub proposal_required_quorum: u64,
    /// Proposal required threshold
    pub proposal_required_threshold: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// ## Description
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// ## Description
    /// Cast vote for an active propose.
    CastVote {
        /// Proposal identifier
        proposal_id: u64,
        /// Vote option
        vote: ProposalVoteOption,
    },
    /// ## Description
    /// End proposal.
    EndProposal {
        /// Proposal identifier
        proposal_id: u64,
    },
    /// ## Description
    /// Execute proposal messages
    ExecuteProposal {
        /// Proposal identifier
        proposal_id: u64,
    },
    /// ## Description
    /// Remove completed proposal in the proposal list.
    RemoveCompletedProposal {
        /// Proposal identifier
        proposal_id: u64,
    },
    /// ## Description
    /// Update current assembly contract
    /// ## Executor
    /// Only assembly contract via passed proposal can execute it
    UpdateConfig(UpdateConfig),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Config returns the base setting of the assembly contract
    Config {},
    /// Proposals returns list of proposals
    Proposals {
        start: Option<u64>,
        limit: Option<u32>,
    },
    /// Proposal returns information about proposal
    Proposal { proposal_id: u64 },
    /// Proposal returns information about proposal votes
    ProposalVotes { proposal_id: u64 },
}

/// ## Description
/// This structure describes a CW20 hook message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    SubmitProposal {
        title: String,
        description: String,
        link: Option<String>,
        messages: Option<Vec<ProposalMessage>>,
    },
}

/// ## Description
/// This structure describes the basic settings for assembly contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// xASTRO token address
    pub xastro_token_addr: Addr,
    /// Staking contract address
    pub staking_addr: Addr,
    /// Proposal voting period
    pub proposal_voting_period: u64,
    /// Proposal effective delay
    pub proposal_effective_delay: u64,
    /// Proposal expiration period
    pub proposal_expiration_period: u64,
    /// Proposal required deposit
    pub proposal_required_deposit: Uint128,
    /// Proposal required quorum
    pub proposal_required_quorum: Decimal,
    /// Proposal required threshold
    pub proposal_required_threshold: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UpdateConfig {
    /// xASTRO token address
    pub xastro_token_addr: Option<String>,
    /// Staking contract address
    pub staking_addr: Option<String>,
    /// Proposal voting period
    pub proposal_voting_period: Option<u64>,
    /// Proposal effective delay
    pub proposal_effective_delay: Option<u64>,
    /// Proposal expiration period
    pub proposal_expiration_period: Option<u64>,
    /// Proposal required deposit
    pub proposal_required_deposit: Option<u128>,
    /// Proposal required quorum
    pub proposal_required_quorum: Option<u64>,
    /// Proposal required threshold
    pub proposal_required_threshold: Option<u64>,
}

/// ## Description
/// This structs describes proposal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Proposal {
    /// Unique ID of proposal
    pub proposal_id: Uint64,
    /// Submitter address of proposal
    pub submitter: Addr,
    /// Status of proposal
    pub status: ProposalStatus,
    /// `For` votes of proposal
    pub for_votes: Uint128,
    /// `Against` votes of proposal
    pub against_votes: Uint128,
    /// Start block of proposal
    pub start_block: u64,
    /// End block of proposal
    pub end_block: u64,
    /// Title of proposal
    pub title: String,
    /// Description of proposal
    pub description: String,
    /// Link of proposal
    pub link: Option<String>,
    /// Messages of proposal
    pub messages: Option<Vec<ProposalMessage>>,
    /// Deposit amount of proposal
    pub deposit_amount: Uint128,
}

/// ## Description
/// This enum describes available statuses for Proposal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ProposalStatus {
    Active,
    Passed,
    Rejected,
    Executed,
    Expired,
}

impl Display for ProposalStatus {
    fn fmt(&self, fmt: &mut Formatter) -> Result {
        match self {
            ProposalStatus::Active {} => fmt.write_str("active"),
            ProposalStatus::Passed {} => fmt.write_str("passed"),
            ProposalStatus::Rejected {} => fmt.write_str("rejected"),
            ProposalStatus::Executed {} => fmt.write_str("executed"),
            ProposalStatus::Expired {} => fmt.write_str("expired"),
        }
    }
}

/// ## Description
/// This structure describes proposal message
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ProposalMessage {
    /// Order of execution of the message
    pub order: Uint64,
    /// Execution message
    pub msg: CosmosMsg,
}

/// ## Description
/// This structure describes proposal vote
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ProposalVote {
    /// Voted option for proposal
    pub option: ProposalVoteOption,
    /// Power of vote
    pub power: Uint128,
}

/// ## Description
/// This enum describes available options for voting on the proposal
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ProposalVoteOption {
    For,
    Against,
}

impl Display for ProposalVoteOption {
    fn fmt(&self, fmt: &mut Formatter) -> Result {
        match self {
            ProposalVoteOption::For {} => fmt.write_str("for"),
            ProposalVoteOption::Against {} => fmt.write_str("against"),
        }
    }
}

/// ## Description
/// This structure describes proposal vote response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ProposalVotesResponse {
    pub proposal_id: u64,
    pub for_votes: u128,
    pub against_votes: u128,
}

/// ## Description
/// This structure describes proposal list response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ProposalListResponse {
    pub proposal_count: Uint64,
    pub proposal_list: Vec<Proposal>,
}
