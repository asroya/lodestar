//! Sign → provision → install, parameterised by platform.
//!
//! This mirrors isideload's `Sideloader::install_app`, but routes every
//! developer-portal call through the right platform path (`/tvos/` for an
//! Apple TV) — the device must be registered, and the provisioning profile
//! issued, for tvOS or the TV refuses the app.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use isideload::{
    dev::developer_session::{
        AppGroupsApi, AppId, AppIdsApi, CertificatesApi, DeveloperDevice, DeveloperDeviceType, DevelopmentCertificate,
        DevicesApi, Profile,
    },
    sideload::{
        application::Application, builder::MaxCertsBehavior, cert_identity::CertificateIdentity, install, sign,
    },
    util::{callbacks::MaxCertsCallbackFuture, keyring_storage::KeyringStorage},
};
use plist::Dictionary;
use tracing::{info, warn};

use crate::{
    device::{self, Device, DeviceKind},
    engine::{InstallRequest, Prompter, Reporter, Session, Stage},
    ipa,
    store::{self, KEYCHAIN_SERVICE, LibraryEntry, Platform},
};

const MACHINE_NAME: &str = "Lodestar";

fn r<T>(x: std::result::Result<T, rootcause::Report>) -> Result<T> {
    x.map_err(|e| anyhow!("{e}"))
}

enum Target {
    Usb(idevice::provider::UsbmuxdProvider),
    Net(Box<device::Tunnel>),
}

pub async fn sideload(
    sess: &mut Session,
    dev: &Device,
    req: &InstallRequest,
    rep: &Reporter,
    _prompter: &Prompter,
) -> Result<LibraryEntry> {
    let meta = ipa::inspect(&req.ipa)?;

    // 1. Connect to the device and learn its identity.
    rep.stage(Stage::Connect, 0.02, format!("Connecting to {}…", dev.name));
    let (target, udid, dev_name, platform) = match &dev.kind {
        DeviceKind::Usbmux { device_id, udid, .. } => {
            let p = device::usb_provider(*device_id, udid);
            (Target::Usb(p), udid.clone(), dev.name.clone(), dev.platform)
        }
        DeviceKind::Network { host, ip, tunnel_port, paired, .. } => {
            if !paired {
                bail!("Pair {} first (Devices → Pair)", dev.name);
            }
            let port = tunnel_port.ok_or_else(|| {
                anyhow!("{} isn't advertising its tunnel service. Wake the TV and leave the pairing screen.", dev.name)
            })?;
            rep.stage(Stage::Connect, 0.04, "Opening encrypted tunnel…");
            let t = device::open_tunnel(host, *ip, port).await?;
            let udid = t.prop("UniqueDeviceID").unwrap_or_else(|| t.handshake.uuid.clone());
            let name = t.prop("DeviceName").unwrap_or_else(|| dev.name.clone());
            let platform = t.platform();
            info!("Connected to {name} ({udid}), {}", platform.label());
            (Target::Net(Box::new(t)), udid, name, platform)
        }
    };

    if platform != meta.platform {
        warn!(
            "{} is a {} app but {} runs {} — it will most likely be rejected",
            meta.name,
            meta.platform.label(),
            dev_name,
            platform.label()
        );
    }
    // Apple's Xcode service only has the `ios/` path; tvOS is selected with a
    // `subPlatform` field in the body (the `/tvos/` path returns 404).
    let tv = platform == Platform::Tvos;
    let dev_type = DeveloperDeviceType::Ios;

    // 2. Register the device and get a signing identity.
    let team = sess.team.clone();
    rep.stage(Stage::Register, 0.08, "Registering device with your team…");
    if tv {
        ensure_tv_registered(sess, &dev_name, &udid).await?;
    } else {
        r(sess.dev.ensure_device_registered(&team, &dev_name, &udid, dev_type.clone()).await)?;
    }

    // Reading the signing key can raise a macOS Keychain prompt that blocks
    // until it is answered, so say so instead of looking frozen.
    rep.stage(
        Stage::Register,
        0.12,
        "Getting your signing certificate. If macOS asks for Keychain access, choose Always Allow.",
    );
    let storage = KeyringStorage::new(KEYCHAIN_SERVICE.to_string());
    let cert = signing_identity(sess, &storage).await?;

    // 3. Unpack and customise the bundle.
    rep.stage(Stage::Register, 0.18, "Unpacking app…");
    let _unpacked = TempExtract::for_ipa(&req.ipa);
    let mut app = r(Application::new(req.ipa.clone()))?;
    if req.options.remove_extensions {
        let plugins = app.bundle.bundle_dir.join("PlugIns");
        let exts = app.bundle.bundle_dir.join("Extensions");
        let _ = std::fs::remove_dir_all(&plugins);
        let _ = std::fs::remove_dir_all(&exts);
        let dir = app.bundle.bundle_dir.clone();
        app = r(Application::new(dir))?;
    }
    customise(&mut app.bundle.app_info, &req.options);

    let original_id =
        app.bundle.bundle_identifier().ok_or_else(|| anyhow!("App has no bundle identifier"))?.to_string();
    let signed_id = if req.options.custom_bundle_id.trim().is_empty() {
        format!("{original_id}.{}", team.team_id)
    } else {
        req.options.custom_bundle_id.trim().to_string()
    };
    r(app.update_bundle_id(&original_id, &signed_id))?;
    let display_name = app
        .bundle
        .app_info
        .get("CFBundleDisplayName")
        .and_then(|v| v.as_string())
        .map(str::to_string)
        .unwrap_or_else(|| meta.name.clone());

    // 4. App IDs, app group and profiles — all on the device's platform.
    rep.stage(Stage::Register, 0.24, "Registering App IDs…");
    let mut app_ids = register_app_ids(sess, &app, &dev_type).await?;
    let main_app_id = app_ids
        .iter()
        .find(|a| a.identifier == signed_id)
        .cloned()
        .ok_or_else(|| anyhow!("App ID {signed_id} was not created"))?;

    let group_id = format!("group.{signed_id}");
    match r(sess.dev.ensure_app_group(&team, &display_name, &group_id, dev_type.clone()).await) {
        Ok(group) => {
            for id in app_ids.iter_mut() {
                let ok = r(id.ensure_group_feature(&mut sess.dev, &team).await).is_ok()
                    && r(sess.dev.assign_app_group(&team, &group, id, dev_type.clone()).await).is_ok();
                if !ok {
                    warn!("Couldn't attach app group to {}; continuing", id.identifier);
                }
            }
        }
        Err(e) => warn!("App group unavailable ({e}); continuing without it"),
    }

    rep.stage(Stage::Register, 0.32, "Downloading provisioning profiles…");
    let main_profile: Profile = download_profile(sess, &main_app_id, tv).await?;
    let mut ext_profiles: Vec<(String, Profile, Dictionary)> = Vec::new();
    for id in app_ids.iter().filter(|a| a.identifier != main_app_id.identifier) {
        let p = download_profile(sess, id, tv).await.with_context(|| format!("profile for {}", id.identifier))?;
        let parsed = r(apple_codesign_profile_entitlements(&p))?;
        ext_profiles.push((id.identifier.clone(), p, parsed));
    }
    let expires_at = SystemTime::from(main_profile.date_expire)
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_else(|_| store::now() + 7 * 86_400);

    // 5. Sign.
    rep.stage(Stage::Sign, 0.40, "Signing…");
    r(app.bundle.write_info())?;
    for b in app.bundle.app_extensions_mut() {
        r(b.write_info())?;
    }
    for b in app.bundle.frameworks_mut() {
        r(b.write_info())?;
    }
    let rep2 = rep.clone();
    r(sign::sign(
        &mut app,
        &cert,
        &main_profile,
        &mut ext_profiles,
        &None,
        &team,
        Some(move |f: f32| {
            let rep = rep2.clone();
            async move { rep.fraction(0.40 + (f - 0.4).max(0.0) * 0.4) }
        }),
    )
    .await)
    .context("Code signing failed")?;
    let signed_dir = app.bundle.bundle_dir.clone();

    // 6. Upload + install.
    rep.stage(Stage::Transfer, 0.62, format!("Sending to {dev_name}…"));
    let last = Arc::new(AtomicU64::new(0));
    let rep3 = rep.clone();
    let progress = move |pct: u64| {
        if pct != last.swap(pct, Ordering::Relaxed) {
            rep3.fraction(0.62 + pct as f32 / 100.0 * 0.38);
        }
    };
    match target {
        Target::Usb(p) => r(install::install_app(&p, &signed_dir, progress).await)?,
        Target::Net(mut t) => {
            let t = &mut *t;
            r(install::install_app_rsd(&mut t.handle, &mut t.handshake, &signed_dir, progress).await)?
        }
    }

    // 7. Keep a copy of the IPA so it can be refreshed in a week.
    let id = req
        .library_id
        .clone()
        .unwrap_or_else(|| format!("{}-{}", store::sanitize(&signed_id), store::sanitize(&dev.key)));
    let ipa_path = keep_ipa(&req.ipa, &id)?;

    Ok(LibraryEntry {
        id,
        name: display_name,
        original_bundle_id: original_id,
        signed_bundle_id: signed_id,
        version: meta.version,
        device_key: dev.key.clone(),
        device_name: dev_name,
        platform,
        installed_at: store::now(),
        expires_at,
        ipa_path,
        options: req.options.clone(),
    })
}

fn customise(info: &mut Dictionary, o: &store::InstallOptions) {
    if !o.custom_name.trim().is_empty() {
        info.insert("CFBundleDisplayName".into(), o.custom_name.trim().into());
    }
    if o.remove_device_restrictions {
        info.remove("UISupportedDevices");
    }
    if o.file_sharing {
        info.insert("UIFileSharingEnabled".into(), true.into());
        info.insert("UISupportsDocumentBrowser".into(), true.into());
    }
}

async fn register_app_ids(sess: &mut Session, app: &Application, dev_type: &DeveloperDeviceType) -> Result<Vec<AppId>> {
    let team = sess.team.clone();
    let mut wanted: Vec<(String, String)> = vec![(
        app.bundle.bundle_identifier().unwrap_or_default().to_string(),
        app.bundle.bundle_name().unwrap_or("App").to_string(),
    )];
    for ext in app.bundle.app_extensions() {
        wanted.push((
            ext.bundle_identifier().unwrap_or_default().to_string(),
            ext.bundle_name().unwrap_or("Extension").to_string(),
        ));
    }

    let existing = r(sess.dev.list_app_ids(&team, dev_type.clone()).await)?;
    let missing: Vec<_> =
        wanted.iter().filter(|(id, _)| !existing.app_ids.iter().any(|a| &a.identifier == id)).collect();
    if let Some(avail) = existing.available_quantity
        && avail >= 0
        && missing.len() as i64 > avail
    {
        bail!(
            "This app needs {} new App ID(s) but your free account has {} left this week. \
             Try “Remove app extensions”, or wait for the 7-day window to reset.",
            missing.len(),
            avail
        );
    }
    for (id, name) in missing {
        let clean = isideload::dev::normalize_app_names(name);
        r(sess.dev.add_app_id(&team, &clean, id, dev_type.clone()).await)
            .with_context(|| format!("creating App ID {id}"))?;
    }
    let listed = r(sess.dev.list_app_ids(&team, dev_type.clone()).await)?;
    Ok(listed.app_ids.into_iter().filter(|a| wanted.iter().any(|(id, _)| &a.identifier == id)).collect())
}

fn dev_url(endpoint: &str) -> String {
    format!("https://developerservices2.apple.com/services/QH65B2/ios/{endpoint}.action?clientId=XABBG36SBA")
}

fn platform_body(sess: &Session, tv: bool, extra: Vec<(&str, plist::Value)>) -> Dictionary {
    let mut d = Dictionary::new();
    d.insert("teamId".into(), sess.team.team_id.clone().into());
    if tv {
        d.insert("DTDK_Platform".into(), "ios".into());
        d.insert("subPlatform".into(), "tvOS".into());
    }
    for (k, v) in extra {
        d.insert(k.into(), v);
    }
    d
}

async fn ensure_tv_registered(sess: &mut Session, name: &str, udid: &str) -> Result<()> {
    let body = platform_body(sess, true, vec![]);
    let devices: Vec<DeveloperDevice> =
        r(sess.dev.send_dev_request(&dev_url("listDevices"), body, "devices").await).context("Listing team devices")?;
    if devices.iter().any(|d| d.device_number.eq_ignore_ascii_case(udid)) {
        info!("Apple TV is already a development device");
        return Ok(());
    }
    info!("Registering Apple TV {udid} as a development device");
    let body = platform_body(sess, true, vec![("name", name.into()), ("deviceNumber", udid.into())]);
    let res: std::result::Result<DeveloperDevice, rootcause::Report> =
        sess.dev.send_dev_request(&dev_url("addDevice"), body, "device").await;
    if let Err(e) = res {
        let already = e
            .iter_reports()
            .find_map(|n| n.downcast_current_context::<isideload::SideloadError>())
            .is_some_and(|err| matches!(err, isideload::SideloadError::DeveloperError(35, _)));
        if !already {
            return Err(anyhow!("Registering the Apple TV failed: {e}"));
        }
    }
    Ok(())
}

async fn download_profile(sess: &mut Session, app_id: &AppId, tv: bool) -> Result<Profile> {
    let body = platform_body(sess, tv, vec![("appIdId", app_id.app_id_id.clone().into())]);
    r(sess.dev.send_dev_request(&dev_url("downloadTeamProvisioningProfile"), body, "provisioningProfile").await)
        .with_context(|| format!("Downloading provisioning profile for {}", app_id.identifier))
}

fn apple_codesign_profile_entitlements(p: &Profile) -> std::result::Result<Dictionary, rootcause::Report> {
    let parsed = apple_codesign_quick::ProvisioningProfile::parse(p.encoded_profile.as_ref())?;
    Ok(parsed.entitlements().clone())
}

type CertsCb = Box<dyn Fn(Vec<DevelopmentCertificate>) -> MaxCertsCallbackFuture + Send + Sync>;

fn dev_error_code(e: &rootcause::Report) -> Option<i64> {
    e.iter_reports().find_map(|n| n.downcast_current_context::<isideload::SideloadError>()).and_then(|err| match err {
        isideload::SideloadError::DeveloperError(code, _) => Some(*code),
        _ => None,
    })
}

/// Get (or create) Lodestar's signing certificate. When Apple's certificate
/// limit is hit (7460) we free a slot ourselves instead of trusting the first
/// entry of `listAllDevelopmentCerts`, which also returns Mac / Apple
/// Development / revoked certificates that the iOS revoke endpoint rejects
/// (7252).
async fn signing_identity(sess: &mut Session, storage: &KeyringStorage) -> Result<CertificateIdentity> {
    let team = sess.team.clone();
    let behavior: MaxCertsBehavior<CertsCb> = MaxCertsBehavior::Error;
    let mut revoked = Vec::new();
    loop {
        match CertificateIdentity::retrieve(MACHINE_NAME, &sess.email, &mut sess.dev, &team, storage, &behavior).await {
            Ok(c) => return Ok(c),
            Err(e) if dev_error_code(&e) == Some(7460) && revoked.len() < 3 => {
                warn!("Apple's development certificate limit reached; freeing a slot");
                revoked.push(free_certificate_slot(sess, &revoked).await?);
            }
            Err(e) => return Err(anyhow!("{e}")),
        }
    }
}

fn is_ios_development(c: &DevelopmentCertificate) -> bool {
    c.certificate_type.as_ref().is_some_and(|t| {
        t.certificate_type_display_id.as_deref() == Some("5QPB9NHCEI")
            || t.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case("iOS Development"))
    })
}

fn cert_platform(c: &DevelopmentCertificate) -> String {
    c.certificate_platform
        .clone()
        .or_else(|| c.certificate_type.as_ref().and_then(|t| t.platform.clone()))
        .unwrap_or_else(|| "?".into())
}

/// Certificates worth trying to revoke, best first: our own, then other iOS
/// Development ones, oldest first. Already revoked or expired certificates and
/// those in `skip` are excluded: the revoke endpoint rejects them (7252), which
/// is what used to abort installs once Apple's certificate limit was reached.
fn revoke_candidates<'a>(
    certs: &'a [DevelopmentCertificate],
    skip: &[String],
    now: SystemTime,
) -> Vec<&'a DevelopmentCertificate> {
    let mut candidates: Vec<&DevelopmentCertificate> = certs
        .iter()
        .filter(|c| c.serial_number.as_ref().is_some_and(|s| !skip.contains(s)))
        .filter(|c| {
            let st = c.status.as_deref().unwrap_or("").to_lowercase();
            !(st.contains("revoked") || st.contains("expired"))
        })
        .filter(|c| c.expiration_date.map(SystemTime::from).is_none_or(|t| t > now))
        .collect();
    candidates.sort_by_key(|c| {
        (
            c.machine_name.as_deref() != Some(MACHINE_NAME),
            !is_ios_development(c),
            !cert_platform(c).eq_ignore_ascii_case("ios"),
            c.expiration_date.map(SystemTime::from).unwrap_or(SystemTime::UNIX_EPOCH),
        )
    });
    candidates
}

/// Revokes one certificate and returns its serial. Our own certificates go
/// first, then other iOS Development ones, oldest first. Each candidate is
/// tried via the classic endpoint, then the v1 API Xcode uses today; a
/// rejection just moves on to the next candidate.
async fn free_certificate_slot(sess: &mut Session, skip: &[String]) -> Result<String> {
    let team = sess.team.clone();
    let certs = r(sess.dev.list_all_development_certs(&team, DeveloperDeviceType::Ios).await)
        .context("Listing development certificates")?;
    for c in &certs {
        info!(
            "Certificate: {} · type {} · platform {} · machine {} · status {} · serial {} · expires {:?}",
            c.name.as_deref().unwrap_or("?"),
            c.certificate_type.as_ref().and_then(|t| t.name.as_deref()).unwrap_or("?"),
            cert_platform(c),
            c.machine_name.as_deref().unwrap_or("?"),
            c.status.as_deref().unwrap_or("?"),
            c.serial_number.as_deref().unwrap_or("?"),
            c.expiration_date.map(SystemTime::from),
        );
    }

    let candidates = revoke_candidates(&certs, skip, SystemTime::now());

    for c in candidates {
        let serial = c.serial_number.clone().unwrap_or_default();
        let label = format!(
            "“{}” ({}, machine {})",
            c.name.as_deref().unwrap_or("certificate"),
            c.certificate_type.as_ref().and_then(|t| t.name.as_deref()).unwrap_or("unknown type"),
            c.machine_name.as_deref().unwrap_or("unknown")
        );
        match r(sess.dev.revoke_development_cert(&team, &serial, DeveloperDeviceType::Ios).await) {
            Ok(()) => {
                warn!("Revoked {label}, serial {serial}");
                return Ok(serial);
            }
            Err(e) => info!("Classic revoke of {label} refused: {e}"),
        }
        if let Some(id) = &c.certificate_id {
            match revoke_v1(sess, id).await {
                Ok(()) => {
                    warn!("Revoked {label} via v1 API, serial {serial}");
                    return Ok(serial);
                }
                Err(e) => info!("v1 revoke of {label} refused: {e:#}"),
            }
        }
    }
    bail!(
        "Your Apple ID has reached Apple's development-certificate limit and none of its \
         certificates could be revoked automatically (see Activity for the list). Revoke one in \
         Xcode → Settings → Accounts → Manage Certificates, then try again."
    )
}

async fn revoke_v1(sess: &mut Session, certificate_id: &str) -> Result<()> {
    use http::header::{ACCEPT, CONTENT_TYPE, HeaderValue, USER_AGENT};
    let gs = sess.dev.get_grandslam_client();
    let mut headers = r(sess.dev.get_headers().await)?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/vnd.api+json"));
    headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.api+json"));
    headers.insert("X-Mme-Client-Info", HeaderValue::from_str(&gs.client_info.client_info)?);
    headers.insert(USER_AGENT, HeaderValue::from_str(&gs.client_info.user_agent)?);
    headers.insert("X-Xcode-Version", HeaderValue::from_static("27.0 (27A5218g)"));
    headers.insert("X-Apple-App-Info", HeaderValue::from_static("com.apple.gs.xcode.auth"));
    let url = format!(
        "https://developerservices2.apple.com/services/v1/certificates/{certificate_id}?teamId={}",
        sess.team.team_id
    );
    let resp = gs.client.delete(&url).headers(headers).send().await.map_err(|e| anyhow!("{e}"))?;
    let status = resp.status();
    if status.is_success() {
        Ok(())
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("HTTP {status}: {}", body.chars().take(300).collect::<String>())
    }
}

fn keep_ipa(src: &Path, id: &str) -> Result<PathBuf> {
    let dst = store::library_dir().join(format!("{id}.ipa"));
    if src != dst {
        std::fs::copy(src, &dst).with_context(|| format!("copying IPA to {}", dst.display()))?;
    }
    Ok(dst)
}

/// Removes the unpacked copy of the IPA that `Application::new` creates in the
/// temp directory (`<ipa file name>_extracted`) when dropped, so a failed
/// install doesn't leave a signed-looking bundle behind.
struct TempExtract(PathBuf);

impl TempExtract {
    fn for_ipa(ipa: &Path) -> Self {
        let name = ipa.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        Self(std::env::temp_dir().join(format!("{name}_extracted")))
    }
}

impl Drop for TempExtract {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use isideload::dev::developer_session::CertificateType;
    use std::time::Duration;

    fn cert(serial: &str, machine: &str, status: &str, days: i64, kind: &str) -> DevelopmentCertificate {
        let when = if days >= 0 {
            SystemTime::now() + Duration::from_secs(days as u64 * 86_400)
        } else {
            SystemTime::now() - Duration::from_secs((-days) as u64 * 86_400)
        };
        DevelopmentCertificate {
            name: Some(format!("cert {serial}")),
            certificate_id: Some(format!("id-{serial}")),
            serial_number: Some(serial.into()),
            machine_id: None,
            machine_name: Some(machine.into()),
            cert_content: None,
            certificate_platform: Some("ios".into()),
            certificate_type: Some(CertificateType {
                certificate_type_display_id: None,
                name: Some(kind.into()),
                platform: Some("ios".into()),
                permission_type: None,
                distribution_type: None,
                distribution_method: None,
                owner_type: None,
                days_overlap: None,
                max_active_certs: None,
            }),
            status: Some(status.into()),
            status_code: None,
            expiration_date: Some(when.into()),
        }
    }

    fn serials(v: &[&DevelopmentCertificate]) -> Vec<String> {
        v.iter().map(|c| c.serial_number.clone().unwrap()).collect()
    }

    #[test]
    fn own_certificate_is_revoked_first() {
        let certs = vec![
            cert("OTHER", "Someone's Mac", "Issued", 300, "iOS Development"),
            cert("MINE", MACHINE_NAME, "Issued", 100, "iOS Development"),
        ];
        let got = revoke_candidates(&certs, &[], SystemTime::now());
        assert_eq!(serials(&got), ["MINE", "OTHER"]);
    }

    #[test]
    fn revoked_and_expired_certificates_are_never_candidates() {
        let certs = vec![
            cert("REVOKED", MACHINE_NAME, "Revoked", 100, "iOS Development"),
            cert("EXPIRED", MACHINE_NAME, "Issued", -3, "iOS Development"),
            cert("LIVE", "Mac", "Issued", 10, "iOS Development"),
        ];
        let got = revoke_candidates(&certs, &[], SystemTime::now());
        assert_eq!(serials(&got), ["LIVE"]);
    }

    #[test]
    fn already_tried_serials_are_skipped() {
        let certs = vec![
            cert("A", MACHINE_NAME, "Issued", 1, "iOS Development"),
            cert("B", "Mac", "Issued", 2, "iOS Development"),
        ];
        let got = revoke_candidates(&certs, &["A".to_string()], SystemTime::now());
        assert_eq!(serials(&got), ["B"]);
    }

    #[test]
    fn oldest_expiry_goes_first_among_equals() {
        let certs = vec![
            cert("LATE", "Mac", "Issued", 200, "iOS Development"),
            cert("SOON", "Mac", "Issued", 20, "iOS Development"),
        ];
        let got = revoke_candidates(&certs, &[], SystemTime::now());
        assert_eq!(serials(&got), ["SOON", "LATE"]);
    }

    #[test]
    fn certificates_without_a_serial_are_ignored() {
        let mut c = cert("X", "Mac", "Issued", 5, "iOS Development");
        c.serial_number = None;
        assert!(revoke_candidates(&[c], &[], SystemTime::now()).is_empty());
    }

    #[test]
    fn customise_applies_only_requested_options() {
        let mut info = Dictionary::new();
        info.insert("UISupportedDevices".into(), plist::Value::Array(vec!["iPhone14,2".into()]));
        info.insert("CFBundleDisplayName".into(), "Original".into());

        let mut untouched = info.clone();
        customise(&mut untouched, &store::InstallOptions::default());
        assert_eq!(untouched, info);

        let mut changed = info.clone();
        customise(
            &mut changed,
            &store::InstallOptions {
                custom_name: "  Renamed  ".into(),
                remove_device_restrictions: true,
                file_sharing: true,
                ..Default::default()
            },
        );
        assert_eq!(changed.get("CFBundleDisplayName").and_then(|v| v.as_string()), Some("Renamed"));
        assert!(!changed.contains_key("UISupportedDevices"));
        assert_eq!(changed.get("UIFileSharingEnabled").and_then(|v| v.as_boolean()), Some(true));
    }

    #[test]
    fn unpack_guard_removes_its_directory_on_drop() {
        let ipa = std::path::Path::new("/nonexistent/Guard Test.ipa");
        let guard = TempExtract::for_ipa(ipa);
        let dir = guard.0.clone();
        assert!(dir.ends_with("Guard Test.ipa_extracted"));
        std::fs::create_dir_all(dir.join("Payload")).unwrap();
        drop(guard);
        assert!(!dir.exists());
    }
}
