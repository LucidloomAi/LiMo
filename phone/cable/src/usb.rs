//! What Windows itself sees on the PC's USB ports, and what that says about
//! the cable, the phone and the PC when no phone can be reached.
//!
//! Two parts, kept apart: the looking, behind the [`UsbLook`] trait (on
//! Windows, the device list in `win/usb.rs`; in the tests, a fake list), and
//! the decision, [`decide`], a pure function from whether a phone could be
//! reached and what Windows listed to a [`Case`]. What a person reads for
//! each case is its caller's: the console's words are in [`crate::flow`],
//! the phone development tool keeps its own.
//!
//! **A phone is recognised by what it is**, never by a maker's list:
//!
//! - a portable device on the USB bus: a device in Windows' WPD class,
//!   which phones join when they offer files (MTP) or pictures (PTP), that
//!   Windows says hangs on the USB bus itself. A USB stick or a memory card
//!   joins the same class, as a volume that Windows makes on its software
//!   bus, so it is left out ([`looks_like_a_phone`]); or
//! - the phone's debugging interface: a USB interface whose compatible id
//!   is `USB\Class_FF&SubClass_42&Prot_01`.
//!
//! A camera or a media player that offers pictures or files over USB looks
//! the same as a phone doing so, and is counted; the words say so where it
//! matters.

#![forbid(unsafe_code)]

/// The compatible id of a phone's debugging interface: vendor class 0xFF,
/// subclass 0x42, protocol 1.
pub const DEBUG_INTERFACE_ID: &str = "USB\\Class_FF&SubClass_42&Prot_01";

/// Windows' class of portable devices (WPD), which MTP and PTP phones join:
/// `{EEC5AD98-8080-425F-922A-DABF3DE3F69A}`.
pub const PORTABLE_DEVICE_CLASS: u128 = 0xEEC5_AD98_8080_425F_922A_DABF_3DE3_F69A;

/// The bus a device on a USB port hangs on, as Windows names it (the
/// device's enumerator).
pub const USB_BUS: &str = "USB";

/// What a device that looks like a phone is, to Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A portable device (the WPD class): the phone offers files or
    /// pictures.
    PortableDevice,
    /// The phone's debugging interface.
    DebugInterface,
}

/// One device Windows lists as present that looks like a phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbDevice {
    /// What it is.
    pub kind: Kind,
    /// Its name as Windows shows it ("Galaxy S20 Ultra 5G"), if it has one.
    pub name: Option<String>,
    /// The bus Windows says it hangs on (its enumerator's name): `USB` for
    /// a device on a USB port, `SWD` for one Windows made itself, such as
    /// the volume of a USB stick. Empty when Windows does not say.
    pub bus: String,
    /// True when Windows says its driver is loaded and it works: started,
    /// with no problem.
    pub working: bool,
}

/// True when a device Windows listed looks like a phone: its debugging
/// interface, or a portable device on the USB bus itself. A portable device
/// on any other bus (a USB stick's or a memory card's volume, which Windows
/// makes on its software bus) does not.
pub fn looks_like_a_phone(device: &UsbDevice) -> bool {
    match device.kind {
        Kind::DebugInterface => true,
        Kind::PortableDevice => device.bus.trim().eq_ignore_ascii_case(USB_BUS),
    }
}

/// Lists the devices present on the PC that look like a phone, reading
/// Windows' own list and changing nothing. The error is why the list could
/// not be read.
pub trait UsbLook {
    /// The devices present now that look like a phone.
    fn phones_seen(&mut self) -> Result<Vec<UsbDevice>, String>;
}

/// For systems where nothing is looked at (all but Windows).
#[derive(Debug, Default)]
pub struct NoLook;

impl UsbLook for NoLook {
    fn phones_seen(&mut self) -> Result<Vec<UsbDevice>, String> {
        Err("this tool reads the list of USB devices on Windows only".to_string())
    }
}

/// True when one of a USB interface's compatible ids is the debugging
/// interface's. Windows writes ids in either case.
pub fn is_debug_interface(compatible_ids: &[String]) -> bool {
    compatible_ids.iter().any(|id| id.trim().eq_ignore_ascii_case(DEBUG_INTERFACE_ID))
}

/// What the connection looks like when no phone can be reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Case {
    /// Windows sees no phone at all: the cable probably only charges.
    NoPhone,
    /// Windows sees a phone as a portable device but no debugging
    /// interface: the cable carries data, and USB debugging is off.
    PhoneWithoutDebugging {
        /// The phone's name or names as Windows shows them.
        names: Vec<String>,
    },
    /// Windows sees the phone's debugging interface but has no working
    /// driver for it.
    NoDriver {
        /// The phone's name or names as Windows shows them.
        names: Vec<String>,
    },
    /// Windows sees the debugging interface working, yet the way to the
    /// phone does not list it: Google's adb's server is behind, or (for
    /// LiMo's own way) its driver is not one LiMo can open.
    DebuggingNotListed {
        /// The phone's name or names as Windows shows them.
        names: Vec<String>,
    },
    /// Windows' list could not be read.
    CannotLook(String),
}

/// The case, from whether the way to the phone lists one (`listed`) and
/// what Windows listed; None when it lists one, in any state: then that way's
/// own answer says what is next, and Windows' list is not needed (the
/// caller need not read it). The names given are the portable devices'
/// when there are any (they carry the phone's own name), else the debugging
/// interfaces'.
pub fn decide(listed: bool, windows_lists: Result<&[UsbDevice], &str>) -> Option<Case> {
    if listed {
        return None;
    }
    let devices: Vec<&UsbDevice> = match windows_lists {
        Ok(devices) => devices.iter().filter(|d| looks_like_a_phone(d)).collect(),
        Err(why) => return Some(Case::CannotLook(why.to_string())),
    };
    let names_of = |kind: Kind| -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for name in devices.iter().filter(|d| d.kind == kind).filter_map(|d| d.name.clone()) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    };
    let portable = names_of(Kind::PortableDevice);
    let names = if portable.is_empty() { names_of(Kind::DebugInterface) } else { portable };
    let debugging: Vec<&&UsbDevice> = devices.iter().filter(|d| d.kind == Kind::DebugInterface).collect();
    Some(if debugging.iter().any(|d| !d.working) {
        Case::NoDriver { names }
    } else if !debugging.is_empty() {
        Case::DebuggingNotListed { names }
    } else if devices.iter().any(|d| d.kind == Kind::PortableDevice) {
        Case::PhoneWithoutDebugging { names }
    } else {
        Case::NoPhone
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(kind: Kind, name: Option<&str>, working: bool) -> UsbDevice {
        UsbDevice { kind, name: name.map(str::to_string), bus: USB_BUS.to_string(), working }
    }

    /// A USB stick's volume, as Windows lists it: a portable device on its
    /// software bus.
    fn stick() -> UsbDevice {
        UsbDevice { kind: Kind::PortableDevice, name: Some("E:\\".to_string()), bus: "SWD".to_string(), working: true }
    }

    #[test]
    fn a_portable_device_is_a_phone_only_on_the_usb_bus_itself() {
        assert!(looks_like_a_phone(&device(Kind::PortableDevice, Some("Galaxy S20 Ultra 5G"), true)));
        let mut lower = device(Kind::PortableDevice, None, true);
        lower.bus = "usb".to_string();
        assert!(looks_like_a_phone(&lower));
        assert!(!looks_like_a_phone(&stick()));
        for bus in ["SWD", "STORAGE", "WPDBUSENUMROOT", ""] {
            let mut other = stick();
            other.bus = bus.to_string();
            assert!(!looks_like_a_phone(&other), "{bus}");
        }
        // The debugging interface is one whatever bus is given.
        let mut debug = device(Kind::DebugInterface, None, false);
        debug.bus = String::new();
        assert!(looks_like_a_phone(&debug));
    }

    #[test]
    fn a_usb_stick_is_no_phone_alone_or_beside_one() {
        assert_eq!(decide(false, Ok(&[stick()])), Some(Case::NoPhone));
        let phone = device(Kind::PortableDevice, Some("Galaxy S20 Ultra 5G"), true);
        assert_eq!(
            decide(false, Ok(&[stick(), phone.clone()])),
            Some(Case::PhoneWithoutDebugging { names: vec!["Galaxy S20 Ultra 5G".to_string()] })
        );
        // Beside a debugging interface only, the stick lends it no name.
        assert_eq!(
            decide(false, Ok(&[stick(), device(Kind::DebugInterface, Some("SAMSUNG_Android"), false)])),
            Some(Case::NoDriver { names: vec!["SAMSUNG_Android".to_string()] })
        );
    }

    #[test]
    fn the_debugging_interface_is_known_by_its_compatible_id_in_any_case() {
        let ids = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(is_debug_interface(&ids(&["USB\\Class_FF&SubClass_42&Prot_01", "USB\\Class_FF&SubClass_42", "USB\\Class_FF"])));
        assert!(is_debug_interface(&ids(&["usb\\class_ff&subclass_42&prot_01"])));
        for other in ["USB\\Class_FF&SubClass_42&Prot_02", "USB\\Class_FF&SubClass_FF&Prot_00", "USB\\Class_06&SubClass_01&Prot_01"] {
            assert!(!is_debug_interface(&ids(&[other])), "{other}");
        }
        assert!(!is_debug_interface(&[]));
    }

    #[test]
    fn each_case_from_what_the_way_to_the_phone_and_windows_list() {
        let phone = device(Kind::PortableDevice, Some("Galaxy S20 Ultra 5G"), true);
        let debug_ok = device(Kind::DebugInterface, Some("ADB Interface"), true);
        let debug_no_driver = device(Kind::DebugInterface, Some("SAMSUNG_Android"), false);
        let galaxy = vec!["Galaxy S20 Ultra 5G".to_string()];
        assert_eq!(decide(false, Ok(&[])), Some(Case::NoPhone));
        assert_eq!(decide(false, Ok(std::slice::from_ref(&phone))), Some(Case::PhoneWithoutDebugging { names: galaxy.clone() }));
        assert_eq!(decide(false, Ok(&[phone.clone(), debug_no_driver.clone()])), Some(Case::NoDriver { names: galaxy.clone() }));
        assert_eq!(
            decide(false, Ok(std::slice::from_ref(&debug_no_driver))),
            Some(Case::NoDriver { names: vec!["SAMSUNG_Android".to_string()] })
        );
        assert_eq!(decide(false, Ok(&[phone.clone(), debug_ok.clone()])), Some(Case::DebuggingNotListed { names: galaxy.clone() }));
        // No driver wins over a working interface, and a phone with no name
        // is still a phone.
        assert!(matches!(decide(false, Ok(&[debug_ok.clone(), debug_no_driver.clone()])), Some(Case::NoDriver { .. })));
        assert_eq!(decide(false, Ok(&[device(Kind::PortableDevice, None, true)])), Some(Case::PhoneWithoutDebugging { names: vec![] }));
        // Two portable devices: both named, once each.
        let camera = device(Kind::PortableDevice, Some("Camera"), true);
        assert_eq!(
            decide(false, Ok(&[phone.clone(), camera, phone.clone()])),
            Some(Case::PhoneWithoutDebugging { names: vec!["Galaxy S20 Ultra 5G".to_string(), "Camera".to_string()] })
        );
        assert_eq!(decide(false, Err("no list")), Some(Case::CannotLook("no list".to_string())));
        // A phone listed: no case, Windows not needed.
        assert_eq!(decide(true, Ok(&[])), None);
        assert_eq!(decide(true, Err("no list")), None);
        assert!(NoLook.phones_seen().is_err());
    }
}
