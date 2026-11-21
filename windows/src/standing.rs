//! How a child's phone stands now, and what it needs a guardian for: derived
//! in this one place from the notes it sent, taken in the order they came
//! (the newest of each kind wins, so a step switched off and later on is
//! on), and from what the master measured of it (how far its clock is off),
//! for the console's answers (`family_today`, `child_today`, `devices`,
//! `device`). The guardian's screens show it; they never derive it.
//!
//! A note's time is by the phone's own clock (LiMo's clock on it), as the
//! core keeps it. The master PC's own device has no notes, and no standing.

#![forbid(unsafe_code)]

use limo_core::{GuardedPage, PhoneNote, PhonePermission, ProtectionLevel};
use limo_hub::CLOCK_FAR_OFF_SECS;
use serde::Serialize;

/// One of the six steps of a phone's setup, as its newest note says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StepStanding {
    /// Which, by the core's name (`accessibility`, `usage_access`...).
    pub permission: PhonePermission,
    /// On or off, as its newest note says; None while no note said.
    pub on: Option<bool>,
    /// Since when, by the phone's clock.
    pub since: Option<i64>,
}

/// The protection level, as last noted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProtectionStanding {
    /// Off, base or strict.
    pub level: ProtectionLevel,
    /// Since when.
    pub since: i64,
}

/// USB debugging, as last noted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UsbDebuggingStanding {
    /// True when it is on: any PC with a cable could then stop LiMo.
    pub on: bool,
    /// Since when.
    pub since: i64,
}

/// A clock far from the master's, as the master measured it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ClockStanding {
    /// By how many seconds it is off; ahead is positive.
    pub off_by_secs: i64,
    /// Since when it was found far off, by the master's clock, when that is
    /// known.
    pub since: Option<i64>,
}

/// The phone's clock moved against its steady count, as last noted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ClockMovedStanding {
    /// By how many seconds; forward is positive.
    pub by_secs: i64,
    /// True when LiMo followed it.
    pub followed: bool,
    /// When.
    pub at: i64,
}

/// A guardian's PIN opened the guarded pages, as last noted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GuardOpenedStanding {
    /// Until when.
    pub until: i64,
    /// When.
    pub at: i64,
}

/// A guarded page came to the front, as last noted for that page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PageStanding {
    /// Which page.
    pub page: GuardedPage,
    /// The home day.
    pub day: i64,
    /// When.
    pub at: i64,
}

/// How a phone stands now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Standing {
    /// True once a guardian finished setup on the phone. The master paired
    /// it, so until that note comes, setup is not finished.
    pub setup_finished: bool,
    /// When, by the phone's clock.
    pub setup_finished_at: Option<i64>,
    /// The six steps, in the order setup asks for them.
    pub steps: Vec<StepStanding>,
    /// The steps switched off now, in setup order.
    pub steps_off: Vec<PhonePermission>,
    /// The protection level.
    pub protection: Option<ProtectionStanding>,
    /// USB debugging.
    pub usb_debugging: Option<UsbDebuggingStanding>,
    /// Its clock, only when it is far off (more than the hub's five
    /// minutes); None when it is near enough or not measured.
    pub clock: Option<ClockStanding>,
    /// Its own clock's last move.
    pub clock_moved: Option<ClockMovedStanding>,
    /// When it was last seen started again.
    pub restarted_at: Option<i64>,
    /// The guarded pages last opened with the PIN.
    pub guard_opened: Option<GuardOpenedStanding>,
    /// Each guarded page last covered by the guard, in a fixed order.
    pub guarded_pages: Vec<PageStanding>,
    /// True when something here needs a guardian: setup not finished, a step
    /// switched off, USB debugging on, or the clock far off.
    pub needs_guardian: bool,
}

/// One step switched off, with what it costs, in the shape the phone's
/// engine gives the child's own Today.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SwitchedOff {
    /// Which.
    pub permission: PhonePermission,
    /// What it costs, in one sentence.
    pub words: String,
    /// Since when, by the phone's clock.
    pub since: Option<i64>,
}

/// What a phone needs a guardian for now. Nothing needed is an empty `off`,
/// `setup_finished` true, `usb_debugging_on` false and no clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GuardianNeeded {
    /// The steps switched off, in setup order.
    pub off: Vec<SwitchedOff>,
    /// False until a guardian finished setup on the phone.
    pub setup_finished: bool,
    /// True while USB debugging is on.
    pub usb_debugging_on: bool,
    /// How far its clock is off, when it is far off.
    pub clock_off_by_secs: Option<i64>,
}

/// What a step switched off costs, in one sentence: the same words the
/// phone's engine shows the child.
pub fn off_words(permission: PhonePermission) -> &'static str {
    match permission {
        PhonePermission::Notifications => "Notifications are switched off, so LiMo cannot show the time left or a warning.",
        PhonePermission::Phone => "Phone access is switched off, so a lock could hide a call.",
        PhonePermission::UsageAccess => "Usage access is switched off, so LiMo cannot count the time in each app.",
        PhonePermission::Accessibility => "Accessibility is switched off, so LiMo cannot see which app is open.",
        PhonePermission::Battery => "Android may put LiMo to sleep to save battery.",
        PhonePermission::Protection => "Protection is switched off, so LiMo can be removed.",
    }
}

impl Standing {
    /// The standing from a phone's notes in the order they came, its clock
    /// as the master measured it (how far off, ahead positive), and since
    /// when the master found it far off. A clock off by no more than
    /// [`CLOCK_FAR_OFF_SECS`] is not said.
    pub fn of(notes: &[PhoneNote], clock_off_by_secs: Option<i64>, clock_far_off_since: Option<i64>) -> Standing {
        let mut setup_finished_at = None;
        let mut steps: Vec<StepStanding> =
            PhonePermission::ALL.iter().map(|&permission| StepStanding { permission, on: None, since: None }).collect();
        let mut protection = None;
        let mut usb_debugging = None;
        let mut clock_moved = None;
        let mut restarted_at = None;
        let mut guard_opened = None;
        let mut pages: Vec<PageStanding> = Vec::new();
        for note in notes {
            match *note {
                PhoneNote::SetupFinished { at } => setup_finished_at = Some(at),
                PhoneNote::PermissionOff { permission, at } | PhoneNote::PermissionOn { permission, at } => {
                    let on = matches!(note, PhoneNote::PermissionOn { .. });
                    if let Some(step) = steps.iter_mut().find(|s| s.permission == permission) {
                        step.on = Some(on);
                        step.since = Some(at);
                    }
                }
                PhoneNote::Protection { level, at } => protection = Some(ProtectionStanding { level, since: at }),
                PhoneNote::UsbDebugging { on, at } => usb_debugging = Some(UsbDebuggingStanding { on, since: at }),
                PhoneNote::GuardedPageOpened { page, day, at } => {
                    pages.retain(|p| p.page != page);
                    pages.push(PageStanding { page, day, at });
                }
                PhoneNote::GuardOpened { until, at } => guard_opened = Some(GuardOpenedStanding { until, at }),
                PhoneNote::Restarted { at } => restarted_at = Some(at),
                PhoneNote::ClockMoved { by_secs, followed, at } => clock_moved = Some(ClockMovedStanding { by_secs, followed, at }),
            }
        }
        pages.sort_by_key(|p| match p.page {
            GuardedPage::DeviceAdmin => 0,
            GuardedPage::Accessibility => 1,
        });
        let clock = clock_off_by_secs
            .filter(|off| off.saturating_abs() > CLOCK_FAR_OFF_SECS)
            .map(|off_by_secs| ClockStanding { off_by_secs, since: clock_far_off_since });
        let steps_off: Vec<PhonePermission> = steps.iter().filter(|s| s.on == Some(false)).map(|s| s.permission).collect();
        let usb_on = usb_debugging.is_some_and(|u| u.on);
        let needs_guardian = setup_finished_at.is_none() || !steps_off.is_empty() || usb_on || clock.is_some();
        Standing {
            setup_finished: setup_finished_at.is_some(),
            setup_finished_at,
            steps,
            steps_off,
            protection,
            usb_debugging,
            clock,
            clock_moved,
            restarted_at,
            guard_opened,
            guarded_pages: pages,
            needs_guardian,
        }
    }

    /// What it needs a guardian for, in the shape the screens read.
    pub fn guardian_needed(&self) -> GuardianNeeded {
        GuardianNeeded {
            off: self
                .steps
                .iter()
                .filter(|s| s.on == Some(false))
                .map(|s| SwitchedOff { permission: s.permission, words: off_words(s.permission).to_string(), since: s.since })
                .collect(),
            setup_finished: self.setup_finished,
            usb_debugging_on: self.usb_debugging.is_some_and(|u| u.on),
            clock_off_by_secs: self.clock.map(|c| c.off_by_secs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_on(at: i64) -> Vec<PhoneNote> {
        PhonePermission::ALL.iter().map(|&permission| PhoneNote::PermissionOn { permission, at }).collect()
    }

    #[test]
    fn a_step_switched_off_and_later_on_is_on_and_the_newest_note_of_each_kind_wins() {
        let mut notes = all_on(100);
        notes.push(PhoneNote::SetupFinished { at: 110 });
        notes.push(PhoneNote::PermissionOff { permission: PhonePermission::Accessibility, at: 200 });
        notes.push(PhoneNote::PermissionOn { permission: PhonePermission::Accessibility, at: 300 });
        notes.push(PhoneNote::PermissionOff { permission: PhonePermission::Battery, at: 400 });
        notes.push(PhoneNote::UsbDebugging { on: true, at: 500 });
        notes.push(PhoneNote::UsbDebugging { on: false, at: 600 });
        let standing = Standing::of(&notes, None, None);
        assert_eq!(standing.steps_off, vec![PhonePermission::Battery]);
        let accessibility = standing.steps.iter().find(|s| s.permission == PhonePermission::Accessibility).unwrap();
        assert_eq!((accessibility.on, accessibility.since), (Some(true), Some(300)));
        assert_eq!(standing.usb_debugging, Some(UsbDebuggingStanding { on: false, since: 600 }));
        assert!(standing.setup_finished && standing.needs_guardian, "battery is off");
        let needed = standing.guardian_needed();
        assert_eq!(needed.off.len(), 1);
        assert_eq!((needed.off[0].permission, needed.off[0].since), (PhonePermission::Battery, Some(400)));
        assert_eq!(needed.off[0].words, "Android may put LiMo to sleep to save battery.");
        assert!(!needed.usb_debugging_on);
    }

    #[test]
    fn the_steps_are_the_six_in_setup_order_and_a_step_never_noted_is_neither_on_nor_off() {
        let standing = Standing::of(&[PhoneNote::PermissionOff { permission: PhonePermission::Protection, at: 5 }], None, None);
        let order: Vec<PhonePermission> = standing.steps.iter().map(|s| s.permission).collect();
        assert_eq!(order, PhonePermission::ALL.to_vec());
        assert!(standing.steps[..5].iter().all(|s| s.on.is_none() && s.since.is_none()));
        assert_eq!(standing.steps_off, vec![PhonePermission::Protection]);
        // No note of setup: it is not finished, and that needs a guardian.
        assert!(!standing.setup_finished && standing.setup_finished_at.is_none() && standing.needs_guardian);
        let all_off: Vec<PhoneNote> = PhonePermission::ALL
            .iter()
            .rev()
            .map(|&permission| PhoneNote::PermissionOff { permission, at: 1 })
            .collect();
        assert_eq!(Standing::of(&all_off, None, None).steps_off, PhonePermission::ALL.to_vec(), "in setup order, whatever order they came in");
    }

    #[test]
    fn all_well_needs_no_guardian() {
        let mut notes = all_on(10);
        notes.push(PhoneNote::SetupFinished { at: 20 });
        notes.push(PhoneNote::Protection { level: ProtectionLevel::Base, at: 20 });
        notes.push(PhoneNote::UsbDebugging { on: false, at: 30 });
        let standing = Standing::of(&notes, Some(-40), None);
        assert!(!standing.needs_guardian);
        assert_eq!(standing.clock, None, "40 seconds off is near enough");
        assert_eq!(standing.protection, Some(ProtectionStanding { level: ProtectionLevel::Base, since: 20 }));
        assert_eq!(
            standing.guardian_needed(),
            GuardianNeeded { off: vec![], setup_finished: true, usb_debugging_on: false, clock_off_by_secs: None }
        );
    }

    #[test]
    fn usb_debugging_on_and_a_clock_far_off_need_a_guardian_and_say_since_when() {
        let mut notes = all_on(10);
        notes.push(PhoneNote::SetupFinished { at: 20 });
        notes.push(PhoneNote::UsbDebugging { on: true, at: 70 });
        let standing = Standing::of(&notes, Some(540), Some(1_000));
        assert!(standing.needs_guardian);
        assert_eq!(standing.clock, Some(ClockStanding { off_by_secs: 540, since: Some(1_000) }));
        let needed = standing.guardian_needed();
        assert!(needed.usb_debugging_on && needed.off.is_empty());
        assert_eq!(needed.clock_off_by_secs, Some(540));
        // Exactly the hub's five minutes is not far off; one more second is.
        assert!(Standing::of(&notes[..7], Some(-300), None).clock.is_none());
        assert!(Standing::of(&notes[..7], Some(-301), None).clock.is_some());
    }

    #[test]
    fn the_other_notes_are_kept_as_their_newest() {
        let notes = vec![
            PhoneNote::Restarted { at: 1 },
            PhoneNote::Restarted { at: 9 },
            PhoneNote::ClockMoved { by_secs: -3_600, followed: true, at: 4 },
            PhoneNote::GuardedPageOpened { page: GuardedPage::Accessibility, day: 3, at: 5 },
            PhoneNote::GuardedPageOpened { page: GuardedPage::DeviceAdmin, day: 3, at: 6 },
            PhoneNote::GuardedPageOpened { page: GuardedPage::Accessibility, day: 4, at: 7 },
            PhoneNote::GuardOpened { until: 900, at: 8 },
        ];
        let standing = Standing::of(&notes, None, None);
        assert_eq!(standing.restarted_at, Some(9));
        assert_eq!(standing.clock_moved, Some(ClockMovedStanding { by_secs: -3_600, followed: true, at: 4 }));
        assert_eq!(
            standing.guarded_pages,
            vec![
                PageStanding { page: GuardedPage::DeviceAdmin, day: 3, at: 6 },
                PageStanding { page: GuardedPage::Accessibility, day: 4, at: 7 }
            ]
        );
        assert_eq!(standing.guard_opened, Some(GuardOpenedStanding { until: 900, at: 8 }));
    }
}
