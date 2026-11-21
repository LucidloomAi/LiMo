//! The driver report: which driver Windows bound to the phone's debugging
//! interface, and what each route can use by its own rules.
//!
//! The facts come from Windows' own lists, read only (`win/drivers.rs`):
//! for each debugging interface present (a USB interface whose compatible
//! id is `USB\Class_FF&SubClass_42&Prot_01`), its name, whether it works,
//! its driver (the driver's service name, and its package: the INF file,
//! its provider and its version), the device above it and that device's
//! driver, whether it offers the Android USB interface class, and the
//! device interface GUIDs WinUSB was told to offer for it. And for each
//! portable device on the USB bus (the phone offering files), its name and
//! driver. No serial number and no instance id is read; nothing is written,
//! installed, started or stopped.
//!
//! What each route can use is decided here, from those facts alone, by the
//! rules read in each route's own source. They say what the route should
//! do; the run says what it did.

#![forbid(unsafe_code)]

/// The Android USB interface class that Google's adb looks for on Windows.
pub const ANDROID_USB_CLASS: &str = "{F72FE0D4-CBCB-407D-8814-9ED673D0DD6B}";

/// A phone's debugging interface, as Windows lists it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DebugInterface {
    /// Its friendly name, else its description.
    pub name: Option<String>,
    /// The bus it hangs on (its enumerator): `USB`.
    pub bus: String,
    /// True when Windows says it is started with no problem.
    pub working: bool,
    /// The configuration manager's problem number, when it has one (28: no
    /// driver installed).
    pub problem: Option<u32>,
    /// The driver's service name: `WinUSB` is Microsoft's own.
    pub service: Option<String>,
    /// The driver package's INF file, as Windows keeps it (`winusb.inf`, or
    /// `oemNN.inf` for a maker's package).
    pub inf: Option<String>,
    /// Who made the driver package, as it says.
    pub provider: Option<String>,
    /// The driver package's version.
    pub version: Option<String>,
    /// The device above it (the phone's composite device), by name.
    pub parent_name: Option<String>,
    /// That device's driver's service name: `usbccgp` is Windows' own
    /// composite driver, `dg_ssudbus` Samsung's.
    pub parent_service: Option<String>,
    /// True when it offers the Android USB interface class.
    pub android_class: bool,
    /// The device interface GUIDs in its registry for WinUSB
    /// (`DeviceInterfaceGUIDs` or `DeviceInterfaceGUID`).
    pub winusb_guids: Vec<String>,
}

/// A portable device on the USB bus: a phone (or a camera) offering files
/// or pictures.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PortableDevice {
    /// Its name as Windows shows it.
    pub name: Option<String>,
    /// True when Windows says it works.
    pub working: bool,
    /// Its driver's service name.
    pub service: Option<String>,
}

/// Everything the report found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    /// The debugging interfaces present.
    pub interfaces: Vec<DebugInterface>,
    /// The portable devices on the USB bus present.
    pub portable: Vec<PortableDevice>,
}

/// Whether a route can use an interface, by its own rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Can {
    /// Yes, and why.
    Yes(String),
    /// No, and why.
    No(String),
}

fn is(value: &Option<String>, names: &[&str]) -> bool {
    value.as_deref().is_some_and(|v| names.iter().any(|n| v.trim().eq_ignore_ascii_case(n)))
}

/// True when the interface is the whole device: the device above it is a
/// USB hub, not a composite device.
fn whole_device(i: &DebugInterface) -> bool {
    i.parent_service.as_deref().is_some_and(|s| s.trim().to_ascii_lowercase().starts_with("usbhub"))
}

fn no_driver(i: &DebugInterface) -> Option<Can> {
    if i.service.is_none() || !i.working {
        let problem = i.problem.map(|p| format!(" (problem {p})")).unwrap_or_default();
        return Some(Can::No(format!("Windows has no working driver on it{problem}")));
    }
    None
}

fn shown(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("none")
}

/// libusb 1.0.27 (`os/windows_winusb.c`): an interface driven by WinUSB,
/// libusbK or libusb0, under Windows' own `usbccgp`, or a whole device so
/// driven. Its source also names Samsung's composite driver, `dg_ssudbus`,
/// but on the test phone (21 Nov 2026) it refused an interface under it,
/// driven by WinUSB, with "Operation not supported or unimplemented on this
/// platform"; so that counts as no. libusbK and libusb0 also need libusbK's
/// own library on the PC.
pub fn libusb_can(i: &DebugInterface) -> Can {
    if let Some(no) = no_driver(i) {
        return no;
    }
    if is(&i.parent_service, &["dg_ssudbus"]) {
        return Can::No(
            "the device above is driven by Samsung's dg_ssudbus, under which libusb 1.0.27 refused the interface on \
             the test phone (\"Operation not supported\"), although its source names that driver"
                .to_string(),
        );
    }
    if !(whole_device(i) || is(&i.parent_service, &["usbccgp"])) {
        return Can::No(format!(
            "libusb reaches an interface only under Windows' own usbccgp, and the device above is driven by {}",
            shown(&i.parent_service)
        ));
    }
    if is(&i.service, &["winusb"]) {
        Can::Yes("its driver is WinUSB".to_string())
    } else if is(&i.service, &["libusbk", "libusb0"]) {
        Can::Yes(format!("its driver is {}, which libusb reaches through libusbK's own library, if that is on this PC", shown(&i.service)))
    } else {
        Can::No(format!("libusb reaches only WinUSB, libusbK or libusb0, and its driver is {}", shown(&i.service)))
    }
}

/// nusb 0.2.9 (`platform/windows_winusb/enumeration.rs`): an interface
/// driven by WinUSB under `usbccgp`, with a device interface GUID in its
/// registry, or a whole device driven by WinUSB.
pub fn nusb_can(i: &DebugInterface) -> Can {
    if let Some(no) = no_driver(i) {
        return no;
    }
    if !is(&i.service, &["winusb"]) {
        return Can::No(format!("nusb reaches only WinUSB, and its driver is {}", shown(&i.service)));
    }
    if whole_device(i) {
        return Can::Yes("the whole device is driven by WinUSB".to_string());
    }
    if !is(&i.parent_service, &["usbccgp"]) {
        return Can::No(format!(
            "nusb reaches an interface of a composite device only under Windows' own usbccgp, and the device above is driven by {}",
            shown(&i.parent_service)
        ));
    }
    if i.winusb_guids.is_empty() {
        return Can::No("WinUSB is bound, but its registry names no device interface GUID, which nusb needs".to_string());
    }
    Can::Yes("its driver is WinUSB, under usbccgp, with a device interface GUID".to_string())
}

/// Google's adb on Windows: an interface that offers the Android USB
/// interface class, through any driver that does (Google's, a maker's, or
/// WinUSB told to offer it).
pub fn adb_exe_can(i: &DebugInterface) -> Can {
    if let Some(no) = no_driver(i) {
        return no;
    }
    if i.android_class {
        Can::Yes(format!("it offers the Android USB interface class {ANDROID_USB_CLASS}"))
    } else {
        Can::No(format!("it does not offer the Android USB interface class {ANDROID_USB_CLASS}"))
    }
}

/// This program's WinUSB route (`route_winusb.rs`), as Google's adb on
/// Windows opens a phone through WinUSB: an interface that offers the
/// Android USB interface class, by which it is found, driven by WinUSB,
/// whatever driver is above it.
pub fn winusb_can(i: &DebugInterface) -> Can {
    if let Some(no) = no_driver(i) {
        return no;
    }
    if !is(&i.service, &["winusb"]) {
        return Can::No(format!("this route speaks through WinUSB only, and its driver is {}", shown(&i.service)));
    }
    if !i.android_class {
        return Can::No(format!(
            "WinUSB is bound, but it does not offer the Android USB interface class {ANDROID_USB_CLASS}, by which this route finds it"
        ));
    }
    Can::Yes("its driver is WinUSB, and it offers the Android USB interface class, by which this route finds it".to_string())
}

fn can_words(can: &Can) -> String {
    match can {
        Can::Yes(why) => format!("yes: {why}"),
        Can::No(why) => format!("no: {why}"),
    }
}

/// The report in words, for the screen and for the results table.
pub fn words(report: &Report) -> Vec<String> {
    let mut out = Vec::new();
    if report.interfaces.is_empty() {
        out.push(
            "Windows lists no debugging interface of a phone. With USB debugging on, the cable in and the phone \
             unlocked, that means the cable carries no data, or USB debugging is off, or the phone shows its \
             interface only once File transfer is chosen on it."
                .to_string(),
        );
    }
    for (n, i) in report.interfaces.iter().enumerate() {
        out.push(format!("Debugging interface {}: {}", n + 1, shown(&i.name)));
        let state = if i.working {
            "working".to_string()
        } else {
            match i.problem {
                Some(p) => format!("not working (problem {p})"),
                None => "not working".to_string(),
            }
        };
        out.push(format!("  On the bus {}, {state}.", if i.bus.is_empty() { "(not said)" } else { &i.bus }));
        out.push(format!("  Its driver (service): {}", shown(&i.service)));
        out.push(format!(
            "  Its driver package: {}, from {}, version {}",
            shown(&i.inf),
            shown(&i.provider),
            shown(&i.version)
        ));
        out.push(format!("  The device above it: {}, driven by {}", shown(&i.parent_name), shown(&i.parent_service)));
        out.push(format!(
            "  Offers the Android USB interface class: {}",
            if i.android_class { "yes" } else { "no" }
        ));
        out.push(format!(
            "  Device interface GUIDs for WinUSB in its registry: {}",
            if i.winusb_guids.is_empty() { "none".to_string() } else { i.winusb_guids.join(", ") }
        ));
        out.push("  What each route can use, by its own rules (the run proves it):".to_string());
        out.push(format!("    this program's own adb over WinUSB: {}", can_words(&winusb_can(i))));
        out.push(format!("    adb_client over libusb: {}", can_words(&libusb_can(i))));
        out.push(format!("    adb_client over nusb: {}", can_words(&nusb_can(i))));
        out.push(format!("    Google's adb.exe: {}", can_words(&adb_exe_can(i))));
    }
    if report.portable.is_empty() {
        out.push("Portable devices on the USB bus (a phone offering files): none.".to_string());
    }
    for p in &report.portable {
        out.push(format!(
            "Portable device on the USB bus: {}, {}, driven by {}",
            shown(&p.name),
            if p.working { "working" } else { "not working" },
            shown(&p.service)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface(service: &str, parent: &str) -> DebugInterface {
        DebugInterface {
            name: Some("ADB Interface".to_string()),
            bus: "USB".to_string(),
            working: true,
            service: Some(service.to_string()),
            parent_service: Some(parent.to_string()),
            ..DebugInterface::default()
        }
    }

    fn yes(can: Can) -> bool {
        matches!(can, Can::Yes(_))
    }

    #[test]
    fn microsofts_own_winusb_under_usbccgp_reaches_every_route_that_wants_it() {
        let mut i = interface("WinUSB", "usbccgp");
        i.winusb_guids = vec![ANDROID_USB_CLASS.to_string()];
        i.android_class = true;
        assert!(yes(libusb_can(&i)));
        assert!(yes(nusb_can(&i)));
        assert!(yes(adb_exe_can(&i)));
        // With no GUID in the registry nusb cannot open it; libusb finds it
        // by its own enumeration.
        i.winusb_guids.clear();
        assert!(!yes(nusb_can(&i)));
        assert!(yes(libusb_can(&i)));
    }

    #[test]
    fn a_makers_own_driver_reaches_only_google_adb() {
        let mut i = interface("ssudadb", "dg_ssudbus");
        i.android_class = true;
        assert!(matches!(libusb_can(&i), Can::No(_)));
        assert!(matches!(nusb_can(&i), Can::No(why) if why.contains("ssudadb")));
        assert!(yes(adb_exe_can(&i)));
        assert!(matches!(winusb_can(&i), Can::No(why) if why.contains("ssudadb")));
    }

    #[test]
    fn the_test_phone_as_windows_listed_it_reaches_only_google_adb_and_the_winusb_route() {
        // Samsung's driver package binds WinUSB to the interface, under
        // Samsung's own composite driver, with the Android class offered.
        let galaxy = DebugInterface {
            name: Some("SAMSUNG Android ADB Interface".to_string()),
            inf: Some("oem56.inf".to_string()),
            provider: Some("SAMSUNG Electronics Co., Ltd.".to_string()),
            android_class: true,
            winusb_guids: vec![ANDROID_USB_CLASS.to_string()],
            ..interface("WINUSB", "dg_ssudbus")
        };
        assert!(matches!(libusb_can(&galaxy), Can::No(why) if why.contains("Operation not supported")));
        assert!(matches!(nusb_can(&galaxy), Can::No(why) if why.contains("dg_ssudbus")));
        assert!(yes(adb_exe_can(&galaxy)));
        assert!(yes(winusb_can(&galaxy)));
        // WinUSB with no Android class: the WinUSB route cannot find it.
        let unfound = DebugInterface { android_class: false, ..galaxy.clone() };
        assert!(matches!(winusb_can(&unfound), Can::No(why) if why.contains("does not offer")));
    }

    #[test]
    fn no_working_driver_reaches_nothing_and_a_whole_device_on_winusb_reaches_both_rust_routes() {
        let mut none = interface("", "usbccgp");
        none.service = None;
        none.working = false;
        none.problem = Some(28);
        for can in [libusb_can(&none), nusb_can(&none), adb_exe_can(&none)] {
            assert!(matches!(can, Can::No(why) if why.contains("problem 28")));
        }
        let whole = interface("WinUSB", "USBHUB3");
        assert!(yes(libusb_can(&whole)));
        assert!(yes(nusb_can(&whole)));
        assert!(!yes(adb_exe_can(&whole)));
        assert!(!yes(winusb_can(&whole)));
        assert!(matches!(winusb_can(&none), Can::No(why) if why.contains("problem 28")));
        assert!(yes(libusb_can(&interface("libusbK", "usbccgp"))));
    }

    #[test]
    fn the_words_say_every_fact_and_no_phone_plainly() {
        let empty = words(&Report::default());
        assert!(empty[0].starts_with("Windows lists no debugging interface"));
        let mut i = interface("WinUSB", "usbccgp");
        i.inf = Some("winusb.inf".to_string());
        i.provider = Some("Microsoft".to_string());
        let report = Report {
            interfaces: vec![i],
            portable: vec![PortableDevice { name: Some("Galaxy S20 Ultra 5G".to_string()), working: true, service: Some("WUDFWpdMtp".to_string()) }],
        };
        let text = words(&report).join("\n");
        assert!(text.contains("Its driver (service): WinUSB"));
        assert!(text.contains("winusb.inf, from Microsoft"));
        assert!(text.contains("driven by usbccgp"));
        assert!(text.contains("adb_client over libusb: yes"));
        assert!(text.contains("this program's own adb over WinUSB: no"));
        assert!(text.contains("Galaxy S20 Ultra 5G, working, driven by WUDFWpdMtp"));
    }
}
