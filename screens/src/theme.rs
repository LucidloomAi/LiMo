//! The Paper and Ink theme: its colours, and the rule that scales a screen.
//!
//! The values are those of `design/tokens.json` and `design/screens/pc.css`.

/// A colour with its alpha, as the design gives it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// 0 to 1.
    pub a: f32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    Rgba { r, g, b, a: 1.0 }
}

impl Rgba {
    /// The same colour at this alpha.
    pub(crate) const fn alpha(self, a: f32) -> Rgba {
        Rgba { r: self.r, g: self.g, b: self.b, a }
    }

    /// The colour for tiny-skia.
    pub(crate) fn skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, alpha_byte(self.a))
    }
}

/// An alpha of 0 to 1 as a byte.
pub(crate) fn alpha_byte(a: f32) -> u8 {
    let a = if a.is_finite() { a.clamp(0.0, 1.0) } else { 0.0 };
    (a * 255.0).round() as u8
}

/// The page.
pub(crate) const PAPER: Rgba = rgb(0xf6, 0xf1, 0xe4);
/// A quieter band, and the tile behind an icon in a row.
pub(crate) const PAPER2: Rgba = rgb(0xef, 0xe7, 0xd2);
/// Cards and buttons.
pub(crate) const CARD: Rgba = rgb(0xff, 0xfd, 0xf6);
/// Text, borders, shadows.
pub(crate) const INK: Rgba = rgb(0x1b, 0x26, 0x38);
/// Secondary text.
pub(crate) const INK_SOFT: Rgba = rgb(0x46, 0x50, 0x6a);
/// Time, and the things that are LiMo's own.
pub(crate) const MARIGOLD: Rgba = rgb(0xe8, 0x9c, 0x1c);
/// Marigold for text and large numbers.
pub(crate) const MARIGOLD_DEEP: Rgba = rgb(0xb9, 0x7a, 0x08);
/// Allowed, at home, connected, done.
pub(crate) const TEAL: Rgba = rgb(0x0e, 0x7c, 0x66);
/// Locked, blocked, stop, danger.
pub(crate) const CORAL: Rgba = rgb(0xcf, 0x43, 0x26);
/// A quiet line: dividers inside a card.
pub(crate) const BORDER: Rgba = INK.alpha(0.16);
/// The hard shadow under a card or a button.
pub(crate) const SHADOW: Rgba = INK.alpha(0.85);
/// The dots of the paper's grid.
pub(crate) const DOT: Rgba = INK.alpha(0.13);
/// The dashed outline of a request that was made.
pub(crate) const REQUEST_LINE: Rgba = INK.alpha(0.42);
/// The faint coral behind the wait after wrong tries.
pub(crate) const WAIT_WASH: Rgba = CORAL.alpha(0.06);
/// The words shown in an empty field.
pub(crate) const PLACEHOLDER: Rgba = INK_SOFT.alpha(0.7);
/// The words shown in the empty PIN box.
pub(crate) const PIN_PLACEHOLDER: Rgba = INK_SOFT.alpha(0.8);

/// The screen the design is laid out for, in pixels.
pub const DESIGN_WIDTH: f32 = 1366.0;
/// The screen the design is laid out for, in pixels.
pub const DESIGN_HEIGHT: f32 = 768.0;
/// The smallest scale: a small laptop screen.
pub const MIN_SCALE: f32 = 0.8;
/// The largest scale: a big monitor.
pub const MAX_SCALE: f32 = 2.2;

/// How much a screen of this size scales the design: the smaller of width
/// over 1366 and height over 768, kept between 0.8 and 2.2. The lock screen
/// uses it, and the time left display and the notices take the same scale
/// from the monitor they are on.
pub fn scale_for(width: u32, height: u32) -> f32 {
    let k = (width as f32 / DESIGN_WIDTH).min(height as f32 / DESIGN_HEIGHT);
    if k.is_finite() { k.clamp(MIN_SCALE, MAX_SCALE) } else { MIN_SCALE }
}

/// A border of `width` design pixels as a browser lays it out at one device
/// pixel to a CSS pixel, before the design is scaled: a whole number of
/// pixels, at least one. The design pages are seen that way, so 1.5 is 1.
pub(crate) fn border_width(width: f32) -> f32 {
    width.floor().max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_follows_the_smaller_side_and_is_kept_in_bounds() {
        assert_eq!(scale_for(1366, 768), 1.0);
        assert!((scale_for(1920, 1080) - 1920.0 / 1366.0).abs() < 1e-6);
        assert!((scale_for(1280, 720) - 1280.0 / 1366.0).abs() < 1e-6);
        assert_eq!(scale_for(640, 480), MIN_SCALE);
        assert_eq!(scale_for(7680, 4320), MAX_SCALE);
        assert_eq!(scale_for(0, 0), MIN_SCALE);
        // A tall screen scales by its width.
        assert!((scale_for(1366, 2000) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn borders_are_whole_pixels_before_the_scale() {
        assert_eq!(border_width(1.5), 1.0);
        assert_eq!(border_width(2.0), 2.0);
        assert_eq!(border_width(0.5), 1.0);
    }
}
