//! When adb sees no phone: what Windows itself sees on the PC's USB ports,
//! and what that says about the cable, the phone and the PC.
//!
//! The look and the cases are LiMo's cable crate's
//! ([`limo_cable::usb`], and the Windows read in `limo_cable::win::usb`):
//! one copy, for this tool and for LiMo on the PC. They are named here as
//! this tool always named them. What this tool prints for each case is its
//! own, here: [`CaseWords::sees_line`] and [`CaseWords::advice`], which name
//! adb, as this tool runs it.

#![forbid(unsafe_code)]

pub use limo_cable::usb::{
    Case, DEBUG_INTERFACE_ID, Kind, NoLook, PORTABLE_DEVICE_CLASS, USB_BUS, UsbDevice, UsbLook, is_debug_interface,
    looks_like_a_phone,
};

use crate::phone::DeviceLine;

/// The case, from what adb listed and what Windows listed; None when adb
/// lists any phone at all, whatever its state: then adb's own answer says
/// what is wrong, and Windows' list is not needed (the caller does not read
/// it).
pub fn decide(adb_lists: &[DeviceLine], windows_lists: Result<&[UsbDevice], &str>) -> Option<Case> {
    limo_cable::usb::decide(!adb_lists.is_empty(), windows_lists)
}

/// " (Galaxy S20 Ultra 5G)", or nothing with no name.
fn named(names: &[String]) -> String {
    if names.is_empty() { String::new() } else { format!(" ({})", names.join(", ")) }
}

/// What this tool prints for a case.
pub trait CaseWords {
    /// The one line that says what this PC sees.
    fn sees_line(&self) -> String;
    /// What to do.
    fn advice(&self) -> String;
}

impl CaseWords for Case {
    fn sees_line(&self) -> String {
        match self {
            Case::NoPhone => "This PC sees no phone on its USB ports.".to_string(),
            Case::PhoneWithoutDebugging { names } => {
                format!("This PC sees what looks like a phone on its USB ports{}, but adb does not.", named(names))
            }
            Case::NoDriver { names } => {
                format!("This PC sees the debugging interface of a phone{}, but has no working driver for it.", named(names))
            }
            Case::DebuggingNotListed { names } => format!(
                "This PC sees the debugging interface of a phone{}, with its driver working, but adb does not list it.",
                named(names)
            ),
            Case::CannotLook(why) => format!("This PC's list of USB devices could not be read: {why}."),
        }
    }

    fn advice(&self) -> String {
        match self {
            Case::NoPhone => "The cable probably only charges: many cables carry no data. Use the cable that came with \
                 the phone, or one known to move files; plug it into a port on the PC itself, not a hub; and unlock \
                 the phone."
                .to_string(),
            Case::PhoneWithoutDebugging { .. } => "If that is the phone, its cable carries data, but USB debugging is \
                 off. Switch it on, on the phone (Settings, Developer options); this tool never turns it on by \
                 itself. If it is on already, unplug the phone and plug it in again, unlocked. If it is something \
                 else that offers pictures or files, such as a camera, the phone's own cable probably only charges: \
                 use the one that came with the phone."
                .to_string(),
            Case::NoDriver { .. } => "The phone maker's USB driver has to be installed on this PC before adb can reach \
                 the phone. Install it, then unplug the phone and plug it in again. This tool installs nothing."
                .to_string(),
            Case::DebuggingNotListed { .. } => "Unplug the phone and plug it in again, unlocked; if adb still lists \
                 nothing, run adb kill-server, then this again."
                .to_string(),
            Case::CannotLook(_) => crate::phone::PickProblem::NoPhone.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adb_listing_a_phone_in_any_state_means_no_case() {
        for state in ["device", "unauthorized", "offline"] {
            let listed = [DeviceLine { serial: "R5".to_string(), state: state.to_string() }];
            assert_eq!(decide(&listed, Ok(&[])), None, "{state}");
            assert_eq!(decide(&listed, Err("no list")), None, "{state}");
        }
        assert_eq!(decide(&[], Ok(&[])), Some(Case::NoPhone));
    }

    #[test]
    fn each_case_says_what_this_pc_sees_and_what_to_do() {
        let galaxy = vec!["Galaxy S20 Ultra 5G".to_string()];
        assert_eq!(Case::NoPhone.sees_line(), "This PC sees no phone on its USB ports.");
        assert_eq!(
            Case::PhoneWithoutDebugging { names: galaxy.clone() }.sees_line(),
            "This PC sees what looks like a phone on its USB ports (Galaxy S20 Ultra 5G), but adb does not."
        );
        assert_eq!(
            Case::PhoneWithoutDebugging { names: vec![] }.sees_line(),
            "This PC sees what looks like a phone on its USB ports, but adb does not."
        );
        assert_eq!(
            Case::NoDriver { names: galaxy.clone() }.sees_line(),
            "This PC sees the debugging interface of a phone (Galaxy S20 Ultra 5G), but has no working driver for it."
        );
        assert!(Case::NoPhone.advice().starts_with("The cable probably only charges"));
        assert!(Case::PhoneWithoutDebugging { names: galaxy.clone() }.advice().contains("(Settings, Developer options)"));
        assert!(Case::NoDriver { names: galaxy }.advice().contains("phone maker's USB driver"));
        assert!(!Case::NoDriver { names: vec![] }.advice().contains("http"), "no download address");
        assert!(Case::CannotLook("x".to_string()).advice().starts_with("No phone is attached."));
        assert!(NoLook.phones_seen().is_err());
    }
}
