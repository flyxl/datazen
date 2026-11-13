#!/usr/bin/env bash
set -euo pipefail

# ── Usage ────────────────────────────────────────────────────────────
# ./bump.sh <new-version>
#
# Examples:
#   ./bump.sh 0.1.0
#   ./bump.sh v1.2.3    (leading "v" is stripped automatically)
# ─────────────────────────────────────────────────────────────────────

REPO_ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
cd "$REPO_ROOT"

if [ $# -lt 1 ]; then
  echo "Usage: $0 <new-version>"
  echo "  e.g. $0 0.0.3"
  exit 1
fi

NEW="${1#v}"

if ! echo "$NEW" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "Error: '$NEW' is not a valid semver (expected X.Y.Z)"
  exit 1
fi

OLD=$(grep -m1 '"version"' package.json | sed 's/.*"\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)".*/\1/')

if [ -z "$OLD" ]; then
  echo "Error: could not read current version from package.json"
  exit 1
fi

if [ "$OLD" = "$NEW" ]; then
  echo "Version is already $NEW, nothing to do."
  exit 0
fi

echo "Bumping version: $OLD → $NEW"
echo ""

# ── 1. Current application version sources ───────────────────────────

update_file() {
  local file="$1" old_pat="$2" new_pat="$3"
  if [ ! -f "$file" ]; then
    echo "  SKIP (not found): $file"
    return
  fi
  if grep -q "$old_pat" "$file"; then
    if [[ "$OSTYPE" == "darwin"* ]]; then
      sed -i '' "s|$old_pat|$new_pat|g" "$file"
    else
      sed -i "s|$old_pat|$new_pat|g" "$file"
    fi
    echo "  ✓ $file"
  else
    echo "  - $file (pattern not found, skipping)"
  fi
}

echo "[1/4] Updating application version sources..."
update_file "package.json"                          "\"version\": \"$OLD\""  "\"version\": \"$NEW\""
update_file "crates/datazen-desktop/Cargo.toml"    "version = \"$OLD\""     "version = \"$NEW\""
update_file "crates/datazen-desktop/tauri.conf.json" "\"version\": \"$OLD\""  "\"version\": \"$NEW\""

# ── 2. Regenerate Cargo.lock ─────────────────────────────────────────

echo ""
echo "[2/4] Regenerating Cargo.lock..."
cargo check -p datazen_desktop --lib
echo "  ✓ Cargo.lock"

# ── 3. Release guards ────────────────────────────────────────────────

echo ""
echo "[3/5] Running release guards..."
node scripts/check-version-consistency.mjs
node scripts/check-ci-docs-consistency.mjs
node scripts/check-structure-editor-guardrails.mjs
node scripts/i18n-sync-check.mjs
git diff --check

# ── 4. Commit ────────────────────────────────────────────────────────

echo ""
echo "[4/5] Committing..."
git add package.json \
  crates/datazen-desktop/Cargo.toml \
  crates/datazen-desktop/tauri.conf.json \
  Cargo.lock \
  .cursor/skills/bump-version/SKILL.md \
  .cursor/skills/bump-version/scripts/bump.sh
git commit -m "chore: bump version to v$NEW"
echo "  ✓ Committed"

# ── 5. Tag & push ────────────────────────────────────────────────────

echo ""
echo "[5/5] Tagging v$NEW and pushing..."

TAG="v$NEW"

# Remove existing local tag if present
if git tag -l "$TAG" | grep -q "$TAG"; then
  git tag -d "$TAG" >/dev/null 2>&1
fi

git tag "$TAG"

push_to_remote() {
  local remote="$1"
  if ! git remote | grep -q "^${remote}$"; then
    return
  fi
  # Delete remote tag if it exists (ignore errors)
  git push "$remote" ":refs/tags/$TAG" 2>/dev/null || true
  git push "$remote" main "$TAG" 2>/dev/null
  echo "  ✓ Pushed to $remote"
}

push_to_remote "origin"
# Optional Gitee mirror (if configured as remote `gitee`)
push_to_remote "gitee"

echo ""
echo "Done! Version bumped to v$NEW"
echo "GitHub Actions will build the release automatically."
