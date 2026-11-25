#!/usr/bin/env bash
# Next release's version, written to Cargo.toml (the one place it is kept; the app's About page,
# Android's versionName/versionCode and the APK's file name all come from there).
# The last number counts up to 10, then the middle one goes up: 2.0.1 ... 2.0.10, then 2.1.0.
#   ./bump-version.sh          # 2.0.4 -> 2.0.5
#   ./bump-version.sh --show   # prints the current version
set -euo pipefail
cd "$(dirname "$0")"
cur=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
[ "${1:-}" = "--show" ] && { echo "$cur"; exit 0; }
IFS=. read -r major minor patch <<<"$cur"
patch=$((patch + 1))
if [ "$patch" -gt 10 ]; then
  minor=$((minor + 1))
  patch=0
fi
next="$major.$minor.$patch"
sed -i "/^\[workspace.package\]/,/^\[/s/^version = \".*\"/version = \"$next\"/" Cargo.toml
echo "$cur -> $next"
