//! The PC's list of USB devices, read from Windows' own device list
//! (SetupAPI and the configuration manager), read only.
//!
//! One pass over every device present, of every class. For each one, what
//! is read: its class (from the device information Windows hands back) and
//! its compatible ids; and, for a portable device or a debugging interface
//! only, its friendly name and its description, the name of the bus it
//! hangs on (its enumerator: `USB`, or `SWD` for a device Windows made
//! itself, such as a USB stick's volume), and its status (started, and
//! whether it has a problem). Whether such a device looks like a phone is
//! decided in [`crate::usb`]. Nothing else is read: no serial number, no
//! instance id, no other property. Nothing is written, installed, started or
//! stopped, and no other program is run.

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_Status, CR_SUCCESS, DN_HAS_PROBLEM, DN_STARTED, SPDRP_COMPATIBLEIDS, SPDRP_DEVICEDESC,
    SPDRP_ENUMERATOR_NAME, SPDRP_FRIENDLYNAME,
};
use windows_sys::core::GUID;

use super::{DeviceSet, same_guid};
use crate::usb::{Kind, PORTABLE_DEVICE_CLASS, UsbDevice, UsbLook, is_debug_interface};

/// Windows' own list of USB devices, read only.
#[derive(Debug, Default)]
pub struct WindowsUsb;

impl UsbLook for WindowsUsb {
    fn phones_seen(&mut self) -> Result<Vec<UsbDevice>, String> {
        phones_seen()
    }
}

/// Every device present that looks like a phone.
fn phones_seen() -> Result<Vec<UsbDevice>, String> {
    let set = DeviceSet::new(None)?;
    let portable = GUID::from_u128(PORTABLE_DEVICE_CLASS);
    let mut seen = Vec::new();
    for data in set.devices()? {
        let kind = if same_guid(&data.ClassGuid, &portable) {
            Kind::PortableDevice
        } else if is_debug_interface(&set.strings(&data, SPDRP_COMPATIBLEIDS)) {
            Kind::DebugInterface
        } else {
            continue;
        };
        let name = set.string(&data, SPDRP_FRIENDLYNAME).or_else(|| set.string(&data, SPDRP_DEVICEDESC));
        let bus = set.string(&data, SPDRP_ENUMERATOR_NAME).unwrap_or_default();
        seen.push(UsbDevice { kind, name, bus, working: working(data.DevInst) });
    }
    Ok(seen)
}

/// True when Windows says the device's driver is loaded and it works:
/// started, with no problem.
fn working(device: u32) -> bool {
    let mut status: u32 = 0;
    let mut problem: u32 = 0;
    // SAFETY: writes two numbers about a device Windows just handed back.
    let answer = unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, device, 0) };
    answer == CR_SUCCESS && status & DN_HAS_PROBLEM == 0 && status & DN_STARTED != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portable_device_class_is_the_one_windows_names() {
        let wpd = GUID::from_u128(PORTABLE_DEVICE_CLASS);
        assert_eq!((wpd.data1, wpd.data2, wpd.data3), (0xEEC5_AD98, 0x8080, 0x425F));
        assert_eq!(wpd.data4, [0x92, 0x2A, 0xDA, 0xBF, 0x3D, 0xE3, 0xF6, 0x9A]);
        assert!(same_guid(&wpd, &GUID::from_u128(PORTABLE_DEVICE_CLASS)));
        assert!(!same_guid(&wpd, &GUID::default()));
    }

    /// Reads this PC's own list, and says only which case it would be with
    /// no phone reached: `cargo test this_pcs_usb_list -- --ignored
    /// --nocapture`. Never a name, an id or a serial number.
    #[test]
    #[ignore]
    fn this_pcs_usb_list() {
        let seen = WindowsUsb.phones_seen();
        let case = crate::usb::decide(false, seen.as_deref().map_err(String::as_str));
        let which = match case {
            Some(crate::usb::Case::NoPhone) => "no phone",
            Some(crate::usb::Case::PhoneWithoutDebugging { .. }) => "a phone, no debugging",
            Some(crate::usb::Case::NoDriver { .. }) => "a debugging interface with no working driver",
            Some(crate::usb::Case::DebuggingNotListed { .. }) => "a debugging interface, working",
            Some(crate::usb::Case::CannotLook(_)) => "the list could not be read",
            None => "none",
        };
        let seen = seen.unwrap_or_default();
        let of = |kind: Kind| seen.iter().filter(|d| d.kind == kind).count();
        let on_usb = seen.iter().filter(|d| d.kind == Kind::PortableDevice && crate::usb::looks_like_a_phone(d)).count();
        let named = seen.iter().filter(|d| d.name.is_some()).count();
        let working = seen.iter().filter(|d| d.working).count();
        println!(
            "portable devices: {} (on the USB bus: {on_usb}); debugging interfaces: {}; with a name: {named}; working: {working}; case: {which}",
            of(Kind::PortableDevice),
            of(Kind::DebugInterface)
        );
    }
}
