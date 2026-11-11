#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="wasm32-unknown-unknown"
OUTPUT="$ROOT/packages/web/public/rune-ui"

if [[ ! -f "$ROOT/vendor/rune/scripts/web-build.sh" ]]; then
  echo "Rune submodule is missing. Run: git submodule update --init --recursive vendor/rune" >&2
  exit 2
fi

if ! rustup target list --installed | grep -Fxq "$TARGET"; then
  echo "Missing Rust target. Run: rustup target add $TARGET" >&2
  exit 2
fi

cd "$ROOT"

# Resolve the workspace lock before Rune's script checks the matching CLI version.
cargo check -p datazen-ui --target "$TARGET" --lib

# Reuse Rune's own packager: it handles wasm-bindgen output, embedded web fonts,
# generated helper assets and the lockfile-matched wasm-bindgen CLI.
bash vendor/rune/scripts/web-build.sh datazen-ui --out "$OUTPUT" --no-html

if [[ ! -f "$OUTPUT/app.js" || ! -f "$OUTPUT/app_bg.wasm" ]]; then
  echo "Rune's WebAssembly packager did not produce app.js and app_bg.wasm under $OUTPUT" >&2
  exit 1
fi

echo "Rune WebAssembly bundle written to packages/web/public/rune-ui/"
