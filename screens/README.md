# LiMo screens

What a child sees on the master PC, drawn into pixels in software: the lock
screen, the time left display and its notices. The look is the one agreed in
the repository's `design/` folder (the Paper and Ink theme, with every size,
colour and word of the PC's screens in `design/screens/pc.css` and `pc.js`),
and these pictures follow it as closely as a browser does.

There is no operating system code here: no window, no clock, no thread, no
file read at run time and no `unsafe`. A helper (the Windows one, and perhaps
one day a Mac one) puts the pictures on the screen, hands in clicks and keys,
and sends on what the child chooses. This crate decides nothing about the
rules: it shows what the engine orders.

```
src/
  lib.rs        the crate's face
  lock/         the lock screen: the machine (mod.rs), its layout (layout.rs)
  overlay.rs    the time left display and the notices
  words.rs      times and spans of time as words a child can read
  text.rs       the fonts: shaping, lines, glyphs
  icons.rs      the design's SVG icons, built in
  paint.rs      boxes, borders, dashes, shadows, the paper, the logo
  scene.rs      a screen laid out: drawing steps, clicks, room for text
  picture.rs    a picture: RGBA pixels
  samples.rs    every state, for the previews and the tests
fonts/          the three fonts of the theme, each with its licence
examples/
  preview.rs    writes a PNG of every state into target/preview/
tests/
  screens.rs    every state at every size, through the public face
```

## How a helper uses it

### The lock screen

One `Lock` for the session. Whenever the service sends a show that is a
lock, hand it over; it is a level, so the same show again changes nothing.

```rust
use limo_screens::{Answer, Clock, Input, Key, Lock, PcClock, Size, WayOut};

let mut lock = Lock::new();
let pc = PcClock::new(utc_offset_minutes, Clock::Hours12);
lock.show(&screen, child_name, now, pc);      // screen: limo_agent::LockScreen

// The main monitor gets the card; every other monitor only says to look
// at the main one.
let picture = lock.draw(Size::new(1920, 1080));   // RGBA, premultiplied
let other = lock.draw_other(Size::new(1280, 1024));

// Clicks, in pixels of the main monitor's picture, and keys.
if let Some(way_out) = lock.input(Input::Click { x, y }) { /* send it */ }
if let Some(way_out) = lock.input(Input::Key(Key::Char('7'))) { /* send it */ }

// The time, a few times a second; drawing again costs nothing until what
// the screen shows changes (the clock's minute, a wait ending).
lock.input(Input::Time { now });
```

What comes back is the way out to send, at most one at a time:

| `WayOut` | To the service (the helper's `Outbox`) |
| --- | --- |
| `AskForTime { minutes, reason }` | `outbox.ask_for_time(minutes, &reason)` |
| `TryCode { code }` | `outbox.try_code(Secret::new(code.take()))` |
| `TryPin { pin, action }` | `outbox.try_pin(Secret::new(pin.take()), action)` |

The answer comes back as the service's `Answer`, and goes in as this crate's
`Answer` with the same three cases: `Asked { request }`, `Tried { outcome }`
(the rules core's `TryOutcome`) and `Refused { message }`. The helper keeps
the number the outbox gave the way out and hands in only the answer that
carries it. While a way out waits for its answer its button is pressed in and
says "Sending…" or "Checking…"; after 30 seconds with no answer it can be
used again.

When the lock is taken down, call `lock.reset()`: the next lock starts with
the ways out, and nothing typed is kept.

`lock.target_at(x, y)` says what is under a point (for a hand cursor, say),
from the same layout the picture was drawn with. `lock.overflows(size)` lists
any text that does not fit its room; it is empty in every state the tests
know.

### The time left display and the notices

One `Overlay` for the session. Each picture is its own size, with its hard
shadow; its corners and the space beside the shadow are clear, so its window
takes each pixel's alpha (a layered window on Windows).

```rust
use limo_screens::{Notice, Overlay, TimeLeft, scale_for};

let mut overlay = Overlay::new();
let scale = scale_for(monitor_width, monitor_height);
if let Some(t) = TimeLeft::from_show(&show, now, pc) {
    let pill = overlay.time_left(&t, scale);
}
let warning = Notice::Warning { warning, now, pc };          // from the show
let closed = Notice::Closed { text };                       // from a notice
let picture = overlay.notice(&warning, scale);
let ok = overlay.notice_ok();   // where its OK button is, in its pixels
```

### Pixels

A `Picture` is RGBA, 8 bits a channel, premultiplied by alpha, rows from the
top, no padding: `picture.rgba()`. `picture.bgra()` gives the order a Windows
bitmap and a layered window take. The lock screen is opaque everywhere.

### Size

Everything is laid out for a screen of 1366 by 768 and scaled as one piece by
the smaller of width over 1366 and height over 768, kept between 0.8 and 2.2
(`scale_for`). The lock screen keeps to that rule wherever its card fits; on
a screen too small for the card at 0.8, such as 640 by 480, it is made
smaller still so that nothing is cut off at an edge (`lock_scale`).

The pictures are drawn as a browser draws the design pages: laid out at one
pixel to a CSS pixel (so a border of 1.5 is 1, as the design pages show it),
then scaled as one piece. Text is set at the scaled size, so it stays sharp.

## What is cached

So that the lock screen can be drawn a few times a second while its clock
ticks:

* **The picture.** Each picture is kept with everything it shows, in words
  (the view). A draw first works out the view for now, which is a few short
  strings; if it is the same as the last one at the same size, the same
  picture comes back, not drawn again (about 20 microseconds). The clock
  shows minutes, so the lock screen is drawn again once a minute, or when
  the child does something. The other monitors and the time left display
  and notices are kept the same way.
* **The paper.** The page with its grid of dots, for each size, copied under
  each new picture.
* **Text.** Each font instance (weight and optical size), each run of words
  shaped, each glyph's outline, and each glyph's coverage at a size and a
  quarter pixel position.
* **Icons and the logo.** Each icon painted at a size and a quarter pixel
  position; the logo at each size.

In a release build, the first draw of the lock screen
takes about 20 ms at 1366 by 768 and 35 ms at 1920 by 1080; drawing it again
after the clock moves on a minute takes about 3.5 ms and 6.5 ms.

## Secrets

A typed PIN or code is kept in memory that is wiped when it is sent, when the
panel is left, when a way out is taken away, and on `reset`. It never appears
in `Debug` output (of the lock, of a `WayOut` or of a `Typed`). The PIN is
drawn only as dots, the code as digits in their cells; a picture with digits
on it is wiped (its pixels set to zero) when the digits are. `Typed::take`
moves what was typed out without a copy, for the helper's own `Secret`.

## The libraries, and their licences

Each is pure Rust and actively kept up.

| Library | Licence | What it does here |
| --- | --- | --- |
| [tiny-skia](https://github.com/linebender/tiny-skia) 0.12 | BSD-3-Clause | Shapes: rounded rectangles, borders, dashes, shadows, the turned stamp, the pixel buffer; reads the logo's PNG and writes the previews |
| [resvg](https://github.com/linebender/resvg) and usvg 0.50 | Apache-2.0 OR MIT | Reads and paints the design's SVG icons as they are. Default features off: no text in SVG, no system fonts, no pictures inside SVG |
| [harfrust](https://github.com/harfbuzz/harfrust) 0.16 | MIT | Shaping and kerning: HarfBuzz's rules, ported to Rust |
| [read-fonts](https://github.com/googlefonts/fontations) 0.47 | MIT OR Apache-2.0 | Reads the fonts: glyph outlines and line metrics at any weight and optical size |
| [ab_glyph_rasterizer](https://github.com/alexheretic/ab-glyph) 0.1 | Apache-2.0 | Glyph outlines into exact coverage, for text that looks like a browser's |
| [zeroize](https://github.com/RustCrypto/utils) 1.9 | Apache-2.0 OR MIT | Wipes a typed PIN or code from memory |
| limo-agent, limo-core | Apache-2.0 | The engine's orders and the rules core's types |

Everything these bring with them, as `cargo metadata` lists it:

| Library | Licence |
| --- | --- |
| adler2 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| arrayref 0.3.9 | BSD-2-Clause |
| arrayvec 0.7.8 | MIT OR Apache-2.0 |
| autocfg 1.5.1 | Apache-2.0 OR MIT |
| bitflags 2.13.2 | MIT OR Apache-2.0 |
| bytemuck 1.25.2, bytemuck_derive 1.12.1 | Zlib OR Apache-2.0 OR MIT |
| cfg-if 1.0.5 | MIT OR Apache-2.0 |
| crc32fast 1.5.5 | MIT OR Apache-2.0 |
| data-url 0.3.2 | MIT OR Apache-2.0 |
| euclid 0.22.14 | MIT OR Apache-2.0 |
| fdeflate 0.3.7 | MIT OR Apache-2.0 |
| flate2 1.1.11 | MIT OR Apache-2.0 |
| float-cmp 0.9.0 | MIT |
| foldhash 0.2.0 | Zlib |
| font-types 0.12.7 | MIT OR Apache-2.0 |
| imagesize 0.15.0 | MIT |
| kurbo 0.13.1 | Apache-2.0 OR MIT |
| log 0.4.35 | MIT OR Apache-2.0 |
| memchr 2.8.3 | Unlicense OR MIT |
| miniz_oxide 0.8.9 and 0.9.1 | MIT OR Zlib OR Apache-2.0 |
| num-traits 0.2.19 | MIT OR Apache-2.0 |
| once_cell 1.21.4 | MIT OR Apache-2.0 |
| pico-args 0.5.0 | MIT |
| png 0.18.1 | MIT OR Apache-2.0 |
| polycool 0.4.0 | MIT OR Apache-2.0 |
| proc-macro2 1.0.107, quote 1.0.47, syn 3.0.10 | MIT OR Apache-2.0 |
| rgb 0.8.53 | MIT |
| roxmltree 0.21.2 | MIT OR Apache-2.0 |
| simd-adler32 0.3.10 | MIT |
| simplecss 0.2.2 | Apache-2.0 OR MIT |
| siphasher 1.0.4 | MIT OR Apache-2.0 |
| smallvec 1.16.6 | MIT OR Apache-2.0 |
| stable_deref_trait 1.2.1 | MIT OR Apache-2.0 |
| strict-num 0.1.1 | MIT |
| svgtypes 0.16.2 | Apache-2.0 OR MIT |
| synstructure 0.14.0 | MIT |
| tiny-skia-path 0.12.0 | BSD-3-Clause |
| unicode-ident 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| yoke 0.8.4, yoke-derive 0.8.4 | Unicode-3.0 |
| zerofrom 0.1.8, zerofrom-derive 0.1.8 | Unicode-3.0 |
| zlib-rs 0.6.8 | Zlib |

limo-agent and limo-core bring their own (serde, serde_json, SQLite through
rusqlite, the RustCrypto hashing crates and others); their READMEs list them.
Where a library offers a choice, LiMo takes the MIT or Apache-2.0 one. No
library was left out for its licence.

## The fonts, bundled

The three fonts of the theme are built into the crate, so a PC that does not
have them shows the same lock screen. Each is the upright variable font, as
Google Fonts publishes it, unchanged, beside its own licence:

| Font | File | Licence | Copyright |
| --- | --- | --- | --- |
| Fraunces | `fonts/fraunces/Fraunces-VariableFont_SOFT,WONK,opsz,wght.ttf` | SIL Open Font License 1.1, `fonts/fraunces/OFL.txt` | The Fraunces Project Authors (github.com/undercasetype/Fraunces): 2018 in its licence file, 2020 in the font's own name table |
| Instrument Sans | `fonts/instrument-sans/InstrumentSans-VariableFont_wdth,wght.ttf` | SIL Open Font License 1.1, `fonts/instrument-sans/OFL.txt` | 2022 The Instrument Sans Project Authors (github.com/Instrument/instrument-sans) |
| Spline Sans Mono | `fonts/spline-sans-mono/SplineSansMono-VariableFont_wght.ttf` | SIL Open Font License 1.1, `fonts/spline-sans-mono/OFL.txt` | 2022 The Spline Sans Mono Project Authors (github.com/SorkinType/SplineSansMono) |

The licence files are those of the Google Fonts repository for each font,
word for word. The fonts may be bundled with software under the OFL as long
as each copy carries the copyright notice and the licence, which these files
do; they are not sold on their own.

The weights are set on the variable fonts' own axes, as a browser sets them:
`wght` for every weight the design uses (400, 500, 600 and 700), and for
Fraunces `opsz` set to the font size in CSS pixels, as a browser's automatic
optical sizing does. Nothing is faked: no font is made bold by drawing it
twice.

The icons are the design's own files, `design/icons/svg/*.svg`, built in as
they are; the logo is `docs/images/lucidloom-logo.png`. Both are read when
the crate is built, never at run time.

## The previews

```text
cargo run --example preview
```

writes a PNG of every state into `screens/target/preview/` (inside the build
folder, which git ignores):

* `lock-<state>-1366.png`, `-1920.png`, `-1280.png`: the lock screen at
  1366 by 768, 1920 by 1080 and 1280 by 720, for every state: each reason
  (`time_used_up`, `blocked_hours`, `no_time_today`, `guardian_lock`,
  `held_by_another_device`, `waiting_for_time`, `account_not_linked`,
  `cannot_check`, `program_blocked`), each way out open (`panel-ask`,
  `panel-ask-empty`, `panel-ask-200`, `panel-ask-sending`, `panel-code`,
  `panel-code-checking`, `panel-pin`, `panel-pin-empty`), a request
  (`asked`, `answered`, `granted`), wrong tries (`wrong`, `wait`,
  `wait-and-answered`), `opened`, `clock_24h`, `long_words`, and
  `other_screen` for every monitor but the main one;
* `lock-time_used_up-640.png` and `lock-panel-pin-640.png`: the smallest
  screen, 640 by 480;
* `pill-<state>.png`: the time left display (`counting`, `paused`, `soon`,
  `last`, `nolimit`, `unlocked`), and `-1920` at the scale of 1920 by 1080;
* `notice-<kind>.png`: the notices (`warn15`, `warn5`, `warn1`, `bed15`,
  `unlock5`, `closed`), and `-1920`.

It also prints how long a draw takes. The sample child is Mira, and it is
Wednesday 18 November 2026, 9:42 pm.

## Tests

```text
cargo test
```

The machine's every path (each way out to its event, wrong tries, the wait,
Escape, a reason of 200 characters, digits past the limit, every answer),
the time words (midnight, a change of day, offsets, 12 and 24 hours), that
no text leaves its room in any state at the three sizes (and at 640 by 480
and 1024 by 768), that every state draws at nine sizes from 640 by 480 to
2560 by 1440 (and the main state at 3840 by 2160), and at odd sizes, without
a panic, clicks and keys anywhere, and
that secrets are wiped and never in `Debug`.
