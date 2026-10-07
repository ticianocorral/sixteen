#!/usr/bin/env bash
# Builds the SixteeN Flatpak and a single-file .flatpak bundle from it —
# the "make a Linux flatpak" one-liner, same spirit as the AppImage script.
# Needs flatpak + flatpak-builder; the Freedesktop 24.08 runtime/sdk and the
# rust extension are pulled from Flathub on first run (--user scope).
# Usage: build-flatpak.sh <version> <out-path>
set -euo pipefail

VERSION="${1:?usage: build-flatpak.sh <version> <out-path>}"
OUT="${2:?usage: build-flatpak.sh <version> <out-path>}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPID="dev.ticianocorral.sixteen"
REPO="$HERE/.flatpak-repo"
BUILD="$HERE/.flatpak-build"

flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y --noninteractive \
  org.freedesktop.Platform//24.08 org.freedesktop.Sdk//24.08 \
  org.freedesktop.Sdk.Extension.rust-stable//24.08

flatpak-builder --force-clean --repo="$REPO" --install-deps-from=flathub \
  "$BUILD" "$HERE/$APPID.yml"

rm -f "$OUT"
# --runtime-repo: quem instala o bundle baixa o runtime sozinho, sem ter o
# Flathub configurado antes.
flatpak build-bundle "$REPO" "$OUT" "$APPID" stable \
  --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo
echo "wrote $OUT (v$VERSION)"
