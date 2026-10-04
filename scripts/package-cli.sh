#!/usr/bin/env bash
# Build `tgg` for one platform and pack it into target/package/:
# tgg-<version>-<platform>.tar.gz, or .zip for Windows, holding the binary,
# README and license.
#
# Usage: package-cli.sh <platform>
#   x86_64-unknown-linux-musl, aarch64-unknown-linux-musl
#                         static, so they run on any Linux
#   universal-apple-darwin
#                         x86_64 and arm64 in one binary
#   x86_64-pc-windows-msvc
#
# Env (macOS): CODESIGN_IDENTITY signs the binary with the hardened runtime;
#              NOTARY_KEY_PATH, NOTARY_KEY_ID, NOTARY_ISSUER_ID also notarize
#              it. Without them it is signed ad hoc.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
platform="${1:?usage: package-cli.sh <platform>}"
VERSION="$(cargo metadata --format-version 1 --no-deps |
  jq -r '.packages[] | select(.name == "tgg-cli") | .version')"
name="tgg-$VERSION-$platform"
stage="target/package/$name"
rm -rf "$stage"
mkdir -p "$stage"

build() {
  rustup target add "$1" >/dev/null
  cargo build --release --locked -p tgg-cli --target "$1"
}

case "$platform" in
  universal-apple-darwin)
    build aarch64-apple-darwin
    build x86_64-apple-darwin
    lipo -create -output "$stage/tgg" \
      target/aarch64-apple-darwin/release/tgg target/x86_64-apple-darwin/release/tgg
    if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
      codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$stage/tgg"
    else
      codesign --force --sign - "$stage/tgg"
    fi
    codesign --verify --strict "$stage/tgg"
    # A bare binary can't be stapled; Gatekeeper finds its notarization online.
    if [[ -n "${NOTARY_KEY_PATH:-}" && -n "${NOTARY_KEY_ID:-}" && -n "${NOTARY_ISSUER_ID:-}" ]]; then
      ditto -c -k "$stage/tgg" "target/package/notarize.zip"
      xcrun notarytool submit target/package/notarize.zip --key "$NOTARY_KEY_PATH" \
        --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER_ID" --wait
      rm target/package/notarize.zip
    fi
    ;;
  *-linux-musl)
    build "$platform"
    cp "target/$platform/release/tgg" "$stage/"
    ;;
  *-windows-msvc)
    build "$platform"
    cp "target/$platform/release/tgg.exe" "$stage/"
    ;;
  *)
    echo "unknown platform $platform" >&2
    exit 1
    ;;
esac

cp crates/tgg-cli/README.md "$stage/"
cp LICENSE "$stage/"
cd target/package
if [[ "$platform" == *-windows-* ]]; then
  7z a -bd "$name.zip" "$name" >/dev/null
else
  tar -czf "$name.tar.gz" "$name"
fi
rm -rf "$name"
