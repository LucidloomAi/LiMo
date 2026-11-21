//! Windows' own lists of devices and drivers (SetupAPI, the configuration
//! manager, and each device's own registry key), read only.
//!
//! One pass over every device present, of every class. For a debugging
//! interface (compatible id `USB\Class_FF&SubClass_42&Prot_01`): its name,
//! its bus, its status, its driver's service, its driver package (INF,
//! provider, version), the device above it with that device's name and
//! service, whether it offers the Android USB interface class (a second
//! list: the devices present that offer it), and the `DeviceInterfaceGUIDs`
//! or `DeviceInterfaceGUID` value of its own registry key, opened to read.
//! For a portable device on the USB bus: its name, status and service.
//! Nothing else is read: no serial number, no instance id. Nothing is
//! written, installed, started or stopped, and no other program is run.

use std::collections::HashSet;

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_Status, CM_Get_Parent, CR_SUCCESS, DICS_FLAG_GLOBAL, DIGCF_ALLCLASSES, DIGCF_DEVICEINTERFACE,
    DIGCF_PRESENT, DIREG_DEV, DN_HAS_PROBLEM, DN_STARTED, HDEVINFO, SETUP_DI_REGISTRY_PROPERTY, SP_DEVINFO_DATA,
    SPDRP_COMPATIBLEIDS, SPDRP_DEVICEDESC, SPDRP_ENUMERATOR_NAME, SPDRP_FRIENDLYNAME, SPDRP_SERVICE,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDevicePropertyW,
    SetupDiGetDeviceRegistryPropertyW, SetupDiOpenDevRegKey,
};
use windows_sys::Win32::Devices::Properties::{
    DEVPKEY_Device_DriverInfPath, DEVPKEY_Device_DriverProvider, DEVPKEY_Device_DriverVersion, DEVPROP_TYPE_STRING,
    DEVPROPTYPE,
};
use windows_sys::Win32::Foundation::{DEVPROPKEY, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, GetLastError};
use windows_sys::Win32::System::Registry::{HKEY, KEY_READ, REG_MULTI_SZ, REG_SZ, RegCloseKey, RegQueryValueExW};
use windows_sys::core::GUID;

use crate::drivers::{DebugInterface, PortableDevice, Report};

/// The compatible id of a phone's debugging interface.
const DEBUG_INTERFACE_ID: &str = "USB\\Class_FF&SubClass_42&Prot_01";
/// Windows' class of portable devices (WPD).
const PORTABLE_DEVICE_CLASS: u128 = 0xEEC5_AD98_8080_425F_922A_DABF_3DE3_F69A;
/// The Android USB interface class, as a number.
const ANDROID_CLASS: u128 = 0xF72F_E0D4_CBCB_407D_8814_9ED6_73D0_DD6B;
/// The most devices looked at.
const MAX_DEVICES: u32 = 10_000;
/// The most bytes read for one property or value.
const MAX_BYTES: u32 = 64 * 1024;

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

fn last_error() -> u32 {
    // SAFETY: reads this thread's last error; takes and writes nothing.
    unsafe { GetLastError() }
}

impl DeviceSet {
    /// The devices present, of every class, or of those offering the
    /// interface class `interface`.
    fn new(interface: Option<&GUID>) -> Result<DeviceSet, String> {
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
    fn devices(&self) -> Result<Vec<SP_DEVINFO_DATA>, String> {
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

    /// A text property of a device, one or several strings, each trimmed,
    /// the empty ones left out.
    fn strings(&self, data: &SP_DEVINFO_DATA, property: SETUP_DI_REGISTRY_PROPERTY) -> Vec<String> {
        let mut needed: u32 = 0;
        let mut kind: u32 = 0;
        // SAFETY: a question of size alone: no buffer and a size of 0;
        // Windows writes how many bytes it needs and the value's type.
        let asked = unsafe {
            SetupDiGetDeviceRegistryPropertyW(self.0, data, property, &mut kind, std::ptr::null_mut(), 0, &mut needed)
        };
        if asked == 0 && last_error() != ERROR_INSUFFICIENT_BUFFER || needed == 0 || needed > MAX_BYTES {
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
    fn string(&self, data: &SP_DEVINFO_DATA, property: SETUP_DI_REGISTRY_PROPERTY) -> Option<String> {
        self.strings(data, property).into_iter().next()
    }

    /// A text property of a device by its property key (the driver's
    /// package, its provider, its version).
    fn keyed_string(&self, data: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<String> {
        let mut kind: DEVPROPTYPE = 0;
        let mut needed: u32 = 0;
        // SAFETY: a question of size alone: no buffer and a size of 0.
        let asked = unsafe {
            SetupDiGetDevicePropertyW(self.0, data, key, &mut kind, std::ptr::null_mut(), 0, &mut needed, 0)
        };
        if asked == 0 && last_error() != ERROR_INSUFFICIENT_BUFFER || needed == 0 || needed > MAX_BYTES {
            return None;
        }
        let mut buffer = vec![0u16; (needed as usize).div_ceil(2)];
        let size = (buffer.len() * 2) as u32;
        // SAFETY: the buffer holds `size` bytes, at least what Windows said
        // it needs, and it writes at most `size` bytes into it.
        let read = unsafe {
            SetupDiGetDevicePropertyW(self.0, data, key, &mut kind, buffer.as_mut_ptr().cast::<u8>(), size, &mut needed, 0)
        };
        if read == 0 || kind != DEVPROP_TYPE_STRING {
            return None;
        }
        split_wide(&buffer).into_iter().next()
    }

    /// The WinUSB device interface GUIDs in the device's own registry key,
    /// opened to read only.
    fn winusb_guids(&self, data: &SP_DEVINFO_DATA) -> Vec<String> {
        // SAFETY: opens the device's own key with KEY_READ; the key is
        // closed below.
        let key: HKEY = unsafe { SetupDiOpenDevRegKey(self.0, data, DICS_FLAG_GLOBAL, 0, DIREG_DEV, KEY_READ) };
        if key.is_null() || key as isize == -1 {
            return Vec::new();
        }
        let mut found = Vec::new();
        for name in ["DeviceInterfaceGUIDs", "DeviceInterfaceGUID"] {
            found.extend(registry_strings(key, name));
        }
        // SAFETY: the key was opened above and is closed once, here.
        unsafe {
            RegCloseKey(key);
        }
        found
    }
}

/// A text or multi text value of a registry key opened to read.
fn registry_strings(key: HKEY, name: &str) -> Vec<String> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut kind: u32 = 0;
    let mut size: u32 = 0;
    // SAFETY: a question of size alone, with a name ending in a zero.
    let asked = unsafe { RegQueryValueExW(key, wide.as_ptr(), std::ptr::null(), &mut kind, std::ptr::null_mut(), &mut size) };
    if asked != 0 || size == 0 || size > MAX_BYTES || (kind != REG_SZ && kind != REG_MULTI_SZ) {
        return Vec::new();
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
    let mut bytes = (buffer.len() * 2) as u32;
    // SAFETY: the buffer holds `bytes` bytes, at least the size Windows
    // gave, and it writes at most that.
    let read = unsafe {
        RegQueryValueExW(key, wide.as_ptr(), std::ptr::null(), &mut kind, buffer.as_mut_ptr().cast::<u8>(), &mut bytes)
    };
    if read != 0 { Vec::new() } else { split_wide(&buffer) }
}

/// Wide text split at its zeros, each part trimmed, empty ones left out.
fn split_wide(buffer: &[u16]) -> Vec<String> {
    buffer
        .split(|unit| *unit == 0)
        .map(|part| String::from_utf16_lossy(part).trim().to_string())
        .filter(|text| !text.is_empty())
        .collect()
}

/// Started with no problem, and the problem number when there is one.
fn status(device: u32) -> (bool, Option<u32>) {
    let mut flags: u32 = 0;
    let mut problem: u32 = 0;
    // SAFETY: writes two numbers about a device Windows just handed back.
    let answer = unsafe { CM_Get_DevNode_Status(&mut flags, &mut problem, device, 0) };
    if answer != CR_SUCCESS {
        return (false, None);
    }
    let has_problem = flags & DN_HAS_PROBLEM != 0;
    (!has_problem && flags & DN_STARTED != 0, has_problem.then_some(problem))
}

/// The device above this one.
fn parent(device: u32) -> Option<u32> {
    let mut above: u32 = 0;
    // SAFETY: writes one number about a device Windows just handed back.
    let answer = unsafe { CM_Get_Parent(&mut above, device, 0) };
    (answer == CR_SUCCESS).then_some(above)
}

fn same_guid(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

/// Reads the report.
pub fn read() -> Result<Report, String> {
    let all = DeviceSet::new(None)?;
    let devices = all.devices()?;
    let android_guid = GUID::from_u128(ANDROID_CLASS);
    let android: HashSet<u32> = DeviceSet::new(Some(&android_guid))
        .and_then(|set| set.devices())
        .map(|list| list.iter().map(|d| d.DevInst).collect())
        .unwrap_or_default();
    let portable_class = GUID::from_u128(PORTABLE_DEVICE_CLASS);
    let mut report = Report::default();
    for data in &devices {
        let name = || all.string(data, SPDRP_FRIENDLYNAME).or_else(|| all.string(data, SPDRP_DEVICEDESC));
        let bus = all.string(data, SPDRP_ENUMERATOR_NAME).unwrap_or_default();
        if same_guid(&data.ClassGuid, &portable_class) {
            if bus.eq_ignore_ascii_case("USB") {
                let (working, _) = status(data.DevInst);
                report.portable.push(PortableDevice { name: name(), working, service: all.string(data, SPDRP_SERVICE) });
            }
            continue;
        }
        let compatible = all.strings(data, SPDRP_COMPATIBLEIDS);
        if !compatible.iter().any(|id| id.eq_ignore_ascii_case(DEBUG_INTERFACE_ID)) {
            continue;
        }
        let (working, problem) = status(data.DevInst);
        let above = parent(data.DevInst).and_then(|p| devices.iter().find(|d| d.DevInst == p));
        report.interfaces.push(DebugInterface {
            name: name(),
            bus,
            working,
            problem,
            service: all.string(data, SPDRP_SERVICE),
            inf: all.keyed_string(data, &DEVPKEY_Device_DriverInfPath),
            provider: all.keyed_string(data, &DEVPKEY_Device_DriverProvider),
            version: all.keyed_string(data, &DEVPKEY_Device_DriverVersion),
            parent_name: above.and_then(|p| all.string(p, SPDRP_FRIENDLYNAME).or_else(|| all.string(p, SPDRP_DEVICEDESC))),
            parent_service: above.and_then(|p| all.string(p, SPDRP_SERVICE)),
            android_class: android.contains(&data.DevInst),
            winusb_guids: all.winusb_guids(data),
        });
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::ANDROID_USB_CLASS;

    #[test]
    fn the_android_class_is_the_one_the_report_names() {
        let guid = GUID::from_u128(ANDROID_CLASS);
        let shown = format!(
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{}}}",
            guid.data1,
            guid.data2,
            guid.data3,
            guid.data4[0],
            guid.data4[1],
            guid.data4[2..].iter().map(|b| format!("{b:02X}")).collect::<String>()
        );
        assert_eq!(shown, ANDROID_USB_CLASS);
        assert_eq!(split_wide(&[0x41, 0, 0x20, 0x42, 0, 0]), vec!["A".to_string(), "B".to_string()]);
    }

    /// Reads this PC's own lists and prints only how many of each there are,
    /// and which facts could be read, never what they are:
    /// `cargo test this_pcs_drivers -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn this_pcs_drivers() {
        let report = read().unwrap();
        println!("debugging interfaces: {}; portable devices on the USB bus: {}", report.interfaces.len(), report.portable.len());
        let read = |v: &Option<String>| if v.is_some() { "read" } else { "none" };
        for i in &report.interfaces {
            println!(
                "name {}, service {}, inf {}, provider {}, version {}, parent {}, parent service {}, guids {}",
                read(&i.name),
                read(&i.service),
                read(&i.inf),
                read(&i.provider),
                read(&i.version),
                read(&i.parent_name),
                read(&i.parent_service),
                i.winusb_guids.len()
            );
        }
    }
}
