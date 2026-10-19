# Changelog

All notable changes to Lodestar are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [1.1.0] - Unreleased

Full notes: [docs/releases/v1.1.0.md](docs/releases/v1.1.0.md).

### Added
- A one-time note after your first successful install saying Lodestar is free, with an
  optional **Buy me a coffee** link. It never appears again, never covers a sign-in or
  pairing prompt, and the link stays at the bottom of the sidebar. Opening it hands one
  fixed URL to your browser; Lodestar itself makes no request.
- A website (`site/`, published to GitHub Pages) with a quick-start guide, screenshots,
  live versions of the app's controls, a one-click download and a support section whose
  payment links live in `site/config.js`.
- Releases now include a disk image, `Lodestar-arm64.dmg`, alongside the zip and a
  `SHA256SUMS.txt`. `packaging/bundle.sh --dmg` builds it locally.
- A release workflow that builds and attaches the zip and disk image when a `v*` tag is
  pushed, and a website workflow that checks the site in a browser before publishing.

### Changed
- First-launch instructions now describe macOS 15's **Open Anyway** step; right-click
  › Open only works on macOS 13 and 14.

## [1.0.0] - 2026-10-15

First public release. Full notes: [docs/releases/v1.0.0.md](docs/releases/v1.0.0.md).

### Added
- Sign any `.ipa` or tvOS `.tipa` with your own Apple ID and install it on an
  iPhone, iPad or Apple TV 4K.
- Apple ID sign-in with two-factor (trusted-device code, SMS), optional Keychain storage.
- Device discovery over `usbmuxd` (USB and Wi-Fi sync) and Bonjour (Apple TV).
- First-time Apple TV pairing with the on-screen PIN, and installation over an
  encrypted CoreDevice tunnel.
- Per-install options: display name, bundle ID, remove device restrictions, enable
  file sharing, remove app extensions.
- **My Apps**: install history with a 7-day countdown, **Refresh** and **Refresh all**,
  and a "due" badge two days before expiry.
- Automatic handling of Apple's development-certificate limit.
- Light and dark appearance that follows macOS, IBM Plex type, and motion that
  respects *Reduce Motion* (`LODESTAR_MOTION_SLOWMO` slows it for design review).
- Custom anisette server via `LODESTAR_ANISETTE_URL` or `anisette_url` in settings.
- `packaging/bundle.sh` (with `--zip`) to build, sign and package `Lodestar.app`.
- 37 unit tests, CI (format, clippy, tests), architecture and user guides.

### Fixed (during pre-release testing)
- Apple TV installs failed with a 404 because tvOS requests used a `/tvos/` path that
  Apple's service doesn't have; they now use the `ios` endpoint with a tvOS platform flag.
- Installs stopped at Apple's certificate limit because the wrong certificate was
  chosen for revocation; candidates are now ranked and revoked or expired ones skipped.
- A mistyped password was saved to the Keychain before sign-in succeeded and retried
  silently on every launch; credentials are now saved only after Apple accepts them.
- A settings file missing any newer field reset every setting, including the saved
  Apple ID; settings now fall back field by field.
- The unpacked app was left in the temp folder when an install failed partway.
- The app crashed in the background on launch when "Remember me" was on, because no
  TLS provider had been installed before the first network request.
- Release binaries embedded absolute paths from the build machine (home folder, username,
  disk name). Builds now remap them, and `packaging/check.sh` fails if any remain.
- **Refresh all** reinstalled apps but never recorded the new expiry, so countdowns stayed
  stale and the library wasn't saved (found by Cursor Bugbot review). It now uses the same
  path as a single install.
- A Bonjour service going away could clear a different Apple TV's ports (matching was by name
  prefix); removals now match the exact service instance. (Bugbot)
- **Remove** in My Apps stayed clickable during a running job, which could delete the IPA a
  refresh was reading; it is disabled while a job runs. (Bugbot)
- Refresh showed the stored file name (which contains your team ID and device UDID)
  instead of the app's name; jobs are now titled "Installing …" or "Refreshing …".
- The install panel looked frozen while macOS waited for Keychain approval; it now
  says what to do.
- The Devices page had an "Add by IP address" control that did nothing; it was
  removed in favour of network troubleshooting guidance.

### Known limitations
- No background auto-refresh, JIT enabling, or tweak/dylib injection.
- macOS only. Tested on Apple silicon; Intel and macOS 13 to 26 are untested.
- Hardware-verified: iPhone over USB and an already-paired Apple TV 4K (Wi-Fi sync).
  Not yet verified: first-time PIN pairing, the network tunnel, SMS two-factor, and
  refresh as a separate action.

[1.0.0]: https://github.com/asroya/lodestar/releases/tag/v1.0.0
