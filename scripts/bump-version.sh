#!/usr/bin/env bash
# Set a new version everywhere: ./scripts/bump-version.sh 1.2.3
# Then add a "## [1.2.3]" section to CHANGELOG.md, commit, and tag v1.2.3.
set -euo pipefail
cd "$(dirname "$0")/.."

new="${1:-}"
if ! [[ "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "usage: $0 MAJOR.MINOR.PATCH[-pre]" >&2
  exit 1
fi
old="$(tr -d '[:space:]' < VERSION)"

echo "$new" > VERSION
# Workspace version and internal crate dependency versions.
sed -i.bak -E "/^\[workspace.package\]/,/^\[/ s/^version = \"[^\"]+\"/version = \"$new\"/" Cargo.toml
sed -i.bak -E "s/^(hushcap-(core|engine) = \{ path = \"[^\"]+\", version = )\"[^\"]+\"/\1\"$new\"/" Cargo.toml
rm -f Cargo.toml.bak

if [[ -f desktop/package.json ]]; then
  (cd desktop && npm version "$new" --no-git-tag-version --allow-same-version >/dev/null)
fi
if [[ -f desktop/src-tauri/tauri.conf.json ]]; then
  node -e '
    const fs = require("fs");
    const p = "desktop/src-tauri/tauri.conf.json";
    const conf = JSON.parse(fs.readFileSync(p, "utf8"));
    conf.version = process.argv[1];
    fs.writeFileSync(p, JSON.stringify(conf, null, 2) + "\n");
  ' "$new"
fi
# Refresh Cargo.lock entries for workspace crates.
cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null

echo "Version bumped: $old -> $new"
./scripts/check-version.sh
