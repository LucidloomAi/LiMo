//! A picture: pixels, ready for a window.

use std::fmt;

use tiny_skia::{IntSize, Pixmap, PixmapMut};
use zeroize::Zeroize;

/// The largest side of a picture, in pixels: an 8K monitor is 7680 wide.
/// A larger size asked for is drawn at this size.
pub const MAX_SIDE: u32 = 8192;

/// A size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Size {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
}

impl Size {
    /// A size in pixels.
    pub const fn new(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    /// The size a picture is drawn at: at least 1 by 1 and at most
    /// [`MAX_SIDE`] each way.
    pub(crate) fn kept(self) -> Size {
        Size { width: self.width.clamp(1, MAX_SIDE), height: self.height.clamp(1, MAX_SIDE) }
    }
}

/// A picture drawn by this crate.
///
/// Its pixels are RGBA, 8 bits each, with the colour premultiplied by the
/// alpha, row by row from the top, with no padding between rows. The lock
/// screen is fully opaque; the time left display and the notices are not:
/// their corners and the space beside their shadows are clear, so their
/// window must take each pixel's alpha.
#[derive(Clone, PartialEq, Eq)]
pub struct Picture {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl fmt::Debug for Picture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Picture").field("width", &self.width).field("height", &self.height).finish()
    }
}

impl Picture {
    /// A clear picture of this size, kept to between 1 by 1 and
    /// [`MAX_SIDE`] each way.
    pub(crate) fn clear(size: Size) -> Picture {
        let size = size.kept();
        let bytes = (size.width as usize) * (size.height as usize) * 4;
        Picture { width: size.width, height: size.height, data: vec![0; bytes] }
    }

    /// The pixels to draw into. None only if tiny-skia refused the size,
    /// which a kept size never is; then nothing is drawn.
    pub(crate) fn canvas(&mut self) -> Option<PixmapMut<'_>> {
        PixmapMut::from_bytes(&mut self.data, self.width, self.height)
    }

    /// Pixels across.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Pixels down.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The size.
    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// The pixels: RGBA, premultiplied, rows from the top, no padding.
    pub fn rgba(&self) -> &[u8] {
        &self.data
    }

    /// The pixels as BGRA, premultiplied, rows from the top: the order a
    /// Windows bitmap and a layered window take.
    pub fn bgra(&self) -> Vec<u8> {
        let mut out = self.data.clone();
        for px in out.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        out
    }

    /// One pixel, RGBA premultiplied, or None outside the picture.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        let px = self.data.get(at..at + 4)?;
        Some([px[0], px[1], px[2], px[3]])
    }

    /// The picture as a PNG file, for previews.
    pub fn png(&self) -> Option<Vec<u8>> {
        let size = IntSize::from_wh(self.width, self.height)?;
        let pixmap = Pixmap::from_vec(self.data.clone(), size)?;
        pixmap.encode_png().ok()
    }

    /// Clears every pixel to zero, so nothing drawn stays in memory.
    pub(crate) fn wipe(&mut self) {
        self.data.zeroize();
        let bytes = (self.width as usize) * (self.height as usize) * 4;
        self.data.resize(bytes, 0);
    }
}
