#!/usr/bin/env bash
# Record a product demo of DataZen using ONLY WebDriver capabilities:
#   - frames come from the W3C screenshot endpoint (in-webview render)
#   - no ffmpeg, no macOS Screen Recording permission, no window capture
#   - frames are muxed into an animated PNG by e2e/assemble-apng.mjs (pure Node)
#
# Usage:
#   bash e2e/record-demo.sh              # full build + record
#   bash e2e/record-demo.sh --skip-build # reuse existing webdriver build
#
# Output: e2e/demo-recording.png (animated; plays in browsers/Safari/Preview)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT="${ROOT}/e2e/demo-recording.png"
FRAMES="${ROOT}/e2e/.demo-recording"

cd "$ROOT"

echo "=== DataZen Demo Recorder (WebDriver-only) ==="

SKIP_BUILD=0
for arg in "$@"; do [[ "$arg" == "--skip-build" ]] && SKIP_BUILD=1; done

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  echo "[1/3] Building app with webdriver feature..."
  node scripts/generate-menu-labels.mjs
  node scripts/with-plugin-inject.mjs -- node scripts/e2e-tauri-build.mjs
else
  echo "[1/3] Skipping build (--skip-build)"
fi

echo "[2/3] Seeding demo databases..."
bash "${SCRIPT_DIR}/setup-demo-data.sh" 2>/dev/null \
  || echo "  (demo data setup skipped or failed)"

rm -rf "$FRAMES"
pkill -f "target/debug/datazen" 2>/dev/null || true
pkill -f "DataZen.app" 2>/dev/null || true
sleep 1

echo "[3/3] Driving the demo flow + capturing WebDriver frames..."
node e2e/run.mjs --skip-build --spec demo-recording.ts || true

node e2e/assemble-apng.mjs --dir e2e/.demo-recording --out "$OUT"

echo ""
echo "Done. Open ${OUT} in a browser to view the demo."
echo ""
echo "Optional mp4 (QuickTime-compatible, true pacing preserved):"
echo "  node e2e/assemble-apng.mjs --dir e2e/.demo-recording --concat ${FRAMES}/ffconcat.txt --concat-only"
echo "  ffmpeg -f concat -safe 0 -i ${FRAMES}/ffconcat.txt -c:v libx264 -pix_fmt yuv420p -movflags +faststart e2e/demo-recording.mp4"
