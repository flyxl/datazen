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
 * Vitest runs test *files* in parallel and several guard suites mutate or read
 * the same trees, so every operation that observes or mutates the real working
 * tree goes through {@link withProbeLock} — an on-disk lock, which is atomic
 * across processes — instead of a per-process mutex that would not help.
 *
 * The lock is a single file, created with `O_CREAT | O_EXCL`: exactly one
 * process can create it, so exactly one process holds it. It used to be a
 * *directory*, and a directory is the wrong shape for a lock: "the directory
 * exists" is a state that a release and an acquisition have to agree on, and
 * they cannot — removing a directory is not atomic (it is a read, a set of
 * unlinks and a final `rmdir`), so a release could destroy a directory a
 * concurrent acquirer had just created. The failure mode is not a crash but a
 * *broken exclusion*: two processes inside the critical section at once, each
 * reading the other's probe as a real violation. A file has neither problem:
 * creating it is one atomic syscall, and removing it is one atomic syscall.
 */
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readdirSync,
  readFileSync,
  rmdirSync,
  rmSync,
  statSync,
  unlinkSync,
  writeFileSync,
  writeSync,
} from 'fs';
import { createHash } from 'crypto';
import { execFileSync } from 'child_process';
import { tmpdir } from 'os';
import { dirname, isAbsolute, join, resolve } from 'path';
import { fileURLToPath } from 'url';

export const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/**
 * The tree's **private** git directory.
 *
 * A worktree's `.git` is a *file* holding `gitdir: <path>`, and that path is
 * the worktree's own git dir (`.git/worktrees/<name>`) — private to that tree.
 * The main checkout's `.git` is a *directory*, and that directory is its own
 * git dir.
 *
 * `--git-common-dir` is deliberately **not** used: that is the *shared* git dir
 * every worktree of the repository points at, and a lock placed there would be
 * one lock for the whole repository — the opposite of what is wanted.
 *
 * The lock protects one *working tree*: a probe planted in one tree is not in
 * another tree, and a scan of one tree never reads the other tree's probe. So
 * two trees have nothing to exclude each other over, and one lock per tree is
 * what makes that true.
 */
function resolveGitDir(repoRoot: string): string {
  const dotGit = join(repoRoot, '.git');
  try {
    if (statSync(dotGit).isDirectory()) return dotGit;
    // A worktree: `.git` is a file pointing at the worktree's own git dir.
    const pointer = readFileSync(dotGit, 'utf8');
    const match = /^\s*gitdir:\s*(.+?)\s*$/m.exec(pointer);
    if (match) {
      const dir = match[1];
      return isAbsolute(dir) ? dir : resolve(repoRoot, dir);
    }
  } catch {
    // Not a worktree pointer, or unreadable — ask git itself.
  }
  try {
    const dir = execFileSync('git', ['rev-parse', '--git-dir'], {
      cwd: repoRoot,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
    if (dir) return isAbsolute(dir) ? dir : resolve(repoRoot, dir);
  } catch {
    // No git either.
  }
  // Neither: a private directory under the system temp dir, keyed by the
  // tree's path. Still one per tree, still outside the working tree.
  const key = createHash('sha256').update(repoRoot).digest('hex').slice(0, 16);
  return join(tmpdir(), `datazen-probe-lock-${key}`);
}

/**
 * The lock file of one working tree.
 *
 * It lives inside that tree's own git directory, which is why it can never
 * show up in `git status`, and why it is one file per tree rather than one
 * per repository.
 */
export function probeLockPath(repoRoot: string = REPO_ROOT): string {
  return join(resolveGitDir(repoRoot), 'dz-boundary-probe.lock');
}

/**
 * Where the lock used to live, before it was moved into the tree's git
 * directory.
 *
 * Under `node_modules/` — which, in a worktree, is a **symlink to the main
 * checkout's** `node_modules`. Every worktree therefore resolved to the *same*
 * lock file, so trees that cannot interfere with each other excluded each
 * other anyway: no correctness gain, only serialization. It is cleaned up
 * (never used) so the old artifact does not linger.
 */
const LEGACY_LOCK_PATH = join(REPO_ROOT, 'node_modules/.cache/dz-boundary-probe.lock');

const LOCK_PATH = probeLockPath();
export const LOCK_TIMEOUT_MS = 60_000;
/**
 * How long a lock whose owner cannot be read is given before it is taken over.
 *
 * **This number is an ASSUMPTION, not a measured upper bound.** What was
 * measured is the *gap* it has to cover, on one machine: the owner is written
 * synchronously right after the lock is created, so a lock that names nobody
 * is either a dead run (the common case: a kill between the two statements, or
 * a write that failed) or a live holder inside that write window. The gap
 * measured p50 0.12ms and a worst case of 17ms over two runs of 20k samples
 * here, and the shipped 2s is ~118x that worst observation. (The *median* would
 * make it look like four orders of magnitude, which is how this number used to
 * be described; the median is not the statistic a safety margin should be
 * sized on.) The 500ms floor the suite pins is the same measurement at ~29x.
 *
 * What is **not** measured is the gap on every machine this runs on: a
 * pathological scheduler pause, a loaded CI runner, or a machine whose clock
 * and filesystem timestamps disagree can all widen it beyond anything sampled
 * here. So the 2s is the size of a budget chosen against a measurement, and it
 * is *assumed* to be generous enough everywhere. If it is not, the failure is
 * the one the grace exists to prevent: a second process walks into the
 * critical section while the first is still in its write window.
 *
 * It only has to outlast the create-to-write gap, not a whole critical section
 * (a full-tree boundaries scan measures ~0.32s in process and ~0.42s as a CLI
 * here; ~3.4s was observed under concurrent load and is not reproducible solo),
 * because the owner is written into the lock for the entire rest of the run.
 *
 * That clause is a precondition, not a general truth, and the grace has to be
 * sized against it rather than against the critical section. While a holder is
 * inside the write window the lock names nobody at all: `readLockOwnerPid`
 * returns null, the pid check below is unreachable, and this grace is the only
 * thing between that live holder and a second process. It is deliberately under
 * the 5s per-test budget, so recovering from a killed run stays inside one
 * test instead of turning into a test timeout.
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
 *   - a lock whose owner cannot be read or parsed, where age is the
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

/**
 * Every subtree a mutation probe can land in.
 *
 * The probe is always a **real file in the real working tree** — that is the
 * whole point of it: a guard that has never seen a real file can be green for
 * the wrong reason (wrong directory, wrong root, walk skipped). So a probe left
 * behind by a killed run is not harmless debris either: it is a real violation
 * sitting in the tree, and every guard that scans that tree reads it as one on
 * the next run.
 *
 * Three subtrees, three suites:
 *
 *   - `packages/ui` — the design-system probes (`src/`, `dist/`, `coverage/`,
 *     `node_modules/vendored-lib/`), planted by `check-module-layers.test.ts`
 *     and `check-driver-import-boundaries.test.mjs`;
 *   - `packages/backend-client` — the port-rule probe, which is the one F-07's
 *     subject is scanned for, planted by `check-module-layers.test.ts`;
 *   - `scripts` — the "an unexercised rule cannot pass" probes.
 *
 * The sweep used to know only the first of the three, so a probe left in the
 * other two survived the next run and turned a killed run into a permanently
 * red one. The list is the whole inventory, and {@link PROBE_PATHS} is the
 * matching list of the paths the suites actually write.
 */
const PROBE_ROOTS = ['packages/ui', 'packages/backend-client', 'scripts'];

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

/** The pid that created the lock, or `null` if it left no readable owner. */
function readLockOwnerPid(lockPath: string): number | null {
  try {
    const parsed = JSON.parse(readFileSync(lockPath, 'utf8'));
    return typeof parsed?.pid === 'number' ? parsed.pid : null;
  } catch {
    return null;
  }
}

/**
 * Whether a lock left behind by a dead run can be taken over.
 *
 * A lock file outlives its holder: SIGKILL, an OOM kill or a closed terminal
 * runs no `finally`, so the file stays, nothing holds it, and every later run
 * waits out the full timeout before failing — and the file is under
 * `node_modules/`, so `git status` never shows it. Recovery therefore has to
 * be the *next* run's job, which is what this decides.
 *
 * No lock file at all is **not** stale: it is free, and the retry that follows
 * simply takes it.
 */
export function lockIsStale(lockPath: string, ownerGraceMs: number = LOCK_OWNER_GRACE_MS): boolean {
  let age: number;
  try {
    age = Date.now() - statSync(lockPath).mtimeMs;
  } catch {
    return false; // No lock there — it is free, and the retry will take it.
  }

  const ownerPid = readLockOwnerPid(lockPath);
  if (ownerPid === null) {
    // No usable owner: a kill between the create and the write, a write that
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
 * With no argument every subtree a probe can land in is swept (see
 * {@link PROBE_ROOTS}); with an explicit root only that root is.
 *
 * @returns repo-relative paths removed
 */
export function sweepStaleProbes(root?: string): string[] {
  const roots = root === undefined ? PROBE_ROOTS.map((rel) => join(REPO_ROOT, rel)) : [root];
  const removed: string[] = [];
  for (const one of roots) removed.push(...sweepProbeRoot(one));
  return removed;
}

/** {@link sweepStaleProbes}, restricted to one subtree. */
function sweepProbeRoot(root: string): string[] {
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
 * Give the lock back, without ever taking it away from somebody else.
 *
 * The removal is one `unlink` — atomic, and it removes exactly the file this
 * call created. The inode check is what makes "exactly" true even when this
 * holder was evicted in the meantime and the lock was taken over by somebody
 * else: a different file has a different inode, and that lock is not ours to
 * remove.
 */
function releaseLock(lockPath: string, ino: number): void {
  try {
    if (statSync(lockPath).ino !== ino) return; // Taken over — leave it alone.
    unlinkSync(lockPath);
  } catch {
    // Already gone.
  }
}

/**
 * Take over a lock whose holder is gone.
 *
 * Same rule as the release: only the file the verdict was about may be
 * removed. The inode is its identity, so a lock that was taken over between
 * the verdict and this call is left exactly as it is.
 */
function evictLock(lockPath: string, ino: number): void {
  try {
    if (statSync(lockPath).ino !== ino) return;
    unlinkSync(lockPath);
  } catch {
    // Already gone.
  }
}

/**
 * Remove a lock left behind by the *previous* lock format.
 *
 * The lock was a directory until it became a file; a directory left at the
 * path by an earlier run would make the exclusive create fail with EISDIR
 * forever, and it lives inside the tree's git directory, so `git status` never
 * shows it and nothing else would ever clear it.
 *
 * The lock the previous implementation used, under `node_modules/`, is left
 * alone on purpose: that path is shared by every worktree, and a tree still
 * running the previous format may be holding it right now. It is not a lock
 * any more — there is exactly one lock, the one in the tree's git directory.
 */
function clearLegacyLockDir(lockPath: string): void {
  try {
    if (statSync(lockPath).isDirectory()) rmSync(lockPath, { recursive: true, force: true });
  } catch {
    // No lock there, or it is already the file format.
  }
  // The location the previous implementation used, under `node_modules/`.
  // Removed only when it is **empty**: that path is shared by every worktree,
  // and a tree still running the previous format may be holding it right now.
  try {
    rmdirSync(LEGACY_LOCK_PATH);
  } catch {
    // Still held by a tree running the previous format, or already gone.
  }
}

/**
 * Run `fn` with exclusive access to the real working tree.
 *
 * Self-healing: a lock left by a killed run is detected and taken over rather
 * than waited out, and the probes that run left behind are swept before this
 * run starts — in **every** subtree a probe can land in, not only the first
 * one, because a leftover probe is a real violation sitting in the tree and
 * the next guard that scans that tree would report it as one.
 */
export function withProbeLock<T>(fn: () => T): T {
  return withProbeLockAt(
    LOCK_PATH,
    PROBE_ROOTS.map((root) => join(REPO_ROOT, root)),
    fn,
  );
}

/**
 * The lock + sweep machinery, pointed at a caller-chosen lock and probe roots.
 *
 * The parameters exist so the recovery paths can be tested against a private
 * lock instead of the shared one: a test that planted a fake lock at
 * {@link LOCK_PATH} would break the very mutual exclusion every other guard
 * test depends on.
 *
 * @param {string} lockPath the lock file to take
 * @param {string | readonly string[]} probeRoots the subtrees to sweep for debris
 * @param {number} [timeoutMs] how long to wait for a *live* holder before failing
 * @param {number} [ownerGraceMs] how long an ownerless lock is left alone
 */
export function withProbeLockAt<T>(
  lockPath: string,
  probeRoots: string | readonly string[],
  fn: () => T,
  timeoutMs: number = LOCK_TIMEOUT_MS,
  ownerGraceMs: number = LOCK_OWNER_GRACE_MS,
): T {
  mkdirSync(dirname(lockPath), { recursive: true });
  clearLegacyLockDir(lockPath);
  const deadline = Date.now() + timeoutMs;
  let cleared = 0;
  let fd = -1;
  for (;;) {
    try {
      // `wx` is `O_CREAT | O_EXCL | O_WRONLY`: the file is created only if it
      // does not exist, and the check and the create are one syscall. Exactly
      // one process can win it, so exactly one process holds the lock.
      fd = openSync(lockPath, 'wx');
      break;
    } catch (err) {
      if (!isErrnoException(err) || err.code !== 'EEXIST') throw err;
      let ino = 0;
      try {
        ino = statSync(lockPath).ino;
      } catch {
        ino = 0; // Vanished — the retry will win it.
      }
      if (cleared < MAX_STALE_CLEARANCES && lockIsStale(lockPath, ownerGraceMs)) {
        cleared += 1;
        evictLock(lockPath, ino);
        continue; // Retry immediately instead of sleeping on a dead holder.
      }
      if (Date.now() > deadline) {
        const ownerPid = readLockOwnerPid(lockPath);
        throw new Error(
          `withProbeLock: timed out after ${timeoutMs}ms waiting for ${lockPath}` +
            (ownerPid === null
              ? ' (the lock names nobody: the holder wrote none before the wait started)'
              : ` (owner pid ${ownerPid} is still running)`),
        );
      }
      sleepSync(25);
    }
  }
  let ino = 0;
  try {
    ino = statSync(lockPath).ino;
  } catch {
    // The identity is only used to make the release exact; without it the
    // release still removes the lock, it just cannot prove it is the same one.
  }
  try {
    // Written after the create, so a lock is never owned by a pid that has not
    // claimed it yet; until then the age cap is the only signal.
    writeSync(fd, JSON.stringify({ pid: process.pid, at: Date.now() }));
  } finally {
    closeSync(fd);
  }
  try {
    for (const root of typeof probeRoots === 'string' ? [probeRoots] : probeRoots) {
      sweepStaleProbes(root);
    }
    return fn();
  } finally {
    releaseLock(lockPath, ino);
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
 *
 * The list spans all three {@link PROBE_ROOTS}: a probe that is not covered
 * here is a probe whose gitignore coverage was never asserted, and one that
 * gitignore does not cover is a `git add -A` away from a committed test
 * artifact.
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
  'packages/backend-client/src/__boundaryProbe__.ts',
  'scripts/__boundaryProbe__/bare/__boundaryProbe__.ts',
  'scripts/__boundaryProbe__/buried/dist/__boundaryProbe__.ts',
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
