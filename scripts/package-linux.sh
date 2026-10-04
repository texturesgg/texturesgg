#!/usr/bin/env bash
# Build the app and package it for Linux:
#   target/package/textures.gg-<version>-linux-<arch>.tar.gz
# holding the binary, the desktop entry, the icon, the licenses, and an
# install.sh that copies them into ~/.local.
#
# Env: PROFILE=debug packages an unoptimized build (default: release).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${PROFILE:-release}"
VERSION="$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --format-version 1 --no-deps |
  jq -r '.packages[] | select(.name == "tgg-editor") | .version')"
ARCH="$(uname -m)"
OUT_DIR="$ROOT/target/package"
NAME="textures.gg-$VERSION-linux-$ARCH"
STAGE="$OUT_DIR/$NAME"

cd "$ROOT"
if [[ "$PROFILE" == release ]]; then
  cargo build --locked --release -p tgg-editor
else
  cargo build --locked -p tgg-editor
fi

rm -rf "$STAGE" "$STAGE.tar.gz"
mkdir -p "$STAGE/licenses/fonts"
install -m 755 "$ROOT/target/$PROFILE/tgg-editor" "$STAGE/tgg-editor"
install -m 644 "$ROOT/dist/linux/gg.textures.app.desktop" "$STAGE/"
install -m 644 "$ROOT/dist/linux/gg.textures.app.svg" "$STAGE/"
install -m 644 "$ROOT/dist/linux/gg.textures.app.png" "$STAGE/"
install -m 644 "$ROOT/LICENSE" "$STAGE/licenses/LICENSE"
for font in "$ROOT"/crates/tgg-ui/assets/fonts/*/; do
  install -m 644 "$font/OFL.txt" "$STAGE/licenses/fonts/$(basename "$font").txt"
done

cat >"$STAGE/install.sh" <<'INSTALL'
#!/usr/bin/env bash
# Install textures.gg for this user, in ~/.local. Run it again to update.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
install -Dm755 "$HERE/tgg-editor" "$HOME/.local/bin/tgg-editor"
install -Dm644 "$HERE/gg.textures.app.desktop" "$DATA/applications/gg.textures.app.desktop"
install -Dm644 "$HERE/gg.textures.app.svg" "$DATA/icons/hicolor/scalable/apps/gg.textures.app.svg"
install -Dm644 "$HERE/gg.textures.app.png" "$DATA/icons/hicolor/512x512/apps/gg.textures.app.png"
command -v update-desktop-database >/dev/null && update-desktop-database "$DATA/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -qt "$DATA/icons/hicolor" 2>/dev/null || true
echo "Installed textures.gg. Open it from your app menu, or run tgg-editor."
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) echo "Add ~/.local/bin to PATH to run it from a terminal." ;; esac
INSTALL
chmod 755 "$STAGE/install.sh"

tar -czf "$STAGE.tar.gz" -C "$OUT_DIR" "$NAME"
rm -rf "$STAGE"
echo "packaged: $STAGE.tar.gz"
