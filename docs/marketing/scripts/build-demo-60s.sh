#!/usr/bin/env bash
# Build ~60s demo MP4 from screenshots (no screen recording required).
set -euo pipefail
SHOTS="$(cd "$(dirname "$0")/../../screenshots" && pwd)"
OUT="$(cd "$(dirname "$0")/../video" && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

DUR=12  # seconds per slide, 5 slides = 60s
W=1280

i=0
for img in main-window.png new-connection.png connection-window.png query-editor.png redis-view.png; do
  ffmpeg -y -loop 1 -t "$DUR" -i "$SHOTS/$img" \
    -vf "scale=${W}:-2:flags=lanczos,format=yuv420p" \
    -r 30 -pix_fmt yuv420p "$TMP/part$(printf '%02d' $i).mp4" 2>/dev/null
  i=$((i + 1))
done

printf "file '%s'\n" "$TMP"/part*.mp4 | sort > "$TMP/list.txt"
ffmpeg -y -f concat -safe 0 -i "$TMP/list.txt" -c copy "$OUT/demo-60s.mp4" 2>/dev/null || \
  ffmpeg -y -f concat -safe 0 -i "$TMP/list.txt" -c:v libx264 -pix_fmt yuv420p -movflags +faststart "$OUT/demo-60s.mp4"

ls -lh "$OUT/demo-60s.mp4"
