//! The shapes that cross to Kotlin: plain records, enums without data, and
//! flat errors. They mirror the engine's and the link's own values field by
//! field, with Kotlin's number types (`Long`, `Int`) in place of unsigned
//! ones, and with every Rust enum that carries data turned into a record
//! with a `kind` and optional fields.
//!
//! UniFFI's derive macros expand in this module, and what they write uses
//! unsafe code, so this module cannot forbid it. Nothing in it is written
//! unsafe by hand, and nothing in it decides anything: the conversions are
//! in `convert.rs`, which forbids unsafe code.

use std::fmt;

// ---------------------------------------------------------------------------
// Into the engine: the facts, and a guardian's wish with the PIN.

/// What the host knows at one moment: the engine's `PhoneFacts`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PhoneFacts {
    /// The phone's wall clock, unix seconds (`System.currentTimeMillis() / 1000`).
    pub wall_secs: i64,
    /// `SystemClock.elapsedRealtime()`: milliseconds since the phone
    /// started, counting deep sleep. Never below zero.
    pub elapsed_ms: i64,
    /// `Settings.Global.BOOT_COUNT`. Never below zero.
    pub boot_count: i32,
    /// The phone's time zone now, as seconds from UTC.
    pub utc_offset_secs: i32,
    /// `DateFormat.is24HourFormat`.
    pub hours_24: bool,
    /// `Settings.Global.AUTO_TIME`: the network sets the clock.
    pub auto_time: bool,
    /// The screen is on (`PowerManager.isInteractive`).
    pub interactive: bool,
    /// Android's own lock screen shows (`KeyguardManager.isKeyguardLocked`).
    pub keyguard_locked: bool,
    /// Whether a call is ringing or on.
    pub call: CallState,
    /// What is in front.
    pub front: Front,
    /// The master PC can be reached: the link has a session.
    pub in_reach: bool,
    /// The master asked for a report since the last call.
    pub prodded: bool,
    /// What LiMo has been allowed to do on the phone.
    pub permissions: PhonePermissions,
}

/// Whether a call is ringing or on: Android's call state. Never who, never
/// how long.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CallState {
    /// No call.
    Idle,
    /// A call is ringing.
    Ringing,
    /// A call is on, or being dialled.
    Ongoing,
}

/// What is in front. The engine's `Front`, which carries data for an app,
/// as a kind with optional fields: `package_name` is needed for
/// [`FrontKind::App`] and ignored for the others.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Front {
    /// Which kind of screen.
    pub kind: FrontKind,
    /// The app's package name, such as `org.example.game`: for an app only.
    #[uniffi(default = None)]
    pub package_name: Option<String>,
    /// The app's label under its icon, when the host has it: for an app
    /// only. Shown back to the child on the block cover, never kept.
    #[uniffi(default = None)]
    pub label: Option<String>,
}

/// Which kind of screen is in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FrontKind {
    /// The host cannot tell. The screen counts as in use all the same.
    Unknown,
    /// The home screen, or the list of recent apps.
    Home,
    /// LiMo's own screens, the ways out among them.
    Limo,
    /// The default dialler, or the screen of a call in progress.
    Dialler,
    /// The emergency dialler.
    EmergencyDialler,
    /// Any other app, named by `package_name`.
    App,
}

/// What LiMo has been allowed to do on the phone, in the order setup asks
/// for it. Each is true only when Android itself says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct PhonePermissions {
    /// LiMo may show notifications.
    pub notifications: bool,
    /// LiMo holds Android's Phone permission (`READ_PHONE_STATE`).
    pub phone: bool,
    /// LiMo may read usage access.
    pub usage_access: bool,
    /// LiMo's accessibility service is on.
    pub accessibility: bool,
    /// LiMo is left out of battery optimisation.
    pub battery: bool,
    /// LiMo's protection (its device admin) is on.
    pub protection: bool,
}

/// What a right PIN should do: the core's `PinAction`, which carries data,
/// as a kind with optional fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct PinAction {
    /// What.
    pub kind: PinActionKind,
    /// The minutes to add, for [`PinActionKind::AddMinutes`]: 1 to 1440.
    #[uniffi(default = None)]
    pub minutes: Option<i32>,
    /// Unix seconds, later than now, for [`PinActionKind::UnlockUntil`].
    #[uniffi(default = None)]
    pub until: Option<i64>,
}

/// What a right PIN may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PinActionKind {
    /// Add minutes for today.
    AddMinutes,
    /// Keep the phone open until a moment.
    UnlockUntil,
    /// Confirm "release this phone"; the host does the releasing.
    ConfirmRelease,
}

// ---------------------------------------------------------------------------
// Out of the engine: the orders.

/// Everything to be done after one call: the engine's `PhoneOrders`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PhoneOrders {
    /// What to draw over the screen now.
    pub cover: Cover,
    /// The time left, for LiMo's notification and the child's own screen.
    pub time_left: TimeLeft,
    /// A warning to show now. It comes in one call only.
    pub warning: Option<WarningShown>,
    /// Send a report to the master now.
    pub send_report: bool,
    /// Keep the state now, before anything else: write `save()`.
    pub save_state: bool,
    /// What the guardian should know, noted in this call, each as the
    /// engine's JSON (`{"note":"clock_moved",...}`): for the host's log. The
    /// engine keeps them too, until the master can take them.
    pub notes: Vec<String>,
    /// Call again within this many seconds if nothing happens (at least 1).
    pub next_check_in_secs: i64,
}

/// What to draw over the screen. The engine's `Cover`, which carries data,
/// as a kind with one optional record for each kind that has one.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Cover {
    /// Nothing, a block or the lock.
    pub kind: CoverKind,
    /// For [`CoverKind::Block`].
    pub block: Option<BlockCover>,
    /// For [`CoverKind::Lock`].
    pub lock: Option<LockCover>,
}

/// Which cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CoverKind {
    /// Nothing: the app in front may be used.
    Nothing,
    /// A blocked app is in front: cover it and send the child home.
    Block,
    /// The phone is locked: cover whatever is in front.
    Lock,
}

/// The block cover.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BlockCover {
    /// The blocked app's package name.
    pub package_name: String,
    /// "Game Launcher is not allowed.", or "This app is not allowed.".
    pub title: String,
    /// "It is blocked by your family's rules.".
    pub message: String,
    /// Send the child to the home screen.
    pub go_home: bool,
    /// A call is in progress: offer a way back to it.
    pub call_on: bool,
    /// "Back to the call", while a call is in progress.
    pub back_to_call: Option<String>,
}

/// The lock, with its words and its ways out: the engine's `LockScreen`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LockCover {
    /// Why it is locked.
    pub why: LockWhy,
    /// "Locked", or "One moment".
    pub stamp: String,
    /// Why, in a few words.
    pub title: String,
    /// Why, in plain words.
    pub message: String,
    /// When it opens again if nothing changes, unix seconds by LiMo's clock.
    pub until: Option<i64>,
    /// The same in words, or what to say with no moment known.
    pub until_words: Option<String>,
    /// The apps that stay usable under the lock, by package name.
    pub allowed_packages: Vec<String>,
    /// The child may ask for more time.
    pub can_ask_for_time: bool,
    /// A one time code may be typed.
    pub can_use_code: bool,
    /// A guardian may type the PIN.
    pub can_use_pin: bool,
    /// When the wait after wrong tries ends.
    pub try_wait_until: Option<i64>,
    /// The wait in words.
    pub wait_words: Option<String>,
    /// The child's newest request for more time today, with its answer.
    pub latest_request: Option<RequestShown>,
    /// The same in words.
    pub request_line: Option<RequestLine>,
    /// The ways out offered, in order, with their words.
    pub ways: Vec<WayRow>,
    /// What to say when no way out is offered.
    pub nothing_to_do: Option<NothingToDo>,
    /// A call is in progress: offer a way back to it.
    pub call_on: bool,
    /// "Back to the call", while a call is in progress.
    pub back_to_call: Option<String>,
}

/// Why the phone is locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LockWhy {
    /// Blocked hours, such as bedtime.
    BlockedHours,
    /// Today has no screen time.
    NoTimeToday,
    /// Today's time is used up.
    TimeUsedUp,
    /// More time is coming from the master, which is in reach.
    WaitingForTime,
    /// The time this phone may use away from home is used up.
    AwayTimeUsedUp,
    /// The rest of today's time is on another device.
    HeldByAnotherDevice,
    /// A guardian locked the child's devices.
    GuardianLock,
    /// LiMo cannot read the phone's kept state.
    CannotCheck,
    /// A blocked app (the block cover says this, never the lock).
    AppBlocked,
}

/// A way out from the lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Way {
    /// Ask a guardian for more time.
    Ask,
    /// Type a one time code.
    Code,
    /// A guardian types the PIN.
    Pin,
}

/// One way out, as the lock lists it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WayRow {
    /// The way out.
    pub way: Way,
    /// "Ask for more time".
    pub title: String,
    /// The line under it, or "Wait until 9:47 pm".
    pub sub: String,
    /// It must wait: shown, but cannot be opened.
    pub off: bool,
}

/// What the lock says when it offers no way out.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NothingToDo {
    /// The line in bold.
    pub bold: String,
    /// The line under it.
    pub line: String,
}

/// The newest request, in words.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RequestLine {
    /// "You asked for 30 min at 9:32 pm".
    pub text: String,
    /// "Waiting", "Given 15 min" or "Not this time".
    pub stamp: String,
}

/// A request for more time and how it stands: the core's `RequestView`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RequestShown {
    /// The request.
    pub request: TimeRequest,
    /// Its answer so far.
    pub answer: RequestAnswer,
    /// The minutes given, for [`RequestAnswer::Granted`].
    pub granted_minutes: Option<i32>,
    /// When it was answered, by the master's clock.
    pub answered_at: Option<i64>,
    /// False while the request has not reached the master yet.
    pub reached_master: bool,
}

/// The answer to a request, so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RequestAnswer {
    /// Not answered yet.
    Waiting,
    /// Granted, with `granted_minutes`.
    Granted,
    /// Refused.
    Refused,
}

/// A child's request for more time: the core's `TimeRequest`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TimeRequest {
    /// Unique for the child.
    pub id: String,
    /// The child.
    pub child_id: String,
    /// The device it was made on.
    pub device_id: String,
    /// The home day it asks for time on.
    pub day: i64,
    /// The minutes asked for.
    pub minutes: i32,
    /// The child's reason, possibly empty.
    pub reason: String,
    /// When it was made, by the device's clock.
    pub asked_at: i64,
}

/// The time left: the engine's `TimeLeft`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TimeLeft {
    /// The best known time left today, in seconds; none with no limit.
    pub seconds_left: Option<i64>,
    /// Time is being counted now.
    pub counting: bool,
    /// When a guardian's unlock ends.
    pub unlocked_until: Option<i64>,
    /// The master is out of reach.
    pub away: bool,
    /// The phone is locked.
    pub locked: bool,
    /// "1 h 12 min left", and the like.
    pub words: String,
}

/// A warning that a lock is coming: the engine's `WarningShown`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WarningShown {
    /// What the lock will be for.
    pub cause: WarningCause,
    /// The warning point reached, in minutes.
    pub minutes: i32,
    /// When the lock is expected, unix seconds by LiMo's clock.
    pub lock_at: i64,
    /// The time running out is this phone's away time.
    pub away: bool,
    /// "5 minutes of screen time left today.".
    pub message: String,
    /// "Save what you are doing.".
    pub hint: String,
    /// "Locks about 9:47 pm". (`when` in the engine; a keyword in Kotlin.)
    pub when_words: String,
}

/// What a warning is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum WarningCause {
    /// Today's time is running out.
    TimeRunsOut,
    /// Blocked hours are about to start.
    BlockedHoursStart,
    /// A guardian's unlock is about to end.
    UnlockEnds,
}

// ---------------------------------------------------------------------------
// Out of the engine: answers to the other calls.

/// What the phone says to a code or a PIN typed: the engine's `TryAnswer`,
/// with the core's `TryOutcome` (which carries data) as a kind with
/// optional fields.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TryAnswer {
    /// What became of it.
    pub kind: TryKind,
    /// What to say: "30 minutes added.".
    pub message: String,
    /// "The phone opens now." when it is open after a right code or PIN.
    pub opens_now: Option<String>,
    /// The time added, in seconds, for [`TryKind::Added`].
    pub added_secs: Option<i64>,
    /// The end of the unlock, for [`TryKind::Unlocked`].
    pub unlocked_until: Option<i64>,
    /// Wrong guesses since the last right PIN, for [`TryKind::Wrong`].
    pub wrong_guesses: Option<i32>,
    /// When the next try may be made, for [`TryKind::Wrong`] (once a wait is
    /// in force) and [`TryKind::MustWait`].
    pub wait_until: Option<i64>,
}

/// What became of a code or a PIN typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TryKind {
    /// Right: time was added for today.
    Added,
    /// Right: the phone is unlocked until `unlocked_until`.
    Unlocked,
    /// Right: "release this phone" is confirmed. The host releases it.
    ReleaseConfirmed,
    /// Wrong, and counted.
    Wrong,
    /// Not tried: a wait is in force.
    MustWait,
    /// Not the right shape. Not counted.
    Malformed,
    /// A right code already used today. Not counted.
    AlreadyUsed,
    /// No code secret or no PIN record on this phone. Not counted.
    NotSet,
}

/// What became of a grant: the engine's `GrantTaken`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct GrantTaken {
    /// Taken, stale, or for another device.
    pub outcome: GrantOutcome,
    /// The grant asks for a report at once: send one now.
    pub report_again: bool,
}

/// The core's `GrantOutcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GrantOutcome {
    /// Taken.
    Taken,
    /// Older than one taken already; only its confirmations were used.
    Stale,
    /// For another child or device; ignored.
    NotForThisDevice,
}

/// Why the kept state could not be loaded: the engine's `LoadProblem`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LoadProblem {
    /// What was wrong.
    pub kind: LoadProblemKind,
    /// The form it was written in, for [`LoadProblemKind::Newer`].
    pub format: Option<i32>,
    /// In plain words.
    pub message: String,
}

/// What was wrong with a kept state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LoadProblemKind {
    /// There was nothing to load.
    Empty,
    /// It is not a LiMo phone state.
    NotLimo,
    /// It was written by a newer LiMo.
    Newer,
    /// It is damaged.
    Damaged,
}

/// The phone as the engine holds it now: a few numbers the host may read
/// itself, and the engine's whole `PhoneView` as JSON for the child's own
/// screen (React Native reads it; Kotlin passes it on).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PhoneView {
    /// The child.
    pub child_id: String,
    /// This phone, as the master knows it.
    pub device_id: String,
    /// The home day of the last decision.
    pub day: i64,
    /// Counted use on this phone that day, reported or not, in seconds.
    pub counted_secs: i64,
    /// The part of it beyond everything the phone held, under an unlock.
    pub over_secs: i64,
    /// Whole seconds counted but not yet recorded in the device state.
    pub open_secs: i64,
    /// How many slices of use the master has not confirmed yet.
    pub unreported_slices: i32,
    /// How many notes for the guardian the master has not taken yet.
    pub notes_kept: i32,
    /// How far the wall clock is ahead of LiMo's clock, in seconds.
    pub clock_correction_secs: i64,
    /// True once a guardian has finished setup on this phone; until then
    /// nothing is counted or covered. The words are in the JSON (`setup`).
    pub setup_finished: bool,
    /// The engine's whole `PhoneView`, as JSON.
    pub json: String,
}

/// The versions of what is inside the library.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Versions {
    /// This layer, limo-phone-ffi.
    pub ffi: String,
    /// The phone's engine, limo-phone.
    pub engine: String,
    /// The rules core, limo-core.
    pub core: String,
    /// The link crate, limo-link.
    pub link: String,
    /// The version of the link it speaks.
    pub link_protocol: i32,
}

// ---------------------------------------------------------------------------
// The link: in and out.

/// What a phone says about itself when it pairs. The form is always the
/// child's: this library is only in the child form of the app.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PairRequest {
    /// The phone's name, as the family will see it ("Asha's phone").
    pub name: String,
    /// The platform label, "android".
    pub platform: String,
    /// `Build.VERSION.SDK_INT`, such as 33.
    pub android_version: Option<i32>,
}

/// One app on the phone, for the app list the master keeps.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AppEntry {
    /// The package name.
    pub package_name: String,
    /// The label under its icon.
    pub label: String,
}

/// What came from the master on a session: the link's `PhoneEvent`, which
/// carries data, as a kind with optional fields.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LinkEvent {
    /// What came.
    pub kind: LinkEventKind,
    /// For [`LinkEventKind::Grant`]: the core's `Grant` as JSON, for
    /// `PhoneEngine.takeGrant` as it is. Kotlin never reads it.
    pub grant: Option<Vec<u8>>,
    /// The master's clock, unix seconds, for a grant and a pong.
    pub master_now: Option<i64>,
}

/// What came from the master.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LinkEventKind {
    /// The handshake finished; send a report now.
    Opened,
    /// The answer to a report.
    Grant,
    /// The master asks for a report at once.
    ReportNow,
    /// The answer to a ping.
    Pong,
}

// ---------------------------------------------------------------------------
// Errors. Flat: Kotlin gets one exception class for each variant, with the
// words below as its message. None of them holds a PIN, a code or a secret.

/// Why the engine did not do what was asked.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Error)]
#[uniffi(flat_error)]
pub enum EngineError {
    /// The core refused: a code secret of the wrong length.
    SecretWrongLength {
        /// The core's words.
        message: String,
    },
    /// The core refused: minutes outside what is allowed.
    MinutesOutOfRange {
        /// The core's words.
        message: String,
    },
    /// The core refused: the reason is too long.
    ReasonTooLong {
        /// The core's words.
        message: String,
    },
    /// The core refused: today has no limit.
    NoLimitToday {
        /// The core's words.
        message: String,
    },
    /// The core refused: a request is still waiting.
    RequestWaiting {
        /// The core's words.
        message: String,
    },
    /// The core refused: as many requests today as may be made.
    TooManyRequests {
        /// The core's words.
        message: String,
    },
    /// The core refused: an unlock out of range.
    UnlockOutOfRange {
        /// The core's words.
        message: String,
    },
    /// The kept state could not be loaded, so nothing can be changed.
    CannotCheck {
        /// The engine's words.
        message: String,
    },
    /// A value this layer cannot hand on: a number below zero where none
    /// can be, a field missing for its kind, or bytes that are not text.
    OutOfRange {
        /// Which, in plain words.
        what: &'static str,
    },
    /// Bytes handed in as a grant are not one.
    NotAGrant,
    /// Bytes handed in as a welcome are not one.
    NotAWelcome,
    /// The pairing has no welcome yet, or its engine was made already.
    NotWelcomed,
    /// An earlier call failed inside the engine (a panic). Drop this engine
    /// and load it again from the kept state.
    Broken,
    /// Something that cannot fail did: a bug in this layer.
    Internal {
        /// What, in plain words.
        what: &'static str,
    },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::SecretWrongLength { message }
            | EngineError::MinutesOutOfRange { message }
            | EngineError::ReasonTooLong { message }
            | EngineError::NoLimitToday { message }
            | EngineError::RequestWaiting { message }
            | EngineError::TooManyRequests { message }
            | EngineError::UnlockOutOfRange { message }
            | EngineError::CannotCheck { message } => f.write_str(message),
            EngineError::OutOfRange { what } => write!(f, "LiMo was handed a value it cannot use: {what}."),
            EngineError::NotAGrant => f.write_str("What was handed in as a grant is not one."),
            EngineError::NotAWelcome => f.write_str("What was handed in as a welcome is not one."),
            EngineError::NotWelcomed => {
                f.write_str("The pairing has no welcome yet, or the engine was made from it already.")
            }
            EngineError::Broken => f.write_str(
                "An earlier call failed inside the engine. Load the engine again from the kept state.",
            ),
            EngineError::Internal { what } => write!(f, "LiMo could not {what}."),
        }
    }
}

impl std::error::Error for EngineError {}

/// Why a link ended, or why a call on it was refused: the link's
/// `LinkError`, variant by variant, with a refusal split by its reason and
/// an unanswered question split by whether a report was waiting. The rule is
/// the link's: an error from `receive` or `tick` ends the link (close the
/// socket); an error from a call that sends changes nothing.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Error)]
#[uniffi(flat_error)]
pub enum LinkError {
    /// A frame said it was longer than its step allows.
    FrameTooLong {
        /// The link's words.
        message: String,
    },
    /// A handshake frame of a length its step cannot have.
    WrongSize {
        /// The link's words.
        message: String,
    },
    /// A frame of a length no message can have.
    BadLength {
        /// The link's words.
        message: String,
    },
    /// Not the start of a LiMo link.
    NotLimo {
        /// The link's words.
        message: String,
    },
    /// Version 0 of the link.
    BadVersion {
        /// The link's words.
        message: String,
    },
    /// Something did not decrypt.
    Garbled {
        /// The link's words.
        message: String,
    },
    /// An offer the master does not have.
    UnknownOffer {
        /// The link's words.
        message: String,
    },
    /// A phone the master does not know.
    UnknownPhone {
        /// The link's words.
        message: String,
    },
    /// The PC that answered is not the one the offer names.
    WrongMaster {
        /// The link's words.
        message: String,
    },
    /// A message that cannot be read.
    Malformed {
        /// The link's words.
        message: String,
    },
    /// A message that may not come now.
    Unexpected {
        /// The link's words.
        message: String,
    },
    /// A message with a value out of bounds.
    OutOfBounds {
        /// The link's words.
        message: String,
    },
    /// A report or a grant for another device.
    OtherDevice {
        /// The link's words.
        message: String,
    },
    /// The master refused: this phone was removed from the family.
    RefusedRemoved {
        /// The link's words.
        message: String,
    },
    /// The master refused: it does not know this phone.
    RefusedUnknown {
        /// The link's words.
        message: String,
    },
    /// The master refused: a report about another device.
    RefusedOtherDevice {
        /// The link's words.
        message: String,
    },
    /// The master refused: it speaks another version of the link.
    RefusedVersion {
        /// The link's words.
        message: String,
    },
    /// The master refused: the offer was used already.
    RefusedOfferUsed {
        /// The link's words.
        message: String,
    },
    /// The master refused: the offer has expired.
    RefusedOfferExpired {
        /// The link's words.
        message: String,
    },
    /// The master refused: it cannot do that yet.
    RefusedNotSupported {
        /// The link's words.
        message: String,
    },
    /// The handshake did not finish in time.
    HandshakeTooSlow {
        /// The link's words.
        message: String,
    },
    /// A report went unanswered: call `PhoneEngine.lostContact`.
    NoAnswerToReport {
        /// The link's words.
        message: String,
    },
    /// A question other than a report went unanswered.
    NoAnswer {
        /// The link's words.
        message: String,
    },
    /// Nothing came for too long (the master's side only).
    Silent {
        /// The link's words.
        message: String,
    },
    /// Too many questions are waiting already.
    TooManyWaiting {
        /// The link's words.
        message: String,
    },
    /// A message too long to send.
    MessageTooLong {
        /// The link's words.
        message: String,
    },
    /// The link is not open yet.
    NotOpen {
        /// The link's words.
        message: String,
    },
    /// The link has ended; start a new one.
    Closed {
        /// The link's words.
        message: String,
    },
    /// The operating system gave no random bytes.
    NoRandomness {
        /// The link's words.
        message: String,
    },
    /// A pairing offer that cannot be used.
    BadOffer {
        /// The link's words.
        message: String,
    },
    /// The crypto library refused something: a bug in the link crate.
    Internal {
        /// The link's words.
        message: String,
    },
    /// Bytes handed in as a report are not one. Nothing was sent.
    NotAReport,
    /// The pairing has no welcome yet.
    NotWelcomed,
    /// A value this layer cannot hand on, such as a time below zero.
    OutOfRange {
        /// Which, in plain words.
        what: &'static str,
    },
    /// An earlier call failed inside the link (a panic). Close the socket
    /// and start again.
    Broken,
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkError::FrameTooLong { message }
            | LinkError::WrongSize { message }
            | LinkError::BadLength { message }
            | LinkError::NotLimo { message }
            | LinkError::BadVersion { message }
            | LinkError::Garbled { message }
            | LinkError::UnknownOffer { message }
            | LinkError::UnknownPhone { message }
            | LinkError::WrongMaster { message }
            | LinkError::Malformed { message }
            | LinkError::Unexpected { message }
            | LinkError::OutOfBounds { message }
            | LinkError::OtherDevice { message }
            | LinkError::RefusedRemoved { message }
            | LinkError::RefusedUnknown { message }
            | LinkError::RefusedOtherDevice { message }
            | LinkError::RefusedVersion { message }
            | LinkError::RefusedOfferUsed { message }
            | LinkError::RefusedOfferExpired { message }
            | LinkError::RefusedNotSupported { message }
            | LinkError::HandshakeTooSlow { message }
            | LinkError::NoAnswerToReport { message }
            | LinkError::NoAnswer { message }
            | LinkError::Silent { message }
            | LinkError::TooManyWaiting { message }
            | LinkError::MessageTooLong { message }
            | LinkError::NotOpen { message }
            | LinkError::Closed { message }
            | LinkError::NoRandomness { message }
            | LinkError::BadOffer { message }
            | LinkError::Internal { message } => f.write_str(message),
            LinkError::NotAReport => f.write_str("What was handed in as a report is not one; nothing was sent."),
            LinkError::NotWelcomed => f.write_str("The pairing has no welcome yet."),
            LinkError::OutOfRange { what } => write!(f, "LiMo was handed a value it cannot use: {what}."),
            LinkError::Broken => f.write_str("An earlier call failed inside the link. Close it and start again."),
        }
    }
}

impl std::error::Error for LinkError {}
