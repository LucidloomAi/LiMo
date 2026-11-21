//! Writes a throwaway APK, as the console's tests make them, to
//! `<folder>\phone\limo-child.apk`: LiMo's package name, version 0.3.0, a
//! release build, and nothing to run in it. For a dry build of the
//! installer (`../installer/build.ps1 -Throwaway`), which must never carry a
//! real phone app it was not given.
//!
//! ```text
//! cargo run --no-default-features --example throwaway_apk -- <folder>
//! ```

#[path = "../src/test_apk.rs"]
#[allow(dead_code)]
mod test_apk;

fn main() {
    let Some(folder) = std::env::args().nth(1) else {
        eprintln!("throwaway_apk <folder>");
        std::process::exit(2);
    };
    let path = test_apk::apk_in(std::path::Path::new(&folder), limo_cable::commands::PACKAGE, false);
    println!("{}", path.display());
}
