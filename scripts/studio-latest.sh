#!/usr/bin/env bash
# Prints studio/latest.json for an app release, from the packages it
# publishes:
#
#   scripts/studio-latest.sh <packages dir> <tag app-vX.Y.Z> <owner/repo>
#
# The app's update check reads its `version`; textures.gg's download page
# reads the rest (site docs/STUDIO_RELEASES.md, `StudioRelease`): each
# platform's files, the first being its Download button, with their full
# URLs on the GitHub release, sizes and SHA-256.
set -euo pipefail

dir=$1
tag=$2
repo=$3
base="https://github.com/$repo/releases/download/$tag"
shopt -s nullglob

# One file: what it is, what it runs on, and the file.
file() {
  jq -n --arg kind "$1" --arg min_os "$2" --arg url "$base/$(basename "$3")" \
    --argjson size "$(($(wc -c <"$3")))" --arg sha256 "$(sha256sum "$3" | cut -d' ' -f1)" \
    '{kind: $kind, url: $url, size: $size, sha256: $sha256, min_os: $min_os}'
}

windows="$(
  for f in "$dir"/*-windows-*-setup.exe; do file Installer "Windows 10 or later, 64-bit" "$f"; done
  for f in "$dir"/*-windows-*.zip; do file Zip "Windows 10 or later, 64-bit" "$f"; done
)"
macos="$(
  for f in "$dir"/*-macos-arm64.dmg; do file dmg "macOS 12 or later, Apple silicon" "$f"; done
  for f in "$dir"/*-macos-x86_64.dmg; do file dmg "macOS 12 or later, Intel" "$f"; done
)"
# Built on Ubuntu 22.04 (release.yml), so its glibc is the oldest it needs.
linux="$(
  for f in "$dir"/*-linux-x86_64.tar.gz; do file tar.gz "64-bit, glibc 2.35 or later (Ubuntu 22.04, Fedora 36)" "$f"; done
)"

jq -n --arg version "${tag#app-v}" --arg released "$(date -u +%F)" \
  --arg notes_url "https://github.com/$repo/releases/tag/$tag" \
  --argjson windows "$(jq -s . <<<"$windows")" \
  --argjson macos "$(jq -s . <<<"$macos")" \
  --argjson linux "$(jq -s . <<<"$linux")" \
  '{version: $version, released: $released, notes_url: $notes_url,
    assets: {windows: $windows, macos: $macos, linux: $linux}}'
