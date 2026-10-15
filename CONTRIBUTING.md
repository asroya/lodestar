# Contributing to Lodestar

Thanks for helping. Lodestar is small enough that one person can hold all of it in
their head, and the goal is to keep it that way: a single window, no settings
sprawl, everything free.

## Set up

You need a Mac, Rust 1.95 or newer (`rustup update stable`) and Xcode's command line
tools.

```bash
git clone https://github.com/asroya/lodestar.git && cd lodestar
cargo run                    # debug build, opens the window
./packaging/bundle.sh        # signed dist/Lodestar.app (release)
```

`bundle.sh` signs with your Apple Development identity if you have one, which keeps
Keychain access working across rebuilds. Without one it falls back to an ad-hoc
signature, and macOS will ask for Keychain access again after each rebuild.

## Before you open a pull request

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs the same three. Please add a test when you change behaviour that doesn't
need Apple or a device: IPA parsing, ranking logic, persistence and the like are
all easy to cover. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for where things live.

### Testing with a real device

Anything that talks to Apple or hardware has to be checked by hand. Build the tiny
tvOS fixture and install it:

```bash
./testapp/build.sh           # → testapp/build/LodestarHello.tipa
```

In your PR, say what you tested on (device, OS version, free or paid account) and
what you didn't. "Untested on hardware" is a perfectly good thing to write.

### Design changes

* Colours and type come from `theme.rs`; don't hard-code them in pages.
* Motion has to explain a state change. Route every duration through `theme::dur`
  so *Reduce Motion* works, and review it with `LODESTAR_MOTION_SLOWMO=10 cargo run`.
* Check both light and dark (`LODESTAR_THEME=light|dark`).

## Good first contributions

* Background refresh (a login item or LaunchAgent that refreshes apps nearing expiry).
* JIT enabling, or tweak/dylib injection.
* Verifying the first-time Apple TV PIN pairing and reporting findings.
* iPad, Intel and older-macOS testing.
* Notarized release builds.

## Reporting bugs

Open an issue with your macOS version, device and OS, free or paid Apple ID, what you
expected and what happened, plus the **Activity** log. **Remove your Apple ID, team
ID and device UDID from anything you paste.** Never share passwords or the contents of
your Keychain. For security problems use the process in [SECURITY.md](SECURITY.md)
instead of a public issue.

## Licensing of contributions

By contributing you agree your work is released under the project's [MIT license](LICENSE).
Please don't paste in code from closed-source tools or from projects whose licenses
are incompatible with MIT; if you add a dependency, say what its license is and update
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
