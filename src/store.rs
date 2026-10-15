//! Persistent settings and the library of installed apps.
//!
//! Everything lives under `~/Library/Application Support/Lodestar`. Secrets
//! (Apple ID password, signing key, anisette state) go to the macOS keychain,
//! never to these JSON files.

use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

pub const KEYCHAIN_SERVICE: &str = "Lodestar";

pub fn data_dir() -> PathBuf {
    let dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("Lodestar");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn pairings_dir() -> PathBuf {
    let dir = data_dir().join("pairings");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn library_dir() -> PathBuf {
    let dir = data_dir().join("library");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Platform {
    Ios,
    Tvos,
}

impl Platform {
    pub fn label(self) -> &'static str {
        match self {
            Platform::Ios => "iOS",
            Platform::Tvos => "tvOS",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct InstallOptions {
    /// Replaces the home-screen name (CFBundleDisplayName).
    pub custom_name: String,
    /// Exact bundle identifier to sign with. Empty = `<original>.<TEAMID>`.
    pub custom_bundle_id: String,
    /// Strip `UISupportedDevices` so the app installs on any model.
    pub remove_device_restrictions: bool,
    /// Expose the app's Documents folder in Finder/Files.
    pub file_sharing: bool,
    /// Remove app extensions (saves App IDs on free accounts: 10/week).
    pub remove_extensions: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub apple_id: String,
    pub remember_password: bool,
    /// Optional custom anisette v3 server. Empty = library default.
    pub anisette_url: String,
    pub refresh_warning_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub id: String,
    pub name: String,
    pub original_bundle_id: String,
    pub signed_bundle_id: String,
    pub version: String,
    pub device_key: String,
    pub device_name: String,
    pub platform: Platform,
    pub installed_at: i64,
    pub expires_at: i64,
    pub ipa_path: PathBuf,
    pub options: InstallOptions,
}

impl LibraryEntry {
    pub fn seconds_left(&self) -> i64 {
        self.expires_at - now()
    }
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(name: &str) -> T {
    std::fs::read(data_dir().join(name)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write_json<T: Serialize>(name: &str, value: &T) {
    if let Ok(bytes) = serde_json::to_vec_pretty(value) {
        let tmp = data_dir().join(format!("{name}.tmp"));
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(tmp, data_dir().join(name));
        }
    }
}

pub fn load_settings() -> Settings {
    with_defaults(read_json("settings.json"))
}

/// Fills in values a missing or older settings file doesn't carry.
fn with_defaults(mut s: Settings) -> Settings {
    if s.refresh_warning_days == 0 {
        s.refresh_warning_days = 2;
    }
    s
}

pub fn save_settings(s: &Settings) {
    write_json("settings.json", s)
}

pub fn load_library() -> Vec<LibraryEntry> {
    read_json("library.json")
}

pub fn save_library(l: &[LibraryEntry]) {
    write_json("library.json", &l.to_vec())
}

pub fn keychain_password(email: &str) -> Option<String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &format!("password:{email}")).ok()?.get_password().ok()
}

pub fn set_keychain_password(email: &str, password: Option<&str>) {
    if let Ok(entry) = keyring::Entry::new(KEYCHAIN_SERVICE, &format!("password:{email}")) {
        let _ = match password {
            Some(p) => entry.set_password(p),
            None => entry.delete_credential(),
        };
    }
}

pub fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
}

pub fn human_duration(secs: i64) -> String {
    if secs <= 0 {
        return "expired".into();
    }
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_ids_filesystem_safe() {
        assert_eq!(sanitize("com.example.App.ABC123"), "com_example_App_ABC123");
        assert_eq!(sanitize("net:GWABtv.local"), "net_GWABtv_local");
        assert_eq!(sanitize("../../etc/passwd"), "______etc_passwd");
        assert_eq!(sanitize("ok-name"), "ok-name");
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(human_duration(-5), "expired");
        assert_eq!(human_duration(0), "expired");
        assert_eq!(human_duration(59), "0m");
        assert_eq!(human_duration(5 * 60), "5m");
        assert_eq!(human_duration(2 * 3600 + 15 * 60), "2h 15m");
        assert_eq!(human_duration(6 * 86_400 + 23 * 3600), "6d 23h");
    }

    #[test]
    fn library_entries_round_trip_through_json() {
        let entry = LibraryEntry {
            id: "id".into(),
            name: "Stremio".into(),
            original_bundle_id: "com.stremio.pal".into(),
            signed_bundle_id: "com.stremio.pal.TEAM".into(),
            version: "2.0.8".into(),
            device_key: "net:tv.local".into(),
            device_name: "Living Room".into(),
            platform: Platform::Tvos,
            installed_at: 1_000,
            expires_at: 1_000 + 7 * 86_400,
            ipa_path: PathBuf::from("/tmp/x.ipa"),
            options: InstallOptions { custom_name: "S".into(), file_sharing: true, ..Default::default() },
        };
        let json = serde_json::to_string(&vec![entry]).unwrap();
        let back: Vec<LibraryEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].platform, Platform::Tvos);
        assert_eq!(back[0].options.custom_name, "S");
        assert!(back[0].options.file_sharing);
    }

    #[test]
    fn expiry_countdown_follows_the_clock() {
        let mut e = LibraryEntry {
            id: String::new(),
            name: String::new(),
            original_bundle_id: String::new(),
            signed_bundle_id: String::new(),
            version: String::new(),
            device_key: String::new(),
            device_name: String::new(),
            platform: Platform::Ios,
            installed_at: now(),
            expires_at: now() + 3_600,
            ipa_path: PathBuf::new(),
            options: InstallOptions::default(),
        };
        assert!((3_595..=3_600).contains(&e.seconds_left()));
        e.expires_at = now() - 10;
        assert!(e.seconds_left() < 0);
    }

    #[test]
    fn an_empty_settings_file_still_gets_a_two_day_refresh_warning() {
        let s = with_defaults(serde_json::from_str("{}").unwrap());
        assert_eq!(s.refresh_warning_days, 2);
        assert!(s.apple_id.is_empty());
    }

    #[test]
    fn older_settings_files_keep_what_they_have() {
        // A file written before newer fields existed must not reset the rest.
        let s =
            with_defaults(serde_json::from_str(r#"{"apple_id":"me@example.com","remember_password":true}"#).unwrap());
        assert_eq!(s.apple_id, "me@example.com");
        assert!(s.remember_password);
        assert_eq!(s.refresh_warning_days, 2);
        let custom = with_defaults(serde_json::from_str(r#"{"refresh_warning_days":5}"#).unwrap());
        assert_eq!(custom.refresh_warning_days, 5);
    }
}
