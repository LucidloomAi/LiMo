//! The one place the console sends a guardian out of itself: LucidLoom AI's
//! page, in the PC's own browser, on a guardian's tap of "More info". The
//! address is fixed here; nothing the page says can change it.

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

use super::wide;

/// Where "More info" goes.
pub const MORE_INFO: &str = "https://lucidloom.ai";

/// Opens [`MORE_INFO`] in the browser Windows opens web addresses with. True
/// when Windows says it did.
pub fn open_more_info() -> bool {
    let verb = wide("open");
    let address = wide(MORE_INFO);
    // SAFETY: hands Windows two strings that outlive the call; it starts the
    // default browser with the fixed address and returns.
    let result = unsafe { ShellExecuteW(None, PCWSTR(verb.as_ptr()), PCWSTR(address.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    // Windows says success with a number above 32.
    result.0 as usize > 32
}
