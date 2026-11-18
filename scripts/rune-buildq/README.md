# Rune serial build queue (per machine)

Every machine runs its OWN queue so its parallel coder/tester tracks never run
`cargo` concurrently. The scripts live in the repo; the queue state lives in a
per-machine temp dir so it is never committed.

## Layout

- `submit.sh` — caller side: enqueue a job, block, print output, exit with rc.
- `worker.sh` — the single serial worker: FIFO, one job at a time.
- Queue dir: `RUNE_BUILDQ` env, default `${TMPDIR:-/tmp}/rune-buildq`
  (subdirs `inbox/ running/ out/ logs/`, plus `bindings`, `heartbeat`, `worker.pid`, `STOP`).

## Setup (once per machine)

```bash
REPO=/path/to/datazen                       # your checkout
export RUNE_BUILDQ="${TMPDIR:-/tmp}/rune-buildq"

# requester -> worktree binding (one per parallel track you create)
mkdir -p "$RUNE_BUILDQ"
# e.g. requester rune-a1 works only in .worktrees/datazen-rune-a1:
printf 'rune-a1\t%s\n' "$REPO/.worktrees/datazen-rune-a1" >> "$RUNE_BUILDQ/bindings"
# ... one line per track coder/tester requester ...

chmod +x "$REPO/scripts/rune-buildq/"*.sh
nohup bash "$REPO/scripts/rune-buildq/worker.sh" > "$RUNE_BUILDQ/worker.out" 2>&1 &
sleep 2; cat "$RUNE_BUILDQ/heartbeat"          # must advance each second
```

## Use (coders / testers)

```bash
bash "$REPO/scripts/rune-buildq/submit.sh" rune-a1 "$PWD" \
  'CARGO_TARGET_DIR=$PWD/target bash scripts/rune-migrate-gate.sh'
```
`submit.sh` blocks until the worker finishes, prints the captured output and
exits with the job's exit code — a drop-in replacement for the same command.
Never run `cargo`/gate/test directly while other tracks are active.

## Safety

- `submit.sh` rejects a requester bound to a different workdir (exit 2), and
  rejects a result whose `workdir_actual` differs from the request (exit 3). So
  a build always compiles the requesting coder's own worktree state.
- Never edit source files while a job is queued/running; wait for `submit.sh`.
- Worker restart: `touch $RUNE_BUILDQ/STOP` → wait for exit → `rm STOP` → relaunch.
  Left-over `running/` jobs are re-queued automatically.
