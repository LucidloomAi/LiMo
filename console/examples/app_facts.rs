//! Says what an APK is, as the console reads it before it puts LiMo on a
//! phone (limo-cable's `read_apk`): its package, its version name, and
//! whether it is a debug build. The installer's build runs it on the phone
//! app it is given, and refuses anything but LiMo's child form as a release
//! build.
//!
//! ```text
//! cargo run --no-default-features --example app_facts -- <apk>
//! ```
//!
//! Prints one line of JSON: `{"package":...,"version":...,"debuggable":...}`;
//! when the file is not an APK, a sentence saying why, and exit code 1 (both
//! on standard output, which the build reads).

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("app_facts <apk>");
        std::process::exit(2);
    };
    match limo_cable::apk::read_apk(std::path::Path::new(&path)) {
        Ok(facts) => println!(
            "{}",
            serde_json::json!({ "package": facts.package, "version": facts.version_name, "debuggable": facts.debuggable })
        ),
        Err(problem) => {
            println!("{problem}");
            std::process::exit(1);
        }
    }
}
