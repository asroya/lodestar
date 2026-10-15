//! Background engine: owns the Apple developer session, device scanners and
//! the sign → install pipeline. The UI talks to it through [`Engine::send`]
//! and reads [`Shared`] every frame.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::{Result, anyhow};
use isideload::{
    anisette::remote_v3::RemoteV3AnisetteProvider,
    auth::{
        apple_account::{TwoFactorCallbackParams, TwoFactorCallbackResponse},
        builder::AppleAccountBuilder,
    },
    dev::developer_session::{DeveloperSession, DeveloperTeam, TeamsApi},
};
use tokio::sync::{Mutex as AsyncMutex, mpsc};
use tracing::{error, info, warn};

use crate::{
    device::{self, Device, DeviceKind, NetScanner, UsbScanner},
    pipeline,
    store::{self, InstallOptions, LibraryEntry, Settings},
};

// ───────────────────────── shared UI state ─────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AccountState {
    SignedOut,
    SigningIn,
    SignedIn { email: String, name: String, team: String, team_id: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Prompt {
    TwoFactor { sms: bool, error: Option<String> },
    Pin { device: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Connect,
    Register,
    Sign,
    Transfer,
    Done,
}

impl Stage {
    pub const ALL: [Stage; 4] = [Stage::Connect, Stage::Register, Stage::Sign, Stage::Transfer];
    pub fn label(self) -> &'static str {
        match self {
            Stage::Connect => "Connect",
            Stage::Register => "Provision",
            Stage::Sign => "Sign",
            Stage::Transfer => "Install",
            Stage::Done => "Done",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    /// Unique per job, so UI animations never carry over between installs.
    pub id: u64,
    pub title: String,
    pub stage: Stage,
    pub fraction: f32,
    pub detail: String,
    pub result: Option<Result<String, String>>,
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub level: tracing::Level,
    pub text: String,
}

#[derive(Default)]
pub struct Shared {
    pub account: Option<AccountState>,
    pub devices: Vec<Device>,
    pub job: Option<Job>,
    pub prompt: Option<Prompt>,
    pub library: Vec<LibraryEntry>,
    pub settings: Settings,
    pub log: Vec<LogLine>,
    pub toast: Option<(String, bool, f64)>,
}

impl Shared {
    pub fn account(&self) -> AccountState {
        self.account.clone().unwrap_or(AccountState::SignedOut)
    }
}

// ───────────────────────── commands ─────────────────────────

pub struct InstallRequest {
    pub ipa: PathBuf,
    pub device_key: String,
    pub options: InstallOptions,
    /// Set when re-signing an existing library entry.
    pub library_id: Option<String>,
}

pub enum Cmd {
    SignIn { email: String, password: String, remember: bool },
    SignOut,
    Pair { device_key: String },
    Unpair { device_key: String },
    Install(InstallRequest),
    RefreshAll,
}

pub struct Session {
    pub dev: DeveloperSession,
    pub email: String,
    pub team: DeveloperTeam,
}

#[derive(Clone)]
pub struct Engine {
    pub shared: Arc<Mutex<Shared>>,
    cmd_tx: mpsc::UnboundedSender<Cmd>,
    answer_tx: mpsc::UnboundedSender<Option<String>>,
    pub repaint: Arc<dyn Fn() + Send + Sync>,
    net: NetScanner,
}

/// Lets pipeline code ask the user something and await the answer.
#[derive(Clone)]
pub struct Prompter {
    shared: Arc<Mutex<Shared>>,
    rx: Arc<AsyncMutex<mpsc::UnboundedReceiver<Option<String>>>>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Prompter {
    pub async fn ask(&self, prompt: Prompt) -> Option<String> {
        let mut rx = self.rx.lock().await;
        while rx.try_recv().is_ok() {} // drop stale answers
        self.shared.lock().unwrap().prompt = Some(prompt);
        (self.repaint)();
        let answer = rx.recv().await.flatten();
        self.shared.lock().unwrap().prompt = None;
        (self.repaint)();
        answer
    }
}

/// Progress reporter handed to the pipeline.
#[derive(Clone)]
pub struct Reporter {
    shared: Arc<Mutex<Shared>>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Reporter {
    pub fn stage(&self, stage: Stage, fraction: f32, detail: impl Into<String>) {
        let detail = detail.into();
        info!("{detail}");
        if let Some(job) = self.shared.lock().unwrap().job.as_mut() {
            job.stage = stage;
            job.fraction = fraction.clamp(0.0, 1.0);
            job.detail = detail;
        }
        (self.repaint)();
    }
    pub fn fraction(&self, fraction: f32) {
        if let Some(job) = self.shared.lock().unwrap().job.as_mut() {
            job.fraction = job.fraction.max(fraction.clamp(0.0, 1.0));
        }
        (self.repaint)();
    }
}

impl Engine {
    pub fn start(repaint: impl Fn() + Send + Sync + 'static, log: Arc<Mutex<Vec<LogLine>>>) -> Self {
        let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(repaint);
        let settings = store::load_settings();
        let shared = Arc::new(Mutex::new(Shared {
            account: Some(AccountState::SignedOut),
            library: store::load_library(),
            settings: settings.clone(),
            ..Default::default()
        }));
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (answer_tx, answer_rx) = mpsc::unbounded_channel();
        let engine =
            Engine { shared: shared.clone(), cmd_tx, answer_tx, repaint: repaint.clone(), net: NetScanner::new() };
        // Before setup is finished, Local Network access is asked for from the
        // setup screen instead, at the moment the person presses Allow.
        if settings.onboarded {
            engine.start_net_scan();
        }

        let prompter =
            Prompter { shared: shared.clone(), rx: Arc::new(AsyncMutex::new(answer_rx)), repaint: repaint.clone() };
        let reporter = Reporter { shared: shared.clone(), repaint: repaint.clone() };

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("lodestar-rt")
            .build()
            .expect("tokio runtime");

        let e2 = engine.clone();
        std::thread::Builder::new()
            .name("engine".into())
            .spawn(move || {
                rt.block_on(async move {
                    // Mirror the tracing log into the UI.
                    {
                        let shared = e2.shared.clone();
                        let repaint = e2.repaint.clone();
                        tokio::spawn(async move {
                            loop {
                                tokio::time::sleep(Duration::from_millis(200)).await;
                                let lines: Vec<LogLine> = std::mem::take(&mut *log.lock().unwrap());
                                if !lines.is_empty() {
                                    let mut s = shared.lock().unwrap();
                                    s.log.extend(lines);
                                    let n = s.log.len();
                                    if n > 2000 {
                                        s.log.drain(..n - 2000);
                                    }
                                    drop(s);
                                    repaint();
                                }
                            }
                        });
                    }

                    // Device discovery.
                    let net = e2.net.clone();
                    {
                        let shared = e2.shared.clone();
                        let repaint = e2.repaint.clone();
                        let net = net.clone();
                        tokio::spawn(async move {
                            let mut usb = UsbScanner::new();
                            loop {
                                let mut devices = usb.scan().await;
                                let names: Vec<String> = devices.iter().map(|d| d.name.to_lowercase()).collect();
                                devices.extend(
                                    net.devices().into_iter().filter(|d| !names.contains(&d.name.to_lowercase())),
                                );
                                devices.sort_by(|a, b| a.name.cmp(&b.name));
                                let changed = {
                                    let mut s = shared.lock().unwrap();
                                    let changed = s.devices != devices;
                                    s.devices = devices;
                                    changed
                                };
                                if changed {
                                    repaint();
                                }
                                tokio::time::sleep(Duration::from_secs(2)).await;
                            }
                        });
                    }

                    run(e2, cmd_rx, prompter, reporter, settings).await;
                });
            })
            .expect("spawn engine");
        engine
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn answer(&self, value: Option<String>) {
        let _ = self.answer_tx.send(value);
    }

    /// Starts looking for Apple TVs on the network (and so triggers the macOS
    /// Local Network prompt the first time). Safe to call more than once.
    pub fn start_net_scan(&self) {
        let repaint = self.repaint.clone();
        self.net.begin(move || repaint());
    }

    pub fn net_scan_started(&self) -> bool {
        self.net.started()
    }
}

// ───────────────────────── command loop ─────────────────────────

async fn run(
    engine: Engine,
    mut rx: mpsc::UnboundedReceiver<Cmd>,
    prompter: Prompter,
    reporter: Reporter,
    settings: Settings,
) {
    let mut session: Option<Session> = None;

    // Silent sign-in with a keychain-stored password.
    if settings.remember_password && !settings.apple_id.is_empty() {
        match store::keychain_password(&settings.apple_id) {
            Some(pw) => session = sign_in(&engine, &prompter, &settings.apple_id, &pw).await,
            None => warn!("Saved password for {} isn't readable from the keychain; sign in again", settings.apple_id),
        }
    }

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::SignIn { email, password, remember } => {
                session = sign_in(&engine, &prompter, &email, &password).await;
                // Only remember credentials Apple actually accepted; a typo
                // must not be retried silently on every launch.
                if session.is_some() {
                    {
                        let mut s = engine.shared.lock().unwrap();
                        s.settings.apple_id = email.clone();
                        s.settings.remember_password = remember;
                        store::save_settings(&s.settings);
                    }
                    store::set_keychain_password(&email, remember.then_some(password.as_str()));
                }
            }
            Cmd::SignOut => {
                session = None;
                let mut s = engine.shared.lock().unwrap();
                store::set_keychain_password(&s.settings.apple_id, None);
                s.settings.remember_password = false;
                store::save_settings(&s.settings);
                s.account = Some(AccountState::SignedOut);
            }
            Cmd::Pair { device_key } => {
                let dev = find_device(&engine, &device_key);
                let result = match dev {
                    Some(Device {
                        kind: DeviceKind::Network { host, ip, pairing_port, tunnel_port, .. },
                        name,
                        ..
                    }) => {
                        let port = pairing_port.or(tunnel_port).ok_or_else(|| anyhow!("No pairing service"));
                        match port {
                            Ok(port) => {
                                let p = prompter.clone();
                                let n = name.clone();
                                device::pair_network(&host, ip, port, move || {
                                    let p = p.clone();
                                    let n = n.clone();
                                    async move { p.ask(Prompt::Pin { device: n }).await.unwrap_or_default() }
                                })
                                .await
                            }
                            Err(e) => Err(e),
                        }
                    }
                    _ => Err(anyhow!("Device is not a network device")),
                };
                finish_simple(&engine, result.map(|_| "Paired! You can now install apps.".to_string()));
            }
            Cmd::Unpair { device_key } => {
                if let Some(Device { kind: DeviceKind::Network { host, .. }, .. }) = find_device(&engine, &device_key) {
                    let _ = std::fs::remove_file(device::pairing_path(&host));
                    finish_simple(&engine, Ok("Pairing removed".into()));
                }
            }
            Cmd::Install(req) => {
                // Name the app, not the stored file: library copies are named
                // after the bundle ID, team ID and device UDID.
                let app = crate::ipa::inspect(&req.ipa).map(|i| i.name).unwrap_or_else(|_| "app".into());
                start_job(&engine, job_title(req.library_id.is_some(), &app));
                let res = match session.as_mut() {
                    None => Err(anyhow!("Sign in with your Apple ID first")),
                    Some(sess) => match find_device(&engine, &req.device_key) {
                        None => Err(anyhow!("Device is no longer connected")),
                        Some(dev) => pipeline::sideload(sess, &dev, &req, &reporter, &prompter).await,
                    },
                };
                finish_job(&engine, res);
            }
            Cmd::RefreshAll => {
                let entries = engine.shared.lock().unwrap().library.clone();
                let Some(sess) = session.as_mut() else {
                    finish_simple(&engine, Err(anyhow!("Sign in with your Apple ID first")));
                    continue;
                };
                let mut ok = 0;
                let mut failed = Vec::new();
                for e in entries {
                    start_job(&engine, format!("Refreshing {}", e.name));
                    let req = InstallRequest {
                        ipa: e.ipa_path.clone(),
                        device_key: e.device_key.clone(),
                        options: e.options.clone(),
                        library_id: Some(e.id.clone()),
                    };
                    let res = match find_device(&engine, &e.device_key) {
                        None => Err(anyhow!("{} is offline", e.device_name)),
                        Some(dev) => pipeline::sideload(sess, &dev, &req, &reporter, &prompter).await,
                    };
                    match res {
                        Ok(entry) => {
                            // Record it, or the countdown never resets and the library isn't saved.
                            record_entry(&engine, entry);
                            ok += 1;
                        }
                        Err(err) => {
                            error!("{}: {err:#}", e.name);
                            failed.push(format!("{}: {err}", e.name));
                        }
                    }
                }
                let summary = if failed.is_empty() {
                    Ok(format!("Refreshed {ok} app(s)"))
                } else {
                    Err(format!("Refreshed {ok}, failed {}:\n{}", failed.len(), failed.join("\n")))
                };
                let mut s = engine.shared.lock().unwrap();
                if let Some(job) = s.job.as_mut() {
                    job.stage = Stage::Done;
                    job.fraction = 1.0;
                    job.result = Some(summary);
                }
            }
        }
        (engine.repaint)();
    }
}

fn find_device(engine: &Engine, key: &str) -> Option<Device> {
    engine.shared.lock().unwrap().devices.iter().find(|d| d.key == key).cloned()
}

/// "Installing Stremio" or, when re-signing something already installed, "Refreshing Stremio".
fn job_title(refresh: bool, app: &str) -> String {
    format!("{} {app}", if refresh { "Refreshing" } else { "Installing" })
}

static NEXT_JOB: AtomicU64 = AtomicU64::new(1);

fn start_job(engine: &Engine, title: String) {
    engine.shared.lock().unwrap().job = Some(Job {
        id: NEXT_JOB.fetch_add(1, Ordering::Relaxed),
        title,
        stage: Stage::Connect,
        fraction: 0.0,
        detail: "Starting…".into(),
        result: None,
    });
    (engine.repaint)();
}

/// Puts a finished install into the library and saves it. Returns the message to show.
fn record_entry(engine: &Engine, entry: LibraryEntry) -> String {
    let msg = format!("{} is installed on {}", entry.name, entry.device_name);
    let mut s = engine.shared.lock().unwrap();
    merge_entry(&mut s.library, entry);
    store::save_library(&s.library);
    msg
}

/// A reinstall replaces the entry it refreshes (same id, or the same bundle on the same
/// device) instead of duplicating it, so the new expiry date takes effect.
fn merge_entry(library: &mut Vec<LibraryEntry>, entry: LibraryEntry) {
    library.retain(|e| {
        e.id != entry.id && !(e.signed_bundle_id == entry.signed_bundle_id && e.device_key == entry.device_key)
    });
    library.push(entry);
}

fn finish_job(engine: &Engine, res: Result<LibraryEntry>) {
    let result = match res {
        Ok(entry) => Ok(record_entry(engine, entry)),
        Err(e) => {
            error!("{e:#}");
            Err(format!("{e:#}"))
        }
    };
    let mut s = engine.shared.lock().unwrap();
    if let Some(job) = s.job.as_mut() {
        if result.is_ok() {
            job.stage = Stage::Done;
            job.fraction = 1.0;
        }
        job.result = Some(result);
    }
}

fn finish_simple(engine: &Engine, res: Result<String>) {
    let mut s = engine.shared.lock().unwrap();
    match res {
        Ok(m) => s.toast = Some((m, true, 0.0)),
        Err(e) => {
            error!("{e:#}");
            s.toast = Some((format!("{e:#}"), false, 0.0));
        }
    }
}

async fn sign_in(engine: &Engine, prompter: &Prompter, email: &str, password: &str) -> Option<Session> {
    let anisette_url = {
        let mut s = engine.shared.lock().unwrap();
        s.account = Some(AccountState::SigningIn);
        s.settings.anisette_url.clone()
    };
    (engine.repaint)();
    match sign_in_inner(prompter, email, password, &anisette_url).await {
        Ok((sess, name)) => {
            engine.shared.lock().unwrap().account = Some(AccountState::SignedIn {
                email: email.to_string(),
                name,
                team: sess.team.name.clone().unwrap_or_else(|| "Personal Team".into()),
                team_id: sess.team.team_id.clone(),
            });
            info!("Signed in; team {}", sess.team.team_id);
            Some(sess)
        }
        Err(e) => {
            error!("Sign-in failed: {e:#}");
            let mut s = engine.shared.lock().unwrap();
            s.account = Some(AccountState::SignedOut);
            s.toast = Some((format!("Sign-in failed: {e:#}"), false, 0.0));
            None
        }
    }
}

/// The anisette server supplies the device-identity headers Apple expects.
/// Order of precedence: `LODESTAR_ANISETTE_URL`, then `anisette_url` in
/// settings.json, then the library default. Your Apple ID password is never
/// sent to it.
async fn sign_in_inner(
    prompter: &Prompter,
    email: &str,
    password: &str,
    anisette_url: &str,
) -> Result<(Session, String)> {
    let p = prompter.clone();
    let two_factor = move |params: TwoFactorCallbackParams| {
        let p = p.clone();
        async move {
            if params.unknown {
                // Apple didn't say which method; trusted devices is the sane default.
                return Ok(TwoFactorCallbackResponse::SendToDevices);
            }
            let answer = p.ask(Prompt::TwoFactor { sms: params.sms, error: params.last_error.clone() }).await;
            Ok(match answer.as_deref() {
                None | Some("") => TwoFactorCallbackResponse::Abort,
                Some("__sms__") => match params.numbers.first() {
                    Some(n) => TwoFactorCallbackResponse::SendSms(n.id),
                    None => TwoFactorCallbackResponse::ResendCode,
                },
                Some("__resend__") => TwoFactorCallbackResponse::ResendCode,
                Some(code) => TwoFactorCallbackResponse::SubmitCode(code.trim().to_string()),
            })
        }
    };

    let mut builder = AppleAccountBuilder::new(email);
    let custom = std::env::var("LODESTAR_ANISETTE_URL").unwrap_or_else(|_| anisette_url.to_string());
    let custom = custom.trim();
    if !custom.is_empty() {
        info!("Using anisette server {custom}");
        let provider = RemoteV3AnisetteProvider::default().map_err(|e| anyhow!("{e}"))?.set_url(custom);
        builder = builder.anisette_provider(provider);
    }
    let mut account = builder.login(password, two_factor).await.map_err(|e| anyhow!("{e}"))?;
    let name = account.get_name().map(|(f, l)| format!("{f} {l}").trim().to_string()).unwrap_or_default();
    let mut dev = DeveloperSession::from_account(&mut account).await.map_err(|e| anyhow!("{e}"))?;
    let teams = dev.list_teams().await.map_err(|e| anyhow!("{e}"))?;
    let team = teams.into_iter().next().ok_or_else(|| {
        anyhow!("This Apple ID has no developer team. Sign in once at developer.apple.com to accept the agreement.")
    })?;
    Ok((Session { dev, email: email.to_string(), team }, name))
}

#[cfg(test)]
mod tests {
    use super::{job_title, merge_entry};
    use crate::store::{InstallOptions, LibraryEntry, Platform};
    use std::path::PathBuf;

    fn entry(id: &str, bundle: &str, device: &str, expires_at: i64) -> LibraryEntry {
        LibraryEntry {
            id: id.into(),
            name: "App".into(),
            original_bundle_id: "com.x".into(),
            signed_bundle_id: bundle.into(),
            version: "1".into(),
            device_key: device.into(),
            device_name: "Device".into(),
            platform: Platform::Ios,
            installed_at: 0,
            expires_at,
            ipa_path: PathBuf::new(),
            options: InstallOptions::default(),
        }
    }

    #[test]
    fn a_refresh_replaces_its_entry_and_updates_the_expiry() {
        let mut lib = vec![entry("a", "com.x.T", "tv", 100), entry("b", "com.y.T", "tv", 200)];
        merge_entry(&mut lib, entry("a", "com.x.T", "tv", 900));
        assert_eq!(lib.len(), 2, "no duplicate");
        assert_eq!(lib.iter().find(|e| e.id == "a").unwrap().expires_at, 900, "new expiry applied");
        assert_eq!(lib.iter().find(|e| e.id == "b").unwrap().expires_at, 200, "others untouched");
    }

    #[test]
    fn reinstalling_the_same_bundle_on_the_same_device_replaces_it() {
        let mut lib = vec![entry("old", "com.x.T", "tv", 100)];
        merge_entry(&mut lib, entry("new", "com.x.T", "tv", 500));
        assert_eq!(lib.len(), 1);
        assert_eq!(lib[0].id, "new");
    }

    #[test]
    fn the_same_bundle_on_a_different_device_is_a_separate_entry() {
        let mut lib = vec![entry("a", "com.x.T", "tv", 100)];
        merge_entry(&mut lib, entry("b", "com.x.T", "phone", 100));
        assert_eq!(lib.len(), 2);
    }

    #[test]
    fn job_titles_name_the_app_and_the_action() {
        assert_eq!(job_title(false, "Stremio"), "Installing Stremio");
        assert_eq!(job_title(true, "Stremio"), "Refreshing Stremio");
    }
}
