//! Everything that calls Windows itself, and the only unsafe code in this
//! crate: reading the PC's list of USB devices ([`usb`], read only), and the
//! phone's debugging interface through WinUSB ([`winusb`]). Every other
//! module forbids unsafe code, and a test (`tests/house_rules.rs`) checks it,
//! that each unsafe block here says why it is sound, and which Windows calls
//! each file makes.

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_ALLCLASSES, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, SETUP_DI_REGISTRY_PROPERTY, SP_DEVINFO_DATA,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW,
};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, GetLastError};
use windows_sys::core::GUID;

pub mod usb;
#[cfg(feature = "adb")]
pub mod winusb;

/// The most devices looked at, so that a list that never ends cannot hold
/// the caller up.
const MAX_DEVICES: u32 = 10_000;
/// The most bytes read for one property.
const MAX_PROPERTY_BYTES: u32 = 64 * 1024;

/// A device information set, destroyed when dropped.
pub(crate) struct DeviceSet(pub(crate) HDEVINFO);

impl Drop for DeviceSet {
    fn drop(&mut self) {
        // SAFETY: the set came from SetupDiGetClassDevsW and is destroyed
        // once, here.
        unsafe {
            SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}

/// The last error Windows set for this thread.
pub(crate) fn last_error() -> u32 {
    // SAFETY: reads this thread's last error; takes and writes nothing.
    unsafe { GetLastError() }
}

impl DeviceSet {
    /// The devices present, of every class, or of those offering the
    /// device interface class `interface`.
    pub(crate) fn new(interface: Option<&GUID>) -> Result<DeviceSet, String> {
        let (class, flags) = match interface {
            Some(guid) => (guid as *const GUID, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE),
            None => (std::ptr::null(), DIGCF_PRESENT | DIGCF_ALLCLASSES),
        };
        // SAFETY: the class is null or points at a GUID that outlives the
        // call; no enumerator and no window. A new set, which DeviceSet
        // destroys.
        let raw = unsafe { SetupDiGetClassDevsW(class, std::ptr::null(), std::ptr::null_mut(), flags) };
        if raw == -1 {
            return Err(format!("Windows did not give its list of devices (error {})", last_error()));
        }
        Ok(DeviceSet(raw))
    }

    /// Every device in the set.
    pub(crate) fn devices(&self) -> Result<Vec<SP_DEVINFO_DATA>, String> {
        let mut out = Vec::new();
        for index in 0..MAX_DEVICES {
            let mut data = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ClassGuid: GUID::default(),
                DevInst: 0,
                Reserved: 0,
            };
            // SAFETY: data is a whole SP_DEVINFO_DATA with its size set, as
            // the call asks; it writes only into data.
            if unsafe { SetupDiEnumDeviceInfo(self.0, index, &mut data) } == 0 {
                let error = last_error();
                if error == ERROR_NO_MORE_ITEMS {
                    break;
                }
                return Err(format!("Windows' list of devices could not be read through (error {error})"));
            }
            out.push(data);
        }
        Ok(out)
    }

    /// A text property of a device, one or several strings (a multi string
    /// holds several), each trimmed, the empty ones left out; none when the
    /// device has no such property.
    pub(crate) fn strings(&self, data: &SP_DEVINFO_DATA, property: SETUP_DI_REGISTRY_PROPERTY) -> Vec<String> {
        let mut needed: u32 = 0;
        let mut kind: u32 = 0;
        // SAFETY: a question of size alone: no buffer and a size of 0;
        // Windows writes how many bytes it needs into `needed` and its type
        // into `kind`.
        let asked = unsafe {
            SetupDiGetDeviceRegistryPropertyW(self.0, data, property, &mut kind, std::ptr::null_mut(), 0, &mut needed)
        };
        if asked == 0 && last_error() != ERROR_INSUFFICIENT_BUFFER || needed == 0 || needed > MAX_PROPERTY_BYTES {
            return Vec::new();
        }
        let mut buffer = vec![0u16; (needed as usize).div_ceil(2)];
        let size = (buffer.len() * 2) as u32;
        // SAFETY: the buffer holds `size` bytes, at least what Windows said
        // it needs, and it writes at most `size` bytes into it.
        let read = unsafe {
            SetupDiGetDeviceRegistryPropertyW(self.0, data, property, &mut kind, buffer.as_mut_ptr().cast::<u8>(), size, &mut needed)
        };
        if read == 0 { Vec::new() } else { split_wide(&buffer) }
    }

    /// The first text of a property.
    pub(crate) fn string(&self, data: &SP_DEVINFO_DATA, property: SETUP_DI_REGISTRY_PROPERTY) -> Option<String> {
        self.strings(data, property).into_iter().next()
    }
}

/// Wide text split at its zeros, each part trimmed, empty ones left out.
pub(crate) fn split_wide(buffer: &[u16]) -> Vec<String> {
    buffer
        .split(|unit| *unit == 0)
        .map(|part| String::from_utf16_lossy(part).trim().to_string())
        .filter(|text| !text.is_empty())
        .collect()
}

/// True when two GUIDs are the same.
pub(crate) fn same_guid(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_text_is_split_at_its_zeros() {
        assert_eq!(split_wide(&[0x41, 0, 0x20, 0x42, 0, 0]), vec!["A".to_string(), "B".to_string()]);
        let a = GUID::from_u128(1);
        assert!(same_guid(&a, &GUID::from_u128(1)));
        assert!(!same_guid(&a, &GUID::default()));
    }
}
