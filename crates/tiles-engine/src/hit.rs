//! The hover hit map: every data mark's box in device pixels with its hover
//! text, read from the scene (ADR-002), so a terminal surface can show a
//! tooltip for the cell under the pointer without drawing anything again.

use crate::scene::{Item, Part, Scene, Shape};

/// Thin marks (points, lines) get at least this much target, in CSS pixels.
const MIN_TARGET: f32 = 8.0;

/// One hover target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Left edge in device pixels.
    pub left: i32,
    /// Top edge in device pixels.
    pub top: i32,
    /// Right edge in device pixels.
    pub right: i32,
    /// Bottom edge in device pixels.
    pub bottom: i32,
    /// The row the mark draws, if it draws one.
    pub row: Option<usize>,
    /// The hover text, one `name: value` per line.
    pub tip: String,
}

impl Hit {
    fn area(&self) -> i64 {
        i64::from(self.right - self.left) * i64::from(self.bottom - self.top)
    }

    /// Whether a device pixel falls inside.
    #[must_use]
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

/// The hit map of a settled scene at a device scale.
#[must_use]
pub fn hit_map(scene: &Scene, scale: f32) -> Vec<Hit> {
    scene
        .items
        .iter()
        .filter_map(|i| target(i, scale))
        .collect()
}

/// The hover target `item` makes at a device scale, if it makes one.
#[allow(clippy::cast_possible_truncation)] // Card coordinates are far below i32::MAX.
pub(crate) fn target(item: &Item, scale: f32) -> Option<Hit> {
    let tip = item.tip.as_ref()?;
    if !matches!(item.part, Part::Mark | Part::Value | Part::Hit) {
        return None;
    }
    if matches!(item.shape, Shape::Area { .. } | Shape::Text { .. }) {
        return None;
    }
    let mut b = item.bounds();
    let grow = |lo: &mut f32, hi: &mut f32| {
        if *hi - *lo < MIN_TARGET {
            let mid = f32::midpoint(*lo, *hi);
            *lo = mid - MIN_TARGET / 2.0;
            *hi = mid + MIN_TARGET / 2.0;
        }
    };
    grow(&mut b.left, &mut b.right);
    grow(&mut b.top, &mut b.bottom);
    Some(Hit {
        left: (b.left * scale).floor() as i32,
        top: (b.top * scale).floor() as i32,
        right: (b.right * scale).ceil() as i32,
        bottom: (b.bottom * scale).ceil() as i32,
        row: item.datum,
        tip: tip.clone(),
    })
}

/// The target under a device pixel: the smallest box that contains it, so
/// a point on a line wins over the line.
#[must_use]
pub fn hit_at(hits: &[Hit], x: i32, y: i32) -> Option<&Hit> {
    hits.iter()
        .filter(|h| h.contains(x, y))
        .min_by_key(|h| h.area())
}
