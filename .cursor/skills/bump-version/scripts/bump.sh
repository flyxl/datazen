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

# ── 1. Config files (bare semver) ────────────────────────────────────

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

echo "[1/5] Updating config files..."
update_file "package.json"               "\"version\": \"$OLD\""  "\"version\": \"$NEW\""
update_file "src-tauri/Cargo.toml"       "version = \"$OLD\""     "version = \"$NEW\""
update_file "src-tauri/tauri.conf.json"  "\"version\": \"$OLD\""  "\"version\": \"$NEW\""

# ── 2. Display strings (DataZen vX.Y.Z) ─────────────────────────────

echo ""
echo "[2/5] Updating display strings..."

DISPLAY_FILES=(
  "src/windows/main/MainWindow.tsx"
  "src/windows/data-sync/DataSyncWindow.tsx"
  "e2e/specs/homepage-features.ts"
)

for f in "${DISPLAY_FILES[@]}"; do
  update_file "$f" "DataZen v[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*" "DataZen v$NEW"
done

# ── 3. Regenerate Cargo.lock ─────────────────────────────────────────

echo ""
echo "[3/5] Regenerating Cargo.lock..."
(cd src-tauri && cargo check --quiet 2>/dev/null)
echo "  ✓ src-tauri/Cargo.lock"

# ── 4. Commit ────────────────────────────────────────────────────────

echo ""
echo "[4/5] Committing..."
git add -A
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
  git push "$remote" main --tags 2>/dev/null
  echo "  ✓ Pushed to $remote"
}

push_to_remote "origin"
push_to_remote "github"

echo ""
echo "Done! Version bumped to v$NEW"
echo "GitHub Actions will build the release automatically."
