# Boot, onboarding and permissions: build spec

Status: **approved 2026-10-15, implemented on `feat/onboarding`.** Where the build differs from the first draft, the text below describes what was built, and section 11 lists the differences.

Design reference: the Lodestar Onboarding canvas (private artifact). The boards are Boot animation, Loaders, Boot · Iris, Boot · Starfall, Welcome, Setup 2–6 and Prototype.

## 1. What ships

| Moment | What the user sees | Length |
|---|---|---|
| First launch | Starfall boot → Welcome → Setup | about 3.8 s, then interactive |
| Every later launch | Iris boot → the app (Install page) | about 2.9 s |
| Reduce Motion on | Static mark → 200 ms crossfade → next screen | 0.2 s |
| Any boot | Click, Space, Return or Esc skips straight to the end state | at once |

`LODESTAR_MOTION_SLOWMO` stretches every duration below. Reduce Motion sets every duration to zero, the same way `theme::dur` already does.

Setup appears until the user taps **Continue**. Continue needs both required steps: a signed-in Apple ID and Keychain access. Local Network and a device are optional. Setup can always be reopened from the Apple ID page with "Run setup again".

## 2. Code layout

| File | Change |
|---|---|
| `src/motion.rs` (new) | Spring solver, timeline helpers and keyframe tracks. Pure functions, unit-tested. |
| `src/boot.rs` (new) | The `Boot` struct: the intro, Iris and Starfall. It paints onto a foreground layer and reports `Finished`. |
| `src/onboarding.rs` (new) | The Welcome and Setup screens and the permission probes. |
| `src/theme.rs` | `star_tf()`, the star painted with scale, rotation and offset, plus `star_outline()`, which draws a partial outline. New loader widgets: `working_line`, `search_star`, `beacon`, `done_tick`. |
| `src/app.rs` | `phase: Phase` in front of the existing page router. The 2FA modal renders inline while Setup is showing. |
| `src/store.rs` | `Settings` gains `onboarded: bool` and `boot_variant` (`with_defaults` keeps existing users onboarded if an account or library already exists). |
| `src/engine.rs` | `NetScanner::start` moves behind a `Cmd::StartNetScan` command, so the Local Network prompt appears when the user taps Allow, not at launch. Already-onboarded users start it at launch, as they do today. |

```rust
enum Phase { Boot(Boot), Welcome, Setup, App }
```

## 3. Motion system (`motion.rs`)

These are damped springs in SwiftUI's terms: `response` is the period in seconds and `bounce` runs from 0 to 1. Each one is evaluated in closed form from elapsed time, so it needs no per-frame integration and gives the same frame at any frame rate.

```rust
pub struct Spring { pub response: f32, pub bounce: f32 }
impl Spring {
    /// 0 → 1 with overshoot; exactly 1.0 after `settle()`.
    pub fn at(&self, t: f32) -> f32 {
        let z = 1.0 - self.bounce;                    // damping ratio
        let w = std::f32::consts::TAU / self.response;
        if z >= 1.0 { return 1.0 - (-w * t).exp() * (1.0 + w * t); }
        let wd = w * (1.0 - z * z).sqrt();
        1.0 - (-z * w * t).exp() * ((wd * t).cos() + (z * w / wd) * (wd * t).sin())
    }
    pub fn settle(&self) -> f32 { /* first t after which |x-1| < 0.002 */ }
}
pub const SNAPPY: Spring = Spring { response: 0.42, bounce: 0.18 }; // buttons, rows, letters (0.56 s)
pub const BOUNCY: Spring = Spring { response: 0.50, bounce: 0.35 }; // throb, celebration (0.80 s)
pub const GENTLE: Spring = Spring { response: 0.70, bounce: 0.12 }; // star body swing-in (0.88 s)
pub const SMOOTH: Spring = Spring { response: 0.55, bounce: 0.00 }; // progress bars: never overshoot (0.76 s)
pub const LAND:   Spring = Spring { response: 0.38, bounce: 0.45 }; // done disc pop (0.71 s)
```

Helpers:

- `phase(t, start, len) -> f32`: the progress of one step, from 0 to 1, clamped.
- `track(t, &[(time, value, Ease)])`: piecewise keyframes. Used for the dot's gravity bounce and squash, which are not springs.
- `ease_in(p)` = cubic-bezier(.55,0,1,.45), for gravity.
- `ease_out(p)` = cubic-bezier(0,.55,.45,1), for rising.
- `out_expo(p)` = cubic-bezier(.16,1,.3,1), for rings and throws.
- `in_out(p)` = cubic-bezier(.65,0,.35,1), for outline drawing and travel.

`theme::ease` (cubic-out) and `tween` stay as they are for existing pages.

Frame pacing: while a boot runs, call `ctx.request_repaint()` every frame. For an idle loader, call `request_repaint_after(16 ms)` only while it is visible.

## 4. Shared intro (0–1.5 s)

Coordinates are relative to the star's centre `c`. `L` is the star's half-height (88 px at 1100×760). The mark is centred in the window: the star's centre sits at the window centre minus 8 px, so the star and wordmark together sit optically centred.

| Time (s) | Beat | Implementation |
|---|---|---|
| 0.10–0.95 | The outline traces itself | `star_outline(frac = in_out(phase(t, .10, .85)))`: a polyline over the first N% of `sparkle_points`, 2.7 px stroke. Fades out from 0.95 to 1.30. |
| 0.70–1.58 | The body swings in | Fill alpha follows `phase(t,.70,.40)`. `s = lerp(.84, 1, GENTLE.at(t-.70))`, `rot = lerp(-14°, 0, same)`. |
| 1.05–1.85 | The dot falls, bounces and squashes | y track: −171 px → 0 (ease-in, landing at 52%), up 18 px (ease-out), back to 0 at 86%, then −3 px and 0. The squash track multiplies the dot's (rx, ry): (.82,1.2) while falling, (1.34,.66) on contact, then (.92,1.09), then 1. Drawn with `Shape::ellipse_filled`, with the ellipse's bottom anchored at the landing point. |
| 1.49–2.19 | The star recoils | The star's y offset follows the impulse 0 → +3.5 → −1.2 → +.5 → 0 px. |
| 1.49 and 1.77 | Rings | `circle_stroke` from r = 7 px to 6.5×, alpha .7 → 0, out-expo, 1.2 s and 0.9 s. |
| 0.95–1.75 | Wordmark | One galley per letter, offset +16 px → 0 on SNAPPY, staggered 35 ms per letter. egui has no blur, so alpha does the work that blur does on the canvas. |

## 5. Iris: every later launch (1.5–2.9 s)

1. **Wind-up, 1.5–1.8 s.** Scale 1 → .92 and rotation 0 → −6°, eased in and out. The wordmark fades out from 1.6 s.
2. **Bloom, 1.8–2.9 s.** Scale .92 → 46 and rotation −6° → 0, eased cubic-bezier(.62,0,.12,1). The scale has to reach 46, not 14: the sides curve in so far that the diagonals only reach about 20% of the arm length. Below 46× the window corners stay dark.
3. **The hole.** egui cannot mask. Instead, paint the app normally, then paint the dark ground on a foreground layer as a **ring mesh**. For each of the 40 `sparkle_points` (transformed), take the ray from `c` and pair the point with one on a circle of radius `max(diag, star_max_r·1.02)`. Build the quad strip between those two loops. The star outline is radially monotonic from its centre, so the strip never folds.
4. **The dot leaves, 2.0–2.6 s.** It moves up 600 px with ease-in, its y-radius stretches up to 5×, and its alpha drops to .2.
5. **Push-through, 2.3–2.9 s.** `ctx.set_transform_layer(app_layer, TSTransform::from_scaling(1.06 → 1))` about the window centre, with app opacity 0 → 1 on out-expo. There is no blur.
6. **Done at 2.9 s.** Drop the foreground layer.

## 6. Starfall: first launch only (1.5–3.8 s)

- **Throb at 1.5 s.** Star scale track 1 → 1.15 → .96 → 1.02 → 1 over 0.7 s.
- **Particles.** There are 174, generated once with a fixed seed (inline splitmix32, no new crate) so every first run looks the same. They come in four waves, as `(count, start s, distance × diagonal, growth ×, life s)`:
  - (12, 1.50, 1.3–1.8, 3–6, 1.10)
  - (24, 1.78, 1.3–1.8, 3–6, 1.15)
  - (48, 2.08, 1.3–1.8, 3–6, 1.20)
  - (90, 2.38, 0.12–0.7, 6–14, 1.00)

  Each wave starts at a random angle, and each particle's angle is spread evenly then jittered by ±0.96 of a slot. The start delay is jittered 0–90 ms. Starting size is 4–28 px, spin is ±90°, 10% are vermilion, and 15% twinkle once (alpha 1 → .12 → 1 over 0.32 s).
- **Particle motion.** Position = `dir · d · out_expo(p)`, size = `s0 · lerp(1, growth, out_expo(p))`, rotation = `r · out_expo(p)`, and alpha reaches 1 within the first 7%.
- **Bloom, 2.55–3.35 s.** The central star itself becomes the app's ground. A copy of the star, filled with the app's background colour, is painted above the particles. It grows from its own centre, scale 1 → 48, and counter-rotates 0 → −10°, eased (.7,0,.2,1), until its points pass the window corners. That is the same 46×+ rule as Iris. It is not a circle wipe, so the reveal always reads as coming out of the mark. The app's own background stays transparent until 3.35 s.
- **Hand-off, 3.35–3.8 s.** The Welcome content rises 26 px on SNAPPY and fades in. The particles fade out from 3.45 to 3.75 s, under the flood.
- **Cost.** 174 meshes of 41 vertices is about 7k vertices a frame, which is trivial for egui.

**Colour rule (both variants).** In the dark appearance the ground is `DARK.bg` and the star and particles are paper. The app revealed underneath uses the light palette, because the star is the window. In the light appearance everything inverts. This happens only during the boot. Afterwards the app follows the system appearance as it does now.

## 7. Loaders: one motion per meaning

| Meaning | Widget | Spec |
|---|---|---|
| Working | `working_line(w)` | 2 px track. A 30% vermilion segment travels on `in_out` over 1.15 s and stretches on x from .55 to 1.35 to .55 at peak speed. |
| Searching | `search_star(size)` | The star snaps a quarter turn on SNAPPY, then rests. 1.7 s cycle. |
| Waiting on you | `beacon()` | A 4 px vermilion dot breathes (scale 1 to 1.14, 1.8 s). Two rings out-expo to 4×, 0.9 s apart. **The only vermilion loader.** |
| Progress | existing `bar_live` | The fill target moves on SMOOTH, never overshooting. The shimmer stays. |
| Done | `done_tick()` | The disc scales 0 → 1 on LAND. The tick draws from 0.16 s on out-expo over 0.4 s. The label slides 6 px left-to-right into place. |

Reduce Motion shows the static shapes. The words beside them carry the meaning.

## 8. Setup screen and the real permission hooks

Layout: on the left, 290 px with the title, "N of 4 ready", a bar of four segments (each fills on SMOOTH) and Continue. On the right, four rows: an indicator, a title and a mono tag, a description, a detail line and an action.

| Row | Required | Action → real call | States |
|---|---|---|---|
| Apple ID | yes | **Connect** opens an inline form. **Sign in** sends `Cmd::SignIn { remember: true }`. `Prompt::TwoFactor` renders as an inline 6-cell panel, reusing the modal's cell code; `Prompt::Pin` stays modal. | idle → form → working ("Signing in with Apple…") → code → working ("Checking the code…") → done (`y•••@icloud.com · Personal team`, plus a **Change** link). |
| Keychain | yes | **Allow** runs a probe on a background thread: write, read and delete the `Lodestar` / `setup-probe` item through `keyring`. That is the moment macOS shows its dialog. | idle (disabled until Apple ID is done) → waiting (beacon: "A macOS dialog is open. Enter your Mac password and choose Always Allow.") → done ("Signing key stored") or denied ("Lodestar needs Keychain access to keep your signing key. Try again." plus **Retry**). |
| Local network | no ("For Apple TV") | **Allow** sends `Cmd::StartNetScan`. The first mDNS browse triggers the macOS prompt. | idle → waiting (beacon: "macOS is asking to find devices on your network. Choose Allow.") → done once the browse runs without `EHOSTUNREACH` / a no-route error. If nothing resolves within 8 s, show "If you chose Don't Allow: System Settings › Privacy & Security › Local Network" with an **Open Settings** link (`x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork`). |
| A device | no ("Optional now") | none, it is passive | searching (`search_star`, "Looking for devices…") → found ("Living Room — Apple TV 4K · Wi-Fi" or "iPhone · USB"), from the existing `UsbScanner` / `NetScanner` list. |

Continue arms once both required rows are done, with a one-time LAND scale .94 → 1. Its label changes to **Open Lodestar** when all four are done. The hint line reads, in order of precedence: "Connect your Apple ID to continue", "Waiting for Keychain", "Allow Keychain to continue", "You can finish the rest later", "Everything is ready".

**Row choreography.** Rows enter staggered by 60 ms, rising 14 px on SNAPPY. Panels drop in 8 px with a .985 scale. Detail lines slide 6 px. Status changes cross-fade the indicator, and the done disc pops on LAND.

**Copy.** Use the strings on the canvas exactly. The welcome footer reads "Free and open source · No telemetry · Your password goes only to Apple".

## 9. Tests and checks

These are unit tests in `motion.rs` and `boot.rs`, and they run in `./packaging/check.sh`.

- `Spring::at(0) == 0`, `at(settle()) ≈ 1 ± .002`. SMOOTH never exceeds 1. LAND peaks between 1.10 and 1.14.
- Every `track` is continuous at its keyframes, and each boot timeline ends in its final frame exactly at its stated length.
- With `reduced()`, every boot is finished on its first frame.
- The Starfall seed is deterministic (the same 174 particles twice).
- The ring mesh produces no folded or inverted triangles at scales of 0.92, 1, 10 and 46.
- `Settings` migration: an existing user with an account or a library is treated as onboarded and never sees Starfall.

Then check by hand on hardware, with a clean `~/Library/Application Support/Lodestar`:

- First run: Starfall plays, and each permission prompt appears exactly when its button is pressed.
- Second run: Iris plays.
- Reduce Motion on: crossfade only.
- `LODESTAR_MOTION_SLOWMO=6`: frame-by-frame review.

## 10. Decisions for you to approve

1. **Variant per launch.** Starfall on the first launch only, Iris on every launch after. The alternative is Iris only, keeping Starfall for a first successful install.
2. **Keychain becomes an explicit setup step**, using the probe write. Today the first prompt appears in the middle of the first install. Ad-hoc-signed builds still re-prompt after each update. Fixing that needs the Developer ID signing that is already open as a TODO.
3. **Local Network moves from launch to the Allow button** for first-run users. For already-onboarded users nothing changes.
4. **No blur in egui.** The canvas uses blur on letters and on the Iris push-through. The build uses alpha plus scale instead. A real blur would need a custom `wgpu` pass, which is not worth it here.
5. **Remember me defaults to on** in setup, because the password is what makes one-click refresh work.

Estimate: about 900 lines (motion 150, boot 350, onboarding 350, theme and app wiring 50), plus tests. One PR, reviewed by Bugbot before merge.

## 11. Differences from the approved draft

- **Bloom scale is computed, not fixed.** `boot::cover_scale` binary-searches the smallest scale at which the star (at its final rotation) contains all four window corners, then adds 10%. That is about 21× at 1100×760 and grows with wider windows. A fixed 46× would have been both too large at the design size and too small on a 32:9 display.
- **Iris keeps the hole solid until the push-through.** In the first build, app content showed through the star while it was still small. The hole is now filled with the star colour until 2.3 s, then cross-fades as the camera pushes through.
- **The setup screens live in `src/app/onboarding.rs`**, a child of `app`, so they can share the app's sign-in fields and code-entry cells. `code_field` was lifted out of the 2FA modal, and both now use it.
- **There is no `Cmd::StartNetScan`.** `NetScanner` is created idle and `Engine::start_net_scan()` calls `begin()` directly. It is idempotent, and the engine calls it at launch only when `settings.onboarded` is set.
- **There is no `BOUNCY` spring.** The Starfall throb is a keyframe track, so nothing used it.
- **The Keychain done label is "Keychain ready", not "Signing key stored".** The probe proves access, but the signing key itself is written on the first install.
- **Local Network "done" is a heuristic.** macOS never tells an app it was refused. The row waits up to 6 s after Allow, or until an Apple TV appears, then shows "Listening on your network". After 8 s with no Apple TV, it adds an Open Settings hint.
- **Continue also starts network browsing** (the same as any onboarded launch), so a person who skipped the Local Network row is asked by macOS then, not on their next launch.
- **A "Run setup again" link** is under Apple ID › Setup.
