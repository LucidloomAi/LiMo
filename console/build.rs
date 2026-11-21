// The window's build step: Tauri's, with the one command the page may call
// listed, so that it needs a permission (allow-request) that the console's
// capability grants. Without the list, Tauri allows every app command to
// every window.
//
// Without the `window` feature (cargo test --no-default-features) there is
// no window and nothing to do.

fn main() {
    #[cfg(feature = "window")]
    window();
}

#[cfg(feature = "window")]
fn window() {
    let page = std::path::Path::new("../phone/app/dist/console/index.html");
    println!("cargo:rerun-if-changed=../phone/app/dist/console");
    if !page.is_file() {
        panic!(
            "The console's page is not built yet. In phone/app run `npm run build:console`, \
             or test without the window: cargo test --no-default-features"
        );
    }
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&["request"])),
    )
    .expect("tauri-build failed");
}
