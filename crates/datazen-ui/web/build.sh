#!/usr/bin/env bash
# Build the standalone wasm page (issue-rune H7): the datazen-ui wasm
# package plus its wasm-bindgen glue into this directory's gen/. Serve the
# directory with any static file server (the React host is not involved):
#
#   crates/datazen-ui/web/build.sh
#   python3 -m http.server -d crates/datazen-ui/web 8137
#
# Needs the wasm32-unknown-unknown target and the wasm-bindgen CLI at the
# version the workspace pins (0.2.129).
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$root"

cargo build -p datazen-ui --target wasm32-unknown-unknown

rm -rf crates/datazen-ui/web/gen
mkdir -p crates/datazen-ui/web/gen
wasm-bindgen --target web --out-dir crates/datazen-ui/web/gen \
    target/wasm32-unknown-unknown/debug/datazen_ui.wasm

echo "page ready: crates/datazen-ui/web/index.html"
