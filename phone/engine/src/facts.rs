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
    /// What LiMo has been allowed to do on the phone, all six.
    pub permissions: PhonePermissions,
    /// A page in front that could switch LiMo off or remove it, when the
    /// host says one is: LiMo then covers it with the guard until a guardian
    /// types the PIN ([`crate::Cover::Guard`]). None for every other screen.
    /// The host tells it from what Android says (see [`GuardedPage`]); the
    /// engine holds no package or page name of its own.
    #[serde(default)]
    pub guarded: Option<GuardedPage>,
}

/// A page that could switch LiMo off or remove it from the phone, as the host
/// recognised it in front. The engine covers it with the guard; whether and
/// how such a page is recognised is the host's (its README lists each, and
/// what only a phone can show).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardedPage {
    /// Android's page for LiMo's device admin, which switches LiMo's
    /// protection off (and on some paths removes LiMo with it). Guarded only
    /// while the protection is on: with it off, LiMo can be removed anyway,
    /// and the same page is where a guardian switches it on again.
    DeviceAdmin,
    /// Android's accessibility settings, where LiMo's accessibility service
    /// can be switched off. Without that service LiMo cannot see which app is
    /// open, nor cover anything.
    Accessibility,
}

impl GuardedPage {
    /// Every guarded page, for tests and the host's lists.
    pub const ALL: [GuardedPage; 2] = [GuardedPage::DeviceAdmin, GuardedPage::Accessibility];

    /// True when this page is to be guarded with these permissions: the
    /// device admin's page only while the protection is on.
    pub(crate) fn needs_guard(self, permissions: PhonePermissions) -> bool {
        match self {
            GuardedPage::DeviceAdmin => permissions.protection,
            GuardedPage::Accessibility => true,
        }
    }

    /// The same page in the rules core's words, as a note to the master
    /// names it.
    pub(crate) fn for_master(self) -> limo_core::GuardedPage {
        match self {
            GuardedPage::DeviceAdmin => limo_core::GuardedPage::DeviceAdmin,
            GuardedPage::Accessibility => limo_core::GuardedPage::Accessibility,
        }
    }
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

/// What LiMo has been allowed to do on the phone, in the order setup asks
/// for it. A permission switched off is noted for the guardian, and while
/// any is off the child's Today screen says a guardian is needed
/// ([`crate::TodayView::guardian_needed`]).
///
/// Only two change what the engine decides, and neither through this
/// record: with accessibility off the host cannot say which app is in front
/// ([`Front::Unknown`], which counts all the same) nor draw a cover, and with
/// usage access off it cannot confirm the app in front. The other four
/// change nothing the engine counts or covers: without notifications the
/// host cannot show the time left or a warning, without the phone
/// permission it knows less about calls, without the battery exemption
/// Android may stop LiMo, and without protection LiMo can be removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PhonePermissions {
    /// LiMo may show notifications: the time left, and the warnings in
    /// Android's own shade.
    #[serde(default)]
    pub notifications: bool,
    /// LiMo holds Android's Phone permission, used only to know that a call
    /// rings or is on, never who, so that a lock never hides a call.
    #[serde(default)]
    pub phone: bool,
    /// LiMo may read usage access: how long each app is in front.
    pub usage_access: bool,
    /// LiMo's accessibility service is on: the app in front is known, and
    /// covers can be drawn.
    pub accessibility: bool,
    /// LiMo is left out of Android's battery optimisation, so it is not put
    /// to sleep.
    #[serde(default)]
    pub battery: bool,
    /// LiMo's protection is on (its device admin), so it cannot be removed
    /// without a guardian.
    #[serde(default)]
    pub protection: bool,
}

impl PhonePermissions {
    /// Every permission given.
    pub const ALL: PhonePermissions = PhonePermissions {
        notifications: true,
        phone: true,
        usage_access: true,
        accessibility: true,
        battery: true,
        protection: true,
    };

    /// Whether one permission is given.
    pub fn has(self, permission: Permission) -> bool {
        match permission {
            Permission::Notifications => self.notifications,
            Permission::Phone => self.phone,
            Permission::UsageAccess => self.usage_access,
            Permission::Accessibility => self.accessibility,
            Permission::Battery => self.battery,
            Permission::Protection => self.protection,
        }
    }

    /// The permissions not given, in the order setup asks for them.
    pub fn off(self) -> Vec<Permission> {
        Permission::ALL.into_iter().filter(|p| !self.has(*p)).collect()
    }
}

/// One permission LiMo needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Notifications.
    Notifications,
    /// Android's Phone permission.
    Phone,
    /// Usage access.
    UsageAccess,
    /// LiMo's accessibility service.
    Accessibility,
    /// The battery exemption.
    Battery,
    /// LiMo's protection (its device admin).
    Protection,
}

impl Permission {
    /// Every permission, in the order setup asks for them.
    pub const ALL: [Permission; 6] = [
        Permission::Notifications,
        Permission::Phone,
        Permission::UsageAccess,
        Permission::Accessibility,
        Permission::Battery,
        Permission::Protection,
    ];

    /// The same permission in the rules core's words, as a note to the
    /// master names it.
    pub(crate) fn for_master(self) -> limo_core::PhonePermission {
        match self {
            Permission::Notifications => limo_core::PhonePermission::Notifications,
            Permission::Phone => limo_core::PhonePermission::Phone,
            Permission::UsageAccess => limo_core::PhonePermission::UsageAccess,
            Permission::Accessibility => limo_core::PhonePermission::Accessibility,
            Permission::Battery => limo_core::PhonePermission::Battery,
            Permission::Protection => limo_core::PhonePermission::Protection,
        }
    }
}
