#!/usr/bin/env bash
# Caller side of the Rune serial build queue.
#   submit.sh <requester> <workdir> <command...>
# Enqueues one job, waits for the single serial worker, prints its output and
# exits with the job's exit code (drop-in replacement for running <command>).
# Portable: the queue dir and requester->worktree bindings are configurable.
set -uo pipefail
Q="${RUNE_BUILDQ:-${TMPDIR:-/tmp}/rune-buildq}"
requester=${1:?usage: submit.sh <requester> <workdir> <command...>}
workdir=${2:?usage: submit.sh <requester> <workdir> <command...>}
shift 2
command=$*

# Worktree binding (optional but recommended). $Q/bindings has one
# "requester<TAB>absolute_workdir" per line; when a requester is listed, the
# workdir must match exactly. This ensures a build compiles that requester's
# own (possibly uncommitted) worktree, never another track's.
bindings="$Q/bindings"
if [ -f "$bindings" ]; then
  expected=$(awk -F '\t' -v r="$requester" '$1==r{print $2; exit}' "$bindings")
  if [ -n "$expected" ] && [ "$workdir" != "$expected" ]; then
    echo "BUILDQ: workdir mismatch for $requester: expected $expected, got $workdir" >&2
    exit 2
  fi
fi
[ -d "$workdir" ] || { echo "BUILDQ: workdir does not exist: $workdir" >&2; exit 2; }
mkdir -p "$Q/inbox" "$Q/out"
id="${requester}-$(date +%s)-$$-$RANDOM"
jq -n --arg id "$id" --arg requester "$requester" --arg workdir "$workdir" \
  --arg command "$command" --arg created "$(date -u +%FT%TZ)" \
  '{id:$id,requester:$requester,workdir:$workdir,command:$command,created:$created}' \
  > "$Q/inbox/$id.tmp" || exit 2
mv "$Q/inbox/$id.tmp" "$Q/inbox/$id.json" || exit 2
echo "BUILDQ: queued $id" >&2
waited=0
while [ ! -f "$Q/out/$id.result" ]; do
  sleep 1
  waited=$((waited + 1))
  if [ "$waited" -ge 3600 ]; then
    echo "BUILDQ: TIMEOUT waiting for $id (worker alive? check $Q/heartbeat)" >&2
    exit 124
  fi
done
actual=$(jq -r '.workdir_actual // empty' "$Q/out/$id.result")
if [ -n "$actual" ] && [ "$actual" != "$workdir" ]; then
  echo "BUILDQ: worker executed in $actual but $workdir was requested — result discarded" >&2
  exit 3
fi
jq -r '.tail' "$Q/out/$id.result"
rc=$(jq -r '.exit_code' "$Q/out/$id.result")
echo "BUILDQ: $id finished rc=$rc" >&2
exit "$rc"
