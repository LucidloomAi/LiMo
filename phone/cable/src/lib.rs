//! # limo-cable
//!
//! The PC's side of the USB cable to a child's phone: what Windows sees on
//! its USB ports, and LiMo's own way to the phone over it, with no program
//! of Google's. See the README beside this crate.
//!
//! - [`usb`]: the look at Windows' list of USB devices (read only, in
//!   `win/usb.rs`) and the cases it tells apart, pure: a cable that only
//!   charges, USB debugging off, no driver, a driver LiMo cannot use. The
//!   phone development tool uses this too.
//! - [`apk`]: what an APK says it is, read on the PC before anything is
//!   installed. The phone development tool uses this too.
//! - [`open`]: the PC's side of the cable, [`open::Cable`]: what is plugged
//!   in, and opening the one phone through Windows' own WinUSB
//!   ([`open::WindowsCable`]).
//! - [`phone`]: the phone connected, and every call LiMo makes on it
//!   ([`phone::Phone`]).
//! - [`key`]: LiMo's own adb key, kept for the guardian's Windows account.
//! - [`protocol`]: adb's protocol, LiMo's own, over a [`protocol::Wire`].
//! - [`commands`]: every command sent to a phone, in one place.
//! - [`flow`]: the small state machine the console drives.
//! - `fake` (feature `fake`): a fake phone at the far end of a fake cable.
//!
//! **Unsafe code only in `win/`**, where Windows is called; every other
//! module forbids it, each unsafe block there says why it is sound, and a
//! test (`tests/house_rules.rs`) checks both, and which Windows calls each
//! file makes.

#![warn(missing_docs)]

pub mod apk;
#[cfg(feature = "adb")]
pub mod commands;
#[cfg(feature = "fake")]
pub mod fake;
#[cfg(feature = "adb")]
pub mod flow;
#[cfg(feature = "adb")]
pub mod key;
#[cfg(feature = "adb")]
pub mod open;
#[cfg(feature = "adb")]
pub mod phone;
#[cfg(feature = "adb")]
pub mod protocol;
pub mod usb;
#[cfg(windows)]
pub mod win;
