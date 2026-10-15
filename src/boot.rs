//! Boot animations.
//!
//! Both variants share the same intro: the star's outline traces itself, the
//! body swings in, and the signal dot falls, bounces once and lands. Then:
//!
//! - **Iris** (every launch after the first): the star winds up and blooms
//!   into the window. The app is revealed through the star's shape, as if the
//!   star were the window.
//! - **Starfall** (first launch): the star throbs and throws four waves of
//!   small stars, then blooms from its own centre into the Welcome screen.
//!
//! Timelines are in design seconds (the canvas's units), scaled by
//! `theme::slowmo()`. Under Reduce Motion both become a 200 ms crossfade.
//! Painting is pure: `paint` reads only `self` and the clock.

use std::f32::consts::{PI, TAU};

use eframe::egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};

use crate::{
    motion::{self, Ease, GENTLE, Rng, SNAPPY, phase, track},
    theme::{self, DARK, LIGHT, Pal},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Iris,
    Starfall,
}

/// How the content under the boot should be drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reveal {
    /// Scale about the window centre (Iris's "camera push").
    pub scale: f32,
    /// Opacity of the content (Starfall's hand-off).
    pub alpha: f32,
    /// Extra downward offset in points (Starfall's hand-off).
    pub rise: f32,
}

impl Reveal {
    pub const SETTLED: Reveal = Reveal { scale: 1.0, alpha: 1.0, rise: 0.0 };
}

#[derive(Clone, Debug)]
struct Particle {
    dir: Vec2,
    /// Travel, as a multiple of the window diagonal.
    dist: f32,
    /// Starting half-height multiplier (1.0 ≈ 9.5 pt at the design size).
    s0: f32,
    growth: f32,
    spin: f32,
    delay: f32,
    life: f32,
    accent: bool,
    twinkle: bool,
}

pub struct Boot {
    pub kind: Kind,
    start: f64,
    particles: Vec<Particle>,
}

/// Iris: the bloom has cleared the window.
const IRIS_END: f32 = 2.9;
/// Starfall: the bloom covers the window; the overlay stops painting here.
const STARFALL_COVERED: f32 = 3.35;
/// Starfall: the Welcome content has settled.
const STARFALL_END: f32 = 3.8;
/// Reduce Motion: a plain crossfade.
const REDUCED_END: f32 = 0.2;

/// The design canvas is 1100×760; the star's half-height there is 89.5 pt.
const DESIGN: Vec2 = vec2(1100.0, 760.0);
const STAR_L: f32 = 89.5;

impl Boot {
    pub fn new(kind: Kind, now: f64) -> Self {
        let particles = if kind == Kind::Starfall { particles() } else { Vec::new() };
        Self { kind, start: now, particles }
    }

    /// Seconds into the timeline, in design seconds.
    fn t(&self, now: f64) -> f32 {
        ((now - self.start) as f32 / theme::slowmo()).max(0.0)
    }

    fn length(&self) -> f32 {
        if theme::reduced() {
            REDUCED_END
        } else {
            match self.kind {
                Kind::Iris => IRIS_END,
                Kind::Starfall => STARFALL_END,
            }
        }
    }

    pub fn finished(&self, now: f64) -> bool {
        self.t(now) >= self.length()
    }

    /// Jump to the end (click, Space, Return or Esc).
    pub fn skip(&mut self) {
        self.start = f64::NEG_INFINITY;
    }

    /// Whether the overlay still covers anything (input should not reach the content).
    pub fn covering(&self, now: f64) -> bool {
        let t = self.t(now);
        if theme::reduced() {
            return t < REDUCED_END;
        }
        match self.kind {
            Kind::Iris => t < IRIS_END,
            Kind::Starfall => t < STARFALL_COVERED,
        }
    }

    pub fn reveal(&self, now: f64) -> Reveal {
        let t = self.t(now);
        if theme::reduced() {
            return Reveal::SETTLED;
        }
        match self.kind {
            Kind::Iris => {
                Reveal { scale: 1.0 + 0.06 * (1.0 - motion::out_expo(phase(t, 2.3, 0.6))), alpha: 1.0, rise: 0.0 }
            }
            Kind::Starfall => Reveal {
                scale: 1.0,
                alpha: motion::bezier(0.0, 0.0, 0.58, 1.0, phase(t, STARFALL_COVERED, 0.35)),
                rise: 26.0 * (1.0 - SNAPPY.at(t - STARFALL_COVERED)),
            },
        }
    }

    /// Paints the overlay for this frame. `app` is the palette the content
    /// underneath uses; the boot's ground is the other appearance, and the star
    /// takes the app's background, because the star is the window onto it.
    pub fn paint(&self, painter: &Painter, screen: Rect, app: &Pal, now: f64) {
        let t = self.t(now);
        let ground_pal = if app.bg == LIGHT.bg { &DARK } else { &LIGHT };
        let colors = Colors { ground: ground_pal.bg, star: app.bg, accent: ground_pal.accent };
        let g = Geometry::new(screen);
        if theme::reduced() {
            if t < REDUCED_END {
                let a = 1.0 - t / REDUCED_END;
                painter.rect_filled(screen, 0.0, colors.ground.gamma_multiply(a));
                let pts = theme::sparkle_points(g.c, g.l, 0.0);
                theme::fill_star(painter, g.c, &pts, colors.star.gamma_multiply(a));
                painter.circle_filled(g.dot_rest(), g.dot_r(), colors.accent.gamma_multiply(a));
                wordmark(painter, &g, colors.star, t, a, true);
            }
            return;
        }
        match self.kind {
            Kind::Iris => self.paint_iris(painter, screen, &g, &colors, t),
            Kind::Starfall => self.paint_starfall(painter, screen, &g, &colors, t),
        }
    }

    fn paint_iris(&self, painter: &Painter, screen: Rect, g: &Geometry, k: &Colors, t: f32) {
        if t >= IRIS_END {
            return;
        }
        let intro = Intro::at(t, 1.05);
        // Wind-up, then the bloom: a slow, heavily damped push past the corners.
        let full = cover_scale(g, screen, 0.0);
        let bloom_s = track(
            t,
            &[(1.5, 1.0, Ease::InOut), (1.8, 0.92, Ease::Bezier(0.62, 0.0, 0.12, 1.0)), (IRIS_END, full, Ease::Linear)],
        );
        let bloom_r = track(
            t,
            &[
                (1.5, 0.0, Ease::InOut),
                (1.8, -6f32.to_radians(), Ease::Bezier(0.62, 0.0, 0.12, 1.0)),
                (IRIS_END, 0.0, Ease::Linear),
            ],
        );
        let c = g.c + vec2(0.0, intro.recoil * g.l);
        let l = g.l * intro.scale * bloom_s;
        let outline = theme::sparkle_points(c, l, intro.rot + bloom_r);

        // The ground with a star-shaped hole. Until the camera pushes through,
        // the hole is filled: first with the ground (so the star "fills in"),
        // then with plain star colour, so no app content shows inside the mark.
        ground_with_hole(painter, screen, c, &outline, k.ground);
        let through = motion::out_expo(phase(t, 2.3, 0.6));
        let cover = theme::lerp(k.ground, k.star, intro.fill);
        theme::fill_star(painter, c, &outline, cover.gamma_multiply(1.0 - through));
        if intro.outline_alpha > 0.0 {
            painter.add(Shape::line(
                theme::partial_outline(&outline, intro.outline_frac),
                Stroke::new(2.7 * g.k, k.star.gamma_multiply(intro.outline_alpha)),
            ));
        }
        rings(painter, g, k.accent, t, &[(1.49, 1.2), (1.77, 0.9)]);

        // The dot leaves upward, accelerating, stretched into a streak.
        let leave = phase(t, 2.0, 0.6);
        let lift = motion::ease_in(leave);
        let dot_y = intro.dot_y * g.l - lift * 6.5 * g.l;
        let stretch = 1.0 + 4.0 * lift;
        g.dot(
            painter,
            k.accent.gamma_multiply(intro.dot_alpha * (1.0 - 0.8 * lift)),
            dot_y,
            intro.squash.0,
            intro.squash.1 * stretch,
        );

        let word = 1.0 - phase(t, 1.6, 0.25);
        wordmark(painter, g, k.star, t, word, false);
    }

    fn paint_starfall(&self, painter: &Painter, screen: Rect, g: &Geometry, k: &Colors, t: f32) {
        if t >= STARFALL_COVERED {
            return;
        }
        painter.rect_filled(screen, 0.0, k.ground);
        let intro = Intro::at(t, 0.78);
        // At 1.5 s the star throbs and throws the first wave.
        let throb = track(
            t,
            &[
                (1.5, 1.0, Ease::Bezier(0.2, 0.8, 0.2, 1.0)),
                (1.626, 1.15, Ease::InOut),
                (1.808, 0.96, Ease::InOut),
                (1.976, 1.02, Ease::InOut),
                (2.2, 1.0, Ease::Linear),
            ],
        );
        let c = g.c + vec2(0.0, intro.recoil * g.l);
        let outline = theme::sparkle_points(c, g.l * intro.scale * throb, intro.rot);
        theme::fill_star(painter, c, &outline, k.star.gamma_multiply(intro.fill));
        if intro.outline_alpha > 0.0 {
            painter.add(Shape::line(
                theme::partial_outline(&outline, intro.outline_frac),
                Stroke::new(2.7 * g.k, k.star.gamma_multiply(intro.outline_alpha)),
            ));
        }
        g.dot(painter, k.accent.gamma_multiply(intro.dot_alpha), intro.dot_y * g.l, intro.squash.0, intro.squash.1);
        let word = 1.0 - phase(t, 1.55, 0.2);
        wordmark(painter, g, k.star, t, word, false);

        // Waves of small stars, thrown outward and decelerating.
        let diag = screen.size().length();
        for pt in &self.particles {
            if t < pt.delay {
                continue;
            }
            let p = phase(t, pt.delay, pt.life);
            let e = motion::out_expo(p);
            let pos = g.c + pt.dir * (pt.dist * diag * e);
            let l = 9.5 * g.k * pt.s0 * (1.0 + (pt.growth - 1.0) * e);
            let mut alpha = phase(t, pt.delay, pt.life * 0.07);
            if pt.twinkle {
                let q = phase(t, pt.delay + 0.3, 0.32);
                alpha *= 1.0 - 0.88 * (PI * q).sin();
            }
            let color = if pt.accent { k.accent } else { k.star };
            theme::fill_star(painter, pos, &theme::sparkle_points(pos, l, pt.spin * e), color.gamma_multiply(alpha));
        }

        // The bloom: the star itself grows from its centre until its points
        // pass the corners, becoming the app's ground.
        if t >= 2.55 {
            let end_rot = -10f32.to_radians();
            let full = cover_scale(g, screen, end_rot);
            let s = track(t, &[(2.55, 1.0, Ease::Bezier(0.7, 0.0, 0.2, 1.0)), (STARFALL_COVERED, full, Ease::Linear)]);
            let r =
                track(t, &[(2.55, 0.0, Ease::Bezier(0.7, 0.0, 0.2, 1.0)), (STARFALL_COVERED, end_rot, Ease::Linear)]);
            let outline = theme::sparkle_points(g.c, g.l * s, r);
            theme::fill_star(painter, g.c, &outline, k.star);
        }
    }
}

struct Colors {
    ground: Color32,
    star: Color32,
    accent: Color32,
}

/// Where things sit: the star centred (8 pt above the window centre, so the
/// star and wordmark together sit optically centred), scaled with the window.
struct Geometry {
    c: Pos2,
    l: f32,
    k: f32,
}

impl Geometry {
    fn new(screen: Rect) -> Self {
        let k = (screen.width() / DESIGN.x).min(screen.height() / DESIGN.y).clamp(0.75, 1.4);
        Self { c: screen.center() - vec2(0.0, 8.0 * k), l: STAR_L * k, k }
    }
    fn dot_rest(&self) -> Pos2 {
        self.c - vec2(0.0, 1.243 * self.l)
    }
    fn dot_r(&self) -> f32 {
        0.0826 * self.l
    }
    /// The dot, `dy` above/below its resting place, squashed with its bottom
    /// edge anchored (so it flattens onto the landing point).
    fn dot(&self, painter: &Painter, color: Color32, dy: f32, sx: f32, sy: f32) {
        if color.a() == 0 {
            return;
        }
        let r = self.dot_r();
        let center = self.dot_rest() + vec2(0.0, dy + r * (1.0 - sy));
        painter.add(Shape::ellipse_filled(center, vec2(r * sx, r * sy), color));
    }
}

/// The shared intro, sampled at `t`. `dot_start` is when the dot begins to fall.
struct Intro {
    outline_frac: f32,
    outline_alpha: f32,
    fill: f32,
    scale: f32,
    rot: f32,
    /// Vertical offset of the star, in multiples of its half-height.
    recoil: f32,
    /// Dot offset from rest, in multiples of the star's half-height.
    dot_y: f32,
    dot_alpha: f32,
    squash: (f32, f32),
}

impl Intro {
    fn at(t: f32, dot_start: f32) -> Self {
        let swing = GENTLE.at(t - 0.7);
        let fall = |pct: f32| dot_start + 0.8 * pct;
        let squash = |v: [f32; 9]| {
            track(
                t,
                &[
                    (fall(0.0), v[0], Ease::In),
                    (fall(0.5), v[1], Ease::Bezier(0.2, 0.0, 0.0, 1.0)),
                    (fall(0.53), v[2], Ease::Bezier(0.2, 0.8, 0.2, 1.0)),
                    (fall(0.62), v[3], Ease::InOut),
                    (fall(0.7), v[4], Ease::In),
                    (fall(0.85), v[5], Ease::Bezier(0.2, 0.0, 0.0, 1.0)),
                    (fall(0.875), v[6], Ease::Bezier(0.2, 0.8, 0.2, 1.0)),
                    (fall(0.94), v[7], Ease::Linear),
                    (fall(1.0), v[8], Ease::Linear),
                ],
            )
        };
        let impact = fall(0.55);
        Self {
            outline_frac: motion::in_out(phase(t, 0.1, 0.85)),
            outline_alpha: if t < 0.1 { 0.0 } else { 1.0 - motion::bezier(0.0, 0.0, 0.58, 1.0, phase(t, 0.95, 0.35)) },
            fill: motion::bezier(0.0, 0.0, 0.58, 1.0, phase(t, 0.7, 0.4)),
            scale: 0.84 + 0.16 * swing,
            rot: (-14f32).to_radians() * (1.0 - swing),
            recoil: track(
                t,
                &[
                    (impact, 0.0, Ease::Bezier(0.2, 0.8, 0.2, 1.0)),
                    (impact + 0.084, 0.039, Ease::InOut),
                    (impact + 0.224, -0.013, Ease::InOut),
                    (impact + 0.364, 0.0052, Ease::InOut),
                    (impact + 0.504, -0.0017, Ease::InOut),
                    (impact + 0.7, 0.0, Ease::Linear),
                ],
            ),
            dot_y: track(
                t,
                &[
                    (fall(0.0), -1.913, Ease::In),
                    (fall(0.52), 0.0, Ease::Out),
                    (fall(0.7), -0.2, Ease::In),
                    (fall(0.86), 0.0, Ease::Out),
                    (fall(0.93), -0.035, Ease::In),
                    (fall(1.0), 0.0, Ease::Linear),
                ],
            ),
            dot_alpha: phase(t, fall(0.0), 0.8 * 0.12),
            squash: (
                squash([0.82, 0.8, 1.34, 0.92, 1.0, 0.95, 1.14, 0.98, 1.0]),
                squash([1.2, 1.24, 0.66, 1.09, 1.0, 1.06, 0.86, 1.02, 1.0]),
            ),
        }
    }
}

/// The scale at which the star (rotated by `rot`) covers every corner of the
/// window, plus 10% so the points sweep past rather than stop at the corners.
/// The sides are concave, so this is far larger than the window's diagonal
/// over the star's height: about 21× at the design size.
fn cover_scale(g: &Geometry, screen: Rect, rot: f32) -> f32 {
    let corners = [screen.left_top(), screen.right_top(), screen.left_bottom(), screen.right_bottom()];
    let covers = |s: f32| {
        let outline = theme::sparkle_points(g.c, g.l * s, rot);
        corners.iter().all(|p| inside(*p, &outline))
    };
    let (mut lo, mut hi) = (1.0f32, 400.0f32);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if covers(mid) { hi = mid } else { lo = mid }
    }
    hi * 1.1
}

/// Even-odd point-in-polygon test.
fn inside(p: Pos2, poly: &[Pos2]) -> bool {
    let mut hit = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            hit = !hit;
        }
    }
    hit
}

/// Rings sent out by the dot's landings: `(start, length)` pairs.
fn rings(painter: &Painter, g: &Geometry, color: Color32, t: f32, at: &[(f32, f32)]) {
    for &(start, len) in at {
        if t < start || t > start + len {
            continue;
        }
        let e = motion::out_expo(phase(t, start, len));
        painter.circle_stroke(
            g.dot_rest(),
            g.dot_r() * (1.0 + 5.5 * e),
            Stroke::new(1.5, color.gamma_multiply(0.7 * (1.0 - e))),
        );
    }
}

/// "Lodestar", one letter at a time: each rises on a snappy spring, 35 ms apart.
fn wordmark(painter: &Painter, g: &Geometry, color: Color32, t: f32, alpha: f32, settled: bool) {
    if alpha <= 0.0 {
        return;
    }
    let font = FontId::new(30.0 * g.k, theme::semibold());
    let letters: Vec<_> =
        "Lodestar".chars().map(|ch| painter.layout_no_wrap(ch.to_string(), font.clone(), color)).collect();
    // Tighten like the canvas's -0.6 px letter-spacing.
    let track_px = -0.6 * g.k;
    let width: f32 = letters.iter().map(|l| l.size().x).sum::<f32>() + track_px * (letters.len() - 1) as f32;
    let top = g.c.y + 111.0 * g.k;
    let mut x = g.c.x - width * 0.5;
    for (i, galley) in letters.into_iter().enumerate() {
        let start = 0.95 + i as f32 * 0.035;
        let (rise, a) = if settled {
            (0.0, 1.0)
        } else {
            (16.0 * g.k * (1.0 - SNAPPY.at(t - start)), motion::bezier(0.0, 0.0, 0.58, 1.0, phase(t, start, 0.3)))
        };
        let w = galley.size().x;
        if a > 0.0 {
            painter.galley(pos2(x, top + rise), galley, color.gamma_multiply(a * alpha));
        }
        x += w + track_px;
    }
}

/// Paints `color` over the whole screen except the star outline: a quad strip
/// between the outline and a ring of points far outside the window, along the
/// same rays from `c`. The outline is radially monotonic, so the strip never folds.
fn ground_with_hole(painter: &Painter, screen: Rect, c: Pos2, outline: &[Pos2], color: Color32) {
    let reach = outline.iter().map(|q| (*q - c).length()).fold(0.0, f32::max);
    let far = 1.25 * reach.max(screen.size().length() + (c - screen.center()).length());
    let mut mesh = eframe::egui::Mesh::default();
    for q in outline {
        let d = *q - c;
        let dir = if d.length() > 1e-3 { d.normalized() } else { Vec2::X };
        mesh.colored_vertex(*q, color);
        mesh.colored_vertex(c + dir * far, color);
    }
    let n = outline.len() as u32;
    for i in 0..n {
        let j = (i + 1) % n;
        let (ai, bi, aj, bj) = (2 * i, 2 * i + 1, 2 * j, 2 * j + 1);
        mesh.add_triangle(ai, bi, aj);
        mesh.add_triangle(aj, bi, bj);
    }
    painter.add(Shape::mesh(mesh));
}

/// Four waves, from a fixed seed so every first launch throws the same stars:
/// `(count, start, travel × diagonal, growth, life)`. The last wave is dense,
/// short and slow, so its stars pile up before the bloom.
fn particles() -> Vec<Particle> {
    /// (count, start, travel range, growth range, life)
    type Wave = (usize, f32, (f32, f32), (f32, f32), f32);
    const WAVES: [Wave; 4] = [
        (12, 1.50, (1.3, 1.8), (3.0, 6.0), 1.10),
        (24, 1.78, (1.3, 1.8), (3.0, 6.0), 1.15),
        (48, 2.08, (1.3, 1.8), (3.0, 6.0), 1.20),
        (90, 2.38, (0.12, 0.7), (6.0, 14.0), 1.00),
    ];
    let mut rng = Rng::new(7);
    let mut out = Vec::new();
    for (n, start, (d0, d1), (g0, g1), life) in WAVES {
        let offset = rng.next() * TAU;
        let slot = TAU / n as f32;
        for i in 0..n {
            // An uneven ring: evenly spread, then jittered by up to ±0.96 of a slot.
            let a = offset + i as f32 * slot + rng.range(-0.96, 0.96) * slot;
            out.push(Particle {
                dir: Vec2::angled(a),
                dist: rng.range(d0, d1),
                s0: rng.range(4.0, 28.0) / 20.0,
                growth: rng.range(g0, g1),
                spin: rng.range(-90.0, 90.0).to_radians(),
                delay: start + rng.range(0.0, 0.09),
                life: life * rng.range(0.9, 1.1),
                accent: rng.next() < 0.1,
                twinkle: rng.next() < 0.15,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starfall_throws_the_same_174_stars_every_time() {
        let a = particles();
        let b = particles();
        assert_eq!(a.len(), 174);
        assert!(a.iter().zip(&b).all(|(x, y)| x.dir == y.dir && x.delay == y.delay && x.s0 == y.s0));
        let accents = a.iter().filter(|p| p.accent).count();
        assert!((8..=30).contains(&accents), "about 10% vermilion, got {accents}");
        assert!(a.iter().all(|p| p.delay >= 1.5 && p.delay + p.life < STARFALL_COVERED + 0.5));
    }

    #[test]
    fn the_intro_ends_at_rest() {
        for dot_start in [0.78, 1.05] {
            let i = Intro::at(3.0, dot_start);
            assert_eq!(i.fill, 1.0);
            assert!((i.scale - 1.0).abs() < 1e-3 && i.rot.abs() < 1e-3);
            assert_eq!(i.dot_y, 0.0);
            assert_eq!(i.recoil, 0.0);
            assert_eq!(i.squash, (1.0, 1.0));
            assert_eq!(i.outline_alpha, 0.0);
        }
        // Before anything starts, nothing is visible.
        let i = Intro::at(0.0, 1.05);
        assert_eq!((i.fill, i.outline_alpha, i.dot_alpha), (0.0, 0.0, 0.0));
    }

    #[test]
    fn the_dot_lands_squashed_and_never_dips_below_its_rest() {
        let start = 1.05;
        let impact = Intro::at(start + 0.8 * 0.53, start);
        assert!(impact.squash.0 > 1.25 && impact.squash.1 < 0.75, "{:?}", impact.squash);
        for i in 0..=200 {
            let t = start + 0.8 * i as f32 / 200.0;
            assert!(Intro::at(t, start).dot_y <= 1e-4, "dot below its landing point at {t}");
        }
    }

    #[test]
    fn the_bloom_clears_every_corner_at_any_window_size() {
        for size in [vec2(900.0, 620.0), DESIGN, vec2(2560.0, 1600.0), vec2(3840.0, 1000.0)] {
            let screen = Rect::from_min_size(Pos2::ZERO, size);
            let g = Geometry::new(screen);
            for rot in [0.0, -10f32.to_radians()] {
                let s = cover_scale(&g, screen, rot);
                let outline = theme::sparkle_points(g.c, g.l * s, rot);
                for corner in [screen.left_top(), screen.right_top(), screen.left_bottom(), screen.right_bottom()] {
                    assert!(inside(corner, &outline), "{size:?} rot {rot}: corner {corner:?} not covered");
                }
            }
        }
        // At the design size the star needs roughly 20x to clear the corners.
        let screen = Rect::from_min_size(Pos2::ZERO, DESIGN);
        let s = cover_scale(&Geometry::new(screen), screen, 0.0);
        assert!((15.0..60.0).contains(&s), "{s}");
    }

    #[test]
    fn the_hole_mesh_has_no_folded_triangles() {
        let screen = Rect::from_min_size(Pos2::ZERO, DESIGN);
        let g = Geometry::new(screen);
        for s in [0.92, 1.0, 10.0, 46.0] {
            let outline = theme::sparkle_points(g.c, g.l * s, -0.1);
            let reach = outline.iter().map(|q| (*q - g.c).length()).fold(0.0, f32::max);
            let far = 1.25 * reach.max(screen.size().length());
            // Every quad between consecutive rays must wind the same way.
            let n = outline.len();
            let signs: Vec<bool> = (0..n)
                .map(|i| {
                    let a = outline[i];
                    let b = g.c + (outline[i] - g.c).normalized() * far;
                    let c = outline[(i + 1) % n];
                    (b - a).x * (c - a).y - (b - a).y * (c - a).x > 0.0
                })
                .collect();
            assert!(signs.iter().all(|&x| x == signs[0]), "folded strip at scale {s}");
        }
    }

    #[test]
    fn reveals_settle_and_skip_finishes_at_once() {
        for kind in [Kind::Iris, Kind::Starfall] {
            let mut b = Boot::new(kind, 0.0);
            assert!(!b.finished(0.1));
            assert_eq!(b.reveal(100.0), Reveal::SETTLED);
            b.skip();
            assert!(b.finished(0.0) && !b.covering(0.0));
        }
    }
}
