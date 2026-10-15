# Security policy

Lodestar handles an Apple ID, a signing key and a code-signing pipeline, so security
reports are welcome and taken seriously.

## Reporting a vulnerability

Please **don't open a public issue**. Report it privately through GitHub:
**Security › Report a vulnerability** on this repository
(<https://github.com/asroya/lodestar/security/advisories/new>).

Include what you found, how to reproduce it, and the version you tested. You can
expect an acknowledgement within a few days. This is a volunteer project, so there is
no formal SLA, but credible reports are prioritised and you'll be credited in the
release notes unless you'd rather not be.

## What's in scope

* Anything that could expose an Apple ID, password, signing key or pairing record.
* Writing files outside Lodestar's own folders (path traversal in IPAs or host names).
* Signing or installing something other than what the user selected.
* Unintended network traffic.

## How Lodestar protects your data

* The password is sent only to Apple, using SRP, and is stored (in the macOS Keychain)
  only if you tick **Remember me** and only after Apple accepts it.
* The signing key and anisette state are in the Keychain, not in files.
* Apple TV pairing records are under `~/Library/Application Support/Lodestar/pairings`,
  one file per device, named from a sanitised host name.
* Temporary unpacked apps are deleted when an install ends, successfully or not.
* There is no telemetry, update check or analytics.

## What to be aware of

* The default **anisette server** is a community-run service. It sees your IP address
  and the device-provisioning exchange, but not your password. Self-host it with
  `LODESTAR_ANISETTE_URL` if that matters to you.
* Release builds are **not notarized** and may be ad-hoc signed. Build from source if
  you'd rather not trust a binary.
* A sideloaded app runs with the permissions its provisioning profile grants. Only
  install software you trust.
