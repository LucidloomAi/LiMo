//! The hub's public data: plain structs and enums of strings, whole numbers,
//! options, vectors and the core's own types, so that they can be exposed to
//! other languages later as they are.

use std::fmt;

use limo_core::{DeviceKind, GuardianMode, Grant, RequestView};
use serde::{Deserialize, Serialize};

/// A child's age, in four bands. The hub only keeps it; later pieces may use
/// it to suggest starting rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgeBand {
    /// Under 6.
    UnderSix,
    /// 6 to 9.
    SixToNine,
    /// 10 to 12.
    TenToTwelve,
    /// 13 to 17.
    ThirteenToSeventeen,
}

impl AgeBand {
    /// Every band, youngest first.
    pub const ALL: [AgeBand; 4] =
        [AgeBand::UnderSix, AgeBand::SixToNine, AgeBand::TenToTwelve, AgeBand::ThirteenToSeventeen];

    /// The band in words, for a screen: "6 to 9".
    pub fn label(self) -> &'static str {
        match self {
            AgeBand::UnderSix => "under 6",
            AgeBand::SixToNine => "6 to 9",
            AgeBand::TenToTwelve => "10 to 12",
            AgeBand::ThirteenToSeventeen => "13 to 17",
        }
    }

    /// The name it is stored under.
    pub(crate) fn key(self) -> &'static str {
        match self {
            AgeBand::UnderSix => "under_six",
            AgeBand::SixToNine => "six_to_nine",
            AgeBand::TenToTwelve => "ten_to_twelve",
            AgeBand::ThirteenToSeventeen => "thirteen_to_seventeen",
        }
    }

    /// The band stored under `key`, if any.
    pub(crate) fn from_key(key: &str) -> Option<AgeBand> {
        AgeBand::ALL.into_iter().find(|b| b.key() == key)
    }
}

/// A child in the family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Child {
    /// The child's id, made by the hub. It is the child id in every report
    /// and grant.
    pub child_id: String,
    /// The name the guardian gave.
    pub name: String,
    /// The age band the guardian chose.
    pub age_band: AgeBand,
    /// When the child was added, unix seconds.
    pub added_at: i64,
}

/// A device paired with a child.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// The device's id, made by the hub at pairing.
    pub device_id: String,
    /// The child it is paired with.
    pub child_id: String,
    /// The name the guardian gave it.
    pub name: String,
    /// The master PC itself, or a device that can be carried away.
    pub kind: DeviceKind,
    /// The platform label the service gave at pairing, such as "windows" or
    /// "android".
    pub platform: String,
    /// When it was paired, unix seconds.
    pub paired_at: i64,
    /// When the master last heard from it, unix seconds (pairing counts).
    pub last_heard_at: Option<i64>,
}

/// What the service hands to a device it has just paired. Its `Debug` hides
/// the code secret.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pairing {
    /// The child.
    pub child_id: String,
    /// The new device's id.
    pub device_id: String,
    /// The first grant: the device makes its state from it with
    /// `DeviceState::from_pairing`.
    pub welcome: Grant,
    /// The device's code secret, 32 random bytes: the device takes it with
    /// `DeviceState::set_code_secret`. Send it only over the encrypted
    /// pairing link, and never show it or log it.
    pub code_secret: Vec<u8>,
}

impl fmt::Debug for Pairing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pairing")
            .field("child_id", &self.child_id)
            .field("device_id", &self.device_id)
            .field("welcome", &self.welcome)
            .field("code_secret", &format_args!("hidden ({} bytes)", self.code_secret.len()))
            .finish()
    }
}

/// What became of a password typed by a guardian.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasswordOutcome {
    /// Right. The wrong guesses are cleared.
    Right,
    /// Wrong. Counted as a wrong guess.
    Wrong {
        /// Wrong guesses since the last right password.
        wrong_guesses: u32,
        /// When the next try may be made, if a wait is now in force.
        wait_until: Option<i64>,
    },
    /// Not tried: wrong guesses have set a wait. Not counted.
    MustWait {
        /// When the next try may be made, unix seconds.
        until: i64,
    },
}

/// How a Windows account on the master PC is linked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountLink {
    /// The account is this child's: LiMo limits it.
    Child {
        /// The child.
        child_id: String,
    },
    /// The account is not limited, such as a guardian's own.
    NotLimited,
}

/// A Windows account the guardian has linked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// The account's identifier, opaque to the hub (the service may use the
    /// account's security identifier).
    pub account_id: String,
    /// The name to show for it.
    pub display_name: String,
    /// How it is linked.
    pub link: AccountLink,
}

/// Which child, if any, a Windows account is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStanding {
    /// The account is this child's.
    Child {
        /// The child.
        child_id: String,
    },
    /// The guardian marked it as not limited.
    NotLimited,
    /// The guardian has not linked it. The service decides what to do, such
    /// as asking the guardian.
    NotLinked,
}

/// A device's part of a child's day, for the guardian's screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceToday {
    /// The device.
    pub device_id: String,
    /// Its name.
    pub name: String,
    /// The master PC itself, or a device that can be carried away.
    pub kind: DeviceKind,
    /// Its platform label.
    pub platform: String,
    /// When the master last heard from it, unix seconds.
    pub last_heard_at: Option<i64>,
    /// True when it is in use: the master heard from it within
    /// [`crate::IN_USE_HEARD_SECS`], and that report carried use that ended
    /// at most [`crate::IN_USE_FRESH_SECS`] before it was sent.
    pub in_use: bool,
    /// Its counted use today as the master knows it, in seconds.
    pub used_secs: u64,
    /// What it may still spend today without telling the master: its lease,
    /// its away allowance and time added on it, less what it used. None when
    /// it may hold rules with no limit today.
    pub holds_secs: Option<u64>,
    /// Time it asked for and could not be given yet, in seconds.
    pub waiting_secs: u64,
    /// Away time it is giving back for a device that is waiting, in seconds.
    pub giving_back_secs: u64,
    /// Time added on it today by codes and the PIN, in seconds.
    pub added_secs: u64,
    /// Wrong guesses of codes and the PIN on it, as it last reported.
    pub wrong_guesses: u32,
    /// When an unlock set with the PIN on it ends, if one is in force.
    pub pin_unlocked_until: Option<i64>,
    /// How far its clock was from the master's at its newest report, in
    /// seconds (ahead is positive), when more than
    /// [`crate::CLOCK_FAR_OFF_SECS`]. None when it is close enough.
    pub clock_off_by_secs: Option<i64>,
}

/// One child's day, for the guardian's screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildToday {
    /// The child.
    pub child_id: String,
    /// The child's name.
    pub name: String,
    /// The child's age band.
    pub age_band: AgeBand,
    /// Today's home day number, by the child's rules.
    pub day: i64,
    /// Today's budget in seconds; None when today has no limit.
    pub budget_secs: Option<u64>,
    /// Extra time for today from every source, in seconds.
    pub extra_secs: u64,
    /// Counted use today on all devices, in seconds.
    pub used_secs: u64,
    /// Budget plus extra, less use, in seconds, never below zero; None when
    /// today has no limit.
    pub left_secs: Option<u64>,
    /// By how much use passed the budget plus extra, in seconds: above zero
    /// after a cut in the budget, or after use during an unlock.
    pub over_secs: u64,
    /// The guardian's lock or unlock in force now, as set at the master;
    /// `Nothing` when none is set or it has ended.
    pub guardian: GuardianMode,
    /// How many requests for more time wait for an answer.
    pub waiting_requests: u32,
    /// Every paired device, in the order they were paired.
    pub devices: Vec<DeviceToday>,
}

/// Time used in one app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppUsage {
    /// The app identifier, as the device reported it. May be empty when the
    /// device did not know it.
    pub app_id: String,
    /// All time in the app, in seconds.
    pub secs: u64,
    /// The part of it that counted against the budget, in seconds: none for
    /// an always allowed app.
    pub counted_secs: u64,
}

/// Time used on one device, app by app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceUsage {
    /// The device.
    pub device_id: String,
    /// Its name.
    pub name: String,
    /// True when it has since been removed from the family.
    pub removed: bool,
    /// All time on it, in seconds.
    pub secs: u64,
    /// Its apps, most used first.
    pub apps: Vec<AppUsage>,
}

/// A child's usage over a range of home days. LiMo records which app was used
/// and for how long, and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageView {
    /// The child.
    pub child_id: String,
    /// The first home day, included.
    pub from_day: i64,
    /// The last home day, included.
    pub to_day: i64,
    /// All time on all devices, in seconds.
    pub secs: u64,
    /// The part of it that counted against the budget, in seconds.
    pub counted_secs: u64,
    /// Every app on all devices together, most used first.
    pub apps: Vec<AppUsage>,
    /// Each device, most used first.
    pub devices: Vec<DeviceUsage>,
}

/// Extra time a guardian gave at the master.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtraGiven {
    /// The id it is recorded under.
    pub id: String,
    /// True when it was given now; false when a tap with the same id had
    /// already given it, and nothing changed.
    pub given_now: bool,
}

/// A request for more time that waits for an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitingRequest {
    /// The child who asked.
    pub child_id: String,
    /// The child's name.
    pub child_name: String,
    /// The name of the device it was asked on, or its id if it has none.
    pub device_name: String,
    /// The request, as the core keeps it.
    pub request: RequestView,
}

/// What happened, for a guardian to see later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EventKind {
    /// A device was paired.
    DevicePaired {
        /// The device.
        device_id: String,
        /// Its name.
        name: String,
    },
    /// A device was removed.
    DeviceRemoved {
        /// The device.
        device_id: String,
        /// Its name.
        name: String,
    },
    /// A child's rules were changed.
    RulesChanged {
        /// The new version.
        version: u32,
    },
    /// The guardian PIN was set or changed.
    PinSet,
    /// The guardian PIN was removed.
    PinRemoved,
    /// The guardian password was changed.
    PasswordChanged,
    /// The child's devices were locked from the master.
    Locked {
        /// Until when, or None for until lifted.
        until: Option<i64>,
    },
    /// The child's devices were unlocked from the master.
    Unlocked {
        /// Until when.
        until: i64,
    },
    /// A lock or unlock from the master was lifted.
    Lifted,
    /// A device reported more wrong guesses of codes or the PIN than before.
    WrongGuesses {
        /// The device.
        device_id: String,
        /// Wrong guesses on it since the last right PIN.
        count: u32,
    },
    /// A device's clock was found far from the master's.
    ClockFarOff {
        /// The device.
        device_id: String,
        /// By how many seconds it is off; ahead is positive.
        off_by_secs: i64,
    },
    /// A child's request for more time reached the master. It waits for an
    /// answer ([`crate::Hub::waiting_requests`]).
    RequestArrived {
        /// The device it was made on.
        device_id: String,
        /// The request.
        request_id: String,
        /// The minutes asked for.
        minutes: u32,
    },
}

impl EventKind {
    /// The name it is stored under.
    pub(crate) fn key(&self) -> &'static str {
        match self {
            EventKind::DevicePaired { .. } => "device_paired",
            EventKind::DeviceRemoved { .. } => "device_removed",
            EventKind::RulesChanged { .. } => "rules_changed",
            EventKind::PinSet => "pin_set",
            EventKind::PinRemoved => "pin_removed",
            EventKind::PasswordChanged => "password_changed",
            EventKind::Locked { .. } => "locked",
            EventKind::Unlocked { .. } => "unlocked",
            EventKind::Lifted => "lifted",
            EventKind::WrongGuesses { .. } => "wrong_guesses",
            EventKind::ClockFarOff { .. } => "clock_far_off",
            EventKind::RequestArrived { .. } => "request_arrived",
        }
    }
}

/// One event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Grows with every event; never reused.
    pub id: i64,
    /// When, by the master's clock, unix seconds.
    pub at: i64,
    /// The child it is about, or None for the whole family.
    pub child_id: Option<String>,
    /// What happened.
    pub what: EventKind,
}

/// What [`crate::Hub::tidy`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TidyReport {
    /// Slices folded into daily totals and removed.
    pub slices_folded: u64,
    /// Records of one time codes made on earlier days, dropped.
    pub codes_dropped: u64,
    /// Old events dropped.
    pub events_dropped: u64,
}
