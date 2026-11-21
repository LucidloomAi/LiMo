//! The orders: what the engine asks the host to do, as plain data, with
//! every word the phone shows.
//!
//! Orders are levels, not changes: "cover the screen with this lock", "the
//! time left reads this". Carrying out the same order twice does no harm,
//! and an order missed is given again at the next call. The things given in
//! one call only are a warning and the notes; each says what it is about.

use limo_core::{ProtectionLevel, RequestView, TryOutcome, WarningCause};
use serde::{Deserialize, Serialize};

use crate::facts::{GuardedPage, Permission};

/// Everything to be done after one call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneOrders {
    /// What to draw over the screen now.
    pub cover: Cover,
    /// The time left, for LiMo's notification and the child's own screen.
    pub time_left: TimeLeft,
    /// A warning to show now. It comes in one call only.
    pub warning: Option<WarningShown>,
    /// True when the host should send a report to the master now: call
    /// [`crate::PhoneEngine::outgoing_report`], send it, and hand the answer
    /// to [`crate::PhoneEngine::take_grant`] (or call
    /// [`crate::PhoneEngine::lost_contact`] when none comes). Only ever true
    /// while the master is in reach.
    pub send_report: bool,
    /// True when the host must keep the state now: write
    /// [`crate::PhoneEngine::save`] to the state file before carrying out
    /// anything else.
    pub save_state: bool,
    /// Things the guardian should know, noted in this call: a clock moved, a
    /// restart, a permission switched on or off, the protection level, USB
    /// debugging, a blocked app opened, a guarded page opened. The engine
    /// keeps them too, and carries the kinds the
    /// master takes in its reports until a grant confirms them
    /// ([`PhoneNote::for_master`]). (Setup finished is noted by
    /// [`crate::PhoneEngine::finish_setup`], not in a call.)
    pub notes: Vec<PhoneNote>,
    /// Call again within this many seconds if nothing happens (at least 1):
    /// until the next warning point, the lock, blocked hours starting or
    /// ending, a lock lifting, the day changing. Count it with the elapsed
    /// count, not the wall clock. While the screen is in use, call once a
    /// second anyway; call at once whenever a fact changes; while the master
    /// is in reach, call at least once a minute.
    pub next_check_in_secs: u64,
}

/// What to draw over the screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "cover")]
pub enum Cover {
    /// Nothing: the app in front may be used.
    Nothing,
    /// A blocked app is in front: cover it, and send the child to the home
    /// screen.
    Block {
        /// The blocked app's package name.
        package: String,
        /// Why, in a few words, naming the app by its label when the host
        /// gave one: "Game Launcher is not allowed.", or "This app is not
        /// allowed.".
        title: String,
        /// Why, in plain words: "It is blocked by your family's rules.".
        message: String,
        /// True: send the child to the home screen.
        go_home: bool,
        /// True while a call is in progress: offer a way back to it.
        call_on: bool,
        /// The words for that way back, "Back to the call", while a call is
        /// in progress; None otherwise.
        back_to_call: Option<String>,
    },
    /// The phone is locked: cover whatever is in front with the lock. The
    /// apps the lock names stay usable: the child opens them from it, and
    /// while one is in front the cover is [`Cover::Nothing`].
    Lock(Box<LockScreen>),
    /// A page that could switch LiMo off or remove it is in front: cover it
    /// with the guard, which offers the guardian PIN only. A right PIN opens
    /// the guarded pages for [`crate::GUARD_OPEN_SECS`]
    /// ([`crate::PhoneEngine::try_guard_pin`]). A lock or a block screen that
    /// covers the same moment comes first.
    Guard(Box<GuardScreen>),
}

/// What the guard shows: laid out as the lock is, with one way out, the
/// guardian PIN, and nothing about time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardScreen {
    /// The page it covers.
    pub page: GuardedPage,
    /// The small stamp: "Protected".
    pub stamp: String,
    /// Why, in a few words: "A guardian's PIN is needed here.".
    pub title: String,
    /// Why, in plain words: "These settings can switch LiMo off or remove
    /// it from this phone.".
    pub message: String,
    /// True when a guardian can type the PIN: the phone holds a PIN record.
    pub can_use_pin: bool,
    /// When wrong guesses have set a wait before the next code or PIN try:
    /// the moment it ends. The same wait as the lock's.
    pub try_wait_until: Option<i64>,
    /// The wait in words: "Too many wrong tries. The next try is at 9:47
    /// pm.".
    pub wait_words: Option<String>,
    /// The PIN as the one row: "Guardian PIN" / "Opens these settings for
    /// 5 minutes", or "Wait until 9:47 pm" while it must wait. Empty with no
    /// PIN record; then `nothing_to_do` says why.
    pub ways: Vec<WayRow>,
    /// What the guard says when it offers no way out: a line in bold and one
    /// under it.
    pub nothing_to_do: Option<(String, String)>,
    /// True while a call is in progress: offer a way back to it.
    pub call_on: bool,
    /// The words for that way back, "Back to the call", while a call is in
    /// progress; None otherwise.
    pub back_to_call: Option<String>,
}

/// What the lock shows, and the ways out it offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockScreen {
    /// Why it is locked.
    pub why: LockWhy,
    /// The small stamp: "Locked", or "One moment" while more time comes.
    pub stamp: String,
    /// Why, in a few words, for the top of the lock.
    pub title: String,
    /// Why, in plain words.
    pub message: String,
    /// When the phone opens again if nothing changes, unix seconds by LiMo's
    /// clock, when it is known.
    pub until: Option<i64>,
    /// When it opens again, in words: "Tomorrow, 7:00 am", or, with no time
    /// known, "When a guardian unlocks it". None when nothing can be said.
    pub until_words: Option<String>,
    /// The apps that stay usable under the lock (the rules' always allowed
    /// list), by package name, for the host to show as tiles. The phone, the
    /// emergency call and LiMo itself are always usable besides.
    pub allowed_packages: Vec<String>,
    /// True when the child may ask for more time.
    pub can_ask_for_time: bool,
    /// True when a one time code can be typed. Offered only for a lock that
    /// time can open: a code adds time, and time does not open blocked hours
    /// or a guardian's lock.
    pub can_use_code: bool,
    /// True when a guardian can type the PIN.
    pub can_use_pin: bool,
    /// When wrong guesses have set a wait before the next code or PIN try:
    /// the moment it ends.
    pub try_wait_until: Option<i64>,
    /// The wait in words: "Too many wrong tries. The next try is at 9:47
    /// pm.".
    pub wait_words: Option<String>,
    /// The child's newest request for more time today, with its answer.
    pub latest_request: Option<RequestView>,
    /// The same in words.
    pub request_line: Option<RequestLine>,
    /// The ways out offered, in order, with their words. Empty when none is
    /// offered; then `nothing_to_do` says why.
    pub ways: Vec<WayRow>,
    /// What the lock says when it offers no way out: a line in bold and one
    /// under it.
    pub nothing_to_do: Option<(String, String)>,
    /// True while a call is in progress: the lock covers the app in front
    /// all the same, and offers a way back to the call.
    pub call_on: bool,
    /// The words for that way back, "Back to the call", while a call is in
    /// progress; None otherwise.
    pub back_to_call: Option<String>,
}

/// Why the phone is locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockWhy {
    /// It is blocked hours, such as bedtime.
    BlockedHours,
    /// Today has no screen time at all.
    NoTimeToday,
    /// Today's time is used up.
    TimeUsedUp,
    /// The phone has spent what it holds and is asking the master for more,
    /// which it can reach. This passes at once while the master answers.
    WaitingForTime,
    /// The phone has spent the time it may use away from home: more comes
    /// when it is back in reach of the master.
    AwayTimeUsedUp,
    /// The rest of today's time is on another device, such as a phone out of
    /// the house. It comes here when that device reports from home.
    HeldByAnotherDevice,
    /// A guardian locked the child's devices.
    GuardianLock,
    /// LiMo cannot read the phone's kept state, so it cannot tell how much
    /// time is left.
    CannotCheck,
    /// A blocked app: the block cover says this, never the lock.
    AppBlocked,
}

impl LockWhy {
    /// Every reason, for tests and for the host's lists.
    pub const ALL: [LockWhy; 9] = [
        LockWhy::BlockedHours,
        LockWhy::NoTimeToday,
        LockWhy::TimeUsedUp,
        LockWhy::WaitingForTime,
        LockWhy::AwayTimeUsedUp,
        LockWhy::HeldByAnotherDevice,
        LockWhy::GuardianLock,
        LockWhy::CannotCheck,
        LockWhy::AppBlocked,
    ];
}

/// A way out from the lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Way {
    /// Ask a guardian for more time.
    Ask,
    /// Type a one time code.
    Code,
    /// A guardian types the PIN.
    Pin,
}

/// One way out, as the lock lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WayRow {
    /// The way out.
    pub way: Way,
    /// In a few words: "Ask for more time".
    pub title: String,
    /// The line under it: "A guardian sees your request in LiMo", or "Wait
    /// until 9:47 pm" while it must wait.
    pub sub: String,
    /// True while it must wait: shown, but cannot be opened.
    pub off: bool,
}

/// The newest request, in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestLine {
    /// "You asked for 30 min at 9:32 pm".
    pub text: String,
    /// "Waiting", "Given 15 min" or "Not this time".
    pub stamp: String,
}

/// The time left, for LiMo's notification and the child's own screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeLeft {
    /// The best known time left today, in seconds. None when today has no
    /// limit.
    pub seconds_left: Option<u64>,
    /// True while time is being counted now.
    pub counting: bool,
    /// When a guardian's unlock keeps the phone open, the moment it ends.
    /// Time used meanwhile still counts.
    pub unlocked_until: Option<i64>,
    /// True while the master is out of reach: the time left is what this
    /// phone holds for away from home.
    pub away: bool,
    /// True while the phone is locked (an app that stays usable may be in
    /// front).
    pub locked: bool,
    /// In words: "1 h 12 min left", "1 h 12 min paused", "No limit today",
    /// "Open until 9:30 pm", or the lock's title while locked; "Setup is
    /// not finished on this phone." until a guardian finishes setup.
    pub words: String,
}

/// A warning that a lock is coming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarningShown {
    /// What the lock will be for.
    pub cause: WarningCause,
    /// The warning point reached, in minutes. It tells one warning from
    /// another; the words say the time actually left, which can be less.
    pub minutes: u32,
    /// When the lock is expected, unix seconds by LiMo's clock, if use goes
    /// on.
    pub lock_at: i64,
    /// True when the time running out is this phone's time for away from
    /// home: more comes when it is back in reach.
    pub away: bool,
    /// The warning: "5 minutes of screen time left today.".
    pub message: String,
    /// A hint under it: "Save what you are doing.".
    pub hint: String,
    /// When: "Locks about 9:47 pm".
    pub when: String,
}

/// What the phone says to a code or a PIN typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TryAnswer {
    /// The rules core's outcome.
    pub outcome: TryOutcome,
    /// What to say: "30 minutes added.", "That code is not right. 4 more
    /// tries before a wait.".
    pub message: String,
    /// "The phone opens now." when it is open after a right code or PIN;
    /// None otherwise.
    pub opens_now: Option<String>,
}

/// What became of a grant handed to [`crate::PhoneEngine::take_grant`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantTaken {
    /// The rules core's outcome.
    pub outcome: limo_core::GrantOutcome,
    /// True when the grant asks for a report at once (it lowered this
    /// phone's away allowance): send one now, as the next call would order.
    pub report_again: bool,
}

/// Something the guardian should know. Given in [`PhoneOrders::notes`] in
/// the call it was noted, and kept by the engine: a kind the master takes
/// ([`PhoneNote::for_master`]) until a grant confirms it, the others until
/// newer notes take their place.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "note")]
pub enum PhoneNote {
    /// The wall clock moved against the elapsed count: it was set, or
    /// corrected. No counted time was gained or lost by it.
    ClockMoved {
        /// By how many seconds; forward is positive.
        by_secs: i64,
        /// True when LiMo followed it (automatic time was on), false when it
        /// kept its own clock.
        followed: bool,
        /// When, by LiMo's clock.
        at: i64,
    },
    /// The phone's time zone changed. Days and blocked hours follow the
    /// home zone, so nothing counted changes.
    TimeZoneChanged {
        /// The offset before, seconds from UTC.
        from_offset_secs: i32,
        /// The offset after.
        to_offset_secs: i32,
        /// When, by LiMo's clock.
        at: i64,
    },
    /// The phone was started again since LiMo last saw it.
    Restarted {
        /// When LiMo first saw it again, by its clock.
        at: i64,
    },
    /// LiMo was not called for a while when the phone was in use: it was
    /// stopped, or held up. The use meanwhile was not counted.
    NotWatched {
        /// When LiMo last saw the phone in use, by its clock.
        from: i64,
        /// For how long, in seconds.
        secs: u64,
    },
    /// A permission LiMo needs is off, where the master was last told it is
    /// on: it was switched off.
    PermissionOff {
        /// Which.
        permission: Permission,
        /// When, by LiMo's clock.
        at: i64,
    },
    /// A permission LiMo needs is on, where the master was not told so yet:
    /// switched on, during setup or after, or found on (by the first call
    /// of a LiMo that tells it, on a phone set up before).
    PermissionOn {
        /// Which.
        permission: Permission,
        /// When, by LiMo's clock.
        at: i64,
    },
    /// The phone's protection level, where the master was not told this
    /// one: at the first call, and at every change.
    Protection {
        /// The level now.
        level: ProtectionLevel,
        /// When, by LiMo's clock.
        at: i64,
    },
    /// Whether USB debugging is on, where the master was not told this: at
    /// the first call the host says it, and at every change.
    UsbDebugging {
        /// True when it is on.
        on: bool,
        /// When, by LiMo's clock.
        at: i64,
    },
    /// A blocked app came to the front and was covered. Noted once a day
    /// for each app.
    BlockedAppOpened {
        /// The app's package name.
        package: String,
        /// The home day.
        day: i64,
        /// When, by LiMo's clock.
        at: i64,
    },
    /// A guardian finished setup on the phone: from then on its time is
    /// counted and covers are drawn. Noted once, when it is finished. The
    /// master paired the phone, so until this note comes it knows that
    /// setup is not finished.
    SetupFinished {
        /// When, by LiMo's clock.
        at: i64,
    },
    /// A page that could switch LiMo off or remove it came to the front and
    /// was covered by the guard. Noted once a day for each page.
    GuardedPageOpened {
        /// Which page.
        page: GuardedPage,
        /// The home day.
        day: i64,
        /// When, by LiMo's clock.
        at: i64,
    },
    /// A right guardian PIN on the guard opened the guarded pages.
    GuardOpened {
        /// Until when, by LiMo's clock.
        until: i64,
        /// When, by LiMo's clock.
        at: i64,
    },
}

impl PhoneNote {
    /// The note as a report carries it to the master: the rules core's
    /// kind of the same name, with the same fields
    /// ([`limo_core::PhoneNote`], where every kind the master takes is
    /// listed). None for the kinds that list does not hold, which stay on
    /// the phone: a time zone changed, LiMo not watching, a blocked app
    /// opened.
    pub fn for_master(&self) -> Option<limo_core::PhoneNote> {
        use limo_core::PhoneNote as Master;
        Some(match self {
            PhoneNote::ClockMoved { by_secs, followed, at } => {
                Master::ClockMoved { by_secs: *by_secs, followed: *followed, at: *at }
            }
            PhoneNote::Restarted { at } => Master::Restarted { at: *at },
            PhoneNote::PermissionOff { permission, at } => {
                Master::PermissionOff { permission: permission.for_master(), at: *at }
            }
            PhoneNote::PermissionOn { permission, at } => {
                Master::PermissionOn { permission: permission.for_master(), at: *at }
            }
            PhoneNote::Protection { level, at } => Master::Protection { level: *level, at: *at },
            PhoneNote::UsbDebugging { on, at } => Master::UsbDebugging { on: *on, at: *at },
            PhoneNote::SetupFinished { at } => Master::SetupFinished { at: *at },
            PhoneNote::GuardedPageOpened { page, day, at } => {
                Master::GuardedPageOpened { page: page.for_master(), day: *day, at: *at }
            }
            PhoneNote::GuardOpened { until, at } => Master::GuardOpened { until: *until, at: *at },
            PhoneNote::TimeZoneChanged { .. } | PhoneNote::NotWatched { .. } | PhoneNote::BlockedAppOpened { .. } => {
                return None;
            }
        })
    }
}
