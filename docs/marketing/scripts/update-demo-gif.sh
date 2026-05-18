#!/usr/bin/env bash
# Rebuild docs/screenshots/demo.gif from PNG screenshots.
set -euo pipefail
DIR="$(cd "$(dirname "$0")/../../screenshots" && pwd)"
cd "$DIR"
magick -delay 80 -loop 0 \
  main-window.png new-connection.png connection-window.png query-editor.png redis-view.png \
  -resize 900x -layers Optimize demo.gif
ls -lh demo.gif
