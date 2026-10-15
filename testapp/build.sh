#!/bin/zsh
# Builds a tiny tvOS test app (testapp/build/LodestarHello.tipa) for exercising
# Lodestar without downloading anything. Needs Xcode with the tvOS SDK.
set -euo pipefail
cd "$(dirname "$0")"
OUT="$PWD/build"; APP="$OUT/LodestarHello.app"
rm -rf "$OUT"; mkdir -p "$APP" "$OUT/Payload"
SDK=$(xcrun --sdk appletvos --show-sdk-path)
xcrun --sdk appletvos swiftc -target arm64-apple-tvos17.0 -sdk "$SDK" -parse-as-library -O main.swift -o "$APP/LodestarHello"
cp Info.plist "$APP/Info.plist"
cp -R "$APP" "$OUT/Payload/"
(cd "$OUT" && zip -qr LodestarHello.tipa Payload && rm -rf Payload)
echo "Built $OUT/LodestarHello.tipa"
