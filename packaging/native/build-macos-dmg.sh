#!/usr/bin/env bash
# Assemble the macOS .app bundle + compressed DMG for datazen-native (Rune host).
#
# Why the .app wrapper: a bare Unix executable handed to LaunchServices is run
# by Terminal.app, so double-clicking it opens a terminal window. A bundle with
# Contents/MacOS/<executable> is launched as a GUI app instead — no terminal.
# Do NOT add LSUIElement to "hide the terminal": that only removes the Dock
# icon and menu bar, it has nothing to do with Terminal.app.
#
# Usage: packaging/native/build-macos-dmg.sh [profile]
#   profile: release (default) or dev (fast local check of the packaging)
# Env:
#   APP_NAME      bundle and volume name (default: "DataZen Native")
#   BUNDLE_ID     CFBundleIdentifier (default: com.tbeasy.datazen.native)
#   DMG_FORMAT    hdiutil format (default: UDBZ — ~12% smaller than UDZO)
#   SKIP_CODESIGN=1  skip the ad-hoc codesign step
set -euo pipefail
cd "$(dirname "$0")/../.."

PROFILE="${1:-release}"
case "$PROFILE" in
  release) BIN=target/release/datazen-native ;;
  dev)     BIN=target/debug/datazen-native ;;
  *) echo "unknown profile $PROFILE (release or dev)" >&2; exit 2 ;;
esac
test -x "$BIN" || { echo "missing executable $BIN" >&2; exit 1; }

APP_NAME="${APP_NAME:-DataZen Native}"
BUNDLE_ID="${BUNDLE_ID:-com.tbeasy.datazen.native}"
DMG_FORMAT="${DMG_FORMAT:-UDBZ}"
ICON="crates/datazen-desktop/icons/icon.icns"
test -f "$ICON" || { echo "missing icon $ICON" >&2; exit 1; }

VERSION="$(git describe --tags --always --dirty 2>/dev/null || echo 0.0.0)"
VERSION="${VERSION#v}"
VERSION="${VERSION%%-*}"
ARCH="$(uname -m)"

OUT_DIR=target/dmg
APP="$OUT_DIR/$APP_NAME.app"
STAGE="$OUT_DIR/stage"
DMG="$OUT_DIR/datazen-native-${VERSION}-macOS-${ARCH}.dmg"

rm -rf "$OUT_DIR"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$STAGE"

install -m 755 "$BIN" "$APP/Contents/MacOS/datazen-native"
install -m 644 "$ICON" "$APP/Contents/Resources/DataZen.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>${APP_NAME}</string>
  <key>CFBundleExecutable</key><string>datazen-native</string>
  <key>CFBundleIconFile</key><string>DataZen</string>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
  <key>CFBundleName</key><string>${APP_NAME}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# Ad-hoc signing keeps LaunchServices happy on Apple Silicon. It is NOT a
# Developer ID signature: a downloaded DMG still needs notarization (or the
# user removing com.apple.quarantine) to bypass Gatekeeper.
if [ "${SKIP_CODESIGN:-0}" != "1" ]; then
  codesign --force --deep --sign - "$APP"
fi

# The Applications symlink is the usual "drag to install" affordance.
cp -R "$APP" "$STAGE/$APP_NAME.app"
ln -s /Applications "$STAGE/Applications"

hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format "$DMG_FORMAT" "$DMG"

echo "built $DMG"
echo "--- sizes ---"
ls -lh "$BIN" "$APP/Contents/MacOS/datazen-native" "$DMG"
du -sh "$APP"
