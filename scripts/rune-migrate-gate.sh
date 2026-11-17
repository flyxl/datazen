#!/usr/bin/env bash
# Gate for every React → Rune port task.
#
# A task may leave the tree uncommitted and broken while it is in flight, but
# it is not done until this script is green. Run it from the worktree root:
#
#   scripts/rune-migrate-gate.sh          check + unit tests
#   scripts/rune-migrate-gate.sh --fast   check only (inner loop)
#
# Timings on the reference machine (warm target dir): check ~2.5s, tests ~6.5s.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"

fast=0
[ "${1:-}" = "--fast" ] && fast=1

fail=0
step() {
    local name="$1"; shift
    local log
    log=$(mktemp)
    if "$@" >"$log" 2>&1; then
        local warns
        warns=$(grep -c '^warning: ' "$log" || true)
        printf 'ok    %-28s %s warnings\n' "$name" "${warns:-0}"
    else
        printf 'FAIL  %-28s\n' "$name"
        tail -30 "$log"
        fail=1
    fi
    rm -f "$log"
}

step "check datazen-ui" cargo check -p datazen-ui --offline
if [ "$fail" -eq 0 ] && [ "$fast" -eq 0 ]; then
    step "check the three crates" cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline
    step "test ui + ui-api" cargo test -p datazen-ui-api -p datazen-ui --offline --lib
fi

[ "$fail" -eq 0 ] && echo "gate: PASS" || echo "gate: FAIL"
exit "$fail"
