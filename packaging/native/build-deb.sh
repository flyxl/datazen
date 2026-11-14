#!/usr/bin/env bash
# Assemble the datazen-native .deb (T11): the release binary, the desktop
# entry, the app's own icon, and a control file whose runtime dependency is
# the one native library winit needs that desktops do not all preinstall
# (libxkbcommon). The build itself runs in CI (release-native.yml); this
# script is the packaging half, runnable anywhere dpkg-deb exists.
#
# Usage: packaging/native/build-deb.sh [profile]
#   profile: the cargo profile to build (default: release; "dev" for a fast
#            local check of the packaging itself).
set -euo pipefail
cd "$(dirname "$0")/../.."

PROFILE="${1:-release}"
case "$PROFILE" in
  release) BIN=target/release/datazen-native ;;
  dev)     BIN=target/debug/datazen-native ;;
  *)       echo "unknown profile $PROFILE (release or dev)" >&2; exit 2 ;;
esac

# The version comes from git (a tag in CI, a describe locally); Debian
# versions allow [A-Za-z0-9.+~] — anything else becomes a dot.
VERSION="$(git describe --tags --always --dirty 2>/dev/null || echo 0.0.0)"
VERSION="${VERSION#v}"
VERSION="$(echo "$VERSION" | tr -c 'A-Za-z0-9.+~' '.')"
# A trailing dot is the dirty marker's empty remainder; Debian versions do
# not end in one.
VERSION="${VERSION%.}"
ARCH="$(dpkg --print-architecture)"

cargo build --profile "$PROFILE" -p datazen-rune-host
test -x "$BIN"

STAGE=target/deb/root
OUT="target/deb/datazen-native_${VERSION}_${ARCH}.deb"
rm -rf target/deb
mkdir -p \
  "$STAGE/DEBIAN" \
  "$STAGE/usr/bin" \
  "$STAGE/usr/share/applications" \
  "$STAGE/usr/share/icons/hicolor/128x128/apps"

install -m 755 "$BIN" "$STAGE/usr/bin/datazen-native"
install -m 644 packaging/native/datazen-native.desktop \
  "$STAGE/usr/share/applications/datazen-native.desktop"
install -m 644 crates/datazen-desktop/icons/128x128.png \
  "$STAGE/usr/share/icons/hicolor/128x128/apps/datazen-native.png"

cat > "$STAGE/DEBIAN/control" <<CONTROL
Package: datazen-native
Version: $VERSION
Section: database
Priority: optional
Architecture: $ARCH
Depends: libxkbcommon0
Maintainer: DataZen <noreply@datazen.invalid>
Description: DataZen native desktop host (Rune UI)
 The DataZen database workspace as a pure native binary: the shared
 Rune interface over real use cases — sqlite connections, table
 browsing and editing, query execution, sync/compare/transfer,
 workflows and dashboards. The React/Tauri desktop stays a separate
 package.
CONTROL

dpkg-deb --build --root-owner-group "$STAGE" "$OUT"
echo "built $OUT"
