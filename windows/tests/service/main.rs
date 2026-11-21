//! The service's stories, against a fake PC: the facts it gathers, its
//! helpers, the channel and its limits, the orders it carries out, and the
//! fallback; the console's door, against the fake PC and over a real named
//! pipe, and its answers written for the screens' tests; the link with the
//! phones, over real loopback sockets; and the installer's work in
//! `limo.exe`, its door the real service's. The engine
//! and the family's file are real; only Windows is faked.

mod common;

mod answers;
mod channel;
mod door;
mod facts;
mod fallback;
mod helpers;
mod installer;
mod link;
mod orders;
mod pipe;
