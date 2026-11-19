//! `limo.exe`: LiMo for Windows. See the crate's README for each command.

#![forbid(unsafe_code)]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(limo_windows::run(&args));
}
