//! The Windows accounts on the master PC: each linked to a child, or marked
//! as not limited.

use rusqlite::{OptionalExtension, params};

use crate::error::HubError;
use crate::hub::Hub;
use crate::store;
use crate::text::{self, TextField};
use crate::types::{Account, AccountLink, AccountStanding};

fn link_of(child_id: Option<String>) -> AccountLink {
    match child_id {
        Some(child_id) => AccountLink::Child { child_id },
        None => AccountLink::NotLimited,
    }
}

impl Hub {
    /// Links a Windows account to a child, or marks it as not limited, and
    /// sets the name to show for it. Linking an account again replaces its
    /// link. The account identifier is opaque to the hub; the service may
    /// use the account's security identifier. When a child is removed, the
    /// accounts linked to the child are no longer linked.
    pub fn link_account(&mut self, account_id: &str, display_name: &str, link: AccountLink) -> Result<Account, HubError> {
        let account_id = text::clean(account_id, TextField::AccountId)?;
        let display_name = text::clean(display_name, TextField::AccountName)?;
        let tx = self.change()?;
        store::require_family(&tx)?;
        let child_id = match &link {
            AccountLink::Child { child_id } => Some(store::child(&tx, child_id)?.child_id),
            AccountLink::NotLimited => None,
        };
        tx.execute(
            "INSERT INTO account (id, display_name, child_id) VALUES (?1, ?2, ?3)
             ON CONFLICT (id) DO UPDATE SET display_name = excluded.display_name, child_id = excluded.child_id",
            params![account_id, display_name, child_id],
        )?;
        tx.commit()?;
        Ok(Account { account_id, display_name, link })
    }

    /// Removes an account's link. The account is then not linked at all.
    pub fn unlink_account(&mut self, account_id: &str) -> Result<(), HubError> {
        let tx = self.change()?;
        let removed = tx.execute("DELETE FROM account WHERE id = ?1", [account_id.trim()])?;
        if removed == 0 {
            return Err(HubError::UnknownAccount { account_id: account_id.to_string() });
        }
        tx.commit()?;
        Ok(())
    }

    /// Every linked account, by name.
    pub fn accounts(&self) -> Result<Vec<Account>, HubError> {
        let mut stmt = self.conn.prepare("SELECT id, display_name, child_id FROM account ORDER BY display_name, id")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Account {
                    account_id: row.get(0)?,
                    display_name: row.get(1)?,
                    link: link_of(row.get(2)?),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Which child, if any, this Windows account is. An account the
    /// guardian has not linked is [`AccountStanding::NotLinked`], also when
    /// there is no family yet.
    pub fn account_standing(&self, account_id: &str) -> Result<AccountStanding, HubError> {
        let found: Option<Option<String>> = self
            .conn
            .query_row("SELECT child_id FROM account WHERE id = ?1", [account_id.trim()], |row| row.get(0))
            .optional()?;
        Ok(match found {
            None => AccountStanding::NotLinked,
            Some(None) => AccountStanding::NotLimited,
            Some(Some(child_id)) => AccountStanding::Child { child_id },
        })
    }
}
