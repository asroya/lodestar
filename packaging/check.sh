#!/bin/zsh
# Runs locally everything CI runs, plus a launch smoke test, so a change can be
# verified without GitHub Actions:
#
#   ./packaging/check.sh
#
# Note: this rebuilds dist/Lodestar.app with an ad-hoc signature (no identity
# needed). If you use the app from dist/, run ./packaging/bundle.sh afterwards to
# sign it with your own identity again, otherwise macOS asks for Keychain access.
#
# To run it before every push:  ln -s ../../packaging/check.sh .git/hooks/pre-push
set -euo pipefail
cd "$(dirname "$0")/.."
source ~/.cargo/env 2>/dev/null || true

step() { printf '\n\033[1m▸ %s\033[0m\n' "$1"; }

step "Format"
cargo fmt --check

step "Clippy (warnings are errors)"
cargo clippy --all-targets -- -D warnings

step "Tests"
cargo test

step "Bundle (ad-hoc signed)"
LODESTAR_SIGN_IDENTITY=- ./packaging/bundle.sh --zip

step "Signature verifies"
codesign --verify --deep --strict dist/Lodestar.app

step "Release zip has no stray metadata files"
if unzip -l dist/Lodestar-*.zip | grep -q '\._'; then
  echo "found ._* files in the zip" >&2
  exit 1
fi

step "Binary reveals no build-machine paths"
# Check every form of every path the build could have embedded (logical and real).
leaked=0
for root in "$HOME" "${HOME:A}" "$PWD" "${PWD:A}" "${CARGO_HOME:-}" "${CARGO_HOME:+${CARGO_HOME:A}}"; do
  [[ ${#root} -lt 4 ]] && continue          # skip empty or trivially short values like "/"
  if strings -a dist/Lodestar.app/Contents/MacOS/Lodestar | grep -qF -- "$root"; then
    echo "the binary still contains: $root" >&2
    leaked=1
  fi
done
[[ $leaked -eq 0 ]] || exit 1

step "Binary launches and stays up (6 s, empty home folder)"
SMOKE_HOME="$(mktemp -d)"
HOME="$SMOKE_HOME" dist/Lodestar.app/Contents/MacOS/Lodestar >"$SMOKE_HOME/out.log" 2>&1 &
PID=$!
sleep 6
if ! kill -0 "$PID" 2>/dev/null; then
  echo "the app exited early:" >&2
  cat "$SMOKE_HOME/out.log" >&2
  exit 1
fi
kill "$PID" 2>/dev/null || true
if grep -qi panicked "$SMOKE_HOME/out.log"; then
  echo "the app panicked:" >&2
  cat "$SMOKE_HOME/out.log" >&2
  exit 1
fi
rm -rf "$SMOKE_HOME"

printf '\n\033[1;32m✔ All checks passed\033[0m\n'
