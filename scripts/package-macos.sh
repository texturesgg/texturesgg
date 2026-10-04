#!/usr/bin/env bash
# Build the app for this Mac's architecture and package it:
#   target/package/textures.gg-<version>-macos-<arch>.dmg
# holding textures.gg.app beside a link to /Applications.
#
# Env: CODESIGN_IDENTITY  "Developer ID Application: …" signs the app and the
#                         disk image; without it the app is signed ad hoc and
#                         Gatekeeper asks before the first launch.
#      NOTARY_KEY_PATH, NOTARY_KEY_ID, NOTARY_ISSUER_ID
#                         an App Store Connect API key; with all three (and a
#                         signing identity) the app and the disk image are
#                         notarized and stapled.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --format-version 1 --no-deps |
  jq -r '.packages[] | select(.name == "tgg-editor") | .version')"
ARCH="$(uname -m)"
OUT_DIR="$ROOT/target/package"
APP="$OUT_DIR/textures.gg.app"
DMG="$OUT_DIR/textures.gg-$VERSION-macos-$ARCH.dmg"

NOTARIZE=false
if [[ -n "${NOTARY_KEY_PATH:-}" && -n "${NOTARY_KEY_ID:-}" && -n "${NOTARY_ISSUER_ID:-}" ]]; then
  [[ -n "${CODESIGN_IDENTITY:-}" ]] || { echo "notarizing needs CODESIGN_IDENTITY" >&2; exit 1; }
  NOTARIZE=true
fi

cd "$ROOT"
cargo build --locked --release -p tgg-editor

rm -rf "$APP" "$DMG"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/licenses/fonts"
install -m 755 "$ROOT/target/release/tgg-editor" "$APP/Contents/MacOS/tgg-editor"
sed "s/__VERSION__/$VERSION/g" "$ROOT/dist/macos/Info.plist" >"$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null
install -m 644 "$ROOT/LICENSE" "$APP/Contents/Resources/licenses/LICENSE"
for font in "$ROOT"/crates/tgg-ui/assets/fonts/*/; do
  install -m 644 "$font/OFL.txt" "$APP/Contents/Resources/licenses/fonts/$(basename "$font").txt"
done

ICONSET="$OUT_DIR/AppIcon.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$ROOT/dist/macos/icon-1024.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z $((size * 2)) $((size * 2)) "$ROOT/dist/macos/icon-1024.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
rm -rf "$ICONSET"

# Notarization requires the hardened runtime and a secure timestamp.
if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
  codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$APP"
else
  codesign --force --sign - "$APP"
fi
codesign --verify --strict "$APP"

# A rejected submission can still exit 0; the staple after it then fails.
notarize() {
  xcrun notarytool submit "$1" --key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" \
    --issuer "$NOTARY_ISSUER_ID" --wait
  xcrun stapler staple "$1"
}

if $NOTARIZE; then
  ZIP="$OUT_DIR/textures.gg-notarize.zip"
  ditto -c -k --keepParent "$APP" "$ZIP"
  xcrun notarytool submit "$ZIP" --key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" \
    --issuer "$NOTARY_ISSUER_ID" --wait
  rm -f "$ZIP"
  xcrun stapler staple "$APP"
fi

SOURCE="$OUT_DIR/dmg"
rm -rf "$SOURCE" && mkdir -p "$SOURCE"
cp -R "$APP" "$SOURCE/"
ln -s /Applications "$SOURCE/Applications"
hdiutil create -quiet -volname textures.gg -srcfolder "$SOURCE" -fs HFS+ -format UDZO "$DMG"
rm -rf "$SOURCE"

if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
  codesign --force --timestamp --sign "$CODESIGN_IDENTITY" "$DMG"
fi
if $NOTARIZE; then
  notarize "$DMG"
  spctl --assess --type open --context context:primary-signature "$DMG"
fi
echo "packaged: $DMG"
