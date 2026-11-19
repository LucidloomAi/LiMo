//! The layer driven only through its own API, the calls Kotlin makes, with
//! the testkit playing the master PC.

mod common;
mod day;
mod kept;
mod link;
mod ways_out;

#[test]
fn the_versions_are_the_crates_own() {
    let v = limo_phone_ffi::versions();
    assert_eq!(v.ffi, env!("CARGO_PKG_VERSION"));
    assert_eq!(v.engine, limo_phone::phone_engine_version());
    assert_eq!(v.core, limo_core::core_version());
    assert_eq!(v.link, limo_link::link_version());
    assert_eq!(v.link_protocol, 1);
}
