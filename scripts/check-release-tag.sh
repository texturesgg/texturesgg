#!/usr/bin/env bash
# Fail unless the tag names the version of what it releases: app-v<version>
# for the app (tgg-editor), cli-v<version> for the command line (tgg-cli).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
case "$1" in
  app-v*) crate=tgg-editor prefix=app-v ;;
  cli-v*) crate=tgg-cli prefix=cli-v ;;
  *) echo "tag $1 is neither app-v<version> nor cli-v<version>" >&2; exit 1 ;;
esac
VERSION="$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --format-version 1 --no-deps |
  jq -r --arg crate "$crate" '.packages[] | select(.name == $crate) | .version')"
[[ "$1" == "$prefix$VERSION" ]] || { echo "tag $1 does not match $crate's version $VERSION" >&2; exit 1; }
