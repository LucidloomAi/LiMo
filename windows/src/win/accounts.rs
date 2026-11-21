//! The user accounts of this PC, as Windows lists them, for the guardian to
//! say whose each one is: every normal account that is not switched off,
//! with its security identifier, its full name when it has one, and whether
//! it is in the Administrators group.
//!
//! The list comes from `NetUserEnum` (level 0, normal accounts), each
//! account's details from `NetUserGetInfo` (level 23: its flags, its full
//! name and its security identifier), and the Administrators group's
//! members from `NetLocalGroupGetMembers` (level 0, their security
//! identifiers), the group being found by its own security identifier,
//! `S-1-5-32-544`, since its name is in the PC's language. Only direct
//! members are seen, which is how a home PC makes an administrator.

use std::collections::BTreeSet;
use std::ffi::c_void;

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::NetworkManagement::NetManagement::{
    FILTER_NORMAL_ACCOUNT, LOCALGROUP_MEMBERS_INFO_0, MAX_PREFERRED_LENGTH, NERR_Success, NetApiBufferFree, NetLocalGroupGetMembers,
    NetUserEnum, NetUserGetInfo, UF_ACCOUNTDISABLE, USER_INFO_0, USER_INFO_23,
};
use windows::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows::Win32::Security::{LookupAccountSidW, PSID, SID_NAME_USE};
use windows::core::{PCWSTR, PWSTR};

use super::sessions::{ADMINISTRATORS, sid_text};
use super::{from_wide, wide};
use crate::os::AccountSeen;

/// The most accounts read, which no home PC comes near.
const MAX_ACCOUNTS: usize = 256;

/// A wide string Windows gave, as text.
fn text_of(units: PWSTR) -> String {
    if units.is_null() {
        return String::new();
    }
    // SAFETY: Windows gave a string ending in a NUL, which is read up to it.
    unsafe { units.to_string().unwrap_or_default() }
}

/// The names of every normal account.
fn account_names() -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    let mut resume: u32 = 0;
    loop {
        let mut buffer: *mut u8 = std::ptr::null_mut();
        let mut read: u32 = 0;
        let mut total: u32 = 0;
        // SAFETY: the buffer is Windows' own, read within its count and
        // freed with NetApiBufferFree.
        let status = unsafe {
            NetUserEnum(PCWSTR::null(), 0, FILTER_NORMAL_ACCOUNT, &mut buffer, MAX_PREFERRED_LENGTH, &mut read, &mut total, Some(&mut resume))
        };
        if status != NERR_Success && status != 234 {
            if !buffer.is_null() {
                // SAFETY: frees Windows' own buffer.
                unsafe {
                    NetApiBufferFree(Some(buffer as *const c_void));
                }
            }
            return Err(format!("NetUserEnum failed with error {status}"));
        }
        if !buffer.is_null() {
            // SAFETY: the buffer holds `read` USER_INFO_0 entries.
            let entries = unsafe { std::slice::from_raw_parts(buffer as *const USER_INFO_0, read as usize) };
            names.extend(entries.iter().map(|e| text_of(e.usri0_name)).filter(|n| !n.is_empty()));
            // SAFETY: frees Windows' own buffer, after its strings were copied.
            unsafe {
                NetApiBufferFree(Some(buffer as *const c_void));
            }
        }
        // 234 is ERROR_MORE_DATA: more to come.
        if status != 234 || names.len() >= MAX_ACCOUNTS {
            break;
        }
    }
    names.truncate(MAX_ACCOUNTS);
    Ok(names)
}

/// One account's details: whether it is switched off, its full name, and
/// its security identifier.
fn account_details(name: &str) -> Option<(bool, Option<String>, String)> {
    let user = wide(name);
    let mut buffer: *mut u8 = std::ptr::null_mut();
    // SAFETY: the buffer is Windows' own USER_INFO_23, freed with
    // NetApiBufferFree once copied.
    unsafe {
        let status = NetUserGetInfo(PCWSTR::null(), PCWSTR(user.as_ptr()), 23, &mut buffer);
        if status != NERR_Success || buffer.is_null() {
            if !buffer.is_null() {
                NetApiBufferFree(Some(buffer as *const c_void));
            }
            return None;
        }
        let info = &*(buffer as *const USER_INFO_23);
        let disabled = info.usri23_flags.0 & UF_ACCOUNTDISABLE.0 != 0;
        let full = text_of(info.usri23_full_name);
        let full = if full.trim().is_empty() { None } else { Some(full.trim().to_string()) };
        let sid = sid_text(info.usri23_user_sid);
        NetApiBufferFree(Some(buffer as *const c_void));
        Some((disabled, full, sid?))
    }
}

/// The Administrators group's name in the PC's own language.
fn administrators_name() -> Option<String> {
    let text = wide(ADMINISTRATORS);
    let mut sid = PSID::default();
    // SAFETY: the SID is made by Windows and freed with LocalFree once read.
    unsafe {
        ConvertStringSidToSidW(PCWSTR(text.as_ptr()), &mut sid).ok()?;
        let mut name = vec![0u16; 256];
        let mut name_len: u32 = 256;
        let mut domain = vec![0u16; 256];
        let mut domain_len: u32 = 256;
        let mut kind = SID_NAME_USE::default();
        let found = LookupAccountSidW(
            PCWSTR::null(),
            sid,
            Some(PWSTR(name.as_mut_ptr())),
            &mut name_len,
            Some(PWSTR(domain.as_mut_ptr())),
            &mut domain_len,
            &mut kind,
        );
        let _ = LocalFree(Some(HLOCAL(sid.0)));
        found.ok()?;
        let name = from_wide(&name);
        (!name.is_empty()).then_some(name)
    }
}

/// The security identifiers of the Administrators group's direct members.
fn administrators() -> BTreeSet<String> {
    let Some(group) = administrators_name() else { return BTreeSet::new() };
    let group = wide(&group);
    let mut members = BTreeSet::new();
    let mut buffer: *mut u8 = std::ptr::null_mut();
    let mut read: u32 = 0;
    let mut total: u32 = 0;
    // SAFETY: the buffer is Windows' own, read within its count, and freed
    // with NetApiBufferFree once its SIDs are written out.
    unsafe {
        let status =
            NetLocalGroupGetMembers(PCWSTR::null(), PCWSTR(group.as_ptr()), 0, &mut buffer, MAX_PREFERRED_LENGTH, &mut read, &mut total, None);
        if !buffer.is_null() {
            if status == NERR_Success {
                let entries = std::slice::from_raw_parts(buffer as *const LOCALGROUP_MEMBERS_INFO_0, read as usize);
                members.extend(entries.iter().filter_map(|e| sid_text(e.lgrmi0_sid)));
            }
            NetApiBufferFree(Some(buffer as *const c_void));
        }
    }
    members
}

/// Every normal account of this PC that is not switched off.
pub(crate) fn local_accounts() -> Result<Vec<AccountSeen>, String> {
    let admins = administrators();
    let mut accounts = Vec::new();
    for name in account_names()? {
        let Some((disabled, full_name, account_id)) = account_details(&name) else { continue };
        if disabled {
            continue;
        }
        let is_administrator = admins.contains(&account_id);
        accounts.push(AccountSeen { account_id, name, full_name, is_administrator });
    }
    Ok(accounts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads this PC's own accounts, which changes nothing, and passes
    /// whatever the list holds; when this process's own account is a local
    /// one, it is in the list.
    #[test]
    fn the_accounts_of_this_pc_read() {
        let accounts = local_accounts().unwrap();
        for account in &accounts {
            assert!(account.account_id.starts_with("S-1-5-"), "a security identifier");
            assert!(!account.name.is_empty());
        }
        let own = super::super::door::this_account().unwrap();
        if let Some(mine) = accounts.iter().find(|a| a.account_id == own) {
            assert!(!mine.name.is_empty());
        }
        // The group's name is found from its security identifier.
        assert!(administrators_name().is_some());
    }
}
