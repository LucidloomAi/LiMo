fn main() {
    // Listing the app's commands here makes each one need a permission
    // (allow-<command>) that a capability must grant. Without this list,
    // Tauri allows every app command to every window.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "family_view",
            "give_minutes",
            "lock_child",
            "self_check_state",
            "self_check_report",
        ]),
    ))
    .expect("tauri-build failed");
}
