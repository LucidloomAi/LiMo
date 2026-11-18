//! The agent engine's stories: a fake PC drives the engine through whole
//! days with a fake wall clock and steady count, against a real hub on a
//! real file in a temporary folder.
//!
//! One test program, so that the fake PC in `common` is compiled once.

mod common;

mod clock;
mod day;
mod phone;
mod programs;
mod random;
mod restart;
mod sessions;
mod ways_out;
