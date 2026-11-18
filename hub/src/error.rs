//! Everything the hub can refuse or fail at, in plain words.

use std::fmt;

use limo_core::{CodeError, MasterError, PinError, RuleProblem};
use serde::{Deserialize, Serialize};

use crate::text::TextField;

/// Why a call to the hub did not do what was asked. Every message can be
/// shown to a guardian. None of them holds a password, a PIN or a code
/// secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HubError {
    /// The file is not a LiMo family file.
    NotAFamilyFile,
    /// The file was made by a newer version of LiMo. It was not changed.
    FileTooNew {
        /// The version the file has.
        file_version: u32,
        /// The newest version this code understands.
        known_version: u32,
    },
    /// Another program, most likely another copy of the LiMo service, has
    /// the file open.
    InUse,
    /// The file could not be opened or written: a missing folder, or no
    /// permission.
    CannotOpen {
        /// What the storage layer said, for the service's log.
        detail: String,
    },
    /// The disk is full.
    DiskFull,
    /// The file is damaged: something in it cannot be read.
    Damaged {
        /// What could not be read, for the service's log.
        detail: String,
    },
    /// Another problem with the file or the disk.
    Storage {
        /// What the storage layer said, for the service's log.
        detail: String,
    },
    /// Something happened that the hub does not expect, such as a saved form
    /// of the rules core it does not recognise. Nothing was changed.
    Internal {
        /// What happened, for the service's log.
        detail: String,
    },
    /// The operating system gave no random bytes.
    NoRandomness,

    /// No family has been set up in this file yet.
    NoFamily,
    /// A family is already set up in this file.
    FamilyExists,
    /// A new password is too short.
    PasswordTooShort {
        /// The fewest characters allowed.
        min_chars: u32,
    },
    /// A new password is too long.
    PasswordTooLong {
        /// The most characters allowed.
        max_chars: u32,
    },

    /// A piece of text is empty.
    TextEmpty {
        /// Which text.
        field: TextField,
    },
    /// A piece of text is too long.
    TextTooLong {
        /// Which text.
        field: TextField,
        /// The most characters allowed.
        max_chars: u32,
    },
    /// A piece of text holds characters that cannot be shown, such as
    /// control characters or marks that reverse the direction of writing.
    TextHasHiddenCharacters {
        /// Which text.
        field: TextField,
    },

    /// No child with this id is in the family.
    UnknownChild {
        /// The id given.
        child_id: String,
    },
    /// Another child already has this name.
    ChildNameTaken {
        /// The name.
        name: String,
    },
    /// No device with this id is paired with the family.
    UnknownDevice {
        /// The id given.
        device_id: String,
    },
    /// The device was removed from the family.
    DeviceRemoved {
        /// The id given.
        device_id: String,
    },
    /// The device is paired with another child than the one named.
    DeviceOfAnotherChild {
        /// The id given.
        device_id: String,
    },
    /// The child already has the master PC paired; a child has one.
    MasterPcAlreadyPaired,
    /// The device is not a child's device on the master PC whose state the
    /// hub keeps ([`crate::Hub::pair_master_pc`]).
    NotMasterPc {
        /// The id given.
        device_id: String,
    },
    /// The device has no code secret, so no code can be made for it.
    NoCodeSecret {
        /// The id given.
        device_id: String,
    },
    /// No Windows account with this id is linked.
    UnknownAccount {
        /// The id given.
        account_id: String,
    },

    /// The rules were refused. The problems are the core's, unchanged.
    InvalidRules {
        /// Every problem found.
        problems: Vec<RuleProblem>,
    },
    /// The rules core refused the action.
    Refused {
        /// The core's reason.
        problem: MasterError,
    },
    /// A one time code could not be made.
    Code {
        /// The core's reason.
        problem: CodeError,
    },
    /// A PIN that is not 6 to 8 digits, with nothing else in it. The hub
    /// checks this itself, before the core.
    PinWrongShape {
        /// The fewest digits allowed.
        min_digits: u32,
        /// The most digits allowed.
        max_digits: u32,
    },
    /// The core refused to make the PIN record.
    Pin {
        /// The core's reason, passed on as a whole.
        problem: PinError,
    },

    /// A range of days is the wrong way round or too long.
    DaysOutOfRange {
        /// The most days allowed in one range.
        most: u32,
    },
    /// A setting is outside what is allowed.
    SettingOutOfRange {
        /// The least allowed.
        min: u32,
        /// The most allowed.
        max: u32,
    },
}

impl fmt::Display for HubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HubError::NotAFamilyFile => write!(f, "This file is not a LiMo family file."),
            HubError::FileTooNew { file_version, known_version } => write!(
                f,
                "The family's data was saved by a newer version of LiMo (file version {file_version}; this version understands up to {known_version}). Update LiMo. The file was not changed."
            ),
            HubError::InUse => write!(
                f,
                "The family's data is open in another LiMo program. Close it, or wait for it to finish, and try again."
            ),
            HubError::CannotOpen { .. } => write!(
                f,
                "The family's data file could not be opened or written. Check that the folder exists and that LiMo may use it."
            ),
            HubError::DiskFull => write!(f, "The disk is full, so the family's data could not be saved. Nothing was changed."),
            HubError::Damaged { .. } => write!(
                f,
                "Part of the family's data file is damaged and could not be read. Nothing was changed."
            ),
            HubError::Storage { .. } => write!(
                f,
                "The family's data could not be read or saved because of a problem with the disk. Nothing was changed."
            ),
            HubError::Internal { .. } => write!(
                f,
                "LiMo met something it did not expect. Nothing was changed. Please report this."
            ),
            HubError::NoRandomness => write!(
                f,
                "Windows did not give LiMo the random numbers it needs. Nothing was changed. Try again."
            ),
            HubError::NoFamily => write!(f, "No family is set up yet. Set a guardian password first."),
            HubError::FamilyExists => write!(f, "A family is already set up on this PC."),
            HubError::PasswordTooShort { min_chars } => {
                write!(f, "The password must be at least {min_chars} characters long.")
            }
            HubError::PasswordTooLong { max_chars } => {
                write!(f, "The password must be at most {max_chars} characters long.")
            }
            HubError::TextEmpty { field } => write!(f, "{} cannot be empty.", field.label_capital()),
            HubError::TextTooLong { field, max_chars } => {
                write!(f, "{} must be at most {max_chars} characters long.", field.label_capital())
            }
            HubError::TextHasHiddenCharacters { field } => write!(
                f,
                "{} has a character that cannot be shown. Type it again with letters, digits, spaces and marks.",
                field.label_capital()
            ),
            HubError::UnknownChild { .. } => write!(f, "That child is not in the family."),
            HubError::ChildNameTaken { name } => write!(f, "There is already a child called {name} in the family."),
            HubError::UnknownDevice { .. } => {
                write!(f, "That device is not paired with this family. Pair it again.")
            }
            HubError::DeviceRemoved { .. } => {
                write!(f, "That device was removed from the family. Pair it again to use it.")
            }
            HubError::DeviceOfAnotherChild { .. } => {
                write!(f, "That device is paired with another child.")
            }
            HubError::MasterPcAlreadyPaired => {
                write!(f, "This child already has this PC paired. A child has it once.")
            }
            HubError::NotMasterPc { .. } => {
                write!(f, "That device is not a child's device on this PC that LiMo keeps here.")
            }
            HubError::NoCodeSecret { .. } => write!(
                f,
                "That device cannot take one time codes. Remove it and pair it again."
            ),
            HubError::UnknownAccount { .. } => write!(f, "That Windows account is not linked."),
            HubError::InvalidRules { problems } => {
                write!(f, "The rules were not taken:")?;
                for p in problems {
                    write!(f, " {}", p.message)?;
                }
                Ok(())
            }
            HubError::Refused { problem } => write!(f, "{problem}"),
            HubError::Code { problem } => write!(f, "{problem}"),
            HubError::PinWrongShape { min_digits, max_digits } => write!(
                f,
                "A PIN is {min_digits} to {max_digits} digits, with nothing else in it."
            ),
            HubError::Pin { problem } => write!(f, "The PIN was not set: {problem}"),
            HubError::DaysOutOfRange { most } => write!(
                f,
                "Choose a first day that is not after the last, and at most {most} days in all."
            ),
            HubError::SettingOutOfRange { min, max } => write!(f, "Choose a number from {min} to {max}."),
        }
    }
}

impl std::error::Error for HubError {}

impl HubError {
    /// The core's refusal as the hub's: rules problems stay a list of
    /// problems, everything else is passed on as the core worded it.
    pub(crate) fn from_master(problem: MasterError) -> HubError {
        match problem {
            MasterError::InvalidRules { problems } => HubError::InvalidRules { problems },
            MasterError::BadPin { problem } => HubError::Pin { problem },
            MasterError::DeviceRemoved { device_id } => HubError::DeviceRemoved { device_id },
            problem => HubError::Refused { problem },
        }
    }

    /// A shape the hub did not expect, in the core's saved form or its own.
    pub(crate) fn internal(detail: impl Into<String>) -> HubError {
        HubError::Internal { detail: detail.into() }
    }

    /// A book or record that could not be read back.
    pub(crate) fn damaged(detail: impl Into<String>) -> HubError {
        HubError::Damaged { detail: detail.into() }
    }
}

impl From<rusqlite::Error> for HubError {
    fn from(e: rusqlite::Error) -> HubError {
        use rusqlite::ErrorCode;
        match &e {
            rusqlite::Error::SqliteFailure(failure, _) => match failure.code {
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => HubError::InUse,
                ErrorCode::NotADatabase => HubError::NotAFamilyFile,
                ErrorCode::DatabaseCorrupt => HubError::Damaged { detail: e.to_string() },
                ErrorCode::DiskFull => HubError::DiskFull,
                ErrorCode::CannotOpen | ErrorCode::PermissionDenied | ErrorCode::ReadOnly => {
                    HubError::CannotOpen { detail: e.to_string() }
                }
                _ => HubError::Storage { detail: e.to_string() },
            },
            rusqlite::Error::FromSqlConversionFailure(..)
            | rusqlite::Error::IntegralValueOutOfRange(..)
            | rusqlite::Error::InvalidColumnType(..)
            | rusqlite::Error::QueryReturnedNoRows => HubError::Damaged { detail: e.to_string() },
            _ => HubError::Storage { detail: e.to_string() },
        }
    }
}

impl From<serde_json::Error> for HubError {
    /// Only the kind of problem and where it is: serde_json's own message can
    /// quote part of the data, and the data can hold a secret.
    fn from(e: serde_json::Error) -> HubError {
        HubError::Damaged {
            detail: format!(
                "saved data could not be read ({:?} problem at line {}, column {})",
                e.classify(),
                e.line(),
                e.column()
            ),
        }
    }
}
