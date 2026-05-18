#!/usr/bin/env bash
# Export marketing raster assets from SVG sources. Requires ImageMagick (magick).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ASSETS="$ROOT/assets"
PH="$ASSETS/product-hunt"

mkdir -p "$ASSETS" "$PH"

magick -background none "$ASSETS/og-image.svg" -resize 1200x630 "$ASSETS/og-image.png"

for i in 01-hero 02-databases 03-ssh 04-sql 05-oss; do
  magick -background none "$PH/${i}.svg" -resize 1270x760 "$PH/${i}.png"
done

echo "Exported:"
echo "  $ASSETS/og-image.png"
ls "$PH"/*.png 2>/dev/null || true
