// UniFFI's bindings generator, as a small binary inside this crate, so the
// generator is always the same UniFFI version as the library.
//
// cargo run --features bindgen --bin uniffi-bindgen -- generate <path to the built library> --language kotlin --no-format --out-dir <dir>
//
// app/scripts/build-core.mjs runs it this way after each Android build.

fn main() {
    uniffi::uniffi_bindgen_main()
}
