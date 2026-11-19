// UniFFI's bindings generator, as a small binary inside this crate, so the
// generator is always the same UniFFI version as the library.
//
// `cargo run --features bindgen --bin uniffi-bindgen -- generate <the built library> --language kotlin --no-format --out-dir <folder>`
//
// phone/app/scripts/build-engine.mjs runs it after each build of the library.

fn main() {
    uniffi::uniffi_bindgen_main()
}
