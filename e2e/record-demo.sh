#!/usr/bin/env bash
# Record a product demo of DataZen using WebDriver screenshot frames,
# then assemble into both APNG and MP4.
#
# Usage:
#   bash e2e/record-demo.sh              # reuse existing webdriver build (default)
#   bash e2e/record-demo.sh --build      # build + record
#
# Output (e2e/screenshots/demo/):
#   demo-recording.png  (animated PNG)
#   demo-recording.mp4  (H.264, QuickTime-compatible)
#   frames/*.png        (raw frames)
# Sync to site separately: node scripts/sync-e2e-assets.mjs --demo
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_PNG="${ROOT}/e2e/screenshots/demo/demo-recording.png"
OUT_MP4="${ROOT}/e2e/screenshots/demo/demo-recording.mp4"
FRAMES="${ROOT}/e2e/screenshots/demo/frames"

cd "$ROOT"

echo "=== DataZen Demo Recorder (WebDriver + ffmpeg) ==="

SKIP_BUILD=1
DO_BUILD=0
for arg in "$@"; do
  [[ "$arg" == "--build" ]] && DO_BUILD=1
  [[ "$arg" == "--skip-build" ]] && SKIP_BUILD=1
done
if [[ "$DO_BUILD" -eq 1 ]]; then SKIP_BUILD=0; fi

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  echo "[1/4] Building app with webdriver feature..."
  node scripts/generate-menu-labels.mjs
  node scripts/with-driver-inject.mjs -- node scripts/e2e-tauri-build.mjs
else
  echo "[1/4] Skipping build (using existing webdriver build)"
fi

echo "[2/4] Seeding demo databases..."
bash "${SCRIPT_DIR}/setup-demo-data.sh" 2>/dev/null \
  || echo "  (demo data setup skipped or failed)"

rm -rf "${ROOT}/e2e/screenshots/demo"
mkdir -p "$FRAMES"
pkill -f "target/debug/datazen" 2>/dev/null || true
pkill -f "DataZen.app" 2>/dev/null || true
sleep 1

echo "[3/4] Driving the demo flow + capturing WebDriver frames..."
node e2e/run.mjs --skip-build --spec demo-recording.ts || true

echo "[4/4] Assembling output..."

# Always produce APNG
node e2e/assemble-apng.mjs --dir e2e/screenshots/demo/frames --out "$OUT_PNG"

# Produce MP4 if ffmpeg is available
if command -v ffmpeg &>/dev/null; then
  node e2e/assemble-apng.mjs \
    --dir e2e/screenshots/demo/frames \
    --concat "${FRAMES}/ffconcat.txt" \
    --concat-only
  ffmpeg -y \
    -f concat -safe 0 -i "${FRAMES}/ffconcat.txt" \
    -c:v libx264 -preset slow -crf 18 \
    -pix_fmt yuv420p -movflags +faststart \
    "$OUT_MP4" 2>/dev/null
  echo "  MP4: ${OUT_MP4}"
else
  echo "  [info] ffmpeg not found — skipping MP4. Install ffmpeg for MP4 output."
  echo "  To convert manually:"
  echo "    node e2e/assemble-apng.mjs --dir e2e/screenshots/demo/frames --concat ${FRAMES}/ffconcat.txt --concat-only"
  echo "    ffmpeg -f concat -safe 0 -i ${FRAMES}/ffconcat.txt -c:v libx264 -pix_fmt yuv420p -movflags +faststart e2e/screenshots/demo/demo-recording.mp4"
fi

echo ""
echo "Done."
echo "  APNG: ${OUT_PNG}"
[[ -f "$OUT_MP4" ]] && echo "  MP4:  ${OUT_MP4}"
echo ""

# Sync to site (separate step): node scripts/sync-e2e-assets.mjs --demo
