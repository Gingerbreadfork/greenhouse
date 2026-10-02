#!/usr/bin/env bash
# Writes the updater's latest.json from a release's signature files.
# Usage: latest-json.sh <tag> <dir holding the *.sig files> [notes file]
set -euo pipefail

tag=$1
dir=$2
notes=${3:-/dev/null}
base="https://github.com/Gingerbreadfork/greenhouse/releases/download/$tag"

# One platform entry, from the signature of the first asset matching the glob.
entry() {
  local key=$1 sig
  sig=$(find "$dir" -maxdepth 1 -name "$2.sig" | sort | head -1)
  [ -n "$sig" ] || return 0
  jq -n --arg key "$key" --arg url "$base/$(basename "${sig%.sig}")" --rawfile sig "$sig" \
    '{($key): {url: $url, signature: ($sig | rtrimstr("\n"))}}'
}

{
  entry linux-x86_64-deb '*.deb'
  entry linux-x86_64-rpm '*.rpm'
  entry linux-x86_64-appimage '*.AppImage'
  entry linux-x86_64 '*.AppImage'
  entry windows-x86_64-nsis '*-setup.exe'
  entry windows-x86_64 '*-setup.exe'
} | jq -s --arg version "${tag#v}" --rawfile notes "$notes" \
  --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '{version: $version, notes: $notes, pub_date: $date, platforms: add}'
