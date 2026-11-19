//! Writes a PNG of every state of every screen into `target/preview/`,
//! to set beside the design pages:
//!
//! ```text
//! cargo run --example preview
//! ```
//!
//! The lock screen is drawn at 1366 by 768, 1920 by 1080 and 1280 by 720
//! (and two states at 640 by 480, the smallest);
//! the time left display and the notices at the scale of 1366 by 768 and of
//! 1920 by 1080.

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use limo_screens::{Overlay, Size, samples, scale_for};

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("preview");
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("Cannot make {}: {e}", dir.display());
        return;
    }
    let mut written = 0;
    let save = |name: String, png: Option<Vec<u8>>, written: &mut u32| match png {
        Some(bytes) => match fs::write(dir.join(&name), bytes) {
            Ok(()) => *written += 1,
            Err(e) => eprintln!("Cannot write {name}: {e}"),
        },
        None => eprintln!("Cannot encode {name}"),
    };
    let sizes = [(1366, 768), (1920, 1080), (1280, 720)];
    for (name, mut lock) in samples::lock_states() {
        for (w, h) in sizes {
            let started = Instant::now();
            let png = lock.draw(Size::new(w, h)).png();
            let first = started.elapsed();
            let again = Instant::now();
            let _ = lock.draw(Size::new(w, h));
            let second = again.elapsed();
            if name == "time_used_up" {
                // The clock moves on a minute: drawn again, with everything
                // but the picture itself already cached.
                lock.input(limo_screens::Input::Time { now: samples::NOW + 60 });
                let tick = Instant::now();
                let _ = lock.draw(Size::new(w, h));
                let minute = tick.elapsed();
                lock.input(limo_screens::Input::Time { now: samples::NOW });
                println!("lock {w}x{h}: first draw {first:?}, same state again {second:?}, a minute later {minute:?}");
            }
            save(format!("lock-{name}-{w}.png"), png, &mut written);
        }
        // The smallest screen the design is kept for, for two states.
        if name == "time_used_up" || name == "panel-pin" {
            save(format!("lock-{name}-640.png"), lock.draw(Size::new(640, 480)).png(), &mut written);
        }
    }
    let mut lock = samples::lock_for(&samples::screen(limo_agent::LockWhy::TimeUsedUp));
    for (w, h) in sizes {
        save(format!("lock-other_screen-{w}.png"), lock.draw_other(Size::new(w, h)).png(), &mut written);
    }
    let mut overlay = Overlay::new();
    for (scale, suffix) in [(scale_for(1366, 768), ""), (scale_for(1920, 1080), "-1920")] {
        for (name, t) in samples::time_left_states() {
            save(format!("pill-{name}{suffix}.png"), overlay.time_left(&t, scale).png(), &mut written);
        }
        for (name, n) in samples::notice_states() {
            save(format!("notice-{name}{suffix}.png"), overlay.notice(&n, scale).png(), &mut written);
        }
    }
    println!("{written} pictures in {}", dir.display());
}
