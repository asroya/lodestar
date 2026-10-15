//! Physical motion: damped springs, keyframe tracks and easing curves.
//!
//! Everything here is a pure function of elapsed time, so a frame looks the
//! same at any frame rate and a timeline can be unit-tested. Durations are in
//! "design seconds"; callers divide elapsed time by `theme::slowmo()` before
//! asking, and skip motion entirely under `theme::reduced()`.

use std::f32::consts::TAU;

/// A damped spring, described the way SwiftUI does: `response` is the period
/// of the undamped oscillation in seconds, `bounce` is 0 (critically damped,
/// no overshoot) to 1 (undamped).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub response: f32,
    pub bounce: f32,
}

/// Buttons, rows, letters.
pub const SNAPPY: Spring = Spring { response: 0.42, bounce: 0.18 };
/// Larger moves: the star swinging in.
pub const GENTLE: Spring = Spring { response: 0.70, bounce: 0.12 };
/// Progress. Never overshoots, because a bar must never claim more than is done.
pub const SMOOTH: Spring = Spring { response: 0.55, bounce: 0.0 };
/// A disc landing with real bounce.
pub const LAND: Spring = Spring { response: 0.38, bounce: 0.45 };

impl Spring {
    /// Position at `t` seconds for a move from 0 to 1, starting at rest.
    /// Exactly 1.0 once the spring has settled.
    pub fn at(&self, t: f32) -> f32 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= self.settle() {
            return 1.0;
        }
        self.raw(t)
    }

    fn raw(&self, t: f32) -> f32 {
        let z = (1.0 - self.bounce).clamp(0.0, 1.0);
        let w = TAU / self.response;
        if z >= 1.0 {
            return 1.0 - (-w * t).exp() * (1.0 + w * t);
        }
        let wd = w * (1.0 - z * z).sqrt();
        1.0 - (-z * w * t).exp() * ((wd * t).cos() + (z * w / wd) * (wd * t).sin())
    }

    /// Seconds until the spring stays within 0.2% of its target.
    pub fn settle(&self) -> f32 {
        // The envelope of the oscillation decays as e^(-ζωt); the critically
        // damped case decays as (1+ωt)e^(-ωt). Solve numerically, in 1 ms steps
        // from the far end, so the answer is the last time it was outside.
        let mut t = 5.0;
        while t > 0.0 {
            if (self.raw(t) - 1.0).abs() > 0.002 {
                return t + 0.001;
            }
            t -= 0.001;
        }
        0.0
    }
}

/// Progress of a step that starts at `start` and lasts `len`, clamped to 0..=1.
pub fn phase(t: f32, start: f32, len: f32) -> f32 {
    if len <= 0.0 {
        return if t >= start { 1.0 } else { 0.0 };
    }
    ((t - start) / len).clamp(0.0, 1.0)
}

/// CSS-style cubic Bézier easing with control points (x1, y1) and (x2, y2).
pub fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x == 0.0 || x == 1.0 {
        return x;
    }
    let bx = |s: f32| 3.0 * (1.0 - s) * (1.0 - s) * s * x1 + 3.0 * (1.0 - s) * s * s * x2 + s * s * s;
    let by = |s: f32| 3.0 * (1.0 - s) * (1.0 - s) * s * y1 + 3.0 * (1.0 - s) * s * s * y2 + s * s * s;
    // Bisection is plenty for a curve evaluated a few dozen times a frame.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if bx(mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    by(0.5 * (lo + hi))
}

/// Gravity: starts slow, lands fast.
pub fn ease_in(p: f32) -> f32 {
    bezier(0.55, 0.0, 1.0, 0.45, p)
}
/// Thrown upward: leaves fast, slows to the apex.
pub fn ease_out(p: f32) -> f32 {
    bezier(0.0, 0.55, 0.45, 1.0, p)
}
/// Rings and throws: a fast start that lingers.
pub fn out_expo(p: f32) -> f32 {
    bezier(0.16, 1.0, 0.3, 1.0, p)
}
/// Drawing and travelling.
pub fn in_out(p: f32) -> f32 {
    bezier(0.65, 0.0, 0.35, 1.0, p)
}

/// How a keyframe segment is eased.
#[derive(Clone, Copy, Debug)]
pub enum Ease {
    Linear,
    In,
    Out,
    InOut,
    Bezier(f32, f32, f32, f32),
}

impl Ease {
    pub fn apply(self, p: f32) -> f32 {
        match self {
            Ease::Linear => p.clamp(0.0, 1.0),
            Ease::In => ease_in(p),
            Ease::Out => ease_out(p),
            Ease::InOut => in_out(p),
            Ease::Bezier(a, b, c, d) => bezier(a, b, c, d, p),
        }
    }
}

/// Piecewise keyframes `(time, value, ease into the next keyframe)`, sorted by
/// time. Holds the first value before the start and the last after the end.
pub fn track(t: f32, keys: &[(f32, f32, Ease)]) -> f32 {
    let Some(first) = keys.first() else { return 0.0 };
    if t <= first.0 {
        return first.1;
    }
    for w in keys.windows(2) {
        let (t0, v0, e) = w[0];
        let (t1, v1, _) = w[1];
        if t < t1 {
            let p = e.apply(phase(t, t0, t1 - t0));
            return v0 + (v1 - v0) * p;
        }
    }
    keys[keys.len() - 1].1
}

/// A small, fast, seedable PRNG (SplitMix32) so generated motion is the same
/// on every run.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed)
    }
    /// Uniform in 0..1.
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9);
        let mut z = self.0;
        z = (z ^ (z >> 16)).wrapping_mul(0x85EB_CA6B);
        z = (z ^ (z >> 13)).wrapping_mul(0xC2B2_AE35);
        z ^= z >> 16;
        (z >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(s: Spring) -> f32 {
        (0..2000).map(|i| s.at(i as f32 / 1000.0)).fold(0.0, f32::max)
    }

    #[test]
    fn springs_start_at_rest_and_settle_on_target() {
        for s in [SNAPPY, GENTLE, SMOOTH, LAND] {
            assert_eq!(s.at(0.0), 0.0);
            assert_eq!(s.at(-1.0), 0.0);
            assert_eq!(s.at(s.settle()), 1.0, "{s:?}");
            assert_eq!(s.at(10.0), 1.0, "{s:?}");
            assert!(s.settle() > 0.2 && s.settle() < 1.5, "{s:?} settles in {}", s.settle());
        }
    }

    #[test]
    fn overshoot_matches_each_springs_character() {
        assert!(peak(SMOOTH) <= 1.0, "smooth must never overshoot: {}", peak(SMOOTH));
        let land = peak(LAND);
        assert!((1.10..=1.14).contains(&land), "land peaks at {land}");
        assert!(peak(SNAPPY) < 1.02 && peak(SNAPPY) > 1.0);
    }

    #[test]
    fn settle_times_match_the_design_canvas() {
        // The canvas samples the same springs into CSS linear(); keep them in step.
        for (s, want) in [(SNAPPY, 0.56), (GENTLE, 0.88), (SMOOTH, 0.76), (LAND, 0.71)] {
            assert!((s.settle() - want).abs() < 0.04, "{s:?}: {} vs {want}", s.settle());
        }
    }

    #[test]
    fn bezier_is_monotonic_with_fixed_ends() {
        for f in [ease_in, ease_out, out_expo, in_out] {
            assert_eq!(f(0.0), 0.0);
            assert_eq!(f(1.0), 1.0);
            let mut prev = 0.0;
            for i in 0..=100 {
                let v = f(i as f32 / 100.0);
                assert!(v >= prev - 1e-4, "not monotonic at {i}");
                prev = v;
            }
        }
        assert!(ease_in(0.5) < 0.5 && ease_out(0.5) > 0.5);
    }

    #[test]
    fn tracks_are_continuous_and_hold_their_ends() {
        let keys = [(0.0, -10.0, Ease::In), (0.5, 0.0, Ease::Out), (0.7, -2.0, Ease::In), (1.0, 0.0, Ease::Linear)];
        assert_eq!(track(-1.0, &keys), -10.0);
        assert_eq!(track(2.0, &keys), 0.0);
        for &(k, v, _) in &keys {
            assert!((track(k, &keys) - v).abs() < 1e-3, "jump at {k}");
            // Ease::Out leaves with a steep (but finite-step) slope, so probe very close.
            assert!((track(k + 1e-6, &keys) - v).abs() < 0.05, "jump just after {k}");
        }
        assert_eq!(track(0.3, &[]), 0.0);
    }

    #[test]
    fn rng_is_deterministic_and_in_range() {
        let a: Vec<f32> = (0..50).scan(Rng::new(7), |r, _| Some(r.next())).collect();
        let b: Vec<f32> = (0..50).scan(Rng::new(7), |r, _| Some(r.next())).collect();
        assert_eq!(a, b);
        assert!(a.iter().all(|v| (0.0..1.0).contains(v)));
        assert_ne!(a[0], a[1]);
    }
}
