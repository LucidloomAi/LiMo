//! What the engine can refuse or meet on the way, in plain words.

use std::fmt;

use limo_core::DeviceError;
use limo_hub::HubError;
use serde::{Deserialize, Serialize};

/// What the engine was doing with the family's file when it failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HubStep {
    /// Reading the PC's settings.
    ReadSettings,
    /// Reading which child an account is.
    ReadAccount,
    /// Reading which devices the family's books want to hear from.
    ReadWanted,
    /// Reading whether a child's book changed.
    ReadRevision,
    /// Reading, or pairing, a child's device on this PC.
    LoadChild,
    /// Exchanging a report and a grant with a child's book.
    Exchange,
    /// Keeping a child's device state.
    Save,
    /// Noting an event for the guardian.
    Note,
}

impl HubStep {
    fn words(self) -> &'static str {
        match self {
            HubStep::ReadSettings => "reading this PC's settings",
            HubStep::ReadAccount => "reading which child an account is",
            HubStep::ReadWanted => "reading which devices should report",
            HubStep::ReadRevision => "reading whether a child's record changed",
            HubStep::LoadChild => "reading a child's record on this PC",
            HubStep::Exchange => "bringing a child's time up to date",
            HubStep::Save => "keeping a child's time on this PC",
            HubStep::Note => "noting something for the guardian",
        }
    }
}

/// Why the engine did not do what was asked, or what went wrong while it
/// did. Every message can be shown to a guardian; none holds a PIN, a code
/// or a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineError {
    /// The session is not a child's that LiMo limits, or it was not in the
    /// facts of the last call.
    NotAChildSession {
        /// The session given.
        session_id: u32,
    },
    /// The rules core refused, as it worded it: a request for more time that
    /// cannot be made now, for example.
    Refused {
        /// The core's reason.
        problem: DeviceError,
    },
    /// The family's file could not be read or written.
    Hub {
        /// What the engine was doing.
        step: HubStep,
        /// The hub's reason.
        problem: HubError,
    },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::NotAChildSession { session_id } => write!(
                f,
                "Session {session_id} is not a child's session that LiMo limits on this PC."
            ),
            EngineError::Refused { problem } => write!(f, "{problem}"),
            EngineError::Hub { step, problem } => write!(f, "While {}: {problem}", step.words()),
        }
    }
}

impl std::error::Error for EngineError {}

impl EngineError {
    /// The hub's failure at a step.
    pub(crate) fn hub(step: HubStep, problem: HubError) -> EngineError {
        EngineError::Hub { step, problem }
    }
}
