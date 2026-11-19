//! The icons: the design's own SVG files, built in and painted as they are.

use std::collections::HashMap;
use std::rc::Rc;

use tiny_skia::{Pixmap, Transform};

/// The icons the PC's screens use, each one of `design/icons/svg/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Icon {
    Account,
    Alert,
    Away,
    Bell,
    Block,
    Calendar,
    Check,
    Code,
    Hand,
    Hourglass,
    Info,
    Lock,
    Moon,
    Pin,
    Sync,
    Unlock,
}

impl Icon {
    pub(crate) const ALL: [Icon; 16] = [
        Icon::Account,
        Icon::Alert,
        Icon::Away,
        Icon::Bell,
        Icon::Block,
        Icon::Calendar,
        Icon::Check,
        Icon::Code,
        Icon::Hand,
        Icon::Hourglass,
        Icon::Info,
        Icon::Lock,
        Icon::Moon,
        Icon::Pin,
        Icon::Sync,
        Icon::Unlock,
    ];

    /// The SVG file, as the design exports it.
    fn svg(self) -> &'static str {
        match self {
            Icon::Account => include_str!("../../design/icons/svg/account.svg"),
            Icon::Alert => include_str!("../../design/icons/svg/alert.svg"),
            Icon::Away => include_str!("../../design/icons/svg/away.svg"),
            Icon::Bell => include_str!("../../design/icons/svg/bell.svg"),
            Icon::Block => include_str!("../../design/icons/svg/block.svg"),
            Icon::Calendar => include_str!("../../design/icons/svg/calendar.svg"),
            Icon::Check => include_str!("../../design/icons/svg/check.svg"),
            Icon::Code => include_str!("../../design/icons/svg/code.svg"),
            Icon::Hand => include_str!("../../design/icons/svg/hand.svg"),
            Icon::Hourglass => include_str!("../../design/icons/svg/hourglass.svg"),
            Icon::Info => include_str!("../../design/icons/svg/info.svg"),
            Icon::Lock => include_str!("../../design/icons/svg/lock.svg"),
            Icon::Moon => include_str!("../../design/icons/svg/moon.svg"),
            Icon::Pin => include_str!("../../design/icons/svg/pin.svg"),
            Icon::Sync => include_str!("../../design/icons/svg/sync.svg"),
            Icon::Unlock => include_str!("../../design/icons/svg/unlock.svg"),
        }
    }
}

/// The size an icon's drawing is made at, in its own units.
const GRID: f32 = 48.0;
/// The most icon pictures kept.
const CACHE_LIMIT: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    icon: Icon,
    /// The size in device pixels, in 64ths.
    size: u32,
    quarter_x: u8,
    quarter_y: u8,
}

/// Every icon read, and every icon painted.
pub(crate) struct Icons {
    trees: HashMap<Icon, Option<usvg::Tree>>,
    painted: HashMap<Key, Rc<Pixmap>>,
}

impl Icons {
    pub(crate) fn new() -> Icons {
        Icons { trees: HashMap::new(), painted: HashMap::new() }
    }

    /// True when the icon's file can be read.
    pub(crate) fn readable(&mut self, icon: Icon) -> bool {
        self.tree(icon).is_some()
    }

    fn tree(&mut self, icon: Icon) -> Option<&usvg::Tree> {
        self.trees
            .entry(icon)
            .or_insert_with(|| usvg::Tree::from_str(icon.svg(), &usvg::Options::default()).ok())
            .as_ref()
    }

    /// The icon painted `size` device pixels square with its top left at
    /// device point (`x`, `y`): the picture, and the whole pixel its top
    /// left goes at. None if it cannot be painted.
    pub(crate) fn paint(&mut self, icon: Icon, x: f32, y: f32, size: f32) -> Option<(Rc<Pixmap>, i32, i32)> {
        if !(x.is_finite() && y.is_finite() && size.is_finite()) || size <= 0.0 || size > 1024.0 {
            return None;
        }
        let (ix, qx) = quarter(x);
        let (iy, qy) = quarter(y);
        let key = Key { icon, size: (size * 64.0).round() as u32, quarter_x: qx, quarter_y: qy };
        if let Some(found) = self.painted.get(&key) {
            return Some((found.clone(), ix, iy));
        }
        let (fx, fy) = (f32::from(qx) / 4.0, f32::from(qy) / 4.0);
        let side = (size + 1.0).ceil() as u32;
        let mut pixmap = Pixmap::new(side, side)?;
        let tree = self.tree(icon)?;
        let scale = size / GRID;
        resvg::render(tree, Transform::from_row(scale, 0.0, 0.0, scale, fx, fy), &mut pixmap.as_mut());
        let painted = Rc::new(pixmap);
        if self.painted.len() >= CACHE_LIMIT {
            self.painted.clear();
        }
        self.painted.insert(key, painted.clone());
        Some((painted, ix, iy))
    }
}

/// A device position as a whole pixel and the quarter within it.
fn quarter(v: f32) -> (i32, u8) {
    let whole = v.floor();
    let mut q = ((v - whole) * 4.0).round() as i32;
    let mut i = whole as i32;
    if q >= 4 {
        q = 0;
        i += 1;
    }
    (i, q.clamp(0, 3) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_reads_and_paints() {
        let mut icons = Icons::new();
        for icon in Icon::ALL {
            assert!(icons.readable(icon), "{icon:?}");
            let (painted, _, _) = icons.paint(icon, 10.25, 3.5, 34.0).expect("painted");
            let inked = painted.pixels().iter().filter(|p| p.alpha() > 100).count();
            assert!(inked > 150, "{icon:?} has almost nothing in it");
        }
    }
}
