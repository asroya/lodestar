use std::path::PathBuf;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Margin, Rect, RichText, ScrollArea, Sense, Stroke,
    StrokeKind, Ui, ViewportCommand, vec2,
};

use crate::{
    device::{Device, DeviceKind},
    engine::{AccountState, Cmd, Engine, InstallRequest, Job, Prompt, Stage},
    ipa::{self, IpaInfo},
    store::{self, InstallOptions, Platform},
    theme::{self, *},
};

const RAIL_W: f32 = 212.0;
const CONTENT_MAX: f32 = 760.0;
/// Where section content starts: the label column plus its gap.
const LABEL_COL: f32 = theme::LABEL_W + 8.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Install,
    Devices,
    Library,
    Account,
    Activity,
}

impl Page {
    const ALL: [Page; 5] = [Page::Install, Page::Devices, Page::Library, Page::Account, Page::Activity];
    fn name(self) -> &'static str {
        match self {
            Page::Install => "Install",
            Page::Devices => "Devices",
            Page::Library => "My Apps",
            Page::Account => "Apple ID",
            Page::Activity => "Activity",
        }
    }
}

pub struct LodestarApp {
    engine: Engine,
    page: Page,
    ipa: Option<(PathBuf, Result<IpaInfo, String>)>,
    device: Option<String>,
    options: InstallOptions,
    email: String,
    password: String,
    remember: bool,
    code: String,
    log_errors_only: bool,
    /// When the current page appeared (egui clock), for the arrival animation.
    page_epoch: Option<f64>,
    shown_page: Page,
}

impl LodestarApp {
    pub fn new(cc: &eframe::CreationContext<'_>, engine: Engine) -> Self {
        theme::init_motion();
        theme::install(&cc.egui_ctx);
        let s = engine.shared.lock().unwrap();
        let email = s.settings.apple_id.clone();
        let remember = s.settings.remember_password || email.is_empty();
        drop(s);
        Self {
            engine,
            page: Page::Install,
            ipa: None,
            device: None,
            options: InstallOptions::default(),
            email,
            password: String::new(),
            remember,
            code: String::new(),
            log_errors_only: false,
            page_epoch: None,
            shown_page: Page::Install,
        }
    }

    fn set_ipa(&mut self, path: PathBuf) {
        let info = ipa::inspect(&path).map_err(|e| format!("{e:#}"));
        if let Ok(i) = &info {
            self.options = InstallOptions::default();
            self.options.remove_device_restrictions = i.restricted_devices;
            let devices = self.engine.shared.lock().unwrap().devices.clone();
            let current_ok = self
                .device
                .as_ref()
                .and_then(|k| devices.iter().find(|d| &d.key == k))
                .is_some_and(|d| d.platform == i.platform);
            if !current_ok {
                self.device =
                    devices.iter().find(|d| d.platform == i.platform && !d.needs_pairing()).map(|d| d.key.clone());
            }
        }
        self.ipa = Some((path, info));
    }

    fn pick_file(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("iOS or tvOS app", &["ipa", "tipa"])
            .set_title("Choose an app")
            .pick_file()
        {
            self.set_ipa(p);
        }
    }
}

impl eframe::App for LodestarApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let p = theme::pal(&ctx);

        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(path) = dropped.into_iter().find(|f| {
            f.extension().is_some_and(|e| matches!(e.to_string_lossy().to_lowercase().as_str(), "ipa" | "tipa"))
        }) {
            self.set_ipa(path);
            self.page = Page::Install;
        }

        // Page clock: restarts whenever the page changes (and at launch), and
        // drives the staggered fade-in of each page's blocks.
        let now = ctx.input(|i| i.time);
        if self.page != self.shown_page {
            self.shown_page = self.page;
            self.page_epoch = Some(now);
        }
        let epoch = *self.page_epoch.get_or_insert(now);
        let since = (now - epoch) as f32;
        theme::begin_page(&ctx, since);
        if since < 1.0 * theme::slowmo() && !theme::reduced() {
            ctx.request_repaint();
        }

        let rail = egui::Panel::left("rail")
            .exact_size(RAIL_W)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin { left: 22, right: 18, top: 0, bottom: 20 }))
            .show(ui, |ui| self.rail(ui, &p));
        let r = rail.response.rect;
        ui.painter().vline(r.right(), r.y_range(), Stroke::new(1.0, p.line));

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin { left: 52, right: 40, top: 0, bottom: 0 }))
            .show(ui, |ui| {
                window_drag_strip(ui);
                if self.page == Page::Install {
                    let w = ui.available_width().min(CONTENT_MAX);
                    egui::Panel::bottom("install-footer")
                        .resizable(false)
                        .show_separator_line(false)
                        .frame(egui::Frame::new().fill(p.bg))
                        .show(ui, |ui| {
                            ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
                                ui.set_width(w);
                                self.install_footer(ui, &p);
                            });
                        });
                }
                ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    let w = ui.available_width().min(CONTENT_MAX);
                    ui.set_max_width(w);
                    ui.add_space(6.0);
                    match self.page {
                        Page::Install => self.page_install(ui, &p),
                        Page::Devices => self.page_devices(ui, &p),
                        Page::Library => self.page_library(ui, &p),
                        Page::Account => self.page_account(ui, &p),
                        Page::Activity => self.page_activity(ui, &p),
                    }
                    ui.add_space(40.0);
                });
            });

        self.modal(&ctx, &p);
        self.toast(&ctx, &p);
        // Idle: a slow tick keeps expiry countdowns fresh. Running jobs and
        // animations request their own frames.
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
}

/// The window has a transparent title bar; this strip keeps it draggable.
fn window_drag_strip(ui: &mut Ui) {
    let (_, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click_and_drag());
    if resp.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if resp.double_clicked() {
        let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!max));
    }
}

fn page_header(ui: &mut Ui, p: &Pal, title: &str, context: &str) {
    let opacity = theme::reveal(ui.ctx());
    theme::faded(ui, opacity, |ui| {
        ui.label(display(p, title));
        ui.add_space(-2.0);
        ui.label(data(p, context).color(p.ink3));
        ui.add_space(14.0);
        rule(ui, p);
    });
}

// ───────────────────────── rail ─────────────────────────

impl LodestarApp {
    fn rail(&mut self, ui: &mut Ui, p: &Pal) {
        window_drag_strip(ui);
        ui.add_space(2.0);

        let (devices, apps, expiring, busy) = {
            let s = self.engine.shared.lock().unwrap();
            let warn = s.settings.refresh_warning_days as i64 * 86_400;
            (
                s.devices.len(),
                s.library.len(),
                s.library.iter().filter(|e| e.seconds_left() < warn).count(),
                s.job.as_ref().is_some_and(|j| j.result.is_none()),
            )
        };

        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(square(20.0), Sense::hover());
            // The star's signal dot breathes while an install is running.
            let dot = if busy && !theme::reduced() {
                let t = ui.input(|i| i.time) as f32;
                ui.ctx().request_repaint();
                p.accent.gamma_multiply(0.45 + 0.55 * (0.5 + 0.5 * (t * 4.0).sin()))
            } else {
                p.accent
            };
            theme::star(ui.painter(), r.center() + vec2(0.0, 3.0), 18.0, p.ink, dot);
            ui.add_space(2.0);
            ui.label(RichText::new("Lodestar").family(semibold()).size(16.0).color(p.ink).extra_letter_spacing(-0.2));
        });
        ui.add_space(30.0);

        let mut active_y = None;

        for page in Page::ALL {
            let active = self.page == page;
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
            let lit = theme::tween(ui.ctx(), resp.id.with("lit"), active || resp.hovered(), 0.14);
            let color = theme::lerp(p.ink2, p.ink, lit);
            let family = if active { medium() } else { sans() };
            if active {
                active_y = Some(rect.center().y);
            }
            text_at(
                ui,
                rect.left_center() + vec2(16.0, 0.0),
                Align2::LEFT_CENTER,
                page.name(),
                FontId::new(14.0, family),
                color,
            );
            let (count, c) = match page {
                Page::Devices if devices > 0 => (devices.to_string(), p.ink3),
                Page::Library if expiring > 0 => (format!("{expiring} due"), p.accent),
                Page::Library if apps > 0 => (apps.to_string(), p.ink3),
                Page::Install if busy => ("working".into(), p.accent),
                _ => (String::new(), p.ink3),
            };
            if !count.is_empty() {
                text_at(ui, rect.right_center(), Align2::RIGHT_CENTER, &count, FontId::new(11.0, mono()), c);
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.page = page;
            }
        }
        // The marker slides to the active page instead of jumping.
        if let Some(y) = active_y {
            let y = theme::tween_to(ui.ctx(), Id::new("rail-marker"), y, 0.2);
            let x = ui.min_rect().left() + 2.5;
            ui.painter().rect_filled(
                Rect::from_center_size(egui::pos2(x, y), square(5.0)),
                CornerRadius::ZERO,
                p.accent,
            );
        }

        // Account, pinned to the bottom.
        let account = self.engine.shared.lock().unwrap().account();
        ui.add_space((ui.available_height() - 84.0).max(12.0));
        rule(ui, p);
        ui.add_space(12.0);
        ui.label(label(p, "Apple ID"));
        ui.add_space(2.0);
        match account {
            AccountState::SignedIn { email, team_id, .. } => {
                ui.label(RichText::new(truncate(&email, 24)).size(13.0).color(p.ink));
                ui.label(data(p, &team_id).size(11.0).color(p.ink3));
            }
            AccountState::SigningIn => {
                ui.label(quiet(p, format!("Signing in{}", ellipsis(ui))));
            }
            AccountState::SignedOut => {
                if link(ui, p, "Sign in", None).clicked() {
                    self.page = Page::Account;
                }
            }
        }
    }
}

fn ellipsis(ui: &Ui) -> &'static str {
    match (ui.input(|i| i.time) * 2.0) as i64 % 4 {
        0 => "",
        1 => ".",
        2 => "..",
        _ => "...",
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n - 1).collect::<String>()) }
}

// ───────────────────────── Install ─────────────────────────

impl LodestarApp {
    fn page_install(&mut self, ui: &mut Ui, p: &Pal) {
        let (devices, account) = {
            let s = self.engine.shared.lock().unwrap();
            (s.devices.clone(), s.account())
        };
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        let context = match &account {
            AccountState::SignedIn { email, .. } => {
                format!("{} device{} · signed in as {email}", devices.len(), plural(devices.len()))
            }
            _ => format!("{} device{} · not signed in", devices.len(), plural(devices.len())),
        };
        page_header(ui, p, "Install", &context);

        // App
        section(ui, p, "App", |ui| match self.ipa.clone() {
            None => self.drop_target(ui, p),
            Some((path, Err(e))) => {
                ui.label(body(p, path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
                ui.label(quiet(p, e).color(p.bad));
                if link(ui, p, "Choose another file", None).clicked() {
                    self.pick_file();
                }
            }
            Some((_, Ok(info))) => {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(&info.name)
                                .family(semibold())
                                .size(20.0)
                                .color(p.ink)
                                .extra_letter_spacing(-0.3),
                        );
                        ui.add_space(6.0);
                        let platform = if info.min_os.is_empty() {
                            info.platform.label().to_string()
                        } else {
                            format!("{} {} or later", info.platform.label(), info.min_os)
                        };
                        let mut rows = vec![
                            ("Bundle", info.bundle_id.clone()),
                            ("Version", info.version.clone()),
                            ("Platform", platform),
                            ("Size", ipa::human_size(info.size_bytes)),
                        ];
                        if info.extensions > 0 {
                            rows.push(("Extensions", info.extensions.to_string()));
                        }
                        kv_grid(ui, p, "app-kv", &rows);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        if link(ui, p, "Remove", Some(p.ink3)).clicked() {
                            self.ipa = None;
                        }
                        ui.add_space(10.0);
                        if secondary(ui, p, "Replace…", true).clicked() {
                            self.pick_file();
                        }
                    });
                });
            }
        });

        // Device
        let want = self.ipa.as_ref().and_then(|(_, i)| i.as_ref().ok()).map(|i| i.platform);
        let usable: Vec<&Device> =
            devices.iter().filter(|d| !d.needs_pairing() && want.is_none_or(|w| w == d.platform)).collect();
        if usable.len() == 1 && !devices.iter().any(|d| Some(&d.key) == self.device.as_ref()) {
            self.device = Some(usable[0].key.clone());
        }
        section(ui, p, "Device", |ui| {
            if devices.is_empty() {
                ui.label(body(p, format!("Looking for devices{}", ellipsis(ui))));
                ui.label(quiet(p, "Connect an iPhone or iPad, or open Settings › Remotes and Devices › Remote App and Devices on your Apple TV."));
                return;
            }
            for (i, d) in devices.iter().enumerate() {
                if i > 0 {
                    inset_rule(ui, p, 30.0);
                }
                let selected = self.device.as_deref() == Some(&d.key);
                if device_row(ui, p, d, selected, want).clicked() {
                    if d.needs_pairing() {
                        self.page = Page::Devices;
                    } else {
                        self.device = Some(d.key.clone());
                    }
                }
            }
        });

        // Signing
        section_ruled(ui, p, "Signing", false, |ui| {
            let w = (ui.available_width() - 16.0) / 2.0;
            ui.horizontal_top(|ui| {
                field(ui, p, "Display name", &mut self.options.custom_name, "Unchanged", w, false);
                ui.add_space(8.0);
                field(ui, p, "Bundle ID", &mut self.options.custom_bundle_id, "Automatic", w, false);
            });
            ui.add_space(12.0);
            checkbox(
                ui,
                p,
                &mut self.options.remove_device_restrictions,
                "Remove device restrictions",
                "Lets apps limited to certain models install anywhere.",
            );
            ui.add_space(4.0);
            checkbox(
                ui,
                p,
                &mut self.options.file_sharing,
                "Enable file sharing",
                "Shows the app's documents in Finder and the Files app.",
            );
            ui.add_space(4.0);
            checkbox(
                ui,
                p,
                &mut self.options.remove_extensions,
                "Remove app extensions",
                "Each extension uses one of the 10 App IDs a free account gets per week.",
            );
        });
    }

    /// Pinned to the bottom of the Install page so the action is always in reach.
    fn install_footer(&mut self, ui: &mut Ui, p: &Pal) {
        let (devices, account, job) = {
            let s = self.engine.shared.lock().unwrap();
            (s.devices.clone(), s.account(), s.job.clone())
        };
        let signed_in = matches!(account, AccountState::SignedIn { .. });
        rule(ui, p);
        ui.add_space(16.0);
        let device = self.device.as_ref().and_then(|k| devices.iter().find(|d| &d.key == k)).cloned();
        match &job {
            Some(job) => self.job_panel(ui, p, job),
            None => {
                let app = self.ipa.as_ref().and_then(|(_, i)| i.as_ref().ok()).map(|i| i.name.clone());
                let blocker = if !signed_in {
                    Some("Sign in with your Apple ID to sign apps.")
                } else if app.is_none() {
                    Some("Choose an app to continue.")
                } else if device.is_none() {
                    Some("Choose a device to continue.")
                } else {
                    None
                };
                ui.horizontal(|ui| {
                    ui.add_space(LABEL_COL);
                    match (blocker, &app, &device) {
                        (Some(b), _, _) => {
                            if !signed_in {
                                if link(ui, p, b, Some(p.ink2)).clicked() {
                                    self.page = Page::Account;
                                }
                            } else {
                                ui.label(quiet(p, b));
                            }
                        }
                        (None, Some(a), Some(d)) => {
                            ui.label(RichText::new(a).family(medium()).size(13.5).color(p.ink));
                            ui.label(quiet(p, "to"));
                            ui.label(RichText::new(&d.name).family(medium()).size(13.5).color(p.ink));
                        }
                        _ => {}
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if primary(ui, p, "Install", blocker.is_none()).clicked()
                            && let (Some((path, _)), Some(d)) = (&self.ipa, &device)
                        {
                            self.engine.send(Cmd::Install(InstallRequest {
                                ipa: path.clone(),
                                device_key: d.key.clone(),
                                options: self.options.clone(),
                                library_id: None,
                            }));
                        }
                    });
                });
            }
        }
        ui.add_space(18.0);
    }

    fn drop_target(&mut self, ui: &mut Ui, p: &Pal) {
        let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 92.0), Sense::click());
        // `over`: pointer on the zone. `drag`: a file is being dragged over the window.
        let over = theme::tween(ui.ctx(), resp.id.with("over"), resp.hovered(), 0.12);
        let drag = theme::tween(ui.ctx(), resp.id.with("drag"), hovering, 0.16);
        let fill =
            theme::lerp(theme::lerp(p.surface, p.sunken, over * 0.5), theme::lerp(p.surface, p.accent, 0.08), drag);
        let edge = theme::lerp(theme::lerp(p.line2, p.ink3, over), p.accent, drag);
        ui.painter().rect(rect, CornerRadius::same(8), fill, Stroke::new(1.0 + 0.6 * drag, edge), StrokeKind::Inside);
        let c = rect.center();
        let headline = if hovering { "Release to add" } else { "Drop an .ipa or .tipa file" };
        text_at(ui, c - vec2(0.0, 10.0), Align2::CENTER_CENTER, headline, FontId::new(14.5, medium()), p.ink);
        let sub = text_at(
            ui,
            c + vec2(0.0, 14.0),
            Align2::CENTER_CENTER,
            "or choose one from your Mac",
            FontId::proportional(13.0),
            p.ink2,
        );
        ui.painter().hline(
            sub.x_range(),
            sub.bottom() + 1.0,
            Stroke::new(1.0, if resp.hovered() { p.ink2 } else { p.line2 }),
        );
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.pick_file();
        }
    }

    fn job_panel(&mut self, ui: &mut Ui, p: &Pal, job: &Job) {
        let failed = matches!(job.result, Some(Err(_)));
        let done = matches!(job.result, Some(Ok(_)));
        let ctx = ui.ctx().clone();
        let key = Id::new(("job", job.id));
        let now = ctx.input(|i| i.time);
        // Arrival: the panel fades in; the bar eases to each new value; on
        // completion the accent settles into green (or red).
        let t0 = ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(key.with("t0"), || now));
        let appear = if theme::reduced() { 1.0 } else { theme::ease((now - t0) as f32 / theme::dur(0.25)) };
        if appear < 1.0 {
            ctx.request_repaint();
        }
        let shown = theme::tween_to(&ctx, key.with("frac"), job.fraction, 0.45);
        let settle = theme::tween(&ctx, key.with("settle"), failed || done, 0.3);
        let tone = theme::lerp(p.accent, if failed { p.bad } else { p.good }, settle);
        let running = !failed && !done;

        theme::faded(ui, appear, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(LABEL_COL);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&job.title).family(medium()).size(14.0).color(p.ink));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(data(p, format!("{:.0}%", shown * 100.0)).color(tone));
                        });
                    });
                    if running {
                        theme::bar_live(ui, p, shown, tone, ui.available_width(), 2.0);
                    } else {
                        theme::bar(ui, p, shown, tone, ui.available_width(), 2.0);
                    }
                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        for (i, st) in Stage::ALL.iter().enumerate() {
                            if i > 0 {
                                let (r, _) = ui.allocate_exact_size(vec2(18.0, 12.0), Sense::hover());
                                ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, p.line2));
                            }
                            let passed = theme::tween(&ctx, key.with(("passed", i)), job.stage > *st, 0.25);
                            let c = if job.stage == *st {
                                if failed {
                                    p.bad
                                } else if running && !theme::reduced() {
                                    // The current stage breathes while it works.
                                    p.accent.gamma_multiply(0.6 + 0.4 * (0.5 + 0.5 * (now as f32 * 4.0).sin()))
                                } else {
                                    p.accent
                                }
                            } else {
                                theme::lerp(p.ink3, p.ink2, passed)
                            };
                            ui.label(label(p, st.label()).color(c));
                        }
                    });
                    ui.add_space(6.0);

                    match &job.result {
                        None => {
                            ui.label(quiet(p, &job.detail));
                        }
                        Some(Ok(msg)) => {
                            ui.label(body(p, msg));
                            ui.label(quiet(
                                p,
                                "Free-account signatures last 7 days. Refresh from My Apps before then.",
                            ));
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if link(ui, p, "Done", None).clicked() {
                                    self.engine.shared.lock().unwrap().job = None;
                                }
                                ui.add_space(14.0);
                                if link(ui, p, "View in My Apps", Some(p.ink2)).clicked() {
                                    self.engine.shared.lock().unwrap().job = None;
                                    self.page = Page::Library;
                                }
                            });
                        }
                        Some(Err(msg)) => {
                            egui::Frame::new()
                                .fill(p.bad.gamma_multiply(0.06))
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(Margin::symmetric(12, 10))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(RichText::new(msg).family(mono()).size(12.0).color(p.bad));
                                });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if link(ui, p, "Dismiss", None).clicked() {
                                    self.engine.shared.lock().unwrap().job = None;
                                }
                                ui.add_space(14.0);
                                if link(ui, p, "Copy error", Some(p.ink2)).clicked() {
                                    ui.ctx().copy_text(msg.clone());
                                }
                                ui.add_space(14.0);
                                if link(ui, p, "Open activity log", Some(p.ink2)).clicked() {
                                    self.page = Page::Activity;
                                }
                            });
                        }
                    }
                });
            })
        });
    }
}

fn inset_rule(ui: &mut Ui, p: &Pal, inset: f32) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
    ui.painter().hline((r.left() + inset)..=r.right(), r.center().y, Stroke::new(1.0, p.line));
}

fn kv_grid(ui: &mut Ui, p: &Pal, id: &str, rows: &[(&str, String)]) {
    egui::Grid::new(id).num_columns(2).spacing([20.0, 5.0]).show(ui, |ui| {
        for (k, v) in rows {
            ui.label(label(p, k));
            ui.label(data(p, v));
            ui.end_row();
        }
    });
}

fn connection_tag(d: &Device) -> &'static str {
    match &d.kind {
        DeviceKind::Usbmux { wifi: false, .. } => "USB",
        DeviceKind::Usbmux { wifi: true, .. } => "Wi-Fi",
        DeviceKind::Network { paired: true, .. } => "Network",
        DeviceKind::Network { pairing_port: Some(_), .. } => "Ready to pair",
        DeviceKind::Network { .. } => "Not paired",
    }
}

fn device_row(ui: &mut Ui, p: &Pal, d: &Device, selected: bool, want: Option<Platform>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
    let sel = theme::tween(ui.ctx(), resp.id.with("sel"), selected, 0.18);
    let hot = theme::tween(ui.ctx(), resp.id.with("hot"), resp.hovered(), 0.12);
    ui.painter().rect_filled(
        rect.expand2(vec2(8.0, 0.0)),
        CornerRadius::same(6),
        p.sunken.gamma_multiply(0.6 * hot * (1.0 - sel)),
    );
    // Radio: the ring fills with ink and the centre dot grows in.
    let c = rect.left_center() + vec2(8.0, 0.0);
    ui.painter().circle(
        c,
        7.5,
        p.surface,
        Stroke::new(1.3, theme::lerp(p.ink3, p.ink2, hot).gamma_multiply(1.0 - sel)),
    );
    ui.painter().circle_filled(c, 8.0, p.ink.gamma_multiply(sel));
    ui.painter().circle_filled(c, 3.0 * sel, p.bg);
    let x = rect.left() + 30.0;
    text_at(ui, egui::pos2(x, rect.top() + 17.0), Align2::LEFT_CENTER, &d.name, FontId::new(14.0, medium()), p.ink);
    let mismatch = want.is_some_and(|w| w != d.platform);
    let (sub, sub_c) = if mismatch {
        (format!("{} · can't run {} apps", d.model, if d.platform == Platform::Tvos { "iOS" } else { "tvOS" }), p.warn)
    } else {
        (d.model.clone(), p.ink3)
    };
    text_at(ui, egui::pos2(x, rect.top() + 36.0), Align2::LEFT_CENTER, &sub, FontId::new(12.0, mono()), sub_c);

    let tag = connection_tag(d).to_uppercase();
    let tag_c = if d.needs_pairing() { p.warn } else { p.ink3 };
    let r = text_at(ui, rect.right_center(), Align2::RIGHT_CENTER, &tag, FontId::new(10.5, mono_medium()), tag_c);
    ui.painter().circle_filled(
        egui::pos2(r.left() - 9.0, r.center().y),
        3.0,
        if d.needs_pairing() { p.warn } else { p.good },
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ───────────────────────── Devices ─────────────────────────

impl LodestarApp {
    fn page_devices(&mut self, ui: &mut Ui, p: &Pal) {
        let devices = self.engine.shared.lock().unwrap().devices.clone();
        page_header(ui, p, "Devices", "Watching USB, Wi-Fi sync and the local network");

        section(ui, p, "Found", |ui| {
            if devices.is_empty() {
                ui.label(body(p, format!("Nothing yet{}", ellipsis(ui))));
                ui.label(quiet(p, "Devices appear here as soon as they're connected or advertised on the network."));
            }
            for (i, d) in devices.iter().enumerate() {
                if i > 0 {
                    ui.add_space(4.0);
                    inset_rule(ui, p, 0.0);
                    ui.add_space(4.0);
                }
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(title(p, &d.name));
                        ui.label(data(p, &d.model).size(12.0).color(p.ink3));
                        ui.horizontal(|ui| {
                            dot(ui, if d.needs_pairing() { p.warn } else { p.good });
                            ui.label(label(p, connection_tag(d)).color(if d.needs_pairing() {
                                p.warn
                            } else {
                                p.ink3
                            }));
                        });
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &d.kind {
                        DeviceKind::Network { paired: true, .. } => {
                            if link(ui, p, "Forget", Some(p.bad)).clicked() {
                                self.engine.send(Cmd::Unpair { device_key: d.key.clone() });
                            }
                            ui.add_space(14.0);
                            if link(ui, p, "Pair again", Some(p.ink2)).clicked() {
                                self.engine.send(Cmd::Pair { device_key: d.key.clone() });
                            }
                        }
                        DeviceKind::Network { pairing_port: Some(_), .. } => {
                            if primary(ui, p, "Pair", true).clicked() {
                                self.engine.send(Cmd::Pair { device_key: d.key.clone() });
                            }
                        }
                        DeviceKind::Network { .. } => {
                            ui.label(quiet(p, "Open the pairing screen on the TV").color(p.warn));
                        }
                        DeviceKind::Usbmux { .. } => {
                            if link(ui, p, "Install to this device", Some(p.ink2)).clicked() {
                                self.device = Some(d.key.clone());
                                self.page = Page::Install;
                            }
                        }
                    });
                });
            }
        });

        section(ui, p, "Apple TV", |ui| {
            ui.label(body(p, "Pairing a new Apple TV takes one PIN and is remembered afterwards."));
            ui.add_space(6.0);
            for (n, t) in [
                ("01", "On the TV, open Settings › Remotes and Devices › Remote App and Devices."),
                ("02", "When it appears above as Ready to pair, choose Pair."),
                ("03", "Type the code shown on the TV."),
                ("04", "Leave that screen before installing. The TV only accepts installs outside it."),
            ] {
                ui.horizontal_top(|ui| {
                    ui.label(data(p, n).color(p.ink3));
                    ui.label(quiet(p, t));
                });
            }
            ui.add_space(10.0);
            ui.label(label(p, "Not showing up?"));
            ui.add_space(2.0);
            ui.label(quiet(
                p,
                "Keep the Mac and the TV on the same Wi-Fi (not a guest network), wake the TV, and allow Local Network \
                 access for Lodestar in System Settings › Privacy & Security.",
            ));
        });
    }
}

// ───────────────────────── My Apps ─────────────────────────

impl LodestarApp {
    fn page_library(&mut self, ui: &mut Ui, p: &Pal) {
        let (library, devices, warn_days, signed_in, busy) = {
            let s = self.engine.shared.lock().unwrap();
            (
                s.library.clone(),
                s.devices.clone(),
                s.settings.refresh_warning_days as i64,
                matches!(s.account(), AccountState::SignedIn { .. }),
                s.job.as_ref().is_some_and(|j| j.result.is_none()),
            )
        };
        let n = library.len();
        let plural = if n == 1 { "" } else { "s" };
        let context = match library.iter().map(|e| e.seconds_left()).min() {
            None => "Nothing installed yet".to_string(),
            Some(s) if s <= 0 => format!("{n} app{plural} · at least one has expired"),
            Some(s) => format!("{n} app{plural} · next expiry in {}", store::human_duration(s)),
        };

        let head = theme::reveal(ui.ctx());
        theme::faded(ui, head, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(display(p, "My Apps"));
                    ui.add_space(-2.0);
                    ui.label(data(p, &context).color(p.ink3));
                });
                if !library.is_empty() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if secondary(ui, p, "Refresh all", signed_in && !busy).clicked() {
                            self.engine.send(Cmd::RefreshAll);
                            self.page = Page::Install;
                        }
                    });
                }
            });
            ui.add_space(14.0);
            rule(ui, p);
        });

        if library.is_empty() {
            ui.add_space(24.0);
            ui.label(body(p, "Apps you install appear here with a countdown to when their signature runs out."));
            return;
        }

        let w = ui.available_width();
        let gap = ui.spacing().item_spacing.x;
        let c = [w - 436.0 - 3.0 * gap, 156.0, 130.0, 150.0];
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            for (name, width) in ["App", "Device", "Expires", ""].iter().zip(c) {
                ui.allocate_ui_with_layout(vec2(width, 16.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.set_min_width(width);
                    ui.label(label(p, name));
                });
            }
        });
        ui.add_space(6.0);
        rule(ui, p);

        let mut remove = None;
        for e in &library {
            let left = e.seconds_left();
            let online = devices.iter().any(|d| d.key == e.device_key);
            let tone = if left <= 0 {
                p.bad
            } else if left < warn_days * 86_400 {
                p.warn
            } else {
                p.ink2
            };
            let row_opacity = theme::reveal_step(ui.ctx(), 0.045);
            theme::faded(ui, row_opacity, |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(vec2(c[0], 40.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_min_width(c[0]);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new(&e.name).family(medium()).size(14.0).color(p.ink));
                        ui.label(data(p, format!("{} · {}", e.version, e.signed_bundle_id)).size(11.5).color(p.ink3));
                    });
                    ui.allocate_ui_with_layout(vec2(c[1], 40.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_min_width(c[1]);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(body(p, truncate(&e.device_name, 18)).size(13.5));
                        ui.label(label(p, if online { "Online" } else { "Offline" }).color(if online {
                            p.good
                        } else {
                            p.ink3
                        }));
                    });
                    ui.allocate_ui_with_layout(vec2(c[2], 40.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_min_width(c[2]);
                        ui.spacing_mut().item_spacing.y = 6.0;
                        let text = if left <= 0 { "Expired".to_string() } else { store::human_duration(left) };
                        ui.label(data(p, text).color(tone));
                        let total = (e.expires_at - e.installed_at).max(1) as f32;
                        bar(ui, p, left.max(0) as f32 / total, tone, 96.0, 2.0);
                    });
                    ui.allocate_ui_with_layout(vec2(c[3], 40.0), Layout::right_to_left(Align::Min), |ui| {
                        ui.set_min_width(c[3]);
                        // Removing deletes the stored IPA a running refresh may be reading,
                        // and the finished job would write the entry back, so wait for it.
                        if busy {
                            ui.label(
                                RichText::new("Remove").family(medium()).size(13.0).color(p.ink3.gamma_multiply(0.6)),
                            )
                            .on_hover_text("Wait for the current job to finish.");
                        } else if link(ui, p, "Remove", Some(p.ink3))
                            .on_hover_text("Forget it here. The app stays on the device.")
                            .clicked()
                        {
                            remove = Some(e.id.clone());
                        }
                        ui.add_space(14.0);
                        let can = signed_in && online && !busy && e.ipa_path.exists();
                        if can {
                            if link(ui, p, "Refresh", None).clicked() {
                                self.engine.send(Cmd::Install(InstallRequest {
                                    ipa: e.ipa_path.clone(),
                                    device_key: e.device_key.clone(),
                                    options: e.options.clone(),
                                    library_id: Some(e.id.clone()),
                                }));
                                self.page = Page::Install;
                            }
                        } else {
                            ui.label(
                                RichText::new("Refresh").family(medium()).size(13.0).color(p.ink3.gamma_multiply(0.6)),
                            );
                        }
                    });
                });
                ui.add_space(10.0);
                rule(ui, p);
            });
        }
        if let Some(id) = remove.filter(|_| !busy) {
            let mut s = self.engine.shared.lock().unwrap();
            if let Some(e) = s.library.iter().find(|e| e.id == id) {
                let _ = std::fs::remove_file(&e.ipa_path);
            }
            s.library.retain(|e| e.id != id);
            store::save_library(&s.library);
        }
    }
}

// ───────────────────────── Apple ID ─────────────────────────

impl LodestarApp {
    fn page_account(&mut self, ui: &mut Ui, p: &Pal) {
        let account = self.engine.shared.lock().unwrap().account();
        page_header(ui, p, "Apple ID", "Credentials go only to Apple, and to your keychain if you ask");

        match account {
            AccountState::SignedIn { email, name, team, team_id } => {
                section(ui, p, "Account", |ui| {
                    ui.horizontal_top(|ui| {
                        kv_grid(
                            ui,
                            p,
                            "acct",
                            &[("Name", name), ("Email", email), ("Team", team), ("Team ID", team_id)],
                        );
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            if secondary(ui, p, "Sign out", true).clicked() {
                                self.engine.send(Cmd::SignOut);
                            }
                        });
                    });
                });
            }
            AccountState::SigningIn => {
                section(ui, p, "Account", |ui| {
                    ui.label(body(p, format!("Talking to Apple{}", ellipsis(ui))));
                });
            }
            AccountState::SignedOut => {
                section(ui, p, "Sign in", |ui| {
                    field(ui, p, "Email", &mut self.email, "name@icloud.com", 340.0, false);
                    ui.add_space(6.0);
                    let pw = field(ui, p, "Password", &mut self.password, "", 340.0, true);
                    ui.add_space(10.0);
                    checkbox(
                        ui,
                        p,
                        &mut self.remember,
                        "Remember me",
                        "Keeps the password in your keychain so refreshes take one click.",
                    );
                    ui.add_space(14.0);
                    let ready = self.email.contains('@') && !self.password.is_empty();
                    let enter = pw.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if primary(ui, p, "Sign in", ready).clicked() || (enter && ready) {
                        self.engine.send(Cmd::SignIn {
                            email: self.email.trim().to_string(),
                            password: std::mem::take(&mut self.password),
                            remember: self.remember,
                        });
                    }
                    ui.add_space(4.0);
                    ui.label(
                        quiet(p, "Any Apple ID works. A separate one just for sideloading is a sensible choice.")
                            .color(p.ink3),
                    );
                });
            }
        }

        section(ui, p, "Free limits", |ui| {
            kv_grid(
                ui,
                p,
                "limits",
                &[
                    ("Apps per device", "3 at a time".into()),
                    ("Signature", "7 days, then refresh".into()),
                    ("New App IDs", "10 per 7 days".into()),
                    ("Certificates", "Revoked and reissued as needed".into()),
                ],
            );
            ui.add_space(4.0);
            ui.label(quiet(p, "These are Apple's limits. Lodestar itself has no paid features.").color(p.ink3));
        });
    }
}

// ───────────────────────── Activity ─────────────────────────

impl LodestarApp {
    fn page_activity(&mut self, ui: &mut Ui, p: &Pal) {
        let log = self.engine.shared.lock().unwrap().log.clone();
        let errors = log.iter().filter(|l| l.level <= tracing::Level::WARN).count();
        page_header(ui, p, "Activity", &format!("{} lines · {} warnings or errors", log.len(), errors));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            checkbox(ui, p, &mut self.log_errors_only, "Warnings and errors only", "");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if link(ui, p, "Clear", Some(p.ink3)).clicked() {
                    self.engine.shared.lock().unwrap().log.clear();
                }
                ui.add_space(14.0);
                if link(ui, p, "Copy all", None).clicked() {
                    let text: String = log.iter().map(|l| format!("{} {}\n", l.level, l.text)).collect();
                    ui.ctx().copy_text(text);
                }
            });
        });
        ui.add_space(10.0);
        egui::Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(16, 14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(440.0);
                if log.is_empty() {
                    ui.label(data(p, "Nothing yet.").color(p.ink3));
                }
                ScrollArea::vertical().stick_to_bottom(true).max_height(560.0).id_salt("log").show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    for l in log.iter().filter(|l| !self.log_errors_only || l.level <= tracing::Level::WARN) {
                        let (tag, c) = match l.level {
                            tracing::Level::ERROR => ("ERR", p.bad),
                            tracing::Level::WARN => ("WRN", p.warn),
                            _ => ("   ", p.ink3),
                        };
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new(tag).family(mono_medium()).size(11.0).color(c));
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&l.text)
                                        .family(mono())
                                        .size(12.0)
                                        .color(if l.level <= tracing::Level::WARN { c } else { p.ink2 }),
                                )
                                .wrap(),
                            );
                        });
                    }
                });
            });
    }
}

// ───────────────────────── modal & toast ─────────────────────────

impl LodestarApp {
    fn modal(&mut self, ctx: &egui::Context, p: &Pal) {
        let prompt = self.engine.shared.lock().unwrap().prompt.clone();
        let Some(prompt) = prompt else {
            self.code.clear();
            return;
        };
        let (eyebrow, heading, body_text) = match &prompt {
            Prompt::TwoFactor { sms: false, .. } => (
                "Two-factor authentication",
                "Enter the code",
                "Apple sent a six-digit code to your other devices.".to_string(),
            ),
            Prompt::TwoFactor { sms: true, .. } => (
                "Two-factor authentication",
                "Enter the code",
                "Apple texted a six-digit code to your trusted number.".to_string(),
            ),
            Prompt::Pin { device } => ("Pair Apple TV", "Enter the PIN", format!("Type the code shown on {device}.")),
        };
        let digits = 6usize;
        let error = match &prompt {
            Prompt::TwoFactor { error: Some(e), .. } => Some(e.clone()),
            _ => None,
        };

        let mut submit = None;
        let mut cancel = false;
        egui::Modal::new(Id::new("prompt"))
            .backdrop_color(Color32::from_black_alpha(if ctx.theme() == egui::Theme::Dark { 150 } else { 70 }))
            .frame(
                egui::Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.line2))
                    .corner_radius(CornerRadius::same(10))
                    .shadow(ctx.style_of(ctx.theme()).visuals.window_shadow)
                    .inner_margin(Margin::same(28)),
            )
            .show(ctx, |ui| {
                let cell = vec2(46.0, 54.0);
                let gap = 8.0;
                let total = vec2(cell.x * digits as f32 + gap * (digits - 1) as f32, cell.y);
                ui.set_width(total.x);
                ui.label(label(p, eyebrow));
                ui.add_space(2.0);
                ui.label(RichText::new(heading).family(semibold()).size(20.0).color(p.ink).extra_letter_spacing(-0.3));
                ui.label(quiet(p, body_text));
                ui.add_space(16.0);

                // Segmented code field: a transparent TextEdit under drawn cells.
                let (rect, _) = ui.allocate_exact_size(total, Sense::hover());
                let edit = ui.put(
                    rect,
                    egui::TextEdit::singleline(&mut self.code)
                        .frame(egui::Frame::NONE)
                        .text_color(Color32::TRANSPARENT)
                        .char_limit(digits)
                        .desired_width(total.x),
                );
                edit.request_focus();
                self.code.retain(|c| c.is_ascii_alphanumeric());
                let chars: Vec<char> = self.code.chars().collect();
                let blink = (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0;
                for i in 0..digits {
                    let r = Rect::from_min_size(rect.min + vec2(i as f32 * (cell.x + gap), 0.0), cell);
                    let active = i == chars.len();
                    let focus = theme::tween(ctx, Id::new(("code-cell", i)), active, 0.12);
                    let filled = theme::tween(ctx, Id::new(("code-fill", i)), chars.get(i).is_some(), 0.14);
                    let stroke = Stroke::new(1.0 + 0.5 * focus, theme::lerp(p.line2, p.ink, focus));
                    ui.painter().rect(r, CornerRadius::same(6), p.bg, stroke, StrokeKind::Inside);
                    if let Some(ch) = chars.get(i) {
                        // A typed digit settles into place from slightly small.
                        let size = 24.0 * (0.8 + 0.2 * filled);
                        text_at(
                            ui,
                            r.center(),
                            Align2::CENTER_CENTER,
                            &ch.to_string(),
                            FontId::new(size, mono_medium()),
                            p.ink.gamma_multiply(filled),
                        );
                    } else if active && blink {
                        ui.painter().vline(
                            r.center().x,
                            (r.center().y - 11.0)..=(r.center().y + 11.0),
                            Stroke::new(1.5, p.accent),
                        );
                    }
                }
                if let Some(e) = &error {
                    ui.add_space(8.0);
                    ui.label(quiet(p, e).color(p.bad));
                }
                ui.add_space(18.0);
                let ready = match prompt {
                    Prompt::Pin { .. } => chars.len() >= 4,
                    Prompt::TwoFactor { .. } => chars.len() == digits,
                };
                if primary_sized(ui, p, "Continue", ready, Some(total.x)).clicked()
                    || (ready && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    submit = Some(self.code.clone());
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if let Prompt::TwoFactor { .. } = prompt {
                        if link(ui, p, "Text me instead", Some(p.ink2)).clicked() {
                            submit = Some("__sms__".into());
                        }
                        ui.add_space(14.0);
                    }
                    if link(ui, p, "Cancel", Some(p.ink3)).clicked() {
                        cancel = true;
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        if let Some(code) = submit {
            self.code.clear();
            self.engine.answer(Some(code));
        } else if cancel {
            self.code.clear();
            self.engine.answer(None);
        }
    }

    fn toast(&mut self, ctx: &egui::Context, p: &Pal) {
        let now = ctx.input(|i| i.time);
        let mut s = self.engine.shared.lock().unwrap();
        let Some((msg, ok, t0)) = s.toast.clone() else { return };
        let t0 = if t0 == 0.0 {
            s.toast = Some((msg.clone(), ok, now));
            now
        } else {
            t0
        };
        drop(s);
        const LIFE: f64 = 6.0;
        let fade = theme::dur(0.3) as f64;
        if now - t0 > LIFE {
            self.engine.shared.lock().unwrap().toast = None;
            return;
        }
        // Slides in from the right, holds, then fades out.
        let vis = if theme::reduced() {
            1.0
        } else {
            theme::ease((now - t0) as f32 / fade as f32).min(theme::ease((t0 + LIFE - now) as f32 / fade as f32))
        };
        if now - t0 < fade || t0 + LIFE - now < fade {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        egui::Area::new(Id::new("toast"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-28.0 + (1.0 - vis) * 36.0, -24.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_opacity(vis);
                egui::Frame::new()
                    .fill(p.ink)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(14, 10))
                    .shadow(ctx.style_of(ctx.theme()).visuals.popup_shadow)
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(6.0, 14.0), Sense::hover());
                            ui.painter().rect_filled(
                                Rect::from_center_size(r.center(), square(6.0)),
                                CornerRadius::ZERO,
                                if ok { p.good } else { p.bad },
                            );
                            ui.add_space(4.0);
                            ui.label(RichText::new(&msg).size(13.0).color(p.bg));
                        });
                    });
            });
    }
}
