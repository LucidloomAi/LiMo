//! The service's stories, against a fake PC: the facts it gathers, its
//! helpers, the channel and its limits, the orders it carries out, and the
//! fallback. The engine and the family's file are real; only Windows is
//! faked.

mod common;

mod channel;
mod facts;
mod fallback;
mod helpers;
mod orders;
