#!/usr/bin/env bash
set -euo pipefail

# ── Usage ────────────────────────────────────────────────────────────
# ./scripts/sync-repos.sh [--commit-message "msg"]
#
# Syncs local changes to all external GitHub repositories:
#   1. packages/driver-api → github.com/flyxl/datazen-driver-api
#   2. packages/drivers/kiwi      → github.com/flyxl/datazen-plugin-kiwi
#   3. packages/drivers/olap      → github.com/flyxl/datazen-plugin-olap
# Then updates Cargo.lock Git refs in the main workspace.
# ─────────────────────────────────────────────────────────────────────

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DRIVER_API_DIR="$ROOT/packages/driver-api"
DRIVER_API_REPO="https://github.com/flyxl/datazen-driver-api.git"
DRIVERS_DIR="$ROOT/packages/drivers"

GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

COMMIT_MSG=""
while [[ $# -gt 0 ]]; do
  case $1 in
    --commit-message|-m)
      COMMIT_MSG="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

info()  { echo -e "${CYAN}▸${NC} $*"; }
ok()    { echo -e "${GREEN}✓${NC} $*"; }
warn()  { echo -e "${YELLOW}⚠${NC} $*"; }

# ── 1. Sync driver-api ───────────────────────────────────────────────

sync_driver_api() {
  info "Syncing driver-api to GitHub..."

  local tmp_dir
  tmp_dir=$(mktemp -d)
  trap "rm -rf '$tmp_dir'" RETURN

  git clone --depth 1 "$DRIVER_API_REPO" "$tmp_dir" 2>/dev/null

  local has_diff=false
  if ! diff -rq "$DRIVER_API_DIR/src" "$tmp_dir/src" >/dev/null 2>&1; then
    has_diff=true
  fi
  if ! diff -q "$DRIVER_API_DIR/Cargo.toml" "$tmp_dir/Cargo.toml" >/dev/null 2>&1; then
    has_diff=true
  fi

  if $has_diff; then
    cp -r "$DRIVER_API_DIR/src/"* "$tmp_dir/src/"
    cp "$DRIVER_API_DIR/Cargo.toml" "$tmp_dir/Cargo.toml"

    local msg="${COMMIT_MSG:-"sync: align with main app driver-api"}"
    (cd "$tmp_dir" && git add -A && git commit -m "$msg" && git push origin main)
    ok "driver-api pushed"
  else
    ok "driver-api already up to date"
  fi
}

# ── 2. Sync plugin repos ────────────────────────────────────────────

sync_plugin() {
  local name="$1"
  local dir="$DRIVERS_DIR/$name"

  if [[ ! -d "$dir/.git" ]]; then
    warn "Plugin '$name' not found or not a git repo at $dir — skipping"
    return
  fi

  info "Syncing plugin '$name'..."

  local status
  status=$(cd "$dir" && git status --porcelain -- ':!Cargo.lock' ':!target/')

  if [[ -z "$status" ]]; then
    ok "Plugin '$name' has no changes"
    return
  fi

  local msg="${COMMIT_MSG:-"sync: update $name plugin"}"
  (
    cd "$dir"
    git add -A ':!Cargo.lock' ':!target/'
    git commit -m "$msg"
    git push origin main 2>&1 || {
      warn "Push failed for '$name', trying pull --rebase first..."
      git pull --rebase origin main
      git push origin main
    }
  )
  ok "Plugin '$name' pushed"
}

# ── 3. Update Cargo.lock ────────────────────────────────────────────

update_cargo_lock() {
  info "Updating Cargo.lock Git refs..."
  (
    cd "$ROOT"
    cargo update -p datazen-driver-api 2>/dev/null || true
    cargo update -p datazen-plugin-kiwi 2>/dev/null || true
    cargo update -p datazen-plugin-olap 2>/dev/null || true
  )

  local lock_diff
  lock_diff=$(cd "$ROOT" && git diff --stat -- Cargo.lock)
  if [[ -n "$lock_diff" ]]; then
    ok "Cargo.lock updated"
  else
    ok "Cargo.lock already up to date"
  fi
}

# ── Main ─────────────────────────────────────────────────────────────

echo ""
echo "═══════════════════════════════════════"
echo "  DataZen Repository Sync"
echo "═══════════════════════════════════════"
echo ""

sync_driver_api
echo ""

for plugin in kiwi olap; do
  sync_plugin "$plugin"
done
echo ""

update_cargo_lock

echo ""
echo "═══════════════════════════════════════"
echo "  Done! Review changes with: git status"
echo "═══════════════════════════════════════"
echo ""
