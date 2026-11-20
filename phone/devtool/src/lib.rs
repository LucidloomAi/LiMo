//! # limo-phone-dev
//!
//! A small PC tool that puts a test family on a child's phone over the USB
//! cable, so that LiMo can be tried on a real phone before pairing over the
//! network exists. Scaffolding, like `limo dev-setup` and `limo dev-code`
//! on the PC: it goes once the link (`link/` and its two halves) pairs a
//! phone for real.
//!
//! What it promises (the README next to this crate says more):
//!
//! - **On the phone, only LiMo.** It changes nothing on the phone but LiMo's
//!   own app and LiMo's own files. It never turns on developer options or USB
//!   debugging, and never touches the phone's accounts, its Device Owner or
//!   its settings.
//! - **No secret on a command line.** The pairing file, which holds the
//!   phone's code secret, goes to the phone through adb's standard input;
//!   every argument of every call is fixed text, a package name, a time or a
//!   path.
//! - **Never the real family.** Its test family is a file of its own, in a
//!   folder named for testing, and the Windows service's file is never
//!   opened.
//! - **Every adb call has a timeout**, and one phone only: none, two, one
//!   that has not allowed this PC, or a release build, each has its own
//!   plain message. When adb sees no phone, Windows' own list of USB devices
//!   says why ([`usb`]): a cable that only charges, USB debugging off, or no
//!   driver for the phone's debugging interface.
//! - **A PIN or a code is never printed by mistake**, nor put in an error or
//!   in the tool's log. The code `code` makes is printed once, on purpose.
//!
//! The commands are written against three traits, [`adb::Adb`],
//! [`usb::UsbLook`] and [`ui::Ui`], so that the tests play a phone and a
//! PC's USB ports with fakes and read every line printed.
//!
//! **Unsafe code only in `win/`**, where Windows' list of USB devices is
//! read; every other module forbids it, each unsafe block there says why it
//! is sound, and a test (`tests/house_rules.rs`) checks both.

#![warn(missing_docs)]

pub mod adb;
pub mod apk;
pub mod cli;
pub mod commands;
pub mod family;
pub mod pairing;
pub mod phone;
pub mod places;
pub mod status;
pub mod ui;
pub mod usb;
#[cfg(windows)]
pub mod win;

pub use commands::{DONE, FAILED, REFUSED, Tool, run};

/// Runs the tool with the real terminal, the real adb, this user's folder
/// and this PC's clock.
pub fn run_real(argv: &[String]) -> i32 {
    let places = match places::Places::for_this_user() {
        Ok(places) => places,
        Err(problem) => {
            println!("{problem}");
            return REFUSED;
        }
    };
    let mut adb = adb::RealAdb::new();
    let mut terminal = ui::Terminal;
    let clock = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    };
    #[cfg(windows)]
    let mut usb = win::usb::WindowsUsb;
    #[cfg(not(windows))]
    let mut usb = usb::NoLook;
    let mut tool = Tool { adb: &mut adb, usb: &mut usb, ui: &mut terminal, places, clock: &clock };
    run(&mut tool, argv)
}
