//! Lodestar's visual system.
//!
//! Warm paper and ink neutrals, one signal colour (vermilion) used only for
//! state that needs attention: selection, progress, the star. Structure comes
//! from hairline rules and a label column, not from boxes. IBM Plex Sans for
//! language, IBM Plex Mono for anything a machine produced (identifiers,
//! versions, counts, labels). Follows the system light/dark appearance.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Margin, Pos2, Rect,
    Response, RichText, Sense, Shape, Stroke, StrokeKind, Theme, Ui, Vec2, pos2, vec2,
};

// ───────────────────────── palette ─────────────────────────

#[derive(Clone, Copy)]
pub struct Pal {
    pub bg: Color32,
    pub surface: Color32,
    pub sunken: Color32,
    pub ink: Color32,
    pub ink2: Color32,
    pub ink3: Color32,
    pub line: Color32,
    pub line2: Color32,
    pub accent: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub bad: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const LIGHT: Pal = Pal {
    bg: hex(0xF5F3EF),
    surface: hex(0xFCFBF9),
    sunken: hex(0xECE9E3),
    ink: hex(0x1A1917),
    ink2: hex(0x57534C),
    ink3: hex(0x8F8A81),
    line: hex(0xE2DED6),
    line2: hex(0xCCC7BE),
    accent: hex(0xD9461B),
    good: hex(0x2E7D4F),
    warn: hex(0xA86A12),
    bad: hex(0xC23A2B),
};

pub const DARK: Pal = Pal {
    bg: hex(0x151413),
    surface: hex(0x1C1B19),
    sunken: hex(0x0F0E0D),
    ink: hex(0xEEEBE5),
    ink2: hex(0xB2ADA4),
    ink3: hex(0x7A756D),
    line: hex(0x2A2825),
    line2: hex(0x3C3935),
    accent: hex(0xF0673B),
    good: hex(0x63B98A),
    warn: hex(0xD6A24A),
    bad: hex(0xE8665A),
};

pub fn pal(ctx: &egui::Context) -> Pal {
    if ctx.theme() == Theme::Dark { DARK } else { LIGHT }
}

// ───────────────────────── type ─────────────────────────

pub fn sans() -> FontFamily {
    FontFamily::Proportional
}
pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}
pub fn mono() -> FontFamily {
    FontFamily::Monospace
}
pub fn mono_medium() -> FontFamily {
    FontFamily::Name("mono-medium".into())
}

/// Page title.
pub fn display(p: &Pal, s: impl Into<String>) -> RichText {
    RichText::new(s).family(semibold()).size(30.0).color(p.ink).extra_letter_spacing(-0.6)
}
/// Section / item title.
pub fn title(p: &Pal, s: impl Into<String>) -> RichText {
    RichText::new(s).family(medium()).size(15.0).color(p.ink)
}
pub fn body(p: &Pal, s: impl Into<String>) -> RichText {
    RichText::new(s).family(sans()).size(14.0).color(p.ink)
}
pub fn quiet(p: &Pal, s: impl Into<String>) -> RichText {
    RichText::new(s).family(sans()).size(13.0).color(p.ink2)
}
/// Mono small caps label — the structural voice of the UI.
pub fn label(p: &Pal, s: &str) -> RichText {
    RichText::new(s.to_uppercase()).family(mono_medium()).size(10.5).color(p.ink3).extra_letter_spacing(1.0)
}
/// Machine-produced values: bundle IDs, versions, counts.
pub fn data(p: &Pal, s: impl Into<String>) -> RichText {
    RichText::new(s).family(mono()).size(12.5).color(p.ink2)
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mono_fallbacks = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let faces: [(&str, &'static [u8]); 5] = [
        ("plex", include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf")),
        ("plex-medium", include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf")),
        ("plex-semibold", include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf")),
        ("plex-mono", include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf")),
        ("plex-mono-medium", include_bytes!("../assets/fonts/IBMPlexMono-Medium.ttf")),
    ];
    for (name, bytes) in faces {
        fonts.font_data.insert(name.into(), FontData::from_static(bytes).into());
    }
    let chain = |first: &str, rest: &[String]| {
        let mut v = vec![first.to_string()];
        v.extend(rest.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, chain("plex", &fallbacks));
    fonts.families.insert(medium(), chain("plex-medium", &fallbacks));
    fonts.families.insert(semibold(), chain("plex-semibold", &fallbacks));
    fonts.families.insert(FontFamily::Monospace, chain("plex-mono", &mono_fallbacks));
    fonts.families.insert(mono_medium(), chain("plex-mono-medium", &mono_fallbacks));
    ctx.set_fonts(fonts);

    // Follows the system appearance; LODESTAR_THEME=light|dark overrides it.
    ctx.set_theme(match std::env::var("LODESTAR_THEME").as_deref() {
        Ok("light") => egui::ThemePreference::Light,
        Ok("dark") => egui::ThemePreference::Dark,
        _ => egui::ThemePreference::System,
    });
    ctx.all_styles_mut(|style| {
        use egui::TextStyle::*;
        style.text_styles = [
            (Heading, FontId::new(30.0, semibold())),
            (Body, FontId::proportional(14.0)),
            (Button, FontId::new(13.5, medium())),
            (Small, FontId::proportional(12.0)),
            (Monospace, FontId::monospace(12.5)),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(14.0, 7.0);
        style.spacing.interact_size.y = 32.0;
        style.interaction.selectable_labels = false;
        style.animation_time = dur(0.12);
    });
    for (theme, p) in [(Theme::Light, LIGHT), (Theme::Dark, DARK)] {
        ctx.style_mut_of(theme, |style| {
            let v = &mut style.visuals;
            v.dark_mode = theme == Theme::Dark;
            v.panel_fill = p.bg;
            v.window_fill = p.surface;
            v.extreme_bg_color = p.surface;
            v.faint_bg_color = p.sunken;
            v.code_bg_color = p.sunken;
            v.override_text_color = Some(p.ink);
            v.hyperlink_color = p.ink;
            v.selection.bg_fill = p.accent.gamma_multiply(0.28);
            v.selection.stroke = Stroke::new(1.0, p.accent);
            v.text_cursor.stroke = Stroke::new(1.5, p.accent);
            v.window_corner_radius = CornerRadius::same(10);
            v.window_stroke = Stroke::new(1.0, p.line2);
            v.window_shadow = egui::Shadow {
                offset: [0, 12],
                blur: 40,
                spread: 0,
                color: Color32::from_black_alpha(if v.dark_mode { 120 } else { 40 }),
            };
            v.popup_shadow = v.window_shadow;
            let r = CornerRadius::same(6);
            let w = &mut v.widgets;
            for (state, fill, stroke) in [
                (&mut w.noninteractive, p.surface, p.line),
                (&mut w.inactive, p.surface, p.line2),
                (&mut w.hovered, p.surface, p.ink3),
                (&mut w.active, p.sunken, p.ink2),
                (&mut w.open, p.sunken, p.ink2),
            ] {
                state.bg_fill = fill;
                state.weak_bg_fill = fill;
                state.bg_stroke = Stroke::new(1.0, stroke);
                state.fg_stroke = Stroke::new(1.0, p.ink);
                state.corner_radius = r;
                state.expansion = 0.0;
            }
            v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.ink2);
        });
    }
}

// ───────────────────────── motion ─────────────────────────
//
// Motion explains state changes (a page arriving, a control arming, progress
// advancing); it is never decoration. Every duration collapses to zero when
// macOS "Reduce motion" is on, or when LODESTAR_REDUCE_MOTION=1.

static REDUCED: AtomicBool = AtomicBool::new(false);
/// `LODESTAR_MOTION_SLOWMO=N` stretches every animation N times (1 to 20) so a
/// transition can be inspected frame by frame. Stored as f32 bits.
static SLOWMO: AtomicU32 = AtomicU32::new(0x3f80_0000);

/// Reads the accessibility preference once at startup. The key is absent
/// when the setting is off and `1` when it is on.
pub fn init_motion() {
    let forced = std::env::var("LODESTAR_REDUCE_MOTION").is_ok_and(|v| v == "1");
    let system = std::process::Command::new("defaults")
        .args(["read", "com.apple.universalaccess", "reduceMotion"])
        .output()
        .ok()
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "1");
    REDUCED.store(forced || system, Ordering::Relaxed);
    let slow = std::env::var("LODESTAR_MOTION_SLOWMO")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .map_or(1.0, |v| v.clamp(1.0, 20.0));
    SLOWMO.store(slow.to_bits(), Ordering::Relaxed);
}

/// The slow-motion factor (1.0 normally).
pub fn slowmo() -> f32 {
    f32::from_bits(SLOWMO.load(Ordering::Relaxed))
}

pub fn reduced() -> bool {
    REDUCED.load(Ordering::Relaxed)
}

/// A duration, zero under reduced motion, stretched under slow motion.
pub fn dur(secs: f32) -> f32 {
    if reduced() { 0.0 } else { secs * slowmo() }
}

pub fn ease(t: f32) -> f32 {
    egui::emath::easing::cubic_out(t.clamp(0.0, 1.0))
}

/// Eases 0 to 1 (and back) as `on` flips. Instant under reduced motion.
pub fn tween(ctx: &egui::Context, id: Id, on: bool, secs: f32) -> f32 {
    if reduced() {
        return if on { 1.0 } else { 0.0 };
    }
    ctx.animate_bool_with_time_and_easing(id, on, dur(secs), egui::emath::easing::cubic_out)
}

/// Eases a value toward `target`. Instant under reduced motion.
pub fn tween_to(ctx: &egui::Context, id: Id, target: f32, secs: f32) -> f32 {
    if reduced() { target } else { ctx.animate_value_with_time(id, target, dur(secs)) }
}

const PAGE_SINCE: &str = "lodestar-page-since";
const PAGE_INDEX: &str = "lodestar-page-index";

/// Called once per frame: seconds since the current page appeared. Resets the
/// per-page counter that staggers `reveal`.
pub fn begin_page(ctx: &egui::Context, since: f32) {
    ctx.data_mut(|d| {
        d.insert_temp(Id::new(PAGE_SINCE), since / slowmo());
        d.insert_temp(Id::new(PAGE_INDEX), 0usize);
    });
}

/// Opacity for the next block on the page. Blocks fade in one after another,
/// 60 ms apart (capped), so a page assembles top to bottom.
pub fn reveal(ctx: &egui::Context) -> f32 {
    reveal_step(ctx, 0.06)
}

pub fn reveal_step(ctx: &egui::Context, step: f32) -> f32 {
    if reduced() {
        return 1.0;
    }
    let (since, index) = ctx.data_mut(|d| {
        let since = d.get_temp::<f32>(Id::new(PAGE_SINCE)).unwrap_or(10.0);
        let index = d.get_temp::<usize>(Id::new(PAGE_INDEX)).unwrap_or(0);
        d.insert_temp(Id::new(PAGE_INDEX), index + 1);
        (since, index)
    });
    reveal_opacity(since, index, step)
}

/// Opacity of block `index` once `since` seconds have passed: each block
/// starts `step` after the previous one (the stagger is capped at 0.5 s) and
/// takes 0.3 s to fade in.
fn reveal_opacity(since: f32, index: usize, step: f32) -> f32 {
    ease((since - (index as f32 * step).min(0.5)) / 0.3)
}

/// Runs `add` with everything it paints multiplied by `opacity`.
pub fn faded<R>(ui: &mut Ui, opacity: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.set_opacity(opacity);
        add(ui)
    })
    .inner
}

fn hover_t(ui: &Ui, resp: &Response, active: bool) -> f32 {
    tween(ui.ctx(), resp.id.with("hover"), active && resp.hovered(), 0.12)
}

// ───────────────────────── primitives ─────────────────────────

pub fn rule(ui: &mut Ui, p: &Pal) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, p.line));
}

/// Width of the section label column; content starts at `LABEL_W + 8`.
pub const LABEL_W: f32 = 132.0;

/// A two-column block: mono label on the left, content on the right,
/// separated from the next block by a hairline. Fades in with the page.
pub fn section<R>(ui: &mut Ui, p: &Pal, name: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    section_ruled(ui, p, name, true, add)
}

/// `section` without the closing hairline, for a block that sits directly
/// above a pinned footer (which draws its own).
pub fn section_ruled<R>(ui: &mut Ui, p: &Pal, name: &str, closing_rule: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    let opacity = reveal(ui.ctx());
    faded(ui, opacity, |ui| {
        ui.add_space(20.0);
        let r = ui
            .horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(LABEL_W, 20.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_min_width(LABEL_W);
                    ui.add_space(3.0);
                    ui.label(label(p, name));
                });
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    add(ui)
                })
                .inner
            })
            .inner;
        ui.add_space(20.0);
        if closing_rule {
            rule(ui, p);
        }
        r
    })
}

/// The Lodestar mark: a four-point sparkle with concave sides and a signal
/// dot above its north tip. Same geometry as the app icon.
pub fn star(painter: &egui::Painter, c: Pos2, size: f32, ink: Color32, accent: Color32) {
    fill_star(painter, c, &sparkle_points(c, size * 0.5, 0.0), ink);
    painter.circle_filled(c + vec2(0.0, -size * 0.5 - size * 0.16), size * 0.08, accent);
}

/// Fills a star outline (from `sparkle_points`) as a fan around its centre `c`.
pub fn fill_star(painter: &egui::Painter, c: Pos2, outline: &[Pos2], color: Color32) {
    if color.a() == 0 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(c, color);
    for q in outline {
        mesh.colored_vertex(*q, color);
    }
    let n = outline.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(Shape::mesh(mesh));
    // A hairline outline gives the mesh anti-aliased edges.
    painter.add(Shape::closed_line(outline.to_vec(), Stroke::new(0.6, color)));
}

/// Points per edge of the star outline; the outline has four times as many.
pub const STAR_EDGE_POINTS: usize = 10;

/// The star outline around `c` with half-height `l`, rotated by `rot` radians.
/// Tips on the axes (horizontal arms 86% of vertical), edges bowed toward the
/// centre as quadratic curves. Radially monotonic from `c`: every ray from the
/// centre crosses the outline once.
pub fn sparkle_points(c: Pos2, l: f32, rot: f32) -> Vec<Pos2> {
    let tips = [vec2(0.0, -l), vec2(l * 0.86, 0.0), vec2(0.0, l), vec2(-l * 0.86, 0.0)];
    let (sin, cos) = rot.sin_cos();
    let turn = |v: Vec2| vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
    let mut pts = Vec::with_capacity(4 * STAR_EDGE_POINTS);
    for i in 0..4 {
        let a = tips[i];
        let b = tips[(i + 1) % 4];
        let ctrl = (a + b) * 0.09;
        for k in 0..STAR_EDGE_POINTS {
            let t = k as f32 / STAR_EDGE_POINTS as f32;
            let u = 1.0 - t;
            pts.push(c + turn(a * (u * u) + ctrl * (2.0 * u * t) + b * (t * t)));
        }
    }
    pts
}

/// The first `frac` of a closed outline, as an open polyline (for drawing it on).
pub fn partial_outline(outline: &[Pos2], frac: f32) -> Vec<Pos2> {
    partial_path(outline, frac, true)
}

/// The first `frac` (by length) of a polyline; `closed` includes the edge back
/// to the first point.
fn partial_path(pts: &[Pos2], frac: f32, closed: bool) -> Vec<Pos2> {
    let n = pts.len();
    if n < 2 || frac <= 0.0 {
        return Vec::new();
    }
    let edges = if closed { n } else { n - 1 };
    let len = |i: usize| (pts[(i + 1) % n] - pts[i]).length();
    let total: f32 = (0..edges).map(len).sum();
    let mut left = total * frac.min(1.0);
    let mut out = vec![pts[0]];
    for i in 0..edges {
        let seg = len(i);
        let next = pts[(i + 1) % n];
        if left >= seg {
            out.push(next);
            left -= seg;
        } else {
            out.push(pts[i] + (next - pts[i]) * (left / seg.max(1e-6)));
            break;
        }
    }
    out
}

// ───────────────────────── loaders ─────────────────────────
//
// One motion per meaning. Each takes the time it has been showing (`t`, in
// seconds) and paints into the given painter. Under reduced motion they draw
// their resting shape.

/// WORKING: a vermilion segment travels along a hairline track and stretches
/// while it moves fastest. Busy, and it will finish on its own.
pub fn working_line(painter: &egui::Painter, r: Rect, p: &Pal, t: f32) {
    painter.rect_filled(r, CornerRadius::same(1), p.line);
    let seg_w = r.width() * 0.32;
    let (x, stretch) = if reduced() {
        (r.left() + (r.width() - seg_w) * 0.5, 1.0)
    } else {
        let k = (t / 1.15).fract();
        let x = r.left() - seg_w * 1.1 + crate::motion::in_out(k) * (r.width() + seg_w * 2.2);
        (x, 0.55 + 0.8 * (std::f32::consts::PI * crate::motion::in_out(k)).sin())
    };
    let w = seg_w * stretch;
    let cx = x + seg_w * 0.5;
    let seg = Rect::from_min_max(
        pos2((cx - w * 0.5).max(r.left()), r.top()),
        pos2((cx + w * 0.5).min(r.right()), r.bottom()),
    );
    if seg.width() > 0.0 {
        painter.rect_filled(seg, CornerRadius::same(1), p.accent);
    }
}

/// SEARCHING: the star snaps a quarter turn on a snappy spring, then rests.
pub fn search_star(painter: &egui::Painter, c: Pos2, l: f32, p: &Pal, t: f32) {
    let turn = if reduced() {
        0.0
    } else {
        let cycle = 1.7;
        let n = (t / cycle).floor();
        let k = t - n * cycle;
        (n + crate::motion::SNAPPY.at(k)) * std::f32::consts::FRAC_PI_2
    };
    fill_star(painter, c, &sparkle_points(c, l, turn), p.ink);
    painter.circle_filled(c + vec2(0.0, -l * 1.32), l * 0.17, p.accent);
}

/// WAITING ON YOU: a vermilion dot breathes and sends out rings. The only
/// vermilion loader: something outside Lodestar needs an answer.
pub fn beacon(painter: &egui::Painter, c: Pos2, r: f32, p: &Pal, t: f32) {
    if !reduced() {
        for offset in [0.0, 0.9] {
            let k = ((t + offset) / 1.8).fract();
            let e = crate::motion::out_expo(k);
            painter.circle_stroke(c, r * (1.0 + 3.0 * e), Stroke::new(1.5, p.accent.gamma_multiply(0.6 * (1.0 - e))));
        }
    }
    let breathe = if reduced() {
        1.0
    } else {
        1.0 + 0.14 * (std::f32::consts::PI * crate::motion::in_out((t / 1.8).fract())).sin()
    };
    painter.circle_filled(c, r * breathe, p.accent);
}

/// DONE: the disc lands with real bounce, then the tick draws once it has
/// settled. `t` is the time since the step finished.
pub fn done_tick(painter: &egui::Painter, c: Pos2, r: f32, p: &Pal, t: f32) {
    let (s, k) = if reduced() {
        (1.0, 1.0)
    } else {
        (crate::motion::LAND.at(t), crate::motion::out_expo(crate::motion::phase(t, 0.16, 0.4)))
    };
    painter.circle_filled(c, r * s, p.ink);
    if k > 0.0 {
        let u = r / 12.0;
        let pts = [c + vec2(-5.5, 0.3) * u, c + vec2(-1.7, 4.0) * u, c + vec2(5.6, -3.6) * u];
        let line = partial_path(&pts, k, false);
        painter.add(Shape::line(line, Stroke::new(2.0 * u.max(0.8), p.bg)));
    }
}

/// Eases a value toward `target` along a spring, restarting from wherever it
/// is whenever the target changes. Instant under reduced motion.
pub fn spring_to(ctx: &egui::Context, id: Id, target: f32, spring: crate::motion::Spring) -> f32 {
    if reduced() {
        return target;
    }
    let now = ctx.input(|i| i.time);
    let value_at =
        |(from, to, t0): (f32, f32, f64), now: f64| from + (to - from) * spring.at((now - t0) as f32 / slowmo());
    let state = ctx.data_mut(|d| {
        let e = d.get_temp_mut_or_insert_with(id, || (target, target, now - 60.0));
        if e.1 != target {
            *e = (value_at(*e, now), target, now);
        }
        *e
    });
    let elapsed = (now - state.2) as f32 / slowmo();
    if elapsed < spring.settle() {
        ctx.request_repaint();
    }
    value_at(state, now)
}

/// Design seconds since `key` last changed for `id` (large if it never did
/// while visible). Lets a widget play a one-shot animation on a state change.
pub fn since_change(ctx: &egui::Context, id: Id, key: u64) -> f32 {
    let now = ctx.input(|i| i.time);
    let t0 = ctx.data_mut(|d| {
        let e = d.get_temp_mut_or_insert_with(id, || (key, now - 60.0));
        if e.0 != key {
            *e = (key, now);
        }
        e.1
    });
    ((now - t0) as f32 / slowmo()).max(0.0)
}

pub fn dot(ui: &mut Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(8.0, 14.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 3.0, color);
}

// ───────────────────────── controls ─────────────────────────

fn hover_pointer(resp: Response, enabled: bool) -> Response {
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// Solid ink button. The one strong element on a page. It "arms" with a short
/// fade when it becomes usable, so you notice the moment a step is complete.
pub fn primary(ui: &mut Ui, p: &Pal, text: &str, enabled: bool) -> Response {
    primary_sized(ui, p, text, enabled, None)
}

pub fn primary_sized(ui: &mut Ui, p: &Pal, text: &str, enabled: bool, width: Option<f32>) -> Response {
    let galley = ui.painter().layout_no_wrap(text.into(), FontId::new(13.5, medium()), p.bg);
    let size = vec2(width.unwrap_or(galley.size().x + 36.0), 36.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let armed = tween(ui.ctx(), resp.id.with("armed"), enabled, 0.22);
    let hot = hover_t(ui, &resp, enabled);
    let mut base = lerp(p.ink, lerp(p.ink, p.ink2, 0.3), hot);
    if enabled && resp.is_pointer_button_down_on() {
        base = p.ink2;
    }
    let fill = lerp(p.sunken, base, armed);
    let fg = lerp(p.ink3, p.bg, armed);
    ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::new(13.5, medium()), fg);
    hover_pointer(resp, enabled)
}

/// Hairline-outlined button for secondary actions.
pub fn secondary(ui: &mut Ui, p: &Pal, text: &str, enabled: bool) -> Response {
    let font = FontId::new(13.0, medium());
    let galley = ui.painter().layout_no_wrap(text.into(), font.clone(), p.ink);
    let size = vec2(galley.size().x + 28.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let t = hover_t(ui, &resp, enabled);
    ui.painter().rect(
        rect,
        CornerRadius::same(6),
        p.sunken.gamma_multiply(t),
        Stroke::new(1.0, lerp(p.line2, p.ink3, t)),
        StrokeKind::Inside,
    );
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, if enabled { p.ink } else { p.ink3 });
    hover_pointer(resp, enabled)
}

/// Inline text action; the underline strengthens on hover.
pub fn link(ui: &mut Ui, p: &Pal, text: &str, color: Option<Color32>) -> Response {
    let _ = p;
    let color = color.unwrap_or(p.ink);
    let font = FontId::new(13.0, medium());
    let galley = ui.painter().layout_no_wrap(text.into(), font.clone(), color);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(0.0, 4.0), Sense::click());
    let t = hover_t(ui, &resp, true);
    ui.painter().galley(rect.left_top(), galley, color);
    let y = rect.bottom() - 2.0;
    ui.painter().hline(rect.x_range(), y, Stroke::new(1.0, color.gamma_multiply(0.25 + 0.75 * t)));
    hover_pointer(resp, true)
}

/// Square checkbox. The box fills and the tick draws itself in two strokes.
pub fn checkbox(ui: &mut Ui, p: &Pal, on: &mut bool, text: &str, hint: &str) -> Response {
    let resp = ui
        .horizontal_top(|ui| {
            let (rect, mut resp) = ui.allocate_exact_size(vec2(16.0, 20.0), Sense::click());
            let b = Rect::from_center_size(rect.center(), vec2(16.0, 16.0));
            let t = tween(ui.ctx(), resp.id.with("on"), *on, 0.18);
            let hot = hover_t(ui, &resp, true);
            ui.painter().rect(
                b,
                CornerRadius::same(4),
                p.surface,
                Stroke::new(1.2, lerp(p.line2, p.ink2, hot).gamma_multiply(1.0 - t)),
                StrokeKind::Inside,
            );
            ui.painter().rect_filled(b, CornerRadius::same(4), p.ink.gamma_multiply(t));
            // Tick: first stroke down-right, then up-right, drawn as `k` grows.
            let k = ((t - 0.3) / 0.7).clamp(0.0, 1.0);
            if k > 0.0 {
                let a = b.left_center() + vec2(3.6, 0.3);
                let m = b.center_bottom() + vec2(-1.2, -4.0);
                let z = b.right_top() + vec2(-3.4, 4.2);
                let line = if k <= 0.5 {
                    vec![a, a + (m - a) * (k / 0.5)]
                } else {
                    vec![a, m, m + (z - m) * ((k - 0.5) / 0.5)]
                };
                ui.painter().add(Shape::line(line, Stroke::new(1.8, p.bg)));
            }
            ui.add_space(4.0);
            let text_resp = ui
                .vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(body(p, text));
                    if !hint.is_empty() {
                        ui.label(quiet(p, hint).size(12.5).color(p.ink3));
                    }
                })
                .response
                .interact(Sense::click());
            if resp.clicked() || text_resp.clicked() {
                *on = !*on;
                resp.mark_changed();
            }
            resp | text_resp
        })
        .inner;
    hover_pointer(resp, true)
}

/// Single-line field with a mono label above it.
pub fn field(ui: &mut Ui, p: &Pal, name: &str, value: &mut String, hint: &str, width: f32, password: bool) -> Response {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.label(label(p, name));
        text_input(ui, p, value, hint, width, password)
    })
    .inner
}

pub fn text_input(ui: &mut Ui, p: &Pal, value: &mut String, hint: &str, width: f32, password: bool) -> Response {
    let id = ui.next_auto_id();
    let focused = ui.memory(|m| m.has_focus(id));
    let t = tween(ui.ctx(), id.with("focus"), focused, 0.14);
    let frame = egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, lerp(p.line2, p.ink2, t)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 8));
    ui.add(
        egui::TextEdit::singleline(value)
            .id(id)
            .frame(frame)
            .hint_text(RichText::new(hint).color(p.ink3))
            .password(password)
            .font(FontId::proportional(14.0))
            .text_color(p.ink)
            .desired_width(width),
    )
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn text_at(ui: &Ui, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32) -> Rect {
    ui.painter().text(pos, align, text, font, color)
}

/// Thin determinate bar.
pub fn bar(ui: &mut Ui, p: &Pal, frac: f32, color: Color32, width: f32, height: f32) {
    bar_inner(ui, p, frac, color, width, height, false);
}

/// Progress bar for work in flight: a soft highlight travels along the filled
/// part, so a pause between steps still reads as "working", not "stuck".
pub fn bar_live(ui: &mut Ui, p: &Pal, frac: f32, color: Color32, width: f32, height: f32) {
    bar_inner(ui, p, frac, color, width, height, true);
}

fn bar_inner(ui: &mut Ui, p: &Pal, frac: f32, color: Color32, width: f32, height: f32, shimmer: bool) {
    let (r, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(1), p.line);
    let mut f = r;
    f.set_width(r.width() * frac.clamp(0.0, 1.0));
    ui.painter().rect_filled(f, CornerRadius::same(1), color);
    if shimmer && !reduced() && f.width() > 6.0 {
        let t = ui.input(|i| i.time) as f32;
        let band = 56.0;
        let x = f.left() - band + ((t * 0.8) % 1.0) * (f.width() + band);
        let strip = Rect::from_min_max(pos2(x.max(f.left()), f.top()), pos2((x + band).min(f.right()), f.bottom()));
        if strip.width() > 0.0 {
            ui.painter().rect_filled(strip, CornerRadius::same(1), Color32::WHITE.gamma_multiply(0.4));
        }
        ui.ctx().request_repaint();
    }
}

pub fn square(v: f32) -> Vec2 {
    Vec2::splat(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ctx: &egui::Context, time: f64, f: impl FnOnce(&egui::Context)) {
        let mut f = Some(f);
        let mut out = ctx.run_ui(egui::RawInput { time: Some(time), ..Default::default() }, |ui| {
            if let Some(f) = f.take() {
                f(ui.ctx());
            }
        });
        // egui panics if a frame's texture uploads are dropped unhandled.
        out.textures_delta.clear();
    }

    #[test]
    fn easing_starts_at_zero_ends_at_one_and_never_overshoots() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        let mut prev = 0.0;
        for i in 0..=100 {
            let v = ease(i as f32 / 100.0);
            assert!((0.0..=1.0).contains(&v) && v >= prev, "not monotonic at {i}: {v} < {prev}");
            prev = v;
        }
        assert_eq!(ease(-3.0), 0.0);
        assert_eq!(ease(9.0), 1.0);
    }

    #[test]
    fn blocks_reveal_in_order_and_all_finish() {
        // Before its turn a block is invisible; later blocks start later.
        assert_eq!(reveal_opacity(0.0, 3, 0.06), 0.0);
        let early = reveal_opacity(0.2, 0, 0.06);
        let late = reveal_opacity(0.2, 3, 0.06);
        assert!(early > late && late > 0.0 && early < 1.0);
        // Everything is fully visible after 0.5 s stagger cap + 0.3 s fade.
        for i in [0, 3, 10, 500] {
            assert_eq!(reveal_opacity(0.8, i, 0.06), 1.0, "block {i}");
        }
    }

    #[test]
    fn tween_eases_toward_the_target_over_time() {
        let ctx = egui::Context::default();
        let id = Id::new("t");
        let mut seen = Vec::new();
        for (i, on) in [(0, false), (1, true), (2, true), (3, true), (4, true), (5, true)] {
            frame(&ctx, i as f64 * 0.05, |c| seen.push(tween(c, id, on, 0.2)));
        }
        assert_eq!(seen[0], 0.0);
        assert!(seen[1] <= seen[2] && seen[2] <= seen[3], "{seen:?}");
        assert!(seen[2] > 0.0 && seen[2] < 1.0, "mid-flight value expected: {seen:?}");
        assert_eq!(*seen.last().unwrap(), 1.0, "{seen:?}");
    }

    #[test]
    fn tween_to_moves_a_value_without_jumping() {
        let ctx = egui::Context::default();
        let id = Id::new("v");
        let mut seen = Vec::new();
        for (i, target) in [(0, 10.0), (1, 50.0), (2, 50.0), (3, 50.0), (4, 50.0), (5, 50.0), (6, 50.0)] {
            frame(&ctx, i as f64 * 0.1, |c| seen.push(tween_to(c, id, target, 0.3)));
        }
        assert_eq!(seen[0], 10.0);
        assert!(seen[2] > 10.0 && seen[2] < 50.0, "{seen:?}");
        assert_eq!(*seen.last().unwrap(), 50.0, "{seen:?}");
    }

    #[test]
    fn colour_lerp_is_clamped_and_exact_at_the_ends() {
        let a = Color32::from_rgb(10, 20, 30);
        let b = Color32::from_rgb(110, 120, 130);
        assert_eq!(lerp(a, b, 0.0), a);
        assert_eq!(lerp(a, b, 1.0), b);
        assert_eq!(lerp(a, b, 7.0), b);
        assert_eq!(lerp(a, b, -7.0), a);
        assert_eq!(lerp(a, b, 0.5), Color32::from_rgb(60, 70, 80));
    }

    #[test]
    fn palettes_keep_text_readable_on_their_backgrounds() {
        // WCAG-style contrast: body ink needs 7:1, quiet text 4.5:1, the
        // faintest label colour 3:1 (it is only used for mono captions).
        fn lum(c: Color32) -> f32 {
            let f = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
        }
        fn contrast(a: Color32, b: Color32) -> f32 {
            let (x, y) = (lum(a), lum(b));
            (x.max(y) + 0.05) / (x.min(y) + 0.05)
        }
        for (name, p) in [("light", LIGHT), ("dark", DARK)] {
            assert!(contrast(p.ink, p.bg) >= 7.0, "{name} ink {}", contrast(p.ink, p.bg));
            assert!(contrast(p.ink2, p.bg) >= 4.5, "{name} ink2 {}", contrast(p.ink2, p.bg));
            assert!(contrast(p.ink3, p.bg) >= 3.0, "{name} ink3 {}", contrast(p.ink3, p.bg));
            for (label, c) in [("accent", p.accent), ("good", p.good), ("bad", p.bad), ("warn", p.warn)] {
                assert!(contrast(c, p.bg) >= 3.0, "{name} {label} {}", contrast(c, p.bg));
            }
        }
    }
}
