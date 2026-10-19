# Third-party notices

Lodestar's own source code is MIT licensed (see [LICENSE](LICENSE)). It is built
on the open-source projects below. The full dependency graph (about 680 crates,
nearly all MIT and/or Apache-2.0) is recorded in `Cargo.lock`; list every
license with:

```bash
cargo metadata --format-version 1 | python3 -c "import sys,json,collections as c; \
d=json.load(sys.stdin); print(*c.Counter(p.get('license') for p in d['packages']).most_common(), sep='\n')"
```

## A component under the LGPL

**`apple-codesign-quick`** (<https://github.com/Dadoum/apple-crates>, by Dadoum) is
licensed **LGPL-2.1-or-later**. It performs the code signing and is statically
linked into the Lodestar binary. In practice that means:

* The crate is used **unmodified**, exactly as published on crates.io.
* Its source is available at the repository above and from crates.io.
* You can replace it with your own modified version: point the dependency in
  `Cargo.toml` at your copy (for example with a `[patch.crates-io]` entry) and
  run `./packaging/bundle.sh`. Lodestar's complete source is in this repository,
  which is what makes that relinking possible.
* Lodestar's other code stays under the MIT license; the LGPL applies to the
  library, not to the rest of the program.

This section is a good-faith summary, not legal advice. If you redistribute
modified binaries, read the LGPL-2.1 text for your situation.

## Libraries Lodestar is built around

| Project | Role | License |
|---|---|---|
| [`isideload`](https://github.com/nab138/isideload) (nab138) | Apple ID sign-in (GrandSlam/SRP), developer-portal calls, certificate handling | MIT |
| [`idevice`](https://github.com/jkcoxson/idevice) (jkcoxson) | usbmuxd, lockdown, AFC, installation proxy, RemotePairing, CoreDevice tunnel | MIT |
| [`apple-codesign-quick`](https://github.com/Dadoum/apple-crates) (Dadoum) | Bundle code signing | **LGPL-2.1-or-later** |
| [`egui` / `eframe`](https://github.com/emilk/egui) (Emil Ernerfeldt, Rerun) | Native user interface | MIT OR Apache-2.0 |
| [`mdns-sd`](https://github.com/keepsimple1/mdns-sd) | Bonjour discovery of Apple TVs | MIT OR Apache-2.0 |
| [`rfd`](https://github.com/PolyMeilex/rfd) | Native file picker | MIT |
| [`keyring`](https://github.com/hwchen/keyring-rs) | macOS Keychain access | MIT OR Apache-2.0 |
| [`tokio`](https://github.com/tokio-rs/tokio), [`rustls`](https://github.com/rustls/rustls), [`reqwest`](https://github.com/seanmonstar/reqwest) | Async runtime and networking | MIT / Apache-2.0 |

`isideload` itself credits [Sideloader](https://github.com/Dadoum/Sideloader) by
Dadoum and [Impactor](https://github.com/khcrysalis/Impactor) as references for
Apple's private developer endpoints and certificate handling. Lodestar inherits
that debt gratefully.

Lodestar does not contain code from Sideloadly or any other closed-source tool.

## Fonts

**IBM Plex Sans** and **IBM Plex Mono**, Copyright © 2017 IBM Corp., are
distributed under the SIL Open Font License 1.1. The license text is in
[`assets/fonts/OFL.txt`](assets/fonts/OFL.txt). The font files are compiled into
the application, and Latin subsets of them (WOFF2, in `site/fonts/` with a copy of the
license) are served by the website.

## Trademarks

Apple, iPhone, iPad, Apple TV, tvOS, iOS, macOS, Xcode and Keychain are
trademarks of Apple Inc. Sideloadly is a trademark of its respective owner.
Lodestar is an independent project, not affiliated with or endorsed by Apple or
by the Sideloadly project.
