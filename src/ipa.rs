//! Lightweight IPA inspection (reads only the main Info.plist, no extraction).

use std::{fs::File, io::Read, path::Path};

use anyhow::{Context, Result, bail};

use crate::store::Platform;

#[derive(Debug, Clone)]
pub struct IpaInfo {
    pub name: String,
    pub bundle_id: String,
    pub version: String,
    pub min_os: String,
    pub platform: Platform,
    pub extensions: usize,
    pub size_bytes: u64,
    pub restricted_devices: bool,
}

pub fn inspect(path: &Path) -> Result<IpaInfo> {
    let file = File::open(path).with_context(|| format!("Can't open {}", path.display()))?;
    let size_bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut zip = zip::ZipArchive::new(file).context("Not a valid IPA (zip) file")?;

    let mut info_name = None;
    let mut extensions = 0;
    let names: Vec<String> = zip.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).collect();
    for name in &names {
        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() == 3 && parts[0] == "Payload" && parts[1].ends_with(".app") && parts[2] == "Info.plist" {
            info_name = Some(name.to_string());
        }
        if parts.len() == 5 && parts[2] == "PlugIns" && parts[3].ends_with(".appex") && parts[4] == "Info.plist" {
            extensions += 1;
        }
    }
    let Some(info_name) = info_name else {
        bail!("No Payload/*.app/Info.plist found — is this an IPA?");
    };

    let mut buf = Vec::new();
    zip.by_name(&info_name)?.read_to_end(&mut buf)?;
    let plist: plist::Dictionary = plist::from_bytes(&buf).context("Unreadable Info.plist")?;
    let s = |k: &str| plist.get(k).and_then(|v| v.as_string()).map(str::to_string);

    let is_tv = plist
        .get("CFBundleSupportedPlatforms")
        .and_then(|v| v.as_array())
        .is_some_and(|a| a.iter().any(|p| p.as_string() == Some("AppleTVOS")))
        || plist
            .get("UIDeviceFamily")
            .and_then(|v| v.as_array())
            .is_some_and(|a| a.iter().any(|f| f.as_unsigned_integer() == Some(3)));

    Ok(IpaInfo {
        name: s("CFBundleDisplayName").or_else(|| s("CFBundleName")).unwrap_or_else(|| "Unknown app".into()),
        bundle_id: s("CFBundleIdentifier").unwrap_or_default(),
        version: s("CFBundleShortVersionString").or_else(|| s("CFBundleVersion")).unwrap_or_default(),
        min_os: s("MinimumOSVersion").unwrap_or_default(),
        platform: if is_tv { Platform::Tvos } else { Platform::Ios },
        extensions,
        size_bytes,
        restricted_devices: plist.contains_key("UISupportedDevices"),
    })
}

pub fn human_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1024.0 { format!("{:.1} GB", mb / 1024.0) } else { format!("{mb:.1} MB") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Builds a minimal IPA: `Payload/App.app/Info.plist` plus any extra entries.
    fn make_ipa(dir: &Path, info: plist::Dictionary, extra: &[&str]) -> std::path::PathBuf {
        let path = dir.join("test.ipa");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        let mut xml = Vec::new();
        plist::to_writer_xml(&mut xml, &info).unwrap();
        zip.start_file("Payload/App.app/Info.plist", opts).unwrap();
        zip.write_all(&xml).unwrap();
        for name in extra {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn info(pairs: &[(&str, plist::Value)]) -> plist::Dictionary {
        let mut d = plist::Dictionary::new();
        for (k, v) in pairs {
            d.insert((*k).into(), v.clone());
        }
        d
    }

    #[test]
    fn reads_an_ios_app() {
        let dir = tempfile::tempdir().unwrap();
        let path = make_ipa(
            dir.path(),
            info(&[
                ("CFBundleIdentifier", "com.example.app".into()),
                ("CFBundleDisplayName", "Example".into()),
                ("CFBundleShortVersionString", "2.1".into()),
                ("MinimumOSVersion", "15.0".into()),
                ("UIDeviceFamily", plist::Value::Array(vec![1.into(), 2.into()])),
            ]),
            &[],
        );
        let got = inspect(&path).unwrap();
        assert_eq!(got.name, "Example");
        assert_eq!(got.bundle_id, "com.example.app");
        assert_eq!(got.version, "2.1");
        assert_eq!(got.min_os, "15.0");
        assert_eq!(got.platform, Platform::Ios);
        assert_eq!(got.extensions, 0);
        assert!(!got.restricted_devices);
        assert!(got.size_bytes > 0);
    }

    #[test]
    fn detects_tvos_from_supported_platforms() {
        let dir = tempfile::tempdir().unwrap();
        let path = make_ipa(
            dir.path(),
            info(&[
                ("CFBundleIdentifier", "com.example.tv".into()),
                ("CFBundleSupportedPlatforms", plist::Value::Array(vec!["AppleTVOS".into()])),
            ]),
            &[],
        );
        assert_eq!(inspect(&path).unwrap().platform, Platform::Tvos);
    }

    #[test]
    fn detects_tvos_from_device_family_three() {
        let dir = tempfile::tempdir().unwrap();
        let path = make_ipa(
            dir.path(),
            info(&[("CFBundleIdentifier", "x".into()), ("UIDeviceFamily", plist::Value::Array(vec![3.into()]))]),
            &[],
        );
        assert_eq!(inspect(&path).unwrap().platform, Platform::Tvos);
    }

    #[test]
    fn counts_extensions_and_flags_device_restrictions() {
        let dir = tempfile::tempdir().unwrap();
        let path = make_ipa(
            dir.path(),
            info(&[
                ("CFBundleIdentifier", "x".into()),
                ("UISupportedDevices", plist::Value::Array(vec!["iPhone14,2".into()])),
            ]),
            &[
                "Payload/App.app/PlugIns/One.appex/Info.plist",
                "Payload/App.app/PlugIns/Two.appex/Info.plist",
                // Not extensions: wrong depth or wrong suffix.
                "Payload/App.app/PlugIns/Two.appex/Deep/Info.plist",
                "Payload/App.app/Frameworks/Lib.framework/Info.plist",
            ],
        );
        let got = inspect(&path).unwrap();
        assert_eq!(got.extensions, 2);
        assert!(got.restricted_devices);
    }

    #[test]
    fn falls_back_to_bundle_name_and_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let named = make_ipa(dir.path(), info(&[("CFBundleName", "Short".into())]), &[]);
        assert_eq!(inspect(&named).unwrap().name, "Short");

        let dir2 = tempfile::tempdir().unwrap();
        let bare = make_ipa(dir2.path(), info(&[]), &[]);
        assert_eq!(inspect(&bare).unwrap().name, "Unknown app");
    }

    #[test]
    fn rejects_non_zip_missing_and_payloadless_files() {
        let dir = tempfile::tempdir().unwrap();
        let junk = dir.path().join("junk.ipa");
        std::fs::write(&junk, b"definitely not a zip").unwrap();
        assert!(inspect(&junk).unwrap_err().to_string().contains("valid IPA"));

        assert!(inspect(&dir.path().join("missing.ipa")).is_err());

        let empty = dir.path().join("empty.ipa");
        let mut zip = zip::ZipWriter::new(File::create(&empty).unwrap());
        zip.start_file("README.txt", zip::write::SimpleFileOptions::default()).unwrap();
        zip.finish().unwrap();
        assert!(inspect(&empty).unwrap_err().to_string().contains("Info.plist"));
    }

    #[test]
    fn human_size_scales_units() {
        assert_eq!(human_size(0), "0.0 MB");
        assert_eq!(human_size(5 * 1_048_576), "5.0 MB");
        assert_eq!(human_size(3 * 1_073_741_824), "3.0 GB");
    }
}
