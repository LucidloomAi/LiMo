//! The phone engine's stories: a fake phone drives the engine through whole
//! days with its own wall clock, elapsed count and start count, keeping
//! only what the engine asks to keep, against a real hub on a real file in
//! a temporary folder, with the child's PC beside it when a story wants it.
//!
//! One test program, so that the fake phone in `common` is compiled once.

mod common;

mod clock;
mod covers;
mod day;
mod kept;
mod restart;
mod ways_out;
