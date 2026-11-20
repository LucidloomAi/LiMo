//! `limo-phone-dev`: see the crate's notes in `lib.rs` and its README.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = limo_phone_dev::run_real(&argv);
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
