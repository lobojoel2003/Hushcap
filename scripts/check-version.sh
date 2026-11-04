#!/usr/bin/env bash
# Verify that every manifest carries the version in ./VERSION.
# With a tag argument (e.g. v1.2.3), also verify the tag matches and that
# CHANGELOG.md has a section for it.
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(tr -d '[:space:]' < VERSION)"
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "VERSION '$version' is not semantic (MAJOR.MINOR.PATCH[-pre])" >&2
  exit 1
fi

fail=0
check() {
  local file="$1" found="$2"
  if [[ "$found" != "$version" ]]; then
    echo "✗ $file has version '$found', expected '$version'" >&2
    fail=1
  else
    echo "✓ $file: $found"
  fi
}

check Cargo.toml "$(sed -n '/^\[workspace.package\]/,/^\[/{s/^version *= *"\(.*\)"/\1/p}' Cargo.toml | head -1)"
for crate in hushcap-core hushcap-engine; do
  dep="$(grep -E "^$crate = " Cargo.toml | sed -E 's/.*version = "([^"]+)".*/\1/')"
  check "Cargo.toml ($crate dependency)" "$dep"
done
if [[ -f desktop/package.json ]]; then
  check desktop/package.json "$(node -p "require('./desktop/package.json').version")"
fi
if [[ -f desktop/src-tauri/tauri.conf.json ]]; then
  check desktop/src-tauri/tauri.conf.json "$(node -p "require('./desktop/src-tauri/tauri.conf.json').version")"
fi
if [[ -f desktop/package-lock.json ]]; then
  check desktop/package-lock.json "$(node -p "require('./desktop/package-lock.json').version")"
fi
# Android reads ../VERSION directly; make sure it still does.
if ! grep -q 'rootProject.file("../VERSION")' android/app/build.gradle.kts; then
  echo "✗ android/app/build.gradle.kts no longer reads ../VERSION" >&2
  fail=1
else
  echo "✓ android/app/build.gradle.kts reads VERSION"
fi

if [[ $# -ge 1 && -n "$1" ]]; then
  tag="$1"
  if [[ "$tag" != "v$version" ]]; then
    echo "✗ tag '$tag' does not match VERSION 'v$version'" >&2
    fail=1
  else
    echo "✓ tag $tag"
  fi
  if ! grep -qE "^## \[$version\]" CHANGELOG.md; then
    echo "✗ CHANGELOG.md has no '## [$version]' section" >&2
    fail=1
  else
    echo "✓ CHANGELOG.md has a $version section"
  fi
fi

exit $fail
