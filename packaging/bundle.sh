#!/bin/zsh
# Builds dist/Lodestar.app for the machine you run it on (release profile).
#
#   ./packaging/bundle.sh                build and sign
#   ./packaging/bundle.sh --zip          also write dist/Lodestar-<version>-<arch>.zip
#   ./packaging/bundle.sh --dmg          also write dist/Lodestar-<arch>.dmg
#   ./packaging/bundle.sh --zip --dmg    both (what a release ships)
#
# Signing: uses your first Apple Development / Developer ID identity, or the one
# in LODESTAR_SIGN_IDENTITY (a SHA-1 or name; "-" means ad-hoc, no identity).
set -euo pipefail
cd "$(dirname "$0")/.."
want_zip=0 want_dmg=0
for arg in "$@"; do
  case "$arg" in
    --zip) want_zip=1 ;;
    --dmg) want_dmg=1 ;;
    *) echo "unknown option: $arg (expected --zip and/or --dmg)" >&2; exit 2 ;;
  esac
done
source ~/.cargo/env 2>/dev/null || true
# Rust embeds absolute source paths (panic locations, dependency paths) in the binary.
# Remap this machine's home folder and the project folder so a distributed build doesn't
# reveal the builder's username or disk layout.
#  - Both the logical path ($PWD) and the real one (${PWD:A}, symlinks resolved) are
#    mapped: rustc embeds the real one, and e.g. /tmp is a symlink to /private/tmp.
#  - CARGO_ENCODED_RUSTFLAGS (unit-separator delimited) is used instead of RUSTFLAGS,
#    which is split on spaces and would break on a path like "/Users/Jane Doe".
#  - The last matching mapping wins, so the project folder comes after the home folder.
typeset -U home_roots cargo_roots proj_roots
home_roots=("$HOME" "${HOME:A}")
cargo_roots=()
[[ -n "${CARGO_HOME:-}" ]] && cargo_roots=("$CARGO_HOME" "${CARGO_HOME:A}")
proj_roots=("$PWD" "${PWD:A}")
flags=(${=RUSTFLAGS:-})
for r in $home_roots $cargo_roots; do flags+=("--remap-path-prefix=$r=~"); done
for r in $proj_roots; do flags+=("--remap-path-prefix=$r=."); done
export CARGO_ENCODED_RUSTFLAGS="${(pj:\x1f:)flags}"
unset RUSTFLAGS
cargo build --release
APP="$PWD/dist/Lodestar.app"
rm -rf "$APP"; mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/lodestar "$APP/Contents/MacOS/Lodestar"
ICONSET="$(mktemp -d)/Lodestar.iconset"; mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  swift packaging/render_icon.swift packaging/icon.svg "$ICONSET/icon_${s}x${s}.png" $s
  swift packaging/render_icon.swift packaging/icon.svg "$ICONSET/icon_${s}x${s}@2x.png" $((s*2))
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Lodestar.icns"
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Lodestar</string>
  <key>CFBundleDisplayName</key><string>Lodestar</string>
  <key>CFBundleIdentifier</key><string>dev.lodestar.app</string>
  <key>CFBundleExecutable</key><string>Lodestar</string>
  <key>CFBundleIconFile</key><string>Lodestar</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSLocalNetworkUsageDescription</key><string>Lodestar finds and installs apps on your Apple TV over the local network.</string>
  <key>NSBonjourServices</key><array><string>_remotepairing._tcp</string><string>_remotepairing-manual-pairing._tcp</string></array>
  <key>CFBundleDocumentTypes</key><array><dict>
    <key>CFBundleTypeName</key><string>iOS App</string>
    <key>CFBundleTypeRole</key><string>Viewer</string>
    <key>CFBundleTypeExtensions</key><array><string>ipa</string><string>tipa</string></array>
    <key>LSHandlerRank</key><string>Alternate</string>
  </dict></array>
</dict></plist>
PLIST
# A stable signing identity keeps keychain access (saved password, signing
# key) valid across rebuilds; ad-hoc signatures change on every build.
# Select by SHA-1 so duplicate certificate names can't make it ambiguous.
IDENTITY="${LODESTAR_SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null | awk '/Apple Development|Developer ID Application/ {print $2; exit}')}"
if [ -n "$IDENTITY" ]; then
  echo "Signing with: $IDENTITY"
  codesign --force --deep --options runtime --sign "$IDENTITY" "$APP"
else
  echo "No signing identity found; using ad-hoc signature"
  codesign --force --deep --sign - "$APP"
fi
echo "Built $APP"

if (( want_zip )); then
  ZIP="$PWD/dist/Lodestar-$VERSION-$(uname -m).zip"
  rm -f "$ZIP"
  ditto -c -k --norsrc --noextattr --noqtn --keepParent "$APP" "$ZIP"
  echo "Wrote $ZIP"
fi

# A disk image people open and drag from: the app beside a shortcut to Applications.
# The file name has no version so releases/latest/download/Lodestar-arm64.dmg always
# points at the newest one; the volume name carries the version instead.
if (( want_dmg )); then
  DMG="$PWD/dist/Lodestar-$(uname -m).dmg"
  STAGE="$(mktemp -d)/Lodestar"
  mkdir -p "$STAGE"
  ditto --norsrc --noextattr --noqtn "$APP" "$STAGE/Lodestar.app"
  ln -s /Applications "$STAGE/Applications"
  rm -f "$DMG"
  hdiutil create -volname "Lodestar $VERSION" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG" >/dev/null
  rm -rf "$(dirname "$STAGE")"
  echo "Wrote $DMG"
fi
