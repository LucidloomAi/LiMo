//! What a helper got when it started, read from itself: its token's
//! UIAccess flag and integrity level, the z order band of a topmost window
//! of its own, and its process object's security descriptor. Ported from the
//! first spike's report.

use std::ffi::c_void;

use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE, HLOCAL, HWND, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1, SE_KERNEL_OBJECT,
};
use windows::Win32::Security::{
    DACL_SECURITY_INFORMATION, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, LABEL_SECURITY_INFORMATION,
    OBJECT_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
    TokenIntegrityLevel, TokenUIAccess,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP};
use windows::core::{BOOL, PWSTR, s, w};

use super::Owned;
use crate::protocol::FromHelper;

/// This process's own token, for reading.
fn own_token() -> Option<Owned> {
    let mut token = HANDLE::default();
    // SAFETY: opens this process's own token for reading; closed by `Owned`.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()? };
    Owned::new(token)
}

/// The UIAccess flag on this process's token.
pub(crate) fn own_uiaccess() -> Option<bool> {
    let token = own_token()?;
    let mut value: u32 = 0;
    let mut returned: u32 = 0;
    // SAFETY: writes a 4 byte value into the one handed to it.
    unsafe { GetTokenInformation(token.raw(), TokenUIAccess, Some(&mut value as *mut u32 as *mut c_void), 4, &mut returned).ok()? };
    Some(value != 0)
}

/// This process's integrity level: the last part of its label's SID.
pub(crate) fn own_integrity() -> Option<u32> {
    let token = own_token()?;
    let mut buffer = [0u64; 16];
    let mut returned: u32 = 0;
    // SAFETY: writes at most the buffer's size; the label's SID points into
    // the buffer, which outlives the reads below.
    unsafe {
        GetTokenInformation(
            token.raw(),
            TokenIntegrityLevel,
            Some(buffer.as_mut_ptr() as *mut c_void),
            std::mem::size_of_val(&buffer) as u32,
            &mut returned,
        )
        .ok()?;
        let label = &*(buffer.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let count = *GetSidSubAuthorityCount(label.Label.Sid);
        if count == 0 {
            return None;
        }
        Some(*GetSidSubAuthority(label.Label.Sid, u32::from(count) - 1))
    }
}

/// This process object's security descriptor, as SDDL.
pub(crate) fn own_process_sddl() -> Option<String> {
    let with_label = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION;
    process_sddl(with_label).or_else(|| process_sddl(OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION))
}

fn process_sddl(parts: OBJECT_SECURITY_INFORMATION) -> Option<String> {
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: both buffers come from Windows and are freed with LocalFree
    // once read.
    unsafe {
        let found = GetSecurityInfo(GetCurrentProcess(), SE_KERNEL_OBJECT, parts, None, None, None, None, Some(&mut descriptor));
        if found != ERROR_SUCCESS {
            return None;
        }
        let mut text = PWSTR::null();
        let converted = ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, SDDL_REVISION_1, parts, &mut text, None);
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        converted.ok()?;
        let sddl = text.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
        sddl
    }
}

/// `BOOL GetWindowBand(HWND, DWORD*)`, which Windows does not document.
type GetWindowBandFn = unsafe extern "system" fn(HWND, *mut u32) -> BOOL;

/// The z order band of a window of this process, if Windows says.
pub(crate) fn window_band(window: HWND) -> Option<u32> {
    // SAFETY: looks the function up at run time, so a Windows without it
    // gives None; the signature is the one it is known to have.
    unsafe {
        let user32 = GetModuleHandleW(w!("user32.dll")).ok()?;
        let found = GetProcAddress(user32, s!("GetWindowBand"))?;
        let get_band = std::mem::transmute::<unsafe extern "system" fn() -> isize, GetWindowBandFn>(found);
        let mut band: u32 = 0;
        get_band(window, &mut band).as_bool().then_some(band)
    }
}

/// The band a topmost window of this process gets: one made 1 by 1 pixel,
/// off screen, never shown, and destroyed at once.
pub(crate) fn topmost_band() -> Option<u32> {
    // SAFETY: makes and destroys a window of this process's own.
    unsafe {
        let window = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("LiMo band check"),
            WS_POPUP,
            -32_000,
            -32_000,
            1,
            1,
            None,
            None,
            None,
            None,
        )
        .ok()?;
        let band = window_band(window);
        let _ = DestroyWindow(window);
        band
    }
}

/// The message that tells the service what this helper got.
pub(crate) fn about() -> FromHelper {
    FromHelper::About {
        uiaccess: own_uiaccess(),
        integrity: own_integrity(),
        band: topmost_band(),
        process_sddl: own_process_sddl(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::about::IL_LOW;

    /// Reads what this test process got: run from a terminal, no UIAccess,
    /// an integrity of at least low, and a descriptor that can be read. The
    /// band check makes one hidden 1 by 1 window, never shown, and destroys
    /// it.
    #[test]
    fn this_process_reads_what_it_got() {
        assert_eq!(own_uiaccess(), Some(false));
        assert!(own_integrity().is_some_and(|level| level >= IL_LOW));
        assert!(own_process_sddl().is_some_and(|sddl| sddl.contains("D:")));
        let FromHelper::About { process_sddl, .. } = about() else { panic!("not about") };
        assert!(process_sddl.is_some());
    }
}
