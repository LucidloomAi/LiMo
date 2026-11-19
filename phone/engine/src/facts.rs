//! The facts: everything the engine knows of the phone, handed in by the
//! host (the Android service) at every call, as plain data.
//!
//! **What LiMo may know about an app is fixed here.** An app is its package
//! name, and the label the app gives itself on the home screen, which is
//! only ever shown back to the child on the block screen: it is never kept
//! in the state and never put in a report. There is no field for a window's
//! text, a web address, a message, a contact, a number dialled or anything
//! typed, anywhere in this crate, so the engine cannot record one or put one
//! in an order. Whether a call is on is a yes or a no: never who, never how
//! long.
//!
//! **The engine holds no list of makers' package names.** Which app is the
//! home screen, the dialler or the emergency dialler differs from maker to
//! maker, so the host says so ([`Front`]); the engine never guesses from a
//! package name.

use serde::{Deserialize, Serialize};

/// What the host knows at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneFacts {
    /// The phone's wall clock, unix seconds. It says what time it is: the
    /// day, the blocked hours, when a lock ends. It is never trusted for how
    /// long anything lasted, and while automatic time is off it is not
    /// followed when it jumps (see [`PhoneFacts::auto_time`]).
    pub wall_secs: i64,
    /// Milliseconds since the phone started, counting deep sleep: Android's
    /// `SystemClock.elapsedRealtime()`. It cannot be set. Every duration the
    /// engine counts comes from it.
    pub elapsed_ms: u64,
    /// How many times the phone has started: Android's
    /// `Settings.Global.BOOT_COUNT`. With `elapsed_ms` it tells a restart of
    /// the phone (the count grows, the elapsed time starts again) from
    /// anything else.
    pub boot_count: u32,
    /// The phone's time zone now, as seconds from UTC. Times the child reads
    /// are said in it; days and blocked hours follow the home zone in the
    /// rules. A change is noted for the guardian.
    pub utc_offset_secs: i32,
    /// True when the phone shows the time in 24 hours ("21:42"), false for
    /// 12 hours ("9:42 pm"): Android's `DateFormat.is24HourFormat`.
    pub hours_24: bool,
    /// True when the phone sets its clock from the network (Android's
    /// `Settings.Global.AUTO_TIME`). Then nobody can set it by hand, and the
    /// engine follows the wall clock when it jumps. While it is off, a jump
    /// is somebody setting the clock, and the engine keeps its own clock,
    /// run on by `elapsed_ms`, so the jump buys nothing.
    pub auto_time: bool,
    /// True when the screen is on (Android's `PowerManager.isInteractive`).
    pub interactive: bool,
    /// True while Android's own lock screen shows (`KeyguardManager.
    /// isKeyguardLocked`): the phone is not in use yet. LiMo never covers it.
    pub keyguard_locked: bool,
    /// Whether a call is ringing or on.
    pub call: CallState,
    /// What is in front.
    pub front: Front,
    /// True while the master PC can be reached: the link has a session.
    pub in_reach: bool,
    /// True when the master asked this phone to report now, since the last
    /// call.
    pub prodded: bool,
    /// What LiMo has been allowed to do on the phone.
    pub permissions: PhonePermissions,
}

/// Whether a call is ringing or on: Android's call state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CallState {
    /// No call.
    #[default]
    Idle,
    /// A call is ringing.
    Ringing,
    /// A call is on, or being dialled.
    Ongoing,
}

/// What is in front, as the host knows it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "front")]
pub enum Front {
    /// The host cannot tell: LiMo's watch on the app in front is off, or has
    /// not seen anything yet. The screen counts as in use all the same.
    Unknown,
    /// The home screen (the default launcher), or the list of recent apps.
    Home,
    /// LiMo's own screens, the ways out among them.
    LiMo,
    /// The phone's default dialler, or the screen of a call in progress.
    Dialler,
    /// The emergency dialler.
    EmergencyDialler,
    /// Any other app.
    App {
        /// Its package name, such as `org.example.game`.
        package: String,
        /// Its label as the phone shows it under its icon, such as "Game
        /// Launcher", when the host has it. The app's own word: the engine
        /// cleans it before showing it, and never keeps it or reports it.
        #[serde(default)]
        label: Option<String>,
    },
}

impl Front {
    /// An app in front, by its package name, with no label.
    pub fn app(package: &str) -> Front {
        Front::App { package: package.to_string(), label: None }
    }

    /// An app in front, by its package name and its label.
    pub fn labelled(package: &str, label: &str) -> Front {
        Front::App { package: package.to_string(), label: Some(label.to_string()) }
    }

    /// The app the rules core decides for: the package of an app in front,
    /// an empty name for an app not known, and none for the home screen,
    /// LiMo, the dialler and the emergency dialler.
    pub(crate) fn rules_app(&self) -> Option<String> {
        match self {
            Front::App { package, .. } => Some(package.clone()),
            Front::Unknown => Some(String::new()),
            Front::Home | Front::LiMo | Front::Dialler | Front::EmergencyDialler => None,
        }
    }

    /// The package of an app in front, if one is known.
    pub(crate) fn package(&self) -> Option<&str> {
        match self {
            Front::App { package, .. } => Some(package),
            _ => None,
        }
    }

    /// The label the host gave for an app in front, as it came.
    pub(crate) fn label(&self) -> Option<&str> {
        match self {
            Front::App { label, .. } => label.as_deref(),
            _ => None,
        }
    }
}

/// What LiMo has been allowed to do on the phone. A permission switched off
/// is noted for the guardian.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PhonePermissions {
    /// LiMo's accessibility service is on: the app in front is known, and
    /// covers can be drawn.
    pub accessibility: bool,
    /// LiMo may read usage access.
    pub usage_access: bool,
}

impl PhonePermissions {
    /// Every permission given.
    pub const ALL: PhonePermissions = PhonePermissions { accessibility: true, usage_access: true };
}

/// One permission LiMo needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// LiMo's accessibility service.
    Accessibility,
    /// Usage access.
    UsageAccess,
}
