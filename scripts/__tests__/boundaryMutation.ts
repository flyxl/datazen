/**
 * Mutation helper for the boundary guards.
 *
 * A synthetic rule over an in-memory tree proves a *predicate* fires. This
 * helper proves the **shipped configuration** fires: it drops a real file into
 * the real working tree, lets the guard walk the real directory, and removes
 * the file again afterwards. That is the only way to know a new guard has
 * teeth — a guard whose rule table is never exercised against a real file can
 * be green for the wrong reason (wrong directory, wrong root, walk skipped).
 *
 * Vitest runs test *files* in parallel and both guard suites mutate the same
 * `packages/ui/` tree, so every operation that observes or mutates that tree
 * goes through {@link withProbeLock} — an on-disk `mkdir` lock, which is atomic
 * across processes — instead of a per-process mutex that would not help.
 */
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';

export const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** Lives under `node_modules/` so it can never end up in `git status`. */
const LOCK_DIR = join(REPO_ROOT, 'node_modules/.cache/dz-boundary-probe.lock');
export const LOCK_TIMEOUT_MS = 60_000;
/**
 * How long a lock with no readable owner file is given before it is taken over.
 *
 * The owner file is written synchronously right after the `mkdir` wins, so a
 * missing one means either a dead run (the common case: a kill between the two
 * statements, or a write that failed) or the microsecond-wide window of a live
 * holder that has not written it yet. Treating "no owner" as stale outright
 * would let a second process into the critical section in exactly that window,
 * which is the collision the lock exists to prevent, so a grace period is
 * required rather than optional.
 *
 * It only has to outlast the mkdir-to-write gap, not a whole critical section
 * (a full-tree boundaries scan measures ~0.32s in process and ~0.42s as a CLI
 * here; ~3.4s was observed under concurrent load and is not reproducible solo),
 * because the owner file is present for the entire rest of the run.
 *
 * That clause is a precondition, not a general truth, and the grace has to be
 * sized against it rather than against the critical section. While a holder is
 * inside the write window it has no owner file at all: `readLockOwnerPid`
 * returns null, the pid check below is unreachable, and this grace is the only
 * thing between that live holder and a second process. The gap measures p50
 * 0.12ms and a worst case of 17ms over two runs of 20k samples, so the shipped
 * 2s is ~118x the worst observation. (The *median* would make it look like
 * four orders of magnitude, which is how this number used to be described; the
 * median is not the statistic a safety margin should be sized on.) The 500ms
 * floor the suite pins is the same measurement at ~29x. It is deliberately under
 * the 5s per-test budget, so recovering from a killed run stays inside one test
 * instead of turning into a test timeout.
 *
 * Scope, because the constant and the branches below are one unit: the ownerless
 * branch in {@link lockIsStale} is what reads this value, and the fifth
 * parameter of {@link withProbeLockAt} exists only so the cases can shrink it
 * instead of spending 2s each. All of it is test infrastructure: no non-test
 * file imports it, and neither guard CLI, `packages/ui` nor `src/` is touched
 * by it, so nothing decided here can reach host product code. It does run as
 * part of the host suite — `vitest.config.ts` includes the `.test.{ts,mjs}`
 * files under `scripts/__tests__` — so these cases *are* host tests.
 */
export const LOCK_OWNER_GRACE_MS = 2_000;
/**
 * Age at which a lock is taken over even though its recorded pid still looks
 * alive. Two situations need it, and neither is visible to a pid check:
 *
 *   - pid reuse: a killed holder's pid may already belong to an unrelated
 *     process, so `process.kill(pid, 0)` reports "alive" for a dead holder;
 *   - an owner file that exists but cannot be read or parsed, where age is the
 *     only signal there is.
 *
 * It is 10x *above* the 60s wait, not below it, and that is deliberate: pid
 * reuse is measured in however long a pid takes to come back, which no
 * single-run budget can bound. The consequence is exact — within one run the
 * cap can only fire for a lock that was already older than the cap when the run
 * started. Recovery *during* a run comes from the pid check and from
 * {@link LOCK_OWNER_GRACE_MS}. It is far above any observed critical section
 * (~0.42s as a CLI solo; ~3.4s was observed under concurrent load and is not
 * reproducible solo), so it cannot fire
 * against a live-but-slow holder; the 5x the suite requires is measured against
 * the 60s wait, which is the window a waiter can actually observe.
 */
export const LOCK_MAX_AGE_MS = 10 * 60_000;
/** How many stale locks to clear before falling back to waiting, as a loop guard. */
const MAX_STALE_CLEARANCES = 16;

/**
 * Filename prefixes reserved for mutation probes.
 *
 * Every probe must use one of them ({@link withTempSourceFiles} enforces it),
 * which is what makes {@link sweepStaleProbes} able to find a probe left behind
 * by a hard kill: a killed process gets no `finally`, so the only thing that can
 * clean up after it is the next run.
 */
const PROBE_PREFIXES = ['__boundaryProbe__', '__uiBoundaryProbe__'];

/** The subtree a probe can land in. Everything here is a throwaway test file. */
const PROBE_ROOT = 'packages/ui';

/**
 * Every subtree a probe can land in, and therefore every subtree the sweep
 * has to cover.
 *
 * The sweep is what recovers from a hard kill: a killed process gets no
 * `finally`, so the probe it planted stays in the tree — and a probe left in
 * `packages/backend-client/src` is not merely untidy. The F-07 guard scans
 * that subtree for transport literals, and a probe contains one *by
 * construction*, so a leftover probe turns the guard permanently red and
 * points at a file nobody wrote. Covering every probe root is what keeps
 * "a killed run leaves debris" from becoming "the guard is broken".
 */
const PROBE_ROOTS = [PROBE_ROOT, 'packages/backend-client/src'];

/** Synchronous sleep — the lock must not be awaited away inside a sync test. */
function sleepSync(ms: number) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

/** `catch` is `unknown` under this program, so narrow to the errno shape the
 *  two branches below test instead of casting the unknown. */
function isErrnoException(err: unknown): err is NodeJS.ErrnoException {
  return typeof err === 'object' && err !== null && 'code' in err;
}

/** `process.kill(pid, 0)` reports existence; `EPERM` means alive but unowned. */
function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    return isErrnoException(err) && err.code === 'EPERM';
  }
}

/** The pid that created the lock, or `null` if it left no readable owner file. */
function readLockOwnerPid(lockDir: string): number | null {
  try {
    const parsed = JSON.parse(readFileSync(join(lockDir, 'owner.json'), 'utf8'));
    return typeof parsed?.pid === 'number' ? parsed.pid : null;
  } catch {
    return null;
  }
}

/**
 * Whether a lock left behind by a dead run can be taken over.
 *
 * A `mkdir` lock alone is a lock that outlives its holder: SIGKILL, an OOM kill
 * or a closed terminal runs no `finally`, so the directory stays, nothing holds
 * it, and every later run waits out the full timeout before failing — and the
 * directory is under `node_modules/`, so `git status` never shows it. Recovery
 * therefore has to be the *next* run's job, which is what this decides.
 */
export function lockIsStale(lockDir: string, ownerGraceMs: number = LOCK_OWNER_GRACE_MS): boolean {
  let age: number;
  try {
    age = Date.now() - statSync(lockDir).mtimeMs;
  } catch {
    return true; // Vanished between the failed mkdir and here — retry will win.
  }

  const ownerPid = readLockOwnerPid(lockDir);
  if (ownerPid === null) {
    // No usable owner: a kill between the mkdir and the write, a write that
    // failed, an unreadable file — or a holder still inside that gap. Age is
    // the only signal, so the grace period is the whole decision.
    return age > ownerGraceMs;
  }
  // Ours to keep: a re-entrant call must not steal its own lock.
  if (ownerPid !== process.pid && !isProcessAlive(ownerPid)) return true;
  return age > LOCK_MAX_AGE_MS;
}

/**
 * Delete probes a previous run left behind, then the empty directories they
 * needed. The lock is held, so anything matching a reserved prefix is debris.
 *
 * @returns repo-relative paths removed
 */
export function sweepStaleProbes(root: string = join(REPO_ROOT, PROBE_ROOT)): string[] {
  const removed: string[] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else if (PROBE_PREFIXES.some((prefix) => entry.name.startsWith(prefix))) {
        rmSync(full, { force: true });
        removed.push(full);
      }
    }
  };
  if (existsSync(root)) walk(root);

  // Probe directories are created for the test and left behind empty by a kill.
  // `rmdirSync` fails on a non-empty directory, so this can never touch a real
  // one — and unlike `rmSync` it removes an empty directory at all.
  for (const rel of ['dist', 'build', 'coverage', 'node_modules/vendored-lib', 'node_modules']) {
    try {
      rmdirSync(join(root, rel));
    } catch {
      // Not empty (or already gone) — leave it alone.
    }
  }
  return removed;
}

/**
 * Run `fn` with exclusive access to the real tree the probes live in.
 *
 * Self-healing: a lock left by a killed run is detected and taken over rather
 * than waited out, and the probes that run left behind are swept before this
 * run starts.
 *
 * The lock is not only for the probers. A probe is a file in the working tree,
 * so a test that *reads* the tree — copying it, or scanning it for a rule —
 * has to hold the same lock, or it can observe a probe that is about to be
 * deleted (a vanished file) or a probe that is deliberately violating
 * (a finding nobody planted). Both were real failures, and both looked like
 * the reader's own bug.
 */
export function withProbeLock<T>(fn: () => T): T {
  return withProbeLockAt(
    LOCK_DIR,
    join(REPO_ROOT, PROBE_ROOT),
    fn,
    LOCK_TIMEOUT_MS,
    LOCK_OWNER_GRACE_MS,
    PROBE_ROOTS.slice(1).map((root) => join(REPO_ROOT, root)),
  );
}

/**
 * The lock + sweep machinery, pointed at a caller-chosen lock and probe root.
 *
 * The parameters exist so the recovery paths can be tested against a private
 * lock instead of the shared one: a test that plants a fake lock at
 * {@link LOCK_DIR} would break the very mutual exclusion every other guard test
 * depends on.
 *
 * @param {number} [timeoutMs] how long to wait for a *live* holder before failing
 * @param {number} [ownerGraceMs] how long an ownerless lock is left alone
 * @param {readonly string[]} [extraSweepRoots] further roots to sweep for debris
 */
export function withProbeLockAt<T>(
  lockDir: string,
  probeRoot: string,
  fn: () => T,
  timeoutMs: number = LOCK_TIMEOUT_MS,
  ownerGraceMs: number = LOCK_OWNER_GRACE_MS,
  extraSweepRoots: readonly string[] = [],
): T {
  mkdirSync(dirname(lockDir), { recursive: true });
  const deadline = Date.now() + timeoutMs;
  let cleared = 0;
  for (;;) {
    try {
      mkdirSync(lockDir);
      break;
    } catch (err) {
      if (!isErrnoException(err) || err.code !== 'EEXIST') throw err;
      if (cleared < MAX_STALE_CLEARANCES && lockIsStale(lockDir, ownerGraceMs)) {
        cleared += 1;
        rmSync(lockDir, { recursive: true, force: true });
        continue; // Retry immediately instead of sleeping on a dead holder.
      }
      if (Date.now() > deadline) {
        const ownerPid = readLockOwnerPid(lockDir);
        throw new Error(
          `withProbeLock: timed out after ${timeoutMs}ms waiting for ${lockDir}` +
            (ownerPid === null
              ? ' (no owner.json: the holder wrote none before the wait started)'
              : ` (owner pid ${ownerPid} is still running)`),
        );
      }
      sleepSync(25);
    }
  }
  try {
    // Written after the mkdir so a lock is never owned by a pid that has not
    // claimed it yet; until then the age cap is the only signal.
    writeFileSync(
      join(lockDir, 'owner.json'),
      JSON.stringify({ pid: process.pid, at: Date.now() }),
    );
    sweepStaleProbes(probeRoot);
    for (const extraRoot of extraSweepRoots) sweepStaleProbes(extraRoot);
    return fn();
  } finally {
    rmSync(lockDir, { recursive: true, force: true });
  }
}

/**
 * Reject a probe path that {@link sweepStaleProbes} could never find again.
 *
 * A hard kill skips the `finally` that deletes the file, and recovery relies on
 * the next run sweeping by filename prefix — so a probe that does not carry a
 * reserved prefix is a probe that can outlive the suite and show up in
 * `git status`.
 */
function assertProbePath(rel: string): void {
  const name = rel.slice(rel.lastIndexOf('/') + 1);
  if (PROBE_PREFIXES.some((prefix) => name.startsWith(prefix))) return;
  throw new Error(
    `probe filenames must start with one of ${PROBE_PREFIXES.join(' / ')} ` +
      `so a killed run's leftovers stay sweepable, got: ${rel}`,
  );
}

/**
 * Every repo-relative path the guard suites write to, for assertions that must
 * hold about all of them at once (see the gitignore check in the suite).
 */
export const PROBE_PATHS = [
  'packages/ui/src/__boundaryProbe__.ts',
  'packages/ui/src/__boundaryProbe__.tsx',
  'packages/ui/src/__boundaryProbe__.js',
  'packages/ui/src/__boundaryProbe__.jsx',
  'packages/ui/src/__boundaryProbe__.mjs',
  'packages/ui/src/__boundaryProbe__.cjs',
  'packages/ui/src/__uiBoundaryProbe__.tsx',
  'packages/ui/dist/__boundaryProbe__.ts',
  'packages/ui/dist/__boundaryProbe__.js',
  'packages/ui/coverage/__boundaryProbe__.js',
  'packages/ui/node_modules/vendored-lib/__boundaryProbe__.js',
];

/**
 * Run `run` while `rel` (repo-relative) exists with `contents`. The file is
 * always removed, including when `run` throws, and an existing file is never
 * overwritten.
 *
 * @param {string} rel repo-relative POSIX path
 * @param {string} contents file body to write
 */
export function withTempSourceFile<T>(rel: string, contents: string, run: () => T): T {
  return withTempSourceFiles([[rel, contents]], run);
}

/**
 * Run `run` while every `[relPath, contents]` pair exists on disk. Missing
 * parent directories are created and — and only those that did not exist
 * before — removed again afterwards, so a probe can exercise a guard's
 * directory-skipping behaviour (`dist/`, `node_modules/`, …) without leaving a
 * tree behind or deleting a real one.
 *
 * @param {Array<[string, string]>} entries repo-relative POSIX path → file body
 */
export function withTempSourceFiles<T>(
  entries: ReadonlyArray<readonly [string, string]>,
  run: () => T,
): T {
  return withProbeLock(() => {
    const createdDirs: string[] = [];
    const createdFiles: string[] = [];
    for (const [rel, contents] of entries) {
      assertProbePath(rel);
      const full = join(REPO_ROOT, rel);
      if (existsSync(full)) {
        cleanup();
        throw new Error(`withTempSourceFiles refused to overwrite a tracked file: ${rel}`);
      }
      let dir = dirname(full);
      const missing: string[] = [];
      while (!existsSync(dir)) {
        missing.push(dir);
        const parent = dirname(dir);
        if (parent === dir) break;
        dir = parent;
      }
      for (const d of missing.reverse()) {
        mkdirSync(d);
        createdDirs.push(d);
      }
      writeFileSync(full, contents);
      createdFiles.push(full);
    }

    function cleanup() {
      for (const file of createdFiles) rmSync(file, { force: true });
      // Deepest first, and only directories this helper created.
      for (const d of createdDirs.sort((a, b) => b.length - a.length)) {
        rmSync(d, { recursive: true, force: true });
      }
    }

    try {
      return run();
    } finally {
      cleanup();
    }
  });
}
