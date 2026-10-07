#!/usr/bin/env bash
# Wraps the already-built sixteen binary into an AppImage.
# Usage: build-appimage.sh <path-to-sixteen-binary> <version> <out-path>
set -euo pipefail

BIN="$1"
VERSION="$2"
OUT="$3"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

APPDIR="$WORK/SixteeN.AppDir"
mkdir -p "$APPDIR/usr/bin"
cp "$BIN" "$APPDIR/usr/bin/sixteen"
chmod +x "$APPDIR/usr/bin/sixteen"
cp "$HERE/sixteen.desktop" "$APPDIR/sixteen.desktop"
cp "$HERE/sixteen.png" "$APPDIR/sixteen.png"
cat > "$APPDIR/AppRun" << 'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/sixteen" "$@"
EOF
chmod +x "$APPDIR/AppRun"

TOOL="$WORK/appimagetool.AppImage"
curl -fsSL -o "$TOOL" \
  https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x "$TOOL"

rm -f "$OUT"
# --appimage-extract-and-run: runners often lack FUSE, this sidesteps it.
"$TOOL" --appimage-extract-and-run "$APPDIR" "$OUT"
echo "wrote $OUT"
