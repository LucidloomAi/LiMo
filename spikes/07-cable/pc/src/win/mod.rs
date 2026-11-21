//! On Windows only, and the only unsafe code in this program: Windows' own
//! lists of devices and drivers, read only (`drivers.rs`), and the phone's
//! debugging interface through WinUSB (`winusb.rs`).

pub mod drivers;
pub mod winusb;
