#!/usr/bin/env bash
# Fail unless the tag is app-v<the app's version>.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --format-version 1 --no-deps |
  jq -r '.packages[] | select(.name == "tgg-editor") | .version')"
[[ "$1" == "app-v$VERSION" ]] || { echo "tag $1 does not match the app's version $VERSION" >&2; exit 1; }
