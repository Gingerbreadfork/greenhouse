#!/usr/bin/env bash
# Generates the offline sources the Flatpak build needs.
#
#   flatpak/generate.sh          sources for a local build of the working tree
#   flatpak/generate.sh v0.5.0   the files for the Flathub repository, pinned to a tag
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
tag=${1:-}
tools=$here/.tools
id=io.github.Gingerbreadfork.Greenhouse

fetch() {
    [ -d "$tools/$1" ] || git clone --quiet --depth 1 "$2" "$tools/$1"
}
fetch flatpak-builder-tools https://github.com/flatpak/flatpak-builder-tools.git
fetch shared-modules https://github.com/flathub/shared-modules.git

if [ -n "$tag" ]; then
    out=$here/flathub
    commit=$(git -C "$root" rev-parse "$tag^{commit}")
    locks=$(mktemp -d)
    trap 'rm -rf "$locks"' EXIT
    git -C "$root" show "$tag:src-tauri/Cargo.lock" > "$locks/Cargo.lock"
    git -C "$root" show "$tag:pnpm-lock.yaml" > "$locks/pnpm-lock.yaml"
    cargo_lock=$locks/Cargo.lock
    pnpm_lock=$locks/pnpm-lock.yaml
else
    out=$here
    cargo_lock=$root/src-tauri/Cargo.lock
    pnpm_lock=$root/pnpm-lock.yaml
fi
mkdir -p "$out"

uv run --quiet --no-project \
    --with 'aiohttp>=3.9.5,<4' --with 'PyYAML>=6' --with 'tomlkit>=0.13.3,<1' \
    python -I "$tools/flatpak-builder-tools/cargo/flatpak-cargo-generator.py" \
    "$cargo_lock" -o "$out/cargo-sources.json"
uv tool run --quiet --from "$tools/flatpak-builder-tools/node" \
    flatpak-node-generator pnpm "$pnpm_lock" -o "$out/node-sources.json" > /dev/null

if [ -z "$tag" ]; then
    rm -rf "$here/shared-modules"
    cp -r "$tools/shared-modules" "$here/shared-modules"
    exit 0
fi

# The Flathub copy builds the tagged release instead of the working tree, and
# takes shared-modules as a git submodule.
python3 -I - "$here/$id.yml" "$out/$id.yml" "$tag" "$commit" <<'EOF'
import re, sys
src, dest, tag, commit = sys.argv[1:]
manifest = open(src).read()
git = (
    "      - type: git\n"
    "        url: https://github.com/Gingerbreadfork/greenhouse.git\n"
    f"        tag: {tag}\n"
    f"        commit: {commit}\n"
)
manifest, n = re.subn(r"      # Replaced by.*?(?=      - type: archive)", git, manifest, flags=re.S)
assert n == 1, "could not find the local source in the manifest"
open(dest, "w").write(manifest)
EOF
echo "Wrote $out. Add shared-modules there with:"
echo "  git submodule add https://github.com/flathub/shared-modules.git"
