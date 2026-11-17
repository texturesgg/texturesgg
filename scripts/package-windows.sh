#!/usr/bin/env bash
# Build the app and package it for Windows (run in Git Bash on Windows):
#   target/package/textures.gg-<version>-windows-x86_64-setup.exe
#     an installer: for this user, a Start menu entry, an uninstaller
#   target/package/textures.gg-<version>-windows-x86_64.zip
#     the same files, to run from anywhere
#
# Env: WINDOWS_SIGN  a command that signs the files it's given in place
#                    (for example a signtool or Azure Artifact Signing call);
#                    without it the app and installer are unsigned and
#                    SmartScreen warns before the first launch.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --format-version 1 --no-deps |
  jq -r '.packages[] | select(.name == "tgg-studio") | .version')"
OUT_DIR="$ROOT/target/package"
NAME="textures.gg-$VERSION-windows-x86_64"
STAGE="$OUT_DIR/$NAME"

sign() {
  if [[ -n "${WINDOWS_SIGN:-}" ]]; then
    # Word splitting is wanted: the command and its own arguments.
    # shellcheck disable=SC2086
    $WINDOWS_SIGN "$@"
  fi
}

cd "$ROOT"
cargo build --locked --release -p tgg-studio

rm -rf "$STAGE" "$STAGE.zip" "$STAGE-setup.exe"
mkdir -p "$STAGE/licenses/fonts"
cp "$ROOT/target/release/tgg-studio.exe" "$STAGE/"
cp "$ROOT/LICENSE" "$STAGE/licenses/LICENSE"
for font in "$ROOT"/crates/tgg-studio/assets/fonts/*/; do
  cp "$font/OFL.txt" "$STAGE/licenses/fonts/$(basename "$font").txt"
done
sign "$STAGE/tgg-studio.exe"

# The installer, from the staged (and signed) files.
ISCC="${ISCC:-iscc}"
command -v "$ISCC" >/dev/null || ISCC="/c/Program Files (x86)/Inno Setup 6/ISCC.exe"
"$ISCC" //Q "//DVersion=$VERSION" "//DSource=$(cygpath -w "$STAGE")" \
  "//DOutput=$(cygpath -w "$OUT_DIR")" "$(cygpath -w "$ROOT/dist/windows/textures.gg.iss")"
sign "$STAGE-setup.exe"

(cd "$OUT_DIR" && 7z a -bd "$NAME.zip" "$NAME" >/dev/null)
rm -rf "$STAGE"
echo "packaged: $STAGE-setup.exe and $STAGE.zip"
