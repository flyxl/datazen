#!/usr/bin/env bash
# Test-tree pruning probe for datazen-ui on low-memory machines.
#
# Problem: full `cargo test -p datazen-ui --lib` links one giant test binary
# and is deterministically OOM-killed on this machine (GNU ld and lld both
# verified, 9 independent observations, link RSS ~5.9 GB against a 6.1 GiB
# cgroup). A failed link is not evidence: it says nothing about the code.
#
# This probe temporarily trims crates/datazen-ui/src/tests/mod.rs down to
# only the modules a slice needs (plus the always-kept support/tokens
# helpers), runs the real test binary, then restores mod.rs byte-identically.
# It never skips a test: it only compiles fewer test modules, so the run is
# real runtime red/green evidence for the modules it covers.
#
# Usage (from the worktree root):
#   scripts/rune-test-probe.sh <module> [<module> ...] [-- <extra cargo test args...>]
#
# Examples:
#   scripts/rune-test-probe.sh column_mapping_editor column_mapping_editor_audit
#   scripts/rune-test-probe.sh ai_chat_panel_input_area ai_chat_panel_input_area_audit -- --exact
#
# Safety:
# - works on the CURRENT mod.rs (including uncommitted tester registrations);
# - backup in $TMPDIR + restore on every exit path (normal, error, INT/TERM);
#   restore is verified byte-identical and a failed restore exits non-zero
#   LOUDLY instead of silently leaving a trimmed tree behind;
# - flock guard: a second concurrent probe refuses instead of clobbering;
# - an unknown module name refuses before touching anything.
#
# Evidence: the full cargo log is kept in $TMPDIR (path printed on every
# run); the summary prints the exit code and the `test result:` lines.
# Never report a pass count from the number of #[test] attributes in a file:
# only a `test result: ok` line from a real run counts.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
export CARGO_BUILD_JOBS=1
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C debuginfo=0"

MOD=crates/datazen-ui/src/tests/mod.rs
LOCK="$CARGO_TARGET_DIR/.rune-test-probe.lock"

die() { echo "probe: $*" >&2; exit 2; }

# --- args: modules, then anything after `--` goes to cargo test ---
mods=()
extra=()
seen_sep=0
for a in "$@"; do
    if [ "$seen_sep" -eq 1 ]; then extra+=("$a"); continue; fi
    if [ "$a" = "--" ]; then seen_sep=1; continue; fi
    mods+=("$a")
done
[ "${#mods[@]}" -ge 1 ] || die "usage: $0 <module> [<module> ...] [-- <extra cargo test args...>]"
for m in "${mods[@]}"; do
    [[ "$m" =~ ^[A-Za-z0-9_]+$ ]] || die "bad module name: $m"
    grep -Eq "^(pub\(crate\) )?mod ${m};" "$MOD" || die "unknown test module: $m (not a 'mod $m;' line in $MOD)"
done

# --- one probe at a time (also guards against a second agent trimming) ---
mkdir -p "$CARGO_TARGET_DIR"
exec 9>"$LOCK"
flock -n 9 || die "another probe is running (lock $LOCK); wait for it, do not trim by hand"

BACKUP=$(mktemp)
LOG=$(mktemp)
cp "$MOD" "$BACKUP"

RESTORED=0
restore() {
    [ "$RESTORED" -eq 1 ] && return 0
    RESTORED=1
    if cp "$BACKUP" "$MOD" && cmp -s "$BACKUP" "$MOD"; then
        echo "probe: mod.rs restored (byte-identical to pre-probe state)"
        return 0
    fi
    echo "probe: RESTORE FAILED — $MOD differs from backup $BACKUP; fix by hand, do NOT commit a trimmed tree" >&2
    return 1
}
trap 'restore; exit 130' INT TERM

full_count=$(grep -cE '^(pub\(crate\) )?mod [A-Za-z0-9_]+;$' "$MOD")

# Trim: drop every `mod X;` line except support/tokens and the requested ones.
# Everything else (imports, `runtime()`, comments) is kept verbatim, so the
# file still compiles exactly as before, only smaller.
# An outer attribute (e.g. `#[cfg(test)]`) sitting directly above a dropped
# `mod` line is dropped with it: leaving it behind is a dangling attribute
# and the file stops compiling (`expected item after attributes`). Attribute
# lines are therefore held back until the following item line decides them.
KEEP="support tokens ${mods[*]}"
if ! KEEP="$KEEP" awk '
function modname(line,   n) {
    if (line ~ /^(pub\(crate\) )?mod [A-Za-z0-9_]+;$/) {
        n = line
        sub(/^(pub\(crate\) )?mod /, "", n)
        sub(/;$/, "", n)
        return n
    }
    return ""
}
function flush() {
    if (held) { printf "%s", buf; buf = ""; held = 0 }
}
{
    if ($0 ~ /^[[:space:]]*#\[.*\]$/) { buf = buf $0 "\n"; held = 1; next }
    if (held && ($0 ~ /^[[:space:]]*$/ || $0 ~ /^[[:space:]]*\/\//)) { buf = buf $0 "\n"; next }
    name = modname($0)
    if (name == "") { flush(); print; next }
    split(ENVIRON["KEEP"], arr, " ")
    for (i in arr) if (arr[i] == name) { flush(); print; next }
    # dropped module line: any attribute (+ blank/comment) lines held above
    # it go with it, so no dangling attribute is left behind.
    buf = ""; held = 0
}
END { # a dangling attribute at EOF never compiled anyway; drop it silently
}
' "$MOD" > "$MOD.tmp"; then
    echo "probe: trim failed; mod.rs untouched" >&2
    exit 2
fi
mv "$MOD.tmp" "$MOD"
kept_count=$(grep -cE '^(pub\(crate\) )?mod [A-Za-z0-9_]+;$' "$MOD")
echo "probe: test modules $full_count -> $kept_count (kept: ${mods[*]})"
echo "probe: log $LOG"

rc=0
cargo test -p datazen-ui --lib --offline "${extra[@]}" >"$LOG" 2>&1 || rc=$?

grep -E '^test result:' "$LOG" || true
bin=$(ls -t "$CARGO_TARGET_DIR/debug/deps"/datazen_ui-* 2>/dev/null | grep -v -E '\.(d|rlib|so|rmeta)$' | head -n 1 || true)
[ -n "${bin:-}" ] && du -h "$bin" | sed 's/^/probe: linked test binary /'

if [ "$rc" -eq 0 ]; then echo "probe: PASS (exit 0)"; else echo "probe: FAIL (exit $rc)"; fi

restore || exit 3
exit "$rc"
