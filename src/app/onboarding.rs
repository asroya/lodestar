//! First run: the Welcome screen and the Setup checklist.
//!
//! Setup asks for everything Lodestar needs up front, one row at a time, at
//! the moment the person presses the button, instead of surprising them with
//! macOS dialogs in the middle of their first install:
//!
//! | Row           | Needed to continue | What pressing the button does                 |
//! |---------------|--------------------|-----------------------------------------------|
//! | Apple ID      | yes                | Signs in (2FA happens inline, not in a modal) |
//! | Keychain      | yes                | `store::keychain_probe` on a worker thread    |
//! | Local network | no (Apple TV only) | Starts Bonjour browsing, so macOS asks once   |
//! | A device      | no                 | Nothing: it just watches for devices          |

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Align, Color32, CornerRadius, Id, Layout, Margin, Pos2, Rect, Sense, Stroke, Ui, vec2};

use super::{LodestarApp, Phase, code_field};
use crate::{
    boot::Reveal,
    device::DeviceKind,
    engine::{AccountState, Cmd, Prompt},
    motion::SMOOTH,
    store,
    theme::{self, *},
};

const LOCAL_NETWORK_SETTINGS: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork";

/// Where the Keychain probe is.
#[derive(Clone, Default, PartialEq)]
enum Probe {
    #[default]
    Idle,
    Waiting,
    Ready,
    Denied,
}

#[derive(Default)]
pub(super) struct Setup {
    /// The Apple ID row is expanded into the sign-in form.
    form_open: bool,
    /// A 2FA code was sent to Apple and we're waiting to hear back.
    code_sent: bool,
    keychain: Arc<Mutex<Probe>>,
    /// When Allow was pressed on the Local Network row (egui clock).
    network_asked: Option<f64>,
    /// When the screen appeared, for its entrance.
    epoch: Option<f64>,
}

/// Status shape on the left of a row. One motion per meaning.
#[derive(Clone, Copy, PartialEq)]
enum Status {
    Idle { next: bool },
    Working,
    Waiting,
    Searching,
    Done,
}

impl Status {
    /// Stable key for `since_change`, so the done disc pops once per change.
    fn key(self) -> u64 {
        match self {
            Status::Idle { .. } => 0,
            Status::Working => 1,
            Status::Waiting => 2,
            Status::Searching => 3,
            Status::Done => 4,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Primary,
    Secondary,
    Disabled,
    Link,
}

struct Row<'a> {
    id: &'a str,
    status: Status,
    title: &'a str,
    tag: &'a str,
    desc: &'a str,
    detail: Option<(String, Color32)>,
    action: Option<(&'a str, Action)>,
}

/// Width of the status column, and so the inset of a row's expanded panel.
const STATUS_W: f32 = 34.0;
const ACTION_W: f32 = 132.0;

impl LodestarApp {
    // ───────────────────────── Welcome ─────────────────────────

    pub(super) fn welcome(&mut self, ui: &mut Ui, p: &Pal, reveal: Reveal) {
        let ctx = ui.ctx().clone();
        // The boot's hand-off is this screen's entrance, so its blocks start settled.
        theme::begin_page(&ctx, 10.0);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            super::window_drag_strip(ui);
            ui.set_opacity(reveal.alpha);
            let full = ui.max_rect();
            // Footer first, pinned to the bottom.
            let footer = Rect::from_min_max(Pos2::new(full.left(), full.bottom() - 44.0), full.max);
            ui.painter().text(
                footer.center(),
                egui::Align2::CENTER_CENTER,
                "Free and open source · No telemetry · Your password goes only to Apple",
                egui::FontId::new(11.0, mono()),
                p.ink2,
            );

            let col_w = 520.0f32.min(full.width() - 48.0);
            let col_h = 430.0;
            let top = full.top() + ((full.height() - 44.0 - col_h) * 0.5).max(24.0) + reveal.rise;
            let col = Rect::from_min_size(Pos2::new(full.center().x - col_w * 0.5, top), vec2(col_w, col_h));
            ui.scope_builder(egui::UiBuilder::new().max_rect(col).layout(Layout::top_down(Align::Min)), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let a = theme::reveal(&ctx);
                theme::faded(ui, a, |ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(26.0, 40.0), Sense::hover());
                    star(ui.painter(), r.center() + vec2(0.0, 4.0), 30.0, p.ink, p.accent);
                    ui.add_space(14.0);
                    ui.label(
                        egui::RichText::new("Welcome to Lodestar")
                            .family(semibold())
                            .size(36.0)
                            .color(p.ink)
                            .extra_letter_spacing(-0.8),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        body(p, "Sign apps with your own Apple ID and install them on your iPhone, iPad or Apple TV. Setup takes about two minutes.")
                            .size(16.0)
                            .color(p.ink2),
                    );
                });
                ui.add_space(26.0);
                let a = theme::reveal(&ctx);
                theme::faded(ui, a, |ui| {
                    rule(ui, p);
                    for (n, head, sub) in [
                        ("01", "Connect your Apple ID", "A free one works. It is what signs your apps."),
                        ("02", "Allow two permissions", "Keychain, to keep your signing key. Local Network, to find an Apple TV."),
                        ("03", "Connect a device", "A cable for iPhone and iPad. Wi-Fi for Apple TV. You can also do this later."),
                    ] {
                        ui.add_space(13.0);
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 18.0;
                            ui.label(data(p, n).color(p.ink2));
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.label(title(p, head));
                                ui.label(quiet(p, sub));
                            });
                        });
                        ui.add_space(13.0);
                        rule(ui, p);
                    }
                });
                ui.add_space(26.0);
                let a = theme::reveal(&ctx);
                theme::faded(ui, a, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 18.0;
                        let go = primary_sized(ui, p, "Get started", true, Some(136.0));
                        ui.label(data(p, "About two minutes").color(p.ink2));
                        if go.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.open_setup();
                        }
                    });
                });
            });
        });
    }

    pub(super) fn open_setup(&mut self) {
        self.setup.epoch = None;
        self.phase = Phase::Setup;
    }

    // ───────────────────────── Setup ─────────────────────────

    pub(super) fn setup(&mut self, ui: &mut Ui, p: &Pal) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let since = (now - *self.setup.epoch.get_or_insert(now)) as f32;
        theme::begin_page(&ctx, since);
        // Loaders, springs and the entrance all animate; idle screens still tick
        // slowly enough to notice devices and dialogs finishing.
        ctx.request_repaint_after(std::time::Duration::from_millis(if theme::reduced() { 250 } else { 16 }));

        let (account, prompt, devices) = {
            let s = self.engine.shared.lock().unwrap();
            (s.account(), s.prompt.clone(), s.devices.clone())
        };
        let keychain = self.setup.keychain.lock().unwrap().clone();
        let signed_in = matches!(account, AccountState::SignedIn { .. });
        if !matches!(account, AccountState::SigningIn) {
            self.setup.code_sent = false;
        }

        let net_started = self.engine.net_scan_started();
        let tv = devices.iter().find(|d| matches!(d.kind, DeviceKind::Network { .. }));
        let asked_for = self.setup.network_asked.map(|t| (now - t) as f32).unwrap_or(f32::MAX);
        let net_waiting = net_started && tv.is_none() && asked_for < 6.0;
        let net_done = net_started && !net_waiting;

        let kc_ready = keychain == Probe::Ready;
        let steps = [signed_in, kc_ready, net_done, !devices.is_empty()];
        let ready = steps.iter().filter(|s| **s).count();
        let all = ready == 4;
        let can_continue = signed_in && kc_ready;
        let next = if !signed_in {
            0
        } else if !kc_ready {
            1
        } else if !net_started {
            2
        } else {
            9
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin { left: 72, right: 72, top: 64, bottom: 56 }))
            .show(ui, |ui| {
                super::window_drag_strip(ui);
                let full = ui.max_rect();
                let left_w = 290.0;
                let gap = 64.0;
                let left = Rect::from_min_size(full.min, vec2(left_w, full.height()));
                let right = Rect::from_min_max(Pos2::new(full.left() + left_w + gap, full.top()), full.max);

                // ── Left: what this is, how far along, and the way out.
                ui.scope_builder(egui::UiBuilder::new().max_rect(left).layout(Layout::top_down(Align::Min)), |ui| {
                    let a = theme::reveal(&ctx);
                    theme::faded(ui, a, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let (r, _) = ui.allocate_exact_size(vec2(22.0, 32.0), Sense::hover());
                        star(ui.painter(), r.center() + vec2(0.0, 3.0), 24.0, p.ink, p.accent);
                        ui.add_space(14.0);
                        ui.label(display(p, if all { "All set" } else { "Set up Lodestar" }));
                        ui.add_space(12.0);
                        ui.label(
                            quiet(p, "Four things, about two minutes. You can come back to the optional ones later.")
                                .size(14.0),
                        );
                        ui.add_space(26.0);
                        let count = if all { "All four ready".to_string() } else { format!("{ready} of 4 ready") };
                        ui.label(data(p, count).color(p.ink));
                        ui.add_space(10.0);
                        let (bar, _) = ui.allocate_exact_size(vec2(left_w, 3.0), Sense::hover());
                        let seg_w = (left_w - 3.0 * 4.0) / 4.0;
                        for i in 0..4 {
                            let r = Rect::from_min_size(bar.min + vec2(i as f32 * (seg_w + 4.0), 0.0), vec2(seg_w, 3.0));
                            ui.painter().rect_filled(r, CornerRadius::same(2), p.line);
                            let f = spring_to(&ctx, Id::new(("setup-seg", i)), if i < ready { 1.0 } else { 0.0 }, SMOOTH);
                            if f > 0.001 {
                                let mut fr = r;
                                fr.set_width(r.width() * f);
                                ui.painter().rect_filled(fr, CornerRadius::same(2), p.ink);
                            }
                        }
                    });
                    let used = ui.min_rect().height();
                    ui.add_space((left.height() - used - 64.0).max(16.0));
                    let a = theme::reveal(&ctx);
                    theme::faded(ui, a, |ui| {
                        let label = if all { "Open Lodestar" } else { "Continue" };
                        if primary_sized(ui, p, label, can_continue, Some(150.0)).clicked() {
                            self.finish_setup();
                        }
                        ui.add_space(10.0);
                        let hint = if !signed_in {
                            "Connect your Apple ID to continue"
                        } else if keychain == Probe::Waiting {
                            "Waiting for Keychain"
                        } else if !kc_ready {
                            "Allow Keychain to continue"
                        } else if all {
                            "Everything is ready"
                        } else {
                            "You can finish the rest later"
                        };
                        ui.label(data(p, hint).color(p.ink2));
                    });
                });

                // ── Right: the four rows.
                ui.scope_builder(egui::UiBuilder::new().max_rect(right).layout(Layout::top_down(Align::Min)), |ui| {
                    ScrollAreaNoBar::show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.set_width(right.width());
                        rule(ui, p);
                        self.apple_row(ui, p, &account, prompt.as_ref(), next == 0);
                        self.keychain_row(ui, p, &keychain, signed_in, next == 1);
                        self.network_row(ui, p, net_started, net_waiting, tv.map(|d| d.name.clone()), asked_for, next == 2);
                        let device = devices.first().map(|d| format!("{} — {}", d.name, d.model));
                        let a = theme::reveal(&ctx);
                        theme::faded(ui, a, |ui| {
                            row(
                                ui,
                                p,
                                Row {
                                    id: "device",
                                    status: if device.is_some() { Status::Done } else { Status::Searching },
                                    title: "A device",
                                    tag: "Optional now",
                                    desc: "Plug in an iPhone or iPad and tap Trust, or open Remote App and Devices on your Apple TV.",
                                    detail: Some(match device {
                                        Some(d) => (d, p.good),
                                        None => (format!("Looking for devices{}", super::ellipsis(ui)), p.ink2),
                                    }),
                                    action: None,
                                },
                            );
                        });
                    });
                });
            });
    }

    fn finish_setup(&mut self) {
        {
            let mut s = self.engine.shared.lock().unwrap();
            s.settings.onboarded = true;
            store::save_settings(&s.settings);
        }
        // From now on Lodestar browses at launch, as it always did.
        self.engine.start_net_scan();
        self.page_epoch = None;
        self.phase = Phase::App;
    }

    fn apple_row(&mut self, ui: &mut Ui, p: &Pal, account: &AccountState, prompt: Option<&Prompt>, next: bool) {
        let a = theme::reveal(ui.ctx());
        theme::faded(ui, a, |ui| {
            let two_factor = matches!(prompt, Some(Prompt::TwoFactor { .. }));
            let (status, detail, action) = match account {
                AccountState::SignedIn { email, team, .. } => (
                    Status::Done,
                    Some((format!("{} · {team}", mask_email(email)), p.good)),
                    Some(("Change", Action::Link)),
                ),
                _ if two_factor => (Status::Idle { next: true }, None, None),
                AccountState::SigningIn => {
                    let what = if self.setup.code_sent { "Checking the code" } else { "Signing in with Apple" };
                    (Status::Working, Some((format!("{what}{}", super::ellipsis(ui)), p.ink2)), None)
                }
                AccountState::SignedOut if self.setup.form_open => (Status::Idle { next: true }, None, None),
                AccountState::SignedOut => (
                    Status::Idle { next },
                    None,
                    Some(("Connect", if next { Action::Primary } else { Action::Secondary })),
                ),
            };
            let clicked = row(
                ui,
                p,
                Row {
                    id: "apple",
                    status,
                    title: "Apple ID",
                    tag: "Required",
                    desc: "Lodestar signs apps with your own Apple ID. A free one works. Your password goes only to Apple.",
                    detail,
                    action,
                },
            );
            if clicked {
                if matches!(account, AccountState::SignedIn { .. }) {
                    self.engine.send(Cmd::SignOut);
                }
                self.setup.form_open = true;
            }
            if two_factor {
                if let Some(Prompt::TwoFactor { sms, error }) = prompt {
                    self.code_panel(ui, p, *sms, error.as_deref());
                }
            } else if matches!(account, AccountState::SignedOut) && self.setup.form_open {
                self.sign_in_panel(ui, p);
            }
        });
    }

    fn sign_in_panel(&mut self, ui: &mut Ui, p: &Pal) {
        let mut enter = false;
        panel(ui, p, "apple-form", |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let w = ((ui.available_width() - 12.0) / 2.0).min(240.0);
                field(ui, p, "Apple ID email", &mut self.email, "name@icloud.com", w, false);
                let pw = field(ui, p, "Password", &mut self.password, "", w, true);
                enter = pw.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            });
            ui.add_space(12.0);
            checkbox(
                ui,
                p,
                &mut self.remember,
                "Remember me",
                "Keeps the password in your keychain so refreshes take one click.",
            );
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let ok = self.email.contains('@') && !self.password.is_empty();
                if primary(ui, p, "Sign in", ok).clicked() || (ok && enter) {
                    self.setup.code_sent = false;
                    self.engine.send(Cmd::SignIn {
                        email: self.email.trim().to_string(),
                        password: std::mem::take(&mut self.password),
                        remember: self.remember,
                    });
                }
                if secondary(ui, p, "Cancel", true).clicked() {
                    self.setup.form_open = false;
                }
            });
        });
    }

    fn code_panel(&mut self, ui: &mut Ui, p: &Pal, sms: bool, error: Option<&str>) {
        let mut answer = None;
        panel(ui, p, "apple-code", |ui| {
            ui.label(title(
                p,
                if sms { "Enter the code from your text message" } else { "Enter the code from your iPhone" },
            ));
            ui.add_space(2.0);
            ui.label(quiet(
                p,
                if sms {
                    "Apple texted a six-digit code to your trusted number."
                } else {
                    "Apple sent a six-digit code to your other devices."
                },
            ));
            ui.add_space(14.0);
            let filled = code_field(ui, p, &mut self.code, 6, vec2(44.0, 52.0));
            if filled == 6 {
                answer = Some(Some(std::mem::take(&mut self.code)));
            }
            if let Some(e) = error {
                ui.add_space(8.0);
                ui.label(quiet(p, e).color(p.bad));
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                if !sms && link(ui, p, "Text me instead", Some(p.ink2)).clicked() {
                    answer = Some(Some("__sms__".into()));
                }
                if link(ui, p, "Use a different Apple ID", Some(p.ink2)).clicked() {
                    answer = Some(None);
                }
            });
        });
        match answer {
            Some(Some(code)) => {
                self.code.clear();
                self.setup.code_sent = code != "__sms__";
                self.engine.answer(Some(code));
            }
            Some(None) => {
                self.code.clear();
                self.engine.answer(None);
            }
            None => {}
        }
    }

    fn keychain_row(&mut self, ui: &mut Ui, p: &Pal, probe: &Probe, signed_in: bool, next: bool) {
        let a = theme::reveal(ui.ctx());
        theme::faded(ui, a, |ui| {
            let (status, detail, action) = match probe {
                Probe::Ready => (Status::Done, Some(("Keychain ready".to_string(), p.good)), None),
                Probe::Waiting => (
                    Status::Waiting,
                    Some((
                        "If a macOS dialog opens, enter your Mac password and choose Always Allow.".to_string(),
                        p.warn,
                    )),
                    None,
                ),
                Probe::Denied => (
                    Status::Idle { next },
                    Some((
                        "Keychain access was refused. Lodestar needs it to keep your signing key.".to_string(),
                        p.bad,
                    )),
                    Some(("Try again", if next { Action::Primary } else { Action::Secondary })),
                ),
                Probe::Idle if !signed_in => (
                    Status::Idle { next: false },
                    Some(("Available after Apple ID".to_string(), p.ink2)),
                    Some(("Allow", Action::Disabled)),
                ),
                Probe::Idle => (
                    Status::Idle { next },
                    None,
                    Some(("Allow", if next { Action::Primary } else { Action::Secondary })),
                ),
            };
            let clicked = row(
                ui,
                p,
                Row {
                    id: "keychain",
                    status,
                    title: "Keychain",
                    tag: "Required",
                    desc: "Stores your signing key on this Mac so apps can be refreshed. macOS may ask once.",
                    detail,
                    action,
                },
            );
            if clicked {
                let state = self.setup.keychain.clone();
                let ctx = ui.ctx().clone();
                *state.lock().unwrap() = Probe::Waiting;
                std::thread::Builder::new()
                    .name("keychain-probe".into())
                    .spawn(move || {
                        let result = store::keychain_probe();
                        if let Err(e) = &result {
                            tracing::warn!("Keychain probe failed: {e}");
                        }
                        *state.lock().unwrap() = if result.is_ok() { Probe::Ready } else { Probe::Denied };
                        ctx.request_repaint();
                    })
                    .expect("spawn keychain probe");
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn network_row(
        &mut self,
        ui: &mut Ui,
        p: &Pal,
        started: bool,
        waiting: bool,
        tv: Option<String>,
        asked_for: f32,
        next: bool,
    ) {
        let a = theme::reveal(ui.ctx());
        theme::faded(ui, a, |ui| {
            let (status, detail, action) = if !started {
                (Status::Idle { next }, None, Some(("Allow", if next { Action::Primary } else { Action::Secondary })))
            } else if waiting {
                (
                    Status::Waiting,
                    Some(("If macOS asks to find devices on your network, choose Allow.".to_string(), p.warn)),
                    None,
                )
            } else {
                let text = match &tv {
                    Some(name) => format!("Allowed · found {name}"),
                    None => "Listening on your network".to_string(),
                };
                (Status::Done, Some((text, p.good)), None)
            };
            if row(
                ui,
                p,
                Row {
                    id: "network",
                    status,
                    title: "Local network",
                    tag: "For Apple TV",
                    desc: "Lets Lodestar find an Apple TV on your Wi-Fi. Not needed for iPhone over USB.",
                    detail,
                    action,
                },
            ) {
                self.setup.network_asked = Some(ui.input(|i| i.time));
                self.engine.start_net_scan();
            }
            // macOS never tells an app it was refused, so after a while with no
            // Apple TV in sight, say where the switch is.
            if started && tv.is_none() && asked_for > 8.0 && asked_for < f32::MAX {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(STATUS_W + 12.0);
                    ui.label(
                        quiet(
                            p,
                            "No Apple TV yet. If you chose Don't Allow, turn Lodestar on in Local Network settings.",
                        )
                        .size(12.5),
                    );
                    if link(ui, p, "Open Settings", Some(p.ink2)).clicked() {
                        let _ = std::process::Command::new("open").arg(LOCAL_NETWORK_SETTINGS).spawn();
                    }
                });
                ui.add_space(14.0);
                rule(ui, p);
            }
        });
    }
}

/// `a•••@icloud.com`: enough to recognise, not enough to read over a shoulder.
fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((user, domain)) => {
            let first: String = user.chars().take(1).collect();
            format!("{first}•••@{domain}")
        }
        None => email.to_string(),
    }
}

/// One checklist row: status, title and tag, description, detail line, action.
/// Returns whether the action was clicked.
fn row(ui: &mut Ui, p: &Pal, r: Row<'_>) -> bool {
    let ctx = ui.ctx().clone();
    let mut clicked = false;
    ui.add_space(18.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let (sr, _) = ui.allocate_exact_size(vec2(STATUS_W - 12.0, 22.0), Sense::hover());
        let t = ui.input(|i| i.time) as f32 / slowmo();
        let since = since_change(&ctx, Id::new(("row-status", r.id)), r.status.key());
        let c = sr.left_center() + vec2(11.0, 0.0);
        match r.status {
            Status::Done => done_tick(ui.painter(), c, 11.0, p, since),
            Status::Working => {
                working_line(ui.painter(), Rect::from_center_size(c, vec2(22.0, 2.0)), p, t);
            }
            Status::Waiting => beacon(ui.painter(), c, 4.0, p, t),
            Status::Searching => search_star(ui.painter(), c + vec2(0.0, 2.0), 7.5, p, t),
            Status::Idle { next } => {
                ui.painter().circle_stroke(c, 9.0, Stroke::new(1.5, if next { p.ink } else { p.line2 }));
            }
        }
        let text_w = (ui.available_width() - ACTION_W - 12.0).max(160.0);
        ui.allocate_ui_with_layout(vec2(text_w, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(text_w);
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.label(title(p, r.title).size(16.0));
                ui.label(label(p, r.tag).color(p.ink2));
            });
            ui.label(quiet(p, r.desc));
            if let Some((text, color)) = &r.detail {
                // The detail line slides in when it changes.
                let k = since_change(&ctx, Id::new(("row-detail", r.id)), hash(text));
                let a = if reduced() { 1.0 } else { crate::motion::out_expo((k / 0.45).min(1.0)) };
                ui.add_space(2.0);
                theme::faded(ui, a, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(6.0 * (1.0 - a));
                        ui.label(data(p, text).size(12.0).color(*color));
                    });
                });
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            if let Some((label_text, kind)) = r.action {
                let resp = match kind {
                    Action::Primary => primary(ui, p, label_text, true),
                    Action::Secondary => secondary(ui, p, label_text, true),
                    Action::Disabled => secondary(ui, p, label_text, false),
                    Action::Link => link(ui, p, label_text, Some(p.ink2)),
                };
                clicked = resp.clicked() && kind != Action::Disabled;
            }
        });
    });
    ui.add_space(18.0);
    rule(ui, p);
    clicked
}

/// An inset, framed panel that drops in under its row each time it appears.
fn panel(ui: &mut Ui, p: &Pal, id: &str, add: impl FnOnce(&mut Ui)) {
    let since = since_shown(ui.ctx(), Id::new(("panel", id)));
    let k = if reduced() { 1.0 } else { crate::motion::SNAPPY.at(since) };
    let a = if reduced() { 1.0 } else { crate::motion::phase(since, 0.0, 0.3) };
    ui.add_space(-4.0 - 8.0 * (1.0 - k));
    ui.horizontal_top(|ui| {
        ui.add_space(STATUS_W + 12.0);
        theme::faded(ui, a, |ui| {
            egui::Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.line))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(18))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width() - 2.0);
                    add(ui);
                });
        });
    });
    ui.add_space(18.0);
    rule(ui, p);
}

/// Design seconds since this was last off screen (a gap of more than a few frames).
fn since_shown(ctx: &egui::Context, id: Id) -> f32 {
    let now = ctx.input(|i| i.time);
    let t0 = ctx.data_mut(|d| {
        let (t0, seen) = d.get_temp_mut_or_insert_with(id, || (now, now));
        if now - *seen > 0.25 {
            *t0 = now;
        }
        *seen = now;
        *t0
    });
    ((now - t0) as f32 / slowmo()).max(0.0)
}

fn hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// A vertical scroll area without a visible bar, for when the window is short.
struct ScrollAreaNoBar;

impl ScrollAreaNoBar {
    fn show(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, add);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_masked_but_recognisable() {
        assert_eq!(mask_email("yuna@icloud.com"), "y•••@icloud.com");
        assert_eq!(mask_email("not-an-email"), "not-an-email");
    }
}
