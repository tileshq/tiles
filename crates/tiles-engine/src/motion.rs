//! The entrance (playbook N1): the card fades in and floats up while its
//! marks grow from their baselines, then its values fade in. Its curves and
//! times are design tokens (`motion` in `tokens.json`); frames are drawn
//! `frame_ms` apart, the spacing a terminal can show, since the wrapper
//! shows at most one frame per 50 ms tick (ADR-030). Under reduce motion
//! (playbook N2) the settled card only fades in.

use crate::tokens::MotionTokens;

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing function.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bezier(pub f32, pub f32, pub f32, pub f32);

impl Bezier {
    /// CSS `ease`.
    pub const EASE: Bezier = Bezier(0.25, 0.1, 0.25, 1.0);
    /// The float's settle curve.
    pub const SETTLE: Bezier = Bezier(0.22, 1.0, 0.36, 1.0);
    /// The marks' grow curve.
    pub const GROW: Bezier = Bezier(0.33, 1.0, 0.68, 1.0);
    /// CSS `ease-out`.
    pub const EASE_OUT: Bezier = Bezier(0.0, 0.0, 0.58, 1.0);

    /// Progress at time fraction `t`, clamped to 0..1.
    #[must_use]
    pub fn at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if t <= 0.0 || t >= 1.0 {
            return t;
        }
        let Bezier(x1, y1, x2, y2) = self;
        let curve = |a: f32, b: f32, s: f32| {
            let u = 1.0 - s;
            3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
        };
        // Solve x(s) = t by bisection: monotonic for CSS curves.
        let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
        for _ in 0..40 {
            let mid = f32::midpoint(lo, hi);
            if curve(x1, x2, mid) < t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        curve(y1, y2, f32::midpoint(lo, hi))
    }
}

impl From<[f32; 4]> for Bezier {
    fn from([x1, y1, x2, y2]: [f32; 4]) -> Self {
        Bezier(x1, y1, x2, y2)
    }
}

/// Where every animated property is at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moment {
    /// Card opacity.
    pub opacity: f32,
    /// How far below its place the card is, in CSS pixels.
    pub lift: f32,
    /// The card's scale about its centre.
    pub scale: f32,
    /// How far the marks have grown, 0 to 1.
    pub grow: f32,
    /// Opacity of value labels and other fading items.
    pub fade: f32,
}

impl Moment {
    /// The settled card.
    pub const FINAL: Moment = Moment {
        opacity: 1.0,
        lift: 0.0,
        scale: 1.0,
        grow: 1.0,
        fade: 1.0,
    };

    /// The state at `ms` into the entrance.
    #[must_use]
    pub fn at(m: &MotionTokens, ms: f32) -> Self {
        let settle = Bezier::SETTLE.at(ms / m.float_ms);
        Self {
            opacity: Bezier::from(m.fade_ease).at(ms / m.fade_ms),
            lift: m.float_px * (1.0 - settle),
            scale: m.start_scale + (1.0 - m.start_scale) * settle,
            grow: Bezier::GROW.at((ms - m.marks_start_ms) / (m.marks_end_ms - m.marks_start_ms)),
            fade: Bezier::from(m.labels_ease)
                .at((ms - m.labels_start_ms) / (m.labels_end_ms - m.labels_start_ms)),
        }
    }

    /// The state at `ms` into the reduced entrance: the settled card fading
    /// in, with nothing moving or growing.
    #[must_use]
    pub fn reduced(m: &MotionTokens, ms: f32) -> Self {
        Self {
            opacity: Bezier::from(m.reduce_ease).at(ms / m.reduce_ms),
            ..Self::FINAL
        }
    }

    /// The moment of each of the entrance's frames, `frame_ms` apart; the
    /// last is the settled card.
    #[must_use]
    pub fn frames(m: &MotionTokens, entrance: Entrance) -> Vec<Moment> {
        let n = frame_count(entrance.duration_ms(m), m.frame_ms);
        #[allow(clippy::cast_precision_loss)] // A frame index.
        let mut out: Vec<Moment> = (0..n - 1)
            .map(|i| entrance.at(m, m.frame_ms * i as f32))
            .collect();
        out.push(Moment::FINAL);
        out
    }
}

/// Which entrance a card plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Entrance {
    /// The full entrance (playbook N1).
    #[default]
    Full,
    /// Reduce motion (playbook N2): an opacity-only fade.
    Reduced,
}

impl Entrance {
    /// How long it runs, from the first frame to the settled card.
    #[must_use]
    pub fn duration_ms(self, m: &MotionTokens) -> f32 {
        match self {
            Entrance::Full => m.duration_ms,
            Entrance::Reduced => m.reduce_ms,
        }
    }

    /// The state at `ms` into it; settled from its end on.
    #[must_use]
    pub fn at(self, m: &MotionTokens, ms: f32) -> Moment {
        if ms >= self.duration_ms(m) {
            return Moment::FINAL;
        }
        let ms = ms.max(0.0);
        match self {
            Entrance::Full => Moment::at(m, ms),
            Entrance::Reduced => Moment::reduced(m, ms),
        }
    }
}

/// Frames, the settled card included, for an entrance `duration_ms` long
/// drawn `frame_ms` apart: one per step begun, and the settled card.
#[must_use]
pub fn frame_count(duration_ms: f32, frame_ms: f32) -> usize {
    let steps = (duration_ms / frame_ms.max(1.0)).ceil();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Clamped first.
    let steps = steps.clamp(1.0, 1000.0) as usize;
    steps + 1
}

#[cfg(test)]
mod tests {
    use super::{Bezier, Entrance, Moment, frame_count};
    use crate::tokens::Tokens;

    #[test]
    fn curves_start_at_zero_and_end_at_one() {
        let tokens = Tokens::from_json(&crate::embedded_tokens().unwrap()).unwrap();
        let m = &tokens.motion;
        for b in [
            Bezier::EASE,
            Bezier::SETTLE,
            Bezier::GROW,
            Bezier::EASE_OUT,
            Bezier::from(m.fade_ease),
            Bezier::from(m.labels_ease),
            Bezier::from(m.reduce_ease),
        ] {
            assert!(b.at(0.0).abs() < 1e-6);
            assert!((b.at(1.0) - 1.0).abs() < 1e-6);
            assert!(b.at(0.5) > 0.4, "{b:?}");
        }
    }

    #[test]
    fn the_strong_ease_out_lands_most_of_a_fade_in_its_first_frame() {
        // Report emil-skills-eval-k3, section 1: at the first 50 ms tick of
        // a 280 ms fade, CSS ease shows 25% and the strong ease-out 63%. The
        // tokens' fade and label curves are that ease-out, and the tokens
        // alone own it.
        let tokens = Tokens::from_json(&crate::embedded_tokens().unwrap()).unwrap();
        let m = &tokens.motion;
        let t = 50.0 / 280.0;
        assert!((Bezier::EASE.at(t) - 0.247).abs() < 0.01);
        for ease in [m.fade_ease, m.labels_ease] {
            assert!((Bezier::from(ease).at(t) - 0.632).abs() < 0.01, "{ease:?}");
        }
    }

    #[test]
    fn the_entrance_has_10_frames_50_ms_apart_over_450_ms_and_ends_settled() {
        let t = Tokens::from_json(&crate::embedded_tokens().unwrap()).unwrap();
        let m = &t.motion;
        assert_eq!((m.frame_ms, m.duration_ms), (50.0, 450.0));
        let frames = Moment::frames(m, Entrance::Full);
        assert_eq!(frames.len(), 10);
        let first = frames[0];
        assert_eq!(first.opacity, 0.0);
        assert!((first.lift - 10.0).abs() < 1e-4);
        assert!((first.scale - 0.985).abs() < 1e-6);
        assert_eq!(first.grow, 0.0);
        assert_eq!(*frames.last().unwrap(), Moment::FINAL);
        // Faded in by 300 ms, floated in by 400 ms; the marks start at
        // 100 ms and land before the values have faded in.
        assert!((Moment::at(m, 300.0).opacity - 1.0).abs() < 1e-6);
        assert!(Moment::at(m, 400.0).lift.abs() < 1e-4);
        assert_eq!(Moment::at(m, 100.0).grow, 0.0);
        assert_eq!(Moment::at(m, 300.0).fade, 0.0);
        assert!(Moment::at(m, 300.0).grow > 0.9);
        assert!((Moment::at(m, 450.0).fade - 1.0).abs() < 1e-6);
        for w in frames.windows(2) {
            assert!(w[1].opacity >= w[0].opacity && w[1].grow >= w[0].grow);
            assert!(w[1].fade >= w[0].fade && w[1].lift <= w[0].lift);
        }
    }

    #[test]
    fn reduce_motion_is_4_frames_over_150_ms_that_only_fade() {
        let t = Tokens::from_json(&crate::embedded_tokens().unwrap()).unwrap();
        let frames = Moment::frames(&t.motion, Entrance::Reduced);
        assert_eq!(frames.len(), 4);
        assert_eq!(frames[0].opacity, 0.0);
        for f in &frames {
            assert_eq!((f.lift, f.scale, f.grow, f.fade), (0.0, 1.0, 1.0, 1.0));
        }
        assert!(frames.windows(2).all(|w| w[1].opacity > w[0].opacity));
        assert_eq!(frames[3], Moment::FINAL);
        // The first visible frame is half faded in, not most of the way:
        // the reduced fade is gentler than the entrance's strong curve.
        assert!(
            (0.4..0.6).contains(&frames[1].opacity),
            "{}",
            frames[1].opacity
        );
        assert!(frames[2].opacity < 0.9, "{}", frames[2].opacity);
    }

    #[test]
    fn frames_are_counted_by_the_50_ms_steps_an_entrance_takes() {
        assert_eq!(frame_count(450.0, 50.0), 10);
        assert_eq!(frame_count(150.0, 50.0), 4);
        assert_eq!(frame_count(540.0, 50.0), 12, "N1 resampled at 50 ms");
        assert_eq!(frame_count(350.0, 50.0), 8, "variant B");
        assert_eq!(frame_count(650.0, 50.0), 14, "a dashboard of five");
    }
}
