#!/usr/bin/env bash
# Copies the licenses of Visr, its fonts and its icons into <dest-dir>/licenses.
# Usage: licenses.sh <dest-dir>. Shared by the macOS bundle and the release's licenses archive.
set -euo pipefail

dest="${1:?usage: licenses.sh <dest-dir>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
licenses="$dest/licenses"

mkdir -p "$licenses/fonts" "$licenses/icons-editors" "$licenses/icons-symbols"
cp "$root/LICENSE" "$licenses/"
cp "$root"/crates/app/assets/fonts/*.txt "$licenses/fonts/"
cp "$root/crates/app/assets/icons/editors/LICENSE.txt" "$licenses/icons-editors/"
cp "$root/crates/ui/assets/icons/symbols/LICENSE.txt" "$licenses/icons-symbols/"
