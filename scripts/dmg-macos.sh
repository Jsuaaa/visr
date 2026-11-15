#!/usr/bin/env bash
# Packs target/release/Visr.app into a disk image with an Applications shortcut to drag it onto.
set -euo pipefail

output="${1:?usage: dmg-macos.sh <output.dmg>}"
root="$(cd "$(dirname "$0")/.." && pwd)"

if [ ! -d "$root/target/release/Visr.app" ]; then
  echo "target/release/Visr.app not found: run scripts/bundle-macos.sh first" >&2
  exit 1
fi

staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT

cp -R "$root/target/release/Visr.app" "$staging/"
ln -s /Applications "$staging/Applications"
hdiutil create -volname Visr -srcfolder "$staging" -ov -format UDZO "$output"
