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
    CM_Get_DevNode_Status, CR_SUCCESS, DIGCF_ALLCLASSES, DIGCF_PRESENT, DN_HAS_PROBLEM, DN_STARTED, HDEVINFO,
    SETUP_DI_REGISTRY_PROPERTY, SP_DEVINFO_DATA, SPDRP_COMPATIBLEIDS, SPDRP_DEVICEDESC, SPDRP_ENUMERATOR_NAME,
    SPDRP_FRIENDLYNAME,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW,
};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, GetLastError};
use windows_sys::core::GUID;

use crate::usb::{Kind, PORTABLE_DEVICE_CLASS, UsbDevice, UsbLook, is_debug_interface};

/// The most devices looked at, so that a list that never ends cannot hold
/// the tool up.
const MAX_DEVICES: u32 = 10_000;
/// The most bytes read for one property.
const MAX_PROPERTY_BYTES: u32 = 64 * 1024;

/// Windows' own list of USB devices, read only.
#[derive(Debug, Default)]
pub struct WindowsUsb;

impl UsbLook for WindowsUsb {
    fn phones_seen(&mut self) -> Result<Vec<UsbDevice>, String> {
        phones_seen()
    }
}

/// A device information set, destroyed when dropped.
struct DeviceSet(HDEVINFO);

impl Drop for DeviceSet {
    fn drop(&mut self) {
        // SAFETY: the set came from SetupDiGetClassDevsW and is destroyed
        // once, here.
        unsafe {
            SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}

/// True when two GUIDs are the same.
fn same_guid(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

/// The last error Windows set for this thread.
fn last_error() -> u32 {
    // SAFETY: reads this thread's last error; takes and writes nothing.
    unsafe { GetLastError() }
}

/// Every device present that looks like a phone.
fn phones_seen() -> Result<Vec<UsbDevice>, String> {
    // SAFETY: no class, no enumerator and no window: a new set of every
    // present device of every class, which DeviceSet destroys.
    let raw = unsafe {
        SetupDiGetClassDevsW(std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), DIGCF_PRESENT | DIGCF_ALLCLASSES)
    };
    if raw == -1 {
        return Err(format!("Windows did not give its list of devices (error {})", last_error()));
    }
    let set = DeviceSet(raw);
    let portable = GUID::from_u128(PORTABLE_DEVICE_CLASS);
    let mut seen = Vec::new();
    for index in 0..MAX_DEVICES {
        let mut data = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ClassGuid: GUID::default(),
            DevInst: 0,
            Reserved: 0,
        };
        // SAFETY: data is a whole SP_DEVINFO_DATA with its size set, as the
        // call asks; it writes only into data.
        if unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut data) } == 0 {
            let error = last_error();
            if error == ERROR_NO_MORE_ITEMS {
                break;
            }
            return Err(format!("Windows' list of devices could not be read through (error {error})"));
        }
        let kind = if same_guid(&data.ClassGuid, &portable) {
            Kind::PortableDevice
        } else if is_debug_interface(&strings(&set, &data, SPDRP_COMPATIBLEIDS)) {
            Kind::DebugInterface
        } else {
            continue;
        };
        let name = strings(&set, &data, SPDRP_FRIENDLYNAME)
            .into_iter()
            .next()
            .or_else(|| strings(&set, &data, SPDRP_DEVICEDESC).into_iter().next());
        let bus = strings(&set, &data, SPDRP_ENUMERATOR_NAME).into_iter().next().unwrap_or_default();
        seen.push(UsbDevice { kind, name, bus, working: working(data.DevInst) });
    }
    Ok(seen)
}

/// A text property of a device, one or several strings (a multi string
/// holds several), each trimmed, the empty ones left out; none when the
/// device has no such property.
fn strings(set: &DeviceSet, data: &SP_DEVINFO_DATA, property: SETUP_DI_REGISTRY_PROPERTY) -> Vec<String> {
    let mut needed: u32 = 0;
    let mut kind: u32 = 0;
    // SAFETY: a question of size alone: no buffer and a size of 0; Windows
    // writes how many bytes it needs into `needed` and its type into `kind`.
    let asked = unsafe {
        SetupDiGetDeviceRegistryPropertyW(set.0, data, property, &mut kind, std::ptr::null_mut(), 0, &mut needed)
    };
    if asked == 0 && last_error() != ERROR_INSUFFICIENT_BUFFER || needed == 0 || needed > MAX_PROPERTY_BYTES {
        return Vec::new();
    }
    let mut buffer = vec![0u16; (needed as usize).div_ceil(2)];
    let size = (buffer.len() * 2) as u32;
    // SAFETY: the buffer holds `size` bytes, at least what Windows said it
    // needs, and it writes at most `size` bytes into it.
    let read = unsafe {
        SetupDiGetDeviceRegistryPropertyW(set.0, data, property, &mut kind, buffer.as_mut_ptr().cast::<u8>(), size, &mut needed)
    };
    if read == 0 {
        return Vec::new();
    }
    buffer
        .split(|unit| *unit == 0)
        .map(|part| String::from_utf16_lossy(part).trim().to_string())
        .filter(|text| !text.is_empty())
        .collect()
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
    /// adb seeing nothing: `cargo test this_pcs_usb_list -- --ignored
    /// --nocapture`. Never a name, an id or a serial number.
    #[test]
    #[ignore]
    fn this_pcs_usb_list() {
        let seen = WindowsUsb.phones_seen();
        let case = crate::usb::decide(&[], seen.as_deref().map_err(String::as_str));
        let which = match case {
            Some(crate::usb::Case::NoPhone) => "no phone",
            Some(crate::usb::Case::PhoneWithoutDebugging { .. }) => "a phone, no debugging",
            Some(crate::usb::Case::NoDriver { .. }) => "a debugging interface with no working driver",
            Some(crate::usb::Case::DebuggingButNotAdb { .. }) => "a debugging interface, working",
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
