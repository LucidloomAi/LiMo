//! What a phone tells the guardian about itself: its notes, and which note
//! replaces which. The kinds are the rules core's ([`limo_core::PhoneNote`]):
//! a phone carries each note in its reports until a grant confirms it, the
//! core takes each once (`ReportOutcome::new_notes`), and the hub keeps
//! those as events and as the newest note of each kind for the phone, which
//! the guardian's screens read (see [`crate::PhoneInfo`]).
//!
//! A kind the core adds later is added to [`place`] here, and nowhere else in
//! the hub: the match below does not build until it is.

pub use limo_core::{GuardedPage, MAX_NOTES_PER_REPORT, PhoneNote, PhonePermission, ProtectionLevel};

fn permission_index(permission: PhonePermission) -> u8 {
    match permission {
        PhonePermission::Notifications => 0,
        PhonePermission::Phone => 1,
        PhonePermission::UsageAccess => 2,
        PhonePermission::Accessibility => 3,
        PhonePermission::Battery => 4,
        PhonePermission::Protection => 5,
    }
}

fn page_index(page: GuardedPage) -> u8 {
    match page {
        GuardedPage::DeviceAdmin => 0,
        GuardedPage::Accessibility => 1,
    }
}

/// Which newest note this one replaces: one place for each kind, and for
/// each permission and each guarded page. A permission switched off and the
/// same one switched on share a place.
fn place(note: &PhoneNote) -> (u8, u8) {
    match note {
        PhoneNote::SetupFinished { .. } => (0, 0),
        PhoneNote::PermissionOff { permission, .. } | PhoneNote::PermissionOn { permission, .. } => {
            (1, permission_index(*permission))
        }
        PhoneNote::Protection { .. } => (2, 0),
        PhoneNote::UsbDebugging { .. } => (3, 0),
        PhoneNote::GuardedPageOpened { page, .. } => (4, page_index(*page)),
        PhoneNote::GuardOpened { .. } => (5, 0),
        PhoneNote::Restarted { .. } => (6, 0),
        PhoneNote::ClockMoved { .. } => (7, 0),
    }
}

/// The newest note of each kind a phone sent, in a fixed order: what the hub
/// keeps for each phone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Newest(Vec<PhoneNote>);

impl Newest {
    /// The notes as kept.
    pub(crate) fn from_notes(mut notes: Vec<PhoneNote>) -> Newest {
        notes.sort_by_key(place);
        notes.dedup_by_key(|n| place(n));
        Newest(notes)
    }

    /// The notes, in order.
    pub(crate) fn notes(&self) -> &[PhoneNote] {
        &self.0
    }

    /// Takes a note that came after every note taken before: it replaces the
    /// note in its place. Notes are taken in the order they came, not by
    /// their times, since a phone's clock can be set back.
    pub(crate) fn take(&mut self, note: &PhoneNote) {
        let at = place(note);
        match self.0.binary_search_by_key(&at, place) {
            Ok(i) => self.0[i] = note.clone(),
            Err(i) => self.0.insert(i, note.clone()),
        }
    }

    /// When setup was finished, if it was.
    pub(crate) fn setup_finished_at(&self) -> Option<i64> {
        self.0.iter().find_map(|n| match n {
            PhoneNote::SetupFinished { at } => Some(*at),
            _ => None,
        })
    }

    /// The permissions whose newest note says `on`, in setup order.
    pub(crate) fn permissions(&self, on: bool) -> Vec<PhonePermission> {
        self.0
            .iter()
            .filter_map(|n| match n {
                PhoneNote::PermissionOn { permission, .. } if on => Some(*permission),
                PhoneNote::PermissionOff { permission, .. } if !on => Some(*permission),
                _ => None,
            })
            .collect()
    }

    /// The protection level, as last noted.
    pub(crate) fn protection(&self) -> Option<ProtectionLevel> {
        self.0.iter().find_map(|n| match n {
            PhoneNote::Protection { level, .. } => Some(*level),
            _ => None,
        })
    }

    /// Whether USB debugging is on, as last noted.
    pub(crate) fn usb_debugging_on(&self) -> Option<bool> {
        self.0.iter().find_map(|n| match n {
            PhoneNote::UsbDebugging { on, .. } => Some(*on),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_note_of_each_kind_is_kept_in_the_order_they_came() {
        let mut newest = Newest::default();
        let off = PhoneNote::PermissionOff { permission: PhonePermission::Accessibility, at: 200 };
        // A clock set back: the note that came last wins, whatever its time.
        let on = PhoneNote::PermissionOn { permission: PhonePermission::Accessibility, at: 100 };
        for note in [
            PhoneNote::Restarted { at: 50 },
            PhoneNote::PermissionOn { permission: PhonePermission::Battery, at: 60 },
            off.clone(),
            PhoneNote::SetupFinished { at: 70 },
            on.clone(),
            PhoneNote::UsbDebugging { on: true, at: 80 },
            PhoneNote::Restarted { at: 90 },
        ] {
            newest.take(&note);
        }
        assert_eq!(newest.notes(), [
            PhoneNote::SetupFinished { at: 70 },
            PhoneNote::PermissionOn { permission: PhonePermission::Accessibility, at: 100 },
            PhoneNote::PermissionOn { permission: PhonePermission::Battery, at: 60 },
            PhoneNote::UsbDebugging { on: true, at: 80 },
            PhoneNote::Restarted { at: 90 },
        ]);
        assert_eq!(newest.permissions(true), vec![PhonePermission::Accessibility, PhonePermission::Battery]);
        assert!(newest.permissions(false).is_empty());
        newest.take(&off);
        assert_eq!(newest.permissions(false), vec![PhonePermission::Accessibility]);
        assert_eq!((newest.setup_finished_at(), newest.usb_debugging_on(), newest.protection()), (Some(70), Some(true), None));
        assert_eq!(Newest::from_notes(newest.notes().iter().rev().cloned().collect()), newest);
    }

    /// The hub keeps notes as JSON in its file, in events and for each
    /// phone; this is the shape it keeps, so a change in it is seen here.
    #[test]
    fn notes_are_kept_in_the_shape_the_file_holds() {
        let note = PhoneNote::GuardedPageOpened { page: GuardedPage::DeviceAdmin, day: 20_775, at: 5 };
        let json = serde_json::to_string(&note).unwrap();
        assert_eq!(json, r#"{"note":"guarded_page_opened","page":"device_admin","day":20775,"at":5}"#);
        assert_eq!(serde_json::from_str::<PhoneNote>(&json).unwrap(), note);
        let off = r#"{"note":"permission_off","permission":"usage_access","at":9}"#;
        assert_eq!(
            serde_json::from_str::<PhoneNote>(off).unwrap(),
            PhoneNote::PermissionOff { permission: PhonePermission::UsageAccess, at: 9 }
        );
        assert!(serde_json::from_str::<PhoneNote>(r#"{"note":"permission_off","permission":"camera","at":9}"#).is_err());
    }
}
