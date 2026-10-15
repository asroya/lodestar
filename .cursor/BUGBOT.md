# Review guidance for Lodestar

Lodestar is a macOS sideloader in Rust (egui UI, tokio engine). It handles an Apple ID, a
signing key and a code-signing pipeline, and it installs software on people's devices.
Please review with that in mind.

## Highest priority

1. **Credentials and secrets** (`src/store.rs`, `src/engine.rs`)
   - The Apple ID password must never be logged, written to a file, put in an error
     message or sent anywhere except Apple's sign-in (via `isideload`).
   - It may be saved to the Keychain only after Apple accepts it, and only if the user
     ticked *Remember me*.
2. **Paths and files** (`src/device.rs`, `src/pipeline.rs`, `src/ipa.rs`)
   - Anything derived from an IPA, a bundle ID or a Bonjour host name must not be able
     to write outside Lodestar's folders (`pairing_file_name` sanitises host names).
   - Temp directories created while unpacking must always be removed (`TempExtract`).
3. **Certificate revocation** (`revoke_candidates`, `free_certificate_slot` in `src/pipeline.rs`)
   - Revoking the wrong certificate silently breaks other apps. Check ranking, the skip list
     and that revoked or expired certificates are never chosen.
4. **Threading** (`src/engine.rs`, `src/app.rs`)
   - The UI thread must never block on network, Keychain or device I/O. Flag `.await`
     while holding the `Shared` mutex, and any lock held across a slow call.
5. **Unsafe claims in docs.** The README distinguishes *verified on hardware* from *not yet
   verified*. Flag any change that presents an unverified path as verified, or states a
   fact about Apple's services, licenses or Sideloadly that the code or tests don't support.

## Also useful

- Logic errors in expiry/countdown maths, device-key matching and library persistence.
- Panics from `unwrap()` on user-influenced data (IPA contents, plists, network input).
- New dependencies: check their licenses against `THIRD_PARTY_NOTICES.md`. The project is
  MIT; one existing dependency, `apple-codesign-quick`, is LGPL-2.1-or-later.
- Motion code: every duration must go through `theme::dur` so *Reduce Motion* works.

## Don't flag

- Long lines or formatting. `cargo fmt` is the authority (`rustfmt.toml`).
- Style preferences already settled in `docs/ARCHITECTURE.md`.
- The absence of tests for code that talks to Apple or to a physical device; those paths are
  verified by hand and documented as such.

## How a good finding reads

Name the file and line, the concrete input or state that triggers it, and the wrong result.
Prefer a few well-evidenced findings over many speculative ones.
