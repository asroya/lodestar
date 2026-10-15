# Architecture

Lodestar is a single Rust binary (about 4,000 lines in `src/`) with an immediate-mode
UI (`egui`) on the main thread and a `tokio` engine on its own thread. They share
one `Arc<Mutex<Shared>>` and talk through two channels: commands go to the engine,
answers to its prompts come back.

```
┌──────────────── UI thread (eframe/egui) ────────────────┐       ┌──────── engine thread (tokio) ────────┐
│ app.rs    pages, rail, modal, toast                      │       │ engine.rs   command loop, sessions     │
│ theme.rs  palette, type, controls, motion                │ ────► │ pipeline.rs sign → provision → install │
│ reads Shared every frame, sends Cmd                      │ Cmd   │ device.rs   usbmuxd + Bonjour + tunnel │
│                                                          │ ◄──── │ store.rs    settings, library, keychain│
└──────────────────────────────────────────────────────────┘ Shared└────────────────────────────────────────┘
```

## Modules

| File | Responsibility |
|---|---|
| `main.rs` | Installs the TLS provider, wires the in-app log, opens the window. |
| `engine.rs` | Owns the Apple developer session. Runs one command at a time (`SignIn`, `Install`, `RefreshAll`, `Pair`…). Exposes `Shared` state and a `Prompter` that lets background code ask the UI for a 2FA code or Apple TV PIN and await the answer. |
| `pipeline.rs` | The sideload itself: connect → register device → certificate → unpack → App IDs → profiles → sign → upload → install. |
| `device.rs` | Finds devices (USB scanner, Bonjour browser) and opens a CoreDevice tunnel to a network device. |
| `ipa.rs` | Reads an IPA's `Info.plist` straight from the zip, without extracting it. |
| `store.rs` | `settings.json`, `library.json`, Keychain access, formatting helpers. |
| `app.rs` | Every page and dialog. Holds no business logic beyond view state. |
| `theme.rs` | Colours, fonts, controls and the motion layer. |

## An install, step by step

1. **Connect.** A USB or Wi-Fi-sync device is reached through macOS's own
   `usbmuxd` using `idevice`. A network-only Apple TV goes through
   `open_tunnel` (below). Either way we learn the UDID, name and platform.
2. **Register the device** with your developer team. Apple's developer service has
   one endpoint tree (`…/QH65B2/ios/…`); for an Apple TV the request body carries
   `subPlatform: tvOS` instead of using a separate path (a `/tvos/` path returns 404).
3. **Signing identity.** Reuse the certificate whose private key is in the Keychain,
   or ask Apple for a new one. If Apple answers *certificate limit reached* (7460),
   `revoke_candidates` ranks what can be revoked (our own first; never revoked or
   expired ones) and `free_certificate_slot` tries the classic endpoint, then the
   v1 API Xcode uses.
4. **Unpack** the IPA to a temp directory guarded by `TempExtract`, which deletes it
   on success *or* failure.
5. **Customise** `Info.plist` (display name, restrictions, file sharing) and set the
   bundle ID to `<original>.<TEAMID>` unless you chose one.
6. **App IDs, app group and profiles.** Register an App ID per bundle (watching the
   weekly quota), attach an app group, download the provisioning profiles.
7. **Sign** with `apple-codesign-quick`, embedding the profiles and entitlements.
8. **Upload and install**: AFC to `PublicStaging/`, then `installation_proxy`
   with `PackageType: Developer`, reporting progress back to the UI.
9. **Remember it.** The IPA is copied into the library and the profile's expiry date
   drives the countdown.

## Apple TV on tvOS 17+

There is no USB data path to a modern Apple TV. Xcode reaches it over the network,
and so does Lodestar (`device.rs`):

1. **Discover** the TV over Bonjour (`_remotepairing._tcp`,
   `_remotepairing-manual-pairing._tcp`).
2. **Pair** once with the PIN on screen (SRP "Pair-Setup", then Ed25519/X25519
   "Pair-Verify"); the resulting key is stored under `…/Lodestar/pairings`.
3. **Tunnel.** Ask the TV to open a listener, connect with TLS-PSK keyed from the
   pairing, run the CDTunnel handshake, and run a small userspace TCP stack over the
   tunnel (`jktcp`).
4. **Resolve services** with an RSD handshake, then use the same AFC and
   installation-proxy clients as for USB, now over the tunnel.

A TV that is already paired with the Mac (for example through Xcode) shows up through
`usbmuxd` as a Wi-Fi-sync device and skips all of this. That is the path verified so
far on hardware.

## Persistence and secrets

* `~/Library/Application Support/Lodestar/`: `settings.json`, `library.json`,
  `library/*.ipa`, `pairings/*.plist`. Written atomically (temp file, then rename).
* Keychain: service `Lodestar` holds the optional password and the signing key; service
  `isideload` holds the anisette state.
* Nothing else is stored, and nothing is sent anywhere except Apple, the anisette
  server and your own devices.

## UI and motion

`theme.rs` defines the palette (`LIGHT`/`DARK`), typography (IBM Plex through named
font families), and controls (`primary`, `secondary`, `link`, `checkbox`, `section`).
Motion helpers live beside them: `tween`/`tween_to` ease values, `reveal` staggers
the blocks of a page, `bar_live` animates progress. Every duration goes through
`dur()`, which is zero under *Reduce Motion* and multiplied by
`LODESTAR_MOTION_SLOWMO` for design review.

## Testing

`cargo test` covers everything that doesn't need Apple or a device: IPA parsing,
certificate ranking, settings migration, persistence helpers, motion maths and
palette contrast. Anything that talks to Apple or hardware is checked by hand;
`testapp/build.sh` builds a tiny tvOS app for that.
