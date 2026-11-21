//! The hub's public data: plain structs and enums of strings, whole numbers,
//! options, vectors and the core's own types, so that they can be exposed to
//! other languages later as they are.

use std::fmt;

use ctutils::CtEq;
use limo_core::{DeviceKind, DeviceState, GuardianMode, Grant, RequestView};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::KEY_BYTES;
use crate::notes::{PhoneNote, PhonePermission, ProtectionLevel};

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

/// What became of a password typed by a guardian, or of the PIN typed on the
/// master PC ([`PinOutcome`]).
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

/// What became of the guardian PIN typed on the master PC
/// ([`crate::Hub::check_pin`]): right, wrong (counted, with the wait it set),
/// or not tried during a wait. The same as for the password.
pub type PinOutcome = PasswordOutcome;

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
    /// The guardian password was reset without the old one, by an
    /// administrator of the master PC ([`crate::Hub::reset_password`]).
    PasswordReset,
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
    /// A Windows account on the master PC that is linked to no child, and
    /// not marked as not limited, was in use. Noted the first time each day
    /// ([`crate::Hub::note_unlinked_account_used`]).
    UnlinkedAccountUsed {
        /// The account's identifier.
        account_id: String,
        /// The account's name as Windows shows it, or empty when it could
        /// not be taken.
        account_name: String,
        /// The day, by the master PC's own calendar, counted from 1 January
        /// 1970.
        day: i64,
    },
    /// The master PC's clock moved against its steady count between two
    /// looks: it was set, or corrected. No counted time was gained or lost
    /// by it.
    ClockJumped {
        /// By how many seconds; forward is positive.
        by_secs: i64,
    },
    /// The master PC's time zone changed. Days and blocked hours still
    /// follow each child's home zone, in the rules.
    TimeZoneChanged {
        /// The zone before, as seconds from UTC.
        from_offset_secs: i32,
        /// The zone now, as seconds from UTC.
        to_offset_secs: i32,
    },
    /// A blocked program was closed on the master PC. Noted the first time
    /// each day for each program ([`crate::Hub::note_blocked_program_closed`]).
    BlockedProgramClosed {
        /// The child's device on the master PC.
        device_id: String,
        /// The program, as LiMo names it.
        program: String,
        /// The child's home day.
        day: i64,
    },
    /// A phone noted something for the guardian, and its report carried it
    /// ([`crate::Hub::handle_report`]). Each note is recorded once, however
    /// often the phone sends it.
    PhoneNoted {
        /// The phone.
        device_id: String,
        /// What it noted.
        note: PhoneNote,
    },
    /// A phone removed from the family was told so, and let go of its limits
    /// ([`crate::Hub::note_released`]).
    DeviceReleased {
        /// The phone.
        device_id: String,
        /// Its name.
        name: String,
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
            EventKind::PasswordReset => "password_reset",
            EventKind::Locked { .. } => "locked",
            EventKind::Unlocked { .. } => "unlocked",
            EventKind::Lifted => "lifted",
            EventKind::WrongGuesses { .. } => "wrong_guesses",
            EventKind::ClockFarOff { .. } => "clock_far_off",
            EventKind::RequestArrived { .. } => "request_arrived",
            EventKind::UnlinkedAccountUsed { .. } => "unlinked_account_used",
            EventKind::ClockJumped { .. } => "clock_jumped",
            EventKind::TimeZoneChanged { .. } => "time_zone_changed",
            EventKind::BlockedProgramClosed { .. } => "blocked_program_closed",
            EventKind::PhoneNoted { .. } => "phone_noted",
            EventKind::DeviceReleased { .. } => "device_released",
        }
    }
}

/// What happens on the master PC in a Windows account that is linked to no
/// child and not marked as not limited.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnlinkedAccounts {
    /// It is used freely, and the guardian is told once a day. The default.
    #[default]
    Free,
    /// It is locked until a guardian links it.
    Locked,
}

impl UnlinkedAccounts {
    /// The name it is stored under.
    pub(crate) fn key(self) -> &'static str {
        match self {
            UnlinkedAccounts::Free => "free",
            UnlinkedAccounts::Locked => "locked",
        }
    }

    /// The value stored under `key`, if any.
    pub(crate) fn from_key(key: &str) -> Option<UnlinkedAccounts> {
        [UnlinkedAccounts::Free, UnlinkedAccounts::Locked].into_iter().find(|u| u.key() == key)
    }
}

/// The settings of LiMo on the master PC that a guardian can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcSettings {
    /// After how many minutes with no keyboard or mouse input the count
    /// pauses, unless something plays: in full screen, with sound, or
    /// keeping the screen awake. 1 to 60, 3 unless changed.
    pub idle_minutes: u32,
    /// What happens in an account that is not linked.
    pub unlinked_accounts: UnlinkedAccounts,
}

/// What the hub keeps for one child's device on the master PC: the device's
/// state, which the agent on the master PC keeps for it, and the agent's own
/// notes. Its `Debug` hides the code secret and the PIN record, as the
/// core's `DeviceState` does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterPcRecord {
    /// The child.
    pub child_id: String,
    /// The child's device on the master PC.
    pub device_id: String,
    /// The device's state, as the core keeps it.
    pub state: DeviceState,
    /// The agent's own notes, kept as given. Empty when the agent has
    /// written none yet.
    pub notes: String,
    /// The child's book revision when this was read: it grows by one every
    /// time the book is written.
    pub book_revision: u64,
}

/// What [`crate::Hub::exchange_on_master_pc`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterPcExchange {
    /// The grant the device took.
    pub grant: Grant,
    /// The child's book revision after the exchange.
    pub book_revision: u64,
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
    /// Pairing offers dropped, a day after they expired.
    pub offers_dropped: u64,
}

/// The master's own private key for the link: 32 random bytes, made when
/// the family is made, kept in the family's file. The link makes the public
/// key from it (`KeyPair::from_private`). Its `Debug` hides it, and it is
/// wiped from memory when dropped.
#[derive(Clone)]
pub struct MasterKey {
    pub(crate) private: Zeroizing<[u8; KEY_BYTES]>,
}

impl MasterKey {
    /// The private key, for the link. Never show it or log it.
    pub fn expose_private(&self) -> &[u8; KEY_BYTES] {
        &self.private
    }
}

impl PartialEq for MasterKey {
    /// Compared in constant time.
    fn eq(&self, other: &MasterKey) -> bool {
        self.private[..].ct_eq(&other.private[..]).to_bool()
    }
}

impl Eq for MasterKey {}

impl fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MasterKey(hidden)")
    }
}

/// A pairing offer the service made for a child, as the hub keeps it. Its
/// `Debug` hides the offer's text, which holds its secret, and the text is
/// wiped from memory when dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct OfferRecord {
    /// The offer's id, 16 lowercase hex digits.
    pub offer_id: String,
    /// The child a phone that uses it is paired with.
    pub child_id: String,
    /// The offer's text form, as the link wrote it. It holds the offer's
    /// secret: hand it only to the link, and never show it or log it.
    pub text: Zeroizing<String>,
    /// When it was made, unix seconds by the master's clock.
    pub made_at: i64,
    /// When it expires: a phone may pair with it until then.
    pub expires_at: i64,
    /// When a phone paired with it, if one did. It works once.
    pub used_at: Option<i64>,
    /// The device paired with it, if one was.
    pub device_id: Option<String>,
}

impl OfferRecord {
    /// True once a phone has paired with it.
    pub fn is_used(&self) -> bool {
        self.used_at.is_some()
    }

    /// True once it has expired at `now`, by the master's clock.
    pub fn is_expired(&self, now: i64) -> bool {
        now > self.expires_at
    }
}

impl fmt::Debug for OfferRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OfferRecord")
            .field("offer_id", &self.offer_id)
            .field("child_id", &self.child_id)
            .field("text", &format_args!("hidden"))
            .field("made_at", &self.made_at)
            .field("expires_at", &self.expires_at)
            .field("used_at", &self.used_at)
            .field("device_id", &self.device_id)
            .finish()
    }
}

/// Which form of the phone app paired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhoneForm {
    /// The child's form, which LiMo limits.
    Child,
    /// The guardian's form. Not paired by this version.
    Guardian,
}

impl PhoneForm {
    /// The name it is stored under.
    pub(crate) fn key(self) -> &'static str {
        match self {
            PhoneForm::Child => "child",
            PhoneForm::Guardian => "guardian",
        }
    }

    /// The form stored under `key`, if any.
    pub(crate) fn from_key(key: &str) -> Option<PhoneForm> {
        [PhoneForm::Child, PhoneForm::Guardian].into_iter().find(|f| f.key() == key)
    }
}

/// What a phone said about itself when it paired with an offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneRequest {
    /// The name it gave, such as "Asha's phone". The hub keeps at most 40
    /// characters of it.
    pub name: String,
    /// Which form of the app is pairing.
    pub form: PhoneForm,
    /// Its platform label, "android".
    pub platform: String,
    /// Android's API level, such as 33 for Android 13, if it said.
    pub android_version: Option<u32>,
}

/// A phone the link may meet, with its key: every phone paired with an
/// offer, removed ones too, so that a removed phone can be told so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkPhone {
    /// Its device id.
    pub device_id: String,
    /// The child it is paired with.
    pub child_id: String,
    /// Its static public key, from the pairing. Public: it is not a secret.
    pub link_key: [u8; KEY_BYTES],
    /// True once it was removed from the family.
    pub removed: bool,
}

/// One app on a phone, as its list gave it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PhoneApp {
    /// The package name, such as `com.example.game`: the app id that its use
    /// and the rules name.
    pub package: String,
    /// The label the phone shows for it, such as "Game".
    pub label: String,
}

/// A phone's list of apps, as the hub keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneApps {
    /// The phone.
    pub device_id: String,
    /// Its apps, in the order the phone listed them.
    pub apps: Vec<PhoneApp>,
    /// The hash the phone gave the list, to see at a glance whether a later
    /// list changed.
    pub hash: String,
    /// When the list was kept, by the master's clock.
    pub kept_at: i64,
}

/// What the master knows of one phone, for the guardian's screens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneInfo {
    /// The phone's device id.
    pub device_id: String,
    /// The child it is paired with.
    pub child_id: String,
    /// Its name.
    pub name: String,
    /// When it was paired.
    pub paired_at: i64,
    /// When the master last heard from it.
    pub last_heard_at: Option<i64>,
    /// Which form of the app it runs.
    pub form: PhoneForm,
    /// Android's API level, as it said when it paired.
    pub android_version: Option<u32>,
    /// True when the link knows its key; false for a phone paired before the
    /// link, which cannot open a session and must be paired again.
    pub has_link_key: bool,
    /// When setup was finished on it, if it was.
    pub setup_finished_at: Option<i64>,
    /// The permissions whose newest note says they are on, in setup order.
    pub switched_on: Vec<PhonePermission>,
    /// The permissions whose newest note says they are off, in setup order.
    pub switched_off: Vec<PhonePermission>,
    /// Its protection level, as last noted.
    pub protection: Option<ProtectionLevel>,
    /// Whether USB debugging is on, as last noted.
    pub usb_debugging_on: Option<bool>,
    /// The newest note of each kind it sent: one for each kind, and for each
    /// permission and guarded page.
    pub notes: Vec<PhoneNote>,
    /// How many apps its newest list holds, if it sent one.
    pub apps: Option<u32>,
    /// When it was removed from the family, if it was.
    pub removed_at: Option<i64>,
    /// When, after it was removed, it was told so and let go of its limits.
    pub released_at: Option<i64>,
}
