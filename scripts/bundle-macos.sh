#!/usr/bin/env bash
# Builds target/release/Visr.app, signed ad hoc so it runs on this Mac without a developer ID.
# Usage: bundle-macos.sh [version] [binary]. The version defaults to the workspace's.
# Pass a binary (like a universal one from lipo) to bundle it instead of building this Mac's own;
# a binary can only be passed after a version.
# The licenses of Visr, its fonts and its icons go in Contents/Resources/licenses.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
pkgid="$(cargo pkgid -p visr --manifest-path "$root/Cargo.toml")"
version="${1:-${pkgid##*[#@]}}"
binary="${2:-$root/target/release/visr}"
app="$root/target/release/Visr.app"
licenses="$app/Contents/Resources/licenses"

if [ -z "${2:-}" ]; then
  cargo build --release -p visr --manifest-path "$root/Cargo.toml"
fi

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$licenses/fonts" "$licenses/icons-editors" "$licenses/icons-symbols"
cp "$binary" "$app/Contents/MacOS/visr"
chmod +x "$app/Contents/MacOS/visr"
sed "s/VERSION/$version/g" "$root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"

cp "$root/LICENSE" "$licenses/"
cp "$root"/crates/app/assets/fonts/*.txt "$licenses/fonts/"
cp "$root/crates/app/assets/icons/editors/LICENSE.txt" "$licenses/icons-editors/"
cp "$root/crates/ui/assets/icons/symbols/LICENSE.txt" "$licenses/icons-symbols/"

codesign --force --sign - "$app"

echo "$app"
