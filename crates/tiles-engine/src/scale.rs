//! Scales: band positions for categories, and linear scales with the nice
//! domains and ticks d3 and Vega use, so the engine's axes land where the
//! prototype's did.

/// d3's tick increment: a step of 1, 2 or 5 times a power of ten that gives
/// about `count` ticks over the span.
#[must_use]
pub fn tick_step(lo: f64, hi: f64, count: f64) -> f64 {
    let span = (hi - lo).abs();
    if span == 0.0 || count <= 0.0 || !span.is_finite() {
        return 1.0;
    }
    let step0 = span / count;
    let power = 10_f64.powf(step0.log10().floor());
    let error = step0 / power;
    let factor = if error >= 50_f64.sqrt() {
        10.0
    } else if error >= 10_f64.sqrt() {
        5.0
    } else if error >= 2_f64.sqrt() {
        2.0
    } else {
        1.0
    };
    factor * power
}

/// Extends a domain outward to round tick steps (d3 `scale.nice`).
#[must_use]
#[allow(clippy::float_cmp, clippy::similar_names)] // Exact equality is the degenerate-domain case d3 checks; lo/nlo pair up.
pub fn nice(mut lo: f64, mut hi: f64, count: f64) -> (f64, f64) {
    if lo == hi {
        if lo == 0.0 {
            return (0.0, 1.0);
        }
        let pad = lo.abs() * 0.1;
        lo -= pad;
        hi += pad;
    }
    for _ in 0..10 {
        let step = tick_step(lo, hi, count);
        let (nlo, nhi) = ((lo / step).floor() * step, (hi / step).ceil() * step);
        if (nlo - lo).abs() < f64::EPSILON && (nhi - hi).abs() < f64::EPSILON {
            break;
        }
        lo = nlo;
        hi = nhi;
    }
    (lo, hi)
}

/// Round tick values inside the domain.
#[must_use]
pub fn ticks(lo: f64, hi: f64, count: f64) -> Vec<f64> {
    let step = tick_step(lo, hi, count);
    let first = (lo / step).ceil();
    let last = (hi / step).floor();
    let mut out = Vec::new();
    let mut i = first;
    while i <= last + 1e-9 && out.len() < 50 {
        out.push(i * step);
        i += 1.0;
    }
    out
}

/// A linear map from a domain to a pixel range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Linear {
    /// Domain start.
    pub d0: f64,
    /// Domain end.
    pub d1: f64,
    /// Range start.
    pub r0: f32,
    /// Range end.
    pub r1: f32,
}

impl Linear {
    /// Maps a value.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::float_cmp)] // Pixel positions fit f32; an exactly empty domain maps to its start.
    pub fn map(&self, v: f64) -> f32 {
        if self.d1 == self.d0 {
            return self.r0;
        }
        let t = (v - self.d0) / (self.d1 - self.d0);
        self.r0 + (f64::from(self.r1 - self.r0) * t) as f32
    }
}

/// A band scale: `n` equal bands over a range, Vega's default paddings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    /// Start of the first band.
    pub start: f32,
    /// Distance from one band's start to the next.
    pub step: f32,
    /// Width of a band.
    pub width: f32,
}

impl Band {
    /// Lays `n` bands over `r0..r1`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Band counts are small.
    pub fn new(n: usize, r0: f32, r1: f32, inner: f32, outer: f32) -> Self {
        let n = n.max(1) as f32;
        let step = (r1 - r0) / (n - inner + 2.0 * outer).max(1e-6);
        Self {
            start: r0 + step * outer,
            step,
            width: step * (1.0 - inner),
        }
    }

    /// Start of band `i`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Band counts are small.
    pub fn at(&self, i: usize) -> f32 {
        self.start + self.step * i as f32
    }

    /// Centre of band `i`.
    #[must_use]
    pub fn center(&self, i: usize) -> f32 {
        self.at(i) + self.width / 2.0
    }
}

/// A point scale: `n` points over a range with half a step of padding at
/// each end (Vega-Lite's default for ordinal x on lines).
#[must_use]
#[allow(clippy::cast_precision_loss)] // Point counts are small.
pub fn points(n: usize, r0: f32, r1: f32, padding: f32) -> Vec<f32> {
    if n == 0 {
        return Vec::new();
    }
    let step = (r1 - r0) / ((n - 1) as f32 + 2.0 * padding).max(1e-6);
    (0..n).map(|i| r0 + step * (padding + i as f32)).collect()
}

#[cfg(test)]
mod tests {
    use super::{Band, nice, points, tick_step, ticks};

    #[test]
    fn nice_matches_d3() {
        assert_eq!(nice(0.0, 431.0, 10.0), (0.0, 450.0));
        assert_eq!(nice(0.0, 0.0, 10.0), (0.0, 1.0));
        assert_eq!(nice(12.0, 97.0, 10.0), (10.0, 100.0));
        assert_eq!(tick_step(0.0, 450.0, 6.0), 100.0);
        assert_eq!(
            ticks(0.0, 450.0, 6.0),
            vec![0.0, 100.0, 200.0, 300.0, 400.0]
        );
    }

    #[test]
    fn bands_fill_the_range() {
        let b = Band::new(3, 0.0, 300.0, 0.1, 0.05);
        let end = b.at(2) + b.width + b.step * 0.05;
        assert!((end - 300.0).abs() < 1e-3, "{b:?}");
    }

    #[test]
    fn points_are_padded_half_a_step() {
        assert_eq!(points(2, 0.0, 300.0, 0.5), vec![75.0, 225.0]);
        assert_eq!(points(1, 0.0, 300.0, 0.5), vec![150.0]);
    }
}
