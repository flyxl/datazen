#!/usr/bin/env bash
# Record a product demo of DataZen using WebDriver screenshot frames,
# then assemble into both APNG and MP4.
#
# Usage:
#   bash e2e/record-demo.sh              # full build + record
#   bash e2e/record-demo.sh --skip-build # reuse existing webdriver build
#
# Output:
#   e2e/demo-recording.png  (animated PNG)
#   e2e/demo-recording.mp4  (H.264, QuickTime-compatible)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_PNG="${ROOT}/e2e/demo-recording.png"
OUT_MP4="${ROOT}/e2e/demo-recording.mp4"
FRAMES="${ROOT}/e2e/.demo-recording"

cd "$ROOT"

echo "=== DataZen Demo Recorder (WebDriver + ffmpeg) ==="

SKIP_BUILD=0
for arg in "$@"; do [[ "$arg" == "--skip-build" ]] && SKIP_BUILD=1; done

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  echo "[1/4] Building app with webdriver feature..."
  node scripts/generate-menu-labels.mjs
  node scripts/with-driver-inject.mjs -- node scripts/e2e-tauri-build.mjs
else
  echo "[1/4] Skipping build (--skip-build)"
fi

echo "[2/4] Seeding demo databases..."
bash "${SCRIPT_DIR}/setup-demo-data.sh" 2>/dev/null \
  || echo "  (demo data setup skipped or failed)"

rm -rf "$FRAMES"
pkill -f "target/debug/datazen" 2>/dev/null || true
pkill -f "DataZen.app" 2>/dev/null || true
sleep 1

echo "[3/4] Driving the demo flow + capturing WebDriver frames..."
node e2e/run.mjs --skip-build --spec demo-recording.ts || true

echo "[4/4] Assembling output..."

# Always produce APNG
node e2e/assemble-apng.mjs --dir e2e/.demo-recording --out "$OUT_PNG"

# Produce MP4 if ffmpeg is available
if command -v ffmpeg &>/dev/null; then
  node e2e/assemble-apng.mjs \
    --dir e2e/.demo-recording \
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
  echo "    node e2e/assemble-apng.mjs --dir e2e/.demo-recording --concat ${FRAMES}/ffconcat.txt --concat-only"
  echo "    ffmpeg -f concat -safe 0 -i ${FRAMES}/ffconcat.txt -c:v libx264 -pix_fmt yuv420p -movflags +faststart e2e/demo-recording.mp4"
fi

echo ""
echo "Done."
echo "  APNG: ${OUT_PNG}"
[[ -f "$OUT_MP4" ]] && echo "  MP4:  ${OUT_MP4}"
echo ""

# Copy MP4 to site assets for the website hero video
SITE_VIDEO="${ROOT}/site/assets/video"
if [[ -f "$OUT_MP4" && -d "$SITE_VIDEO" ]]; then
  cp "$OUT_MP4" "${SITE_VIDEO}/demo-recording.mp4"
  echo "  Copied to ${SITE_VIDEO}/demo-recording.mp4"
fi
