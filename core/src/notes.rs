//! What a device tells the guardian about itself: its notes, every kind in
//! one place.
//!
//! A phone notes what a guardian should know (setup finished, a permission
//! switched off or on, its protection, USB debugging, a guarded page opened,
//! the guard opened, a restart, its clock moved) and carries each note in
//! its reports until a grant confirms it, as it carries its slices. The
//! master takes each note once ([`crate::MasterBook::handle_report`] names
//! the new ones in [`crate::ReportOutcome::new_notes`]), and the device
//! drops it once confirmed.
//!
//! A kind added later is added here, and nowhere else in the core. A report
//! read by a master that does not know a kind leaves that note out and
//! takes the rest, so a phone newer than its PC still reports.

use serde::{Deserialize, Deserializer, Serialize};

/// The most notes a device keeps for the master, and the most the master
/// takes from one report: the newest.
pub const MAX_NOTES_PER_REPORT: usize = 200;

/// One of the six things LiMo needs on a phone, in the order setup asks for
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhonePermission {
    /// Notifications.
    Notifications,
    /// Android's Phone permission, only to know that a call rings or is on.
    Phone,
    /// Usage access.
    UsageAccess,
    /// LiMo's accessibility service.
    Accessibility,
    /// Left out of battery optimisation.
    Battery,
    /// LiMo's protection (its device admin).
    Protection,
}

impl PhonePermission {
    /// Every permission, in the order setup asks for them.
    pub const ALL: [PhonePermission; 6] = [
        PhonePermission::Notifications,
        PhonePermission::Phone,
        PhonePermission::UsageAccess,
        PhonePermission::Accessibility,
        PhonePermission::Battery,
        PhonePermission::Protection,
    ];
}

/// How well LiMo holds on a phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectionLevel {
    /// No protection: LiMo can be removed.
    Off,
    /// Base protection: LiMo's device admin is on, and the pages that could
    /// switch LiMo off are guarded.
    Base,
    /// Strict mode: LiMo is the phone's device owner.
    Strict,
}

/// A page of Android's settings that could switch LiMo off or remove it,
/// which LiMo covers with its guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardedPage {
    /// The page for LiMo's device admin.
    DeviceAdmin,
    /// Android's accessibility settings.
    Accessibility,
}

/// Something a phone noted for the guardian. Times are by the phone's own
/// clock (LiMo's clock on it), in unix seconds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "note")]
pub enum PhoneNote {
    /// A guardian finished setup on the phone: from then on its time is
    /// counted. The master paired the phone, so until this note comes it
    /// knows that setup is not finished.
    SetupFinished {
        /// When.
        at: i64,
    },
    /// A permission LiMo needs was switched off.
    PermissionOff {
        /// Which.
        permission: PhonePermission,
        /// When.
        at: i64,
    },
    /// A permission LiMo needs was switched on.
    PermissionOn {
        /// Which.
        permission: PhonePermission,
        /// When.
        at: i64,
    },
    /// The phone's protection level, when it changed.
    Protection {
        /// The level now.
        level: ProtectionLevel,
        /// When.
        at: i64,
    },
    /// USB debugging was found on, or off again. While it is on, any PC with
    /// a cable can stop LiMo on the phone.
    UsbDebugging {
        /// True when it is on.
        on: bool,
        /// When.
        at: i64,
    },
    /// A guarded page came to the front and was covered by the guard. Noted
    /// once a day for each page.
    GuardedPageOpened {
        /// Which page.
        page: GuardedPage,
        /// The home day.
        day: i64,
        /// When.
        at: i64,
    },
    /// A right guardian PIN on the guard opened the guarded pages.
    GuardOpened {
        /// Until when.
        until: i64,
        /// When.
        at: i64,
    },
    /// The phone was started again since LiMo last saw it.
    Restarted {
        /// When LiMo first saw it again.
        at: i64,
    },
    /// The phone's clock moved against its steady count: it was set, or
    /// corrected. No counted time was gained or lost by it.
    ClockMoved {
        /// By how many seconds; forward is positive.
        by_secs: i64,
        /// True when LiMo followed it (automatic time was on), false when it
        /// kept its own clock.
        followed: bool,
        /// When.
        at: i64,
    },
}

/// A note as a report carries it: with an id, so that the master takes it
/// once and confirms it, as it does a slice. The id is the device id,
/// `/note/` and a number that grows by one with every note the device
/// sends.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SentNote {
    /// The device id, `/note/` and a number.
    pub id: String,
    /// The note.
    pub note: PhoneNote,
}

impl SentNote {
    /// The id of note number `number` from `device_id`.
    pub fn id_for(device_id: &str, number: u64) -> String {
        format!("{device_id}/note/{number}")
    }
}

/// The number in a note id made by `device_id`. None for an id of any
/// other shape, which no device makes.
pub(crate) fn note_number(device_id: &str, note_id: &str) -> Option<u64> {
    let digits = note_id.strip_prefix(device_id)?.strip_prefix("/note/")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// One entry of a report's notes, as read: a note of a known kind, or
/// anything else, which is left out.
#[derive(Deserialize)]
#[serde(untagged)]
enum ReadNote {
    Known(SentNote),
    Other(serde::de::IgnoredAny),
}

/// Reads a report's notes, leaving out every entry that is not a note of a
/// kind this version knows, so that the rest of the report is taken.
pub(crate) fn read_notes<'de, D: Deserializer<'de>>(reader: D) -> Result<Vec<SentNote>, D::Error> {
    let read: Vec<ReadNote> = Vec::deserialize(reader)?;
    Ok(read
        .into_iter()
        .filter_map(|n| match n {
            ReadNote::Known(note) => Some(note),
            ReadNote::Other(_) => None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_ids_are_the_device_note_and_a_number() {
        assert_eq!(SentNote::id_for("phone-1", 7), "phone-1/note/7");
        assert_eq!(note_number("phone-1", "phone-1/note/7"), Some(7));
        for bad in ["phone-1/7", "phone-1/note/", "phone-1/note/x", "phone-2/note/7", "phone-1/note/-1", "phone-1/note/7/1"] {
            assert_eq!(note_number("phone-1", bad), None, "{bad}");
        }
    }

    #[test]
    fn every_permission_once_in_setup_order() {
        let mut sorted = PhonePermission::ALL.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted, PhonePermission::ALL.to_vec());
    }
}
