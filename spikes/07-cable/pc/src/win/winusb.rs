//! The phone's debugging interface through Windows' own WinUSB, the way
//! Google's adb opens it on Windows: no libusb, no nusb, no adb.exe.
//!
//! Found by the Android USB interface class: SetupAPI's list of the device
//! interfaces present of class `{F72FE0D4-CBCB-407D-8814-9ED673D0DD6B}`,
//! which Google's driver, the makers' drivers (Samsung's among them) and an
//! adbd asking WinUSB through Microsoft's descriptors all offer. Each one's
//! path is opened with `CreateFileW` (to read and write, for overlapped
//! input and output, as WinUSB asks), handed to `WinUsb_Initialize`, and its
//! two bulk pipes found with `WinUsb_QueryPipe`. Transfers are overlapped,
//! each with an event of its own, so that the reader and the writer run
//! side by side; each has a time limit (`PIPE_TRANSFER_TIMEOUT`, with a
//! wait of our own behind it that aborts the pipe). Writes that fill their
//! last packet exactly end with a zero length packet
//! (`SHORT_PACKET_TERMINATE`).
//!
//! The device path holds the phone's serial number: it stays in memory, is
//! handed to Windows, and is never shown or written anywhere.

use std::sync::Mutex;
use std::time::Duration;

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA, SPDRP_DEVICEDESC, SPDRP_FRIENDLYNAME,
    SPDRP_SERVICE, SetupDiEnumDeviceInterfaces, SetupDiGetDeviceInterfaceDetailW,
};
use windows_sys::Win32::Devices::Usb::{
    PIPE_TRANSFER_TIMEOUT, SHORT_PACKET_TERMINATE, USB_INTERFACE_DESCRIPTOR, UsbdPipeTypeBulk, WINUSB_INTERFACE_HANDLE,
    WINUSB_PIPE_INFORMATION, WINUSB_PIPE_POLICY, WinUsb_AbortPipe, WinUsb_Free, WinUsb_GetOverlappedResult,
    WinUsb_Initialize, WinUsb_QueryInterfaceSettings, WinUsb_QueryPipe, WinUsb_ReadPipe, WinUsb_SetPipePolicy,
    WinUsb_WritePipe,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_DEVICE_NOT_CONNECTED, ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING,
    ERROR_NO_MORE_ITEMS, ERROR_OPERATION_ABORTED, ERROR_SEM_TIMEOUT, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::OVERLAPPED;
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows_sys::core::GUID;

use super::drivers::{ANDROID_CLASS, DeviceSet, last_error};
use crate::protocol::Wire;

/// How long one write may take.
const WRITE_WAIT: Duration = Duration::from_secs(10);
/// How long past a transfer's own time limit this side waits before it
/// aborts the pipe itself.
const BACKSTOP: Duration = Duration::from_secs(2);
/// The most interfaces looked at.
const MAX_INTERFACES: u32 = 64;

/// One interface of the Android USB class, found. Its `Debug` leaves out
/// the path, which holds the phone's serial number.
pub struct Found {
    path: Vec<u16>,
    /// Its name as Windows shows it.
    pub name: Option<String>,
    /// Its driver's service name.
    pub service: Option<String>,
}

impl std::fmt::Debug for Found {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Found({:?}, driver {:?})", self.name, self.service)
    }
}

/// The interfaces present that offer the Android USB interface class.
pub fn interfaces() -> Result<Vec<Found>, String> {
    let guid = GUID::from_u128(ANDROID_CLASS);
    let set = DeviceSet::new(Some(&guid))?;
    let mut found = Vec::new();
    for index in 0..MAX_INTERFACES {
        let mut data = SP_DEVICE_INTERFACE_DATA {
            cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            InterfaceClassGuid: GUID::default(),
            Flags: 0,
            Reserved: 0,
        };
        // SAFETY: data is a whole SP_DEVICE_INTERFACE_DATA with its size
        // set, the GUID outlives the call, and Windows writes only into data.
        if unsafe { SetupDiEnumDeviceInterfaces(set.0, std::ptr::null(), &guid, index, &mut data) } == 0 {
            let error = last_error();
            if error == ERROR_NO_MORE_ITEMS {
                break;
            }
            return Err(format!("Windows' list of device interfaces could not be read through (error {error})"));
        }
        let mut needed: u32 = 0;
        // SAFETY: a question of size alone: no buffer and a size of 0;
        // Windows writes how many bytes it needs into `needed`.
        unsafe {
            SetupDiGetDeviceInterfaceDetailW(set.0, &data, std::ptr::null_mut(), 0, &mut needed, std::ptr::null_mut());
        }
        let offset = std::mem::offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
        if (needed as usize) < offset + 2 || needed > 64 * 1024 {
            continue;
        }
        // Whole u32s, so the structure's alignment holds.
        let mut buffer = vec![0u32; (needed as usize).div_ceil(4)];
        let detail = buffer.as_mut_ptr().cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
        // SAFETY: the buffer holds at least `needed` bytes, more than the
        // structure's fixed part, and is aligned for it; only its size field
        // is written, as the call asks.
        unsafe {
            (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        }
        let mut info = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ClassGuid: GUID::default(),
            DevInst: 0,
            Reserved: 0,
        };
        // SAFETY: the buffer holds `needed` bytes, the size Windows asked
        // for, and info is a whole SP_DEVINFO_DATA with its size set.
        let read = unsafe { SetupDiGetDeviceInterfaceDetailW(set.0, &data, detail, needed, std::ptr::null_mut(), &mut info) };
        if read == 0 {
            continue;
        }
        let units = (needed as usize - offset) / 2;
        // SAFETY: the path starts `offset` bytes into the buffer, which holds
        // `needed` bytes, so `units` wide characters from there are inside it;
        // the buffer is aligned for u32, so for u16 too.
        let words: &[u16] = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>().add(offset).cast::<u16>(), units) };
        let end = words.iter().position(|&u| u == 0).unwrap_or(words.len());
        let mut path = words[..end].to_vec();
        path.push(0);
        found.push(Found {
            path,
            name: set.string(&info, SPDRP_FRIENDLYNAME).or_else(|| set.string(&info, SPDRP_DEVICEDESC)),
            service: set.string(&info, SPDRP_SERVICE),
        });
    }
    Ok(found)
}

/// Why WinUSB could not open the interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// Another program holds it (Google's adb server, Android Studio).
    Busy,
    /// It went before it could be opened (unplugged, debugging off).
    Gone,
    /// Its driver is not WinUSB, so WinUSB cannot take it.
    NotWinUsb(String),
    /// Anything else.
    Other(String),
}

/// The phone's debugging interface, open through WinUSB.
pub struct WinUsbWire {
    file: HANDLE,
    usb: WINUSB_INTERFACE_HANDLE,
    /// The bulk pipe from the phone.
    pub pipe_in: u8,
    /// The bulk pipe to the phone.
    pub pipe_out: u8,
    packet: usize,
    /// The reader's lock, holding the IN pipe's time limit now set.
    reading: Mutex<u32>,
    writing: Mutex<()>,
}

// SAFETY: WinUSB's handles may be used from any thread. Reads use only the
// IN pipe and writes only the OUT pipe, each behind its own lock, and every
// transfer has its own event and OVERLAPPED, waited for before it is let go.
unsafe impl Send for WinUsbWire {}
// SAFETY: as above: nothing in it is touched without its lock but the two
// handles, which WinUSB lets several threads use at once.
unsafe impl Sync for WinUsbWire {}

impl std::fmt::Debug for WinUsbWire {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WinUsbWire(in 0x{:02x}, out 0x{:02x}, packet {})", self.pipe_in, self.pipe_out, self.packet)
    }
}

impl Drop for WinUsbWire {
    fn drop(&mut self) {
        // SAFETY: both handles were opened by `open` and are let go once,
        // here, WinUSB's first.
        unsafe {
            WinUsb_Free(self.usb);
            CloseHandle(self.file);
        }
    }
}

fn set_policy(usb: WINUSB_INTERFACE_HANDLE, pipe: u8, policy: WINUSB_PIPE_POLICY, value: &[u8]) -> Result<(), String> {
    // SAFETY: the value points at `value.len()` bytes that outlive the call,
    // the size the policy takes (one byte for a flag, four for a time).
    let set = unsafe { WinUsb_SetPipePolicy(usb, pipe, policy, value.len() as u32, value.as_ptr().cast()) };
    if set == 0 { Err(format!("WinUSB would not set a pipe's policy (error {})", last_error())) } else { Ok(()) }
}

/// Opens one interface found by [`interfaces`].
pub fn open(found: &Found) -> Result<WinUsbWire, OpenError> {
    // SAFETY: the path ends in a zero and outlives the call; no security
    // attributes and no template.
    let file = unsafe {
        CreateFileW(
            found.path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
            std::ptr::null_mut(),
        )
    };
    if file == INVALID_HANDLE_VALUE {
        return Err(match last_error() {
            ERROR_ACCESS_DENIED => OpenError::Busy,
            ERROR_FILE_NOT_FOUND | ERROR_DEVICE_NOT_CONNECTED => OpenError::Gone,
            error => OpenError::Other(format!("Windows could not open it (error {error})")),
        });
    }
    let mut usb: WINUSB_INTERFACE_HANDLE = std::ptr::null_mut();
    // SAFETY: the file was opened just above; Windows writes the new handle
    // into `usb`.
    if unsafe { WinUsb_Initialize(file, &mut usb) } == 0 {
        let error = last_error();
        // SAFETY: the file was opened just above and is let go once, here.
        unsafe {
            CloseHandle(file);
        }
        return Err(match error {
            ERROR_ACCESS_DENIED => OpenError::Busy,
            ERROR_DEVICE_NOT_CONNECTED => OpenError::Gone,
            error => OpenError::NotWinUsb(format!("WinUsb_Initialize refused it (error {error}), so its driver is not WinUSB")),
        });
    }
    let mut wire = WinUsbWire { file, usb, pipe_in: 0, pipe_out: 0, packet: 0, reading: Mutex::new(0), writing: Mutex::new(()) };
    let mut interface = USB_INTERFACE_DESCRIPTOR {
        bLength: 0,
        bDescriptorType: 0,
        bInterfaceNumber: 0,
        bAlternateSetting: 0,
        bNumEndpoints: 0,
        bInterfaceClass: 0,
        bInterfaceSubClass: 0,
        bInterfaceProtocol: 0,
        iInterface: 0,
    };
    // SAFETY: Windows writes one whole descriptor into `interface`.
    if unsafe { WinUsb_QueryInterfaceSettings(usb, 0, &mut interface) } == 0 {
        return Err(OpenError::Other(format!("WinUSB did not describe the interface (error {})", last_error())));
    }
    if (interface.bInterfaceClass, interface.bInterfaceSubClass, interface.bInterfaceProtocol) != (0xff, 0x42, 0x01) {
        return Err(OpenError::Other("the interface WinUSB opened is not a debugging interface".to_string()));
    }
    for index in 0..interface.bNumEndpoints {
        let mut pipe = WINUSB_PIPE_INFORMATION { PipeType: 0, PipeId: 0, MaximumPacketSize: 0, Interval: 0 };
        // SAFETY: Windows writes one whole pipe's facts into `pipe`.
        if unsafe { WinUsb_QueryPipe(usb, 0, index, &mut pipe) } == 0 || pipe.PipeType != UsbdPipeTypeBulk {
            continue;
        }
        if pipe.PipeId & 0x80 != 0 {
            wire.pipe_in = pipe.PipeId;
            wire.packet = usize::from(pipe.MaximumPacketSize);
        } else {
            wire.pipe_out = pipe.PipeId;
        }
    }
    if wire.pipe_in == 0 || wire.pipe_out == 0 || wire.packet == 0 {
        return Err(OpenError::Other("the debugging interface lacks its two bulk pipes".to_string()));
    }
    set_policy(usb, wire.pipe_out, SHORT_PACKET_TERMINATE, &[1]).map_err(OpenError::Other)?;
    set_policy(usb, wire.pipe_out, PIPE_TRANSFER_TIMEOUT, &(WRITE_WAIT.as_millis() as u32).to_le_bytes()).map_err(OpenError::Other)?;
    Ok(wire)
}

/// What one transfer did: the bytes it moved, and Windows' error if it did
/// not end well.
struct Moved {
    bytes: u32,
    error: Option<u32>,
}

impl WinUsbWire {
    /// Runs one overlapped transfer that `start` begins on `pipe`, with an
    /// event of its own, and waits for it: at most `wait` and the backstop,
    /// after which the pipe is aborted and its end waited for, so that the
    /// buffer is never let go while Windows may still write into it.
    fn transfer(&self, pipe: u8, wait: Duration, start: impl FnOnce(*const OVERLAPPED) -> i32) -> Moved {
        // SAFETY: a new manual reset event, not signalled, with no name;
        // closed below.
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event.is_null() {
            return Moved { bytes: 0, error: Some(last_error()) };
        }
        let overlapped = OVERLAPPED { hEvent: event, ..OVERLAPPED::default() };
        let began = start(&overlapped);
        let mut error = None;
        if began == 0 {
            let e = last_error();
            if e != ERROR_IO_PENDING {
                error = Some(e);
            }
        }
        let mut bytes: u32 = 0;
        if error.is_none() {
            let limit = (wait + BACKSTOP).as_millis().min(u128::from(u32::MAX - 1)) as u32;
            // SAFETY: waits on the event made above.
            if unsafe { WaitForSingleObject(event, limit) } != WAIT_OBJECT_0 {
                // SAFETY: aborts this interface's pipe; the transfer then ends.
                unsafe {
                    WinUsb_AbortPipe(self.usb, pipe);
                }
            }
            // SAFETY: waits for this transfer's end (TRUE), whatever it was,
            // so nothing writes into its buffer after this returns.
            if unsafe { WinUsb_GetOverlappedResult(self.usb, &overlapped, &mut bytes, 1) } == 0 {
                error = Some(last_error());
            }
        }
        // SAFETY: the event made above, let go once, after the transfer ended.
        unsafe {
            CloseHandle(event);
        }
        Moved { bytes, error }
    }
}

impl Wire for WinUsbWire {
    fn packet(&self) -> usize {
        self.packet
    }

    fn read(&self, want: usize, wait: Duration) -> Result<Option<Vec<u8>>, String> {
        let mut limit = self.reading.lock().map_err(|_| "the reader's lock failed".to_string())?;
        let ms = wait.as_millis().clamp(1, u128::from(u32::MAX)) as u32;
        if *limit != ms {
            set_policy(self.usb, self.pipe_in, PIPE_TRANSFER_TIMEOUT, &ms.to_le_bytes())?;
            *limit = ms;
        }
        let mut buffer = vec![0u8; want];
        let pointer = buffer.as_mut_ptr();
        let moved = self.transfer(self.pipe_in, wait, |overlapped| {
            // SAFETY: the buffer holds `want` bytes and lives until `transfer`
            // has waited for this read's end.
            unsafe { WinUsb_ReadPipe(self.usb, self.pipe_in, pointer, want as u32, std::ptr::null_mut(), overlapped) }
        });
        buffer.truncate((moved.bytes as usize).min(want));
        match moved.error {
            None => Ok(Some(buffer)),
            Some(ERROR_SEM_TIMEOUT | ERROR_OPERATION_ABORTED) if buffer.is_empty() => Ok(None),
            Some(ERROR_SEM_TIMEOUT | ERROR_OPERATION_ABORTED) => Ok(Some(buffer)),
            Some(error) => Err(format!("reading from the phone failed (Windows error {error})")),
        }
    }

    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        let _one_at_a_time = self.writing.lock().map_err(|_| "the writer's lock failed".to_string())?;
        let moved = self.transfer(self.pipe_out, WRITE_WAIT, |overlapped| {
            // SAFETY: the bytes live until `transfer` has waited for this
            // write's end.
            unsafe { WinUsb_WritePipe(self.usb, self.pipe_out, bytes.as_ptr(), bytes.len() as u32, std::ptr::null_mut(), overlapped) }
        });
        match moved.error {
            None if moved.bytes as usize == bytes.len() => Ok(()),
            None => Err(format!("only {} of {} bytes reached the phone", moved.bytes, bytes.len())),
            Some(error) => Err(format!("writing to the phone failed (Windows error {error})")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads this PC's own list of Android class interfaces and prints only
    /// how many there are and which facts could be read; nothing is opened:
    /// `cargo test this_pcs_android_interfaces -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn this_pcs_android_interfaces() {
        let found = interfaces().unwrap();
        println!("Android class interfaces: {}", found.len());
        for f in &found {
            println!(
                "name {}, driver {}, path of {} characters",
                if f.name.is_some() { "read" } else { "none" },
                if f.service.is_some() { "read" } else { "none" },
                f.path.len()
            );
        }
    }
}
