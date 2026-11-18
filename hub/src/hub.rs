//! The [`Hub`] itself: opening and closing the file.
//!
//! The calls are spread over several files by subject: the family and its
//! password and PIN (`family.rs`), children and rules (`children.rs`),
//! devices and the exchange (`devices.rs`), what a guardian does
//! (`guardian.rs`), Windows accounts (`accounts.rs`), the guardian's views
//! (`views.rs`), events (`events.rs`) and keeping the file small (`tidy.rs`).

use std::fmt;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::error::HubError;
use crate::schema;

/// The family's store on the master PC: one SQLite file, held open and
/// locked by this value alone until it is closed or dropped.
///
/// Every call that changes something runs in one transaction and returns
/// only after it has been committed to the disk. Calls that only read take
/// `&self`.
pub struct Hub {
    pub(crate) conn: Connection,
    path: PathBuf,
}

impl fmt::Debug for Hub {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hub").field("path", &self.path).finish()
    }
}

impl Hub {
    /// Opens the family file at `path`, creating an empty store there when
    /// there is no file yet, and brings an older file up to date.
    ///
    /// Refused, with the file left as it was, when it is not a LiMo family
    /// file ([`HubError::NotAFamilyFile`]), when a newer version of LiMo
    /// saved it ([`HubError::FileTooNew`]), or when another hub has it open
    /// ([`HubError::InUse`]). The folder must exist.
    pub fn open(path: &Path) -> Result<Hub, HubError> {
        let conn = schema::open(path)?;
        Ok(Hub { conn, path: path.to_path_buf() })
    }

    /// Closes the file and lets another hub open it. Dropping a hub closes
    /// it too; this says whether closing went well.
    pub fn close(self) -> Result<(), HubError> {
        self.conn.close().map_err(|(_, e)| HubError::from(e))
    }

    /// The path the hub was opened on.
    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    /// A transaction for a change: begun at once, committed by the caller,
    /// rolled back if dropped.
    pub(crate) fn change(&mut self) -> Result<Transaction<'_>, HubError> {
        Ok(self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?)
    }

    /// Rebuilds the file after a deletion, so that nothing deleted is left
    /// anywhere in it, not even a copy of a key in the inner pages of an
    /// index. Deleted rows are already overwritten where they lay (secure
    /// delete); this goes further, so it is done on a best effort basis: if
    /// it cannot run now, such as on a full disk, the deletion before it
    /// still stands and the call that asked for it still succeeds.
    pub(crate) fn compact(&self) {
        let _ = self.conn.execute_batch("VACUUM");
    }
}
