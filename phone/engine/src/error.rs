//! What the engine can refuse, and why a kept state could not be loaded, in
//! plain words.

use std::fmt;

use limo_core::DeviceError;
use serde::{Deserialize, Serialize};

/// Why the engine did not do what was asked. Every message can be shown;
/// none holds a PIN, a code or a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhoneError {
    /// The rules core refused, as it worded it: a request for more time that
    /// cannot be made now, for example.
    Refused {
        /// The core's reason.
        problem: DeviceError,
    },
    /// The phone's kept state could not be loaded, so nothing can be
    /// checked or changed on it.
    CannotCheck,
}

impl fmt::Display for PhoneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhoneError::Refused { problem } => write!(f, "{problem}"),
            PhoneError::CannotCheck => write!(f, "LiMo cannot check the time left just now. Ask a guardian."),
        }
    }
}

impl std::error::Error for PhoneError {}

/// Why a kept state could not be loaded. The engine then locks the phone
/// (see [`crate::PhoneEngine::load`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadProblem {
    /// There was nothing to load.
    Empty,
    /// It is not a LiMo phone state at all.
    NotLiMo,
    /// It was written by a newer LiMo, in a form this one cannot read: the
    /// app was replaced by an older one.
    Newer {
        /// The form it was written in.
        format: u32,
    },
    /// It is damaged: cut short, changed, or not readable.
    Damaged,
}

impl fmt::Display for LoadProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadProblem::Empty => write!(f, "The phone's kept state is empty."),
            LoadProblem::NotLiMo => write!(f, "The phone's kept state is not LiMo's."),
            LoadProblem::Newer { format } => write!(
                f,
                "The phone's kept state was written by a newer LiMo (form {format}); this one reads up to form {}.",
                crate::STATE_FORMAT
            ),
            LoadProblem::Damaged => write!(f, "The phone's kept state is damaged."),
        }
    }
}

impl std::error::Error for LoadProblem {}
