//! Everything that calls Windows itself, and the only unsafe code in this
//! crate: reading the PC's list of USB devices ([`usb`]). Every other
//! module forbids unsafe code, and a test (`tests/house_rules.rs`) checks
//! it and that each unsafe block here says why it is sound.

pub mod usb;
