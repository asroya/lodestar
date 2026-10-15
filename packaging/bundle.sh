#!/bin/zsh
# Builds dist/Lodestar.app for the machine you run it on (release profile).
#
#   ./packaging/bundle.sh          build and sign
#   ./packaging/bundle.sh --zip    also write dist/Lodestar-<version>-<arch>.zip
#
# Signing: uses your first Apple Development / Developer ID identity, or the one
# in LODESTAR_SIGN_IDENTITY (a SHA-1 or name; "-" means ad-hoc, no identity).
set -euo pipefail
cd "$(dirname "$0")/.."
source ~/.cargo/env 2>/dev/null || true
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

if [[ "${1:-}" == "--zip" ]]; then
  ZIP="$PWD/dist/Lodestar-$VERSION-$(uname -m).zip"
  rm -f "$ZIP"
  ditto -c -k --norsrc --noextattr --noqtn --keepParent "$APP" "$ZIP"
  echo "Wrote $ZIP"
fi
