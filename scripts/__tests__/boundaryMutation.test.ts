/**
 * @vitest-environment node
 *
 * Self-healing of the probe lock and the probe sweep.
 *
 * A lock file outlives its holder: SIGKILL, an OOM kill or a closed
 * terminal run no `finally`, so the directory stays, nothing holds it, and every
 * later run waits out the full timeout before failing — from a directory under
 * `node_modules/`, which `git status` never shows. The only available defence is
 * that the *next* run notices and recovers, which is what these cases pin.
 *
 * Each case drives `withProbeLockAt` against a private lock under a temp dir
 * rather than the shared one: a test that planted a fake lock at the shared path
 * would break the mutual exclusion every other guard test relies on.
 */
import { spawn } from 'child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, unlinkSync, utimesSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { dirname, join } from 'path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  LOCK_MAX_AGE_MS,
  LOCK_OWNER_GRACE_MS,
  LOCK_TIMEOUT_MS,
  lockIsStale,
  sweepStaleProbes,
  withProbeLockAt,
} from './boundaryMutation';

/** A pid that is guaranteed not to be running: one past the kernel's max. */
const DEAD_PID = 0x7fff_fffe;

/**
 * A holder that behaves like a real one caught in the write window: it takes
 * the lock, announces itself, and then stays alive without ever writing its
 * owner. It exits on its own only if the lock disappears (another run took it
 * over) or after 30s, so a parent can kill it at a known state.
 */
const HOLDER_SCRIPT = `
const { openSync, closeSync, writeFileSync, existsSync } = require('node:fs');
const [lockPath, ready] = process.argv.slice(1);
const fd = openSync(lockPath, 'wx');
closeSync(fd);
writeFileSync(ready, 'held');
const pause = new Int32Array(new SharedArrayBuffer(4));
const deadline = Date.now() + 30_000;
while (existsSync(lockPath) && Date.now() < deadline) Atomics.wait(pause, 0, 0, 20);
`;

const sandboxes: string[] = [];

function sandbox() {
  const dir = join(tmpdir(), `dz-probe-lock-${process.pid}-${sandboxes.length}`);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(join(dir, 'probe'), { recursive: true });
  sandboxes.push(dir);
  return { lockDir: join(dir, 'probe.lock'), probeRoot: join(dir, 'probe') };
}

/**
 * Plant a lock as if a previous run had been killed while holding it.
 *
 * `ownerPid: null` reproduces the one shape this fix originally missed: a lock
 * whose holder died between winning the lock and writing its owner into it,
 * which is exactly what the pre-fix code left behind — a lock that names
 * nobody.
 */
function plantLock(lockPath: string, ownerPid: number | null, ageMs = 0) {
  writeFileSync(
    lockPath,
    ownerPid === null ? '' : JSON.stringify({ pid: ownerPid, at: Date.now() }),
  );
  if (ageMs > 0) {
    const when = new Date(Date.now() - ageMs);
    utimesSync(lockPath, when, when);
  }
}

function plantProbe(probeRoot: string, rel: string, body = 'leaked by a killed run\n') {
  const full = join(probeRoot, rel);
  mkdirSync(join(full, '..'), { recursive: true });
  writeFileSync(full, body);
  return full;
}

afterEach(() => {
  for (const dir of sandboxes.splice(0)) rmSync(dir, { recursive: true, force: true });
});

describe('withProbeLockAt · recovery from a killed run', () => {
  it('takes over a lock whose owner is gone instead of waiting it out', () => {
    const { lockDir, probeRoot } = sandbox();
    plantLock(lockDir, DEAD_PID);

    const started = Date.now();
    let entered = false;
    withProbeLockAt(lockDir, probeRoot, () => {
      entered = true;
    });

    expect(entered).toBe(true);
    // The point of the whole change: not one 25ms sleep, let alone 60s.
    expect(Date.now() - started).toBeLessThan(5_000);
    expect(existsSync(lockDir)).toBe(false);
  });

  it('takes over a lock with no owner file once the write window has passed', () => {
    // The artifact of the very event this recovery was written for: the
    // pre-fix code never wrote an owner into the lock at all, so every lock it
    // left was ownerless and unidentifiable. Age is the only signal, so the
    // decision is
    // a grace period — long enough not to steal a live holder mid-write,
    // short enough to recover inside a run.
    const { lockDir, probeRoot } = sandbox();
    plantLock(lockDir, null);

    expect(lockIsStale(lockDir)).toBe(false); // still inside the write window

    const started = Date.now();
    let entered = false;
    withProbeLockAt(
      lockDir,
      probeRoot,
      () => {
        entered = true;
      },
      2_500,
      150, // the shipped 2s is pinned by its own case; don't spend it here
    );

    expect(entered).toBe(true);
    expect(Date.now() - started).toBeLessThan(2_500);
    expect(existsSync(lockDir)).toBe(false);
  });

  it('recovers an ownerless lock and the probes it left, exactly as the pre-fix run did', () => {
    // End to end on the same shape, including the sweep: this is the state a
    // SIGKILL during the pre-fix critical section produced.
    const { lockDir, probeRoot } = sandbox();
    plantLock(lockDir, null);
    const leaked = plantProbe(probeRoot, 'src/__boundaryProbe__.ts');
    const bystander = plantProbe(probeRoot, 'src/index.ts', 'export const real = 1;\n');

    let leftovers: string[] = [];
    withProbeLockAt(
      lockDir,
      probeRoot,
      () => {
        leftovers = [leaked, bystander].filter((p) => existsSync(p));
      },
      2_500,
      150,
    );

    expect(leftovers).toEqual([bystander]);
  });

  it('waits for a live holder that has not written its owner yet, and takes over once it dies', async () => {
    // The reason a grace period exists at all rather than "no owner ⇒ stale".
    //
    // A *real* holder drives this: a child process that wins the lock and then
    // pauses — the state `withProbeLockAt` itself passes through between the
    // exclusive create and the write of the owner. Both directions are pinned
    // in one case
    // because they are one decision about one lock:
    //   · holder alive, no owner file → wait. Two processes in the critical
    //     section would collide on the same probe file, which is what the lock
    //     exists to prevent, and only the age can protect that window because
    //     there is no pid to check yet.
    //   · same lock, holder killed → take over. Otherwise a crashed run blocks
    //     every later run, which is the original defect this lock had.
    const { lockDir, probeRoot } = sandbox();
    const ready = join(dirname(lockDir), 'holder-ready');
    const child = spawn(process.execPath, ['-e', HOLDER_SCRIPT, lockDir, ready], {
      stdio: 'ignore',
    });
    try {
      await vi.waitFor(() => expect(existsSync(ready)).toBe(true), {
        timeout: 5_000,
        interval: 10,
      });

      // The precondition the earlier version of this case never checked: at the
      // moment the verdict is taken the holder is running and really has not
      // written its owner, so this cannot quietly degrade into the live-holder
      // case. The lock file exists but names nobody.
      expect(existsSync(lockDir)).toBe(true);
      expect(readFileSync(lockDir, 'utf8')).toBe('');
      expect(child.exitCode).toBe(null);
      expect(lockIsStale(lockDir)).toBe(false);

      let entered = false;
      expect(() => withProbeLockAt(lockDir, probeRoot, () => (entered = true), 300)).toThrow(
        /names nobody/,
      );
      expect(entered).toBe(false);
      // Untouched: another run's recovery must not evict a live holder.
      expect(existsSync(lockDir)).toBe(true);

      // The other side, on the very same lock and with no manual cleanup in
      // between. The holder dies without ever having written its owner, which is
      // the artifact the pre-fix run left behind.
      child.kill('SIGKILL');
      let recovered = false;
      withProbeLockAt(lockDir, probeRoot, () => (recovered = true), 2_500, 150);
      expect(recovered).toBe(true);
    } finally {
      try {
        child.kill('SIGKILL');
      } catch {
        // Already gone.
      }
    }
  });

  it('takes over an unreadable owner file once it is too old to be a write window', () => {
    // Garbage in the lock is the same as no pid: age decides.
    const { lockDir, probeRoot } = sandbox();
    plantLock(lockDir, process.pid);
    writeFileSync(lockDir, 'not json at all');
    const when = new Date(Date.now() - 11 * 60_000);
    utimesSync(lockDir, when, when);

    expect(lockIsStale(lockDir)).toBe(true);

    let entered = false;
    withProbeLockAt(lockDir, probeRoot, () => {
      entered = true;
    });
    expect(entered).toBe(true);
  });

  it('sweeps the probes the killed run left behind before running', () => {
    const { lockDir, probeRoot } = sandbox();
    plantLock(lockDir, DEAD_PID);
    const leaked = [
      plantProbe(probeRoot, 'src/__boundaryProbe__.ts'),
      plantProbe(probeRoot, 'dist/__boundaryProbe__.js'),
      plantProbe(probeRoot, 'node_modules/vendored-lib/__uiBoundaryProbe__.tsx'),
    ];

    let seen: string[] = [];
    withProbeLockAt(lockDir, probeRoot, () => {
      seen = leaked.filter((p) => existsSync(p));
    });

    expect(seen).toEqual([]);
  });

  it('leaves a real source file alone while sweeping', () => {
    const { lockDir, probeRoot } = sandbox();
    plantLock(lockDir, DEAD_PID);
    const real = plantProbe(probeRoot, 'src/PathInput.tsx', 'export const Button = 1;\n');

    withProbeLockAt(lockDir, probeRoot, () => {});

    expect(existsSync(real)).toBe(true);
  });

  it('takes over an aged lock whose pid has been reused', () => {
    // `process.kill(pid, 0)` cannot tell "the holder is alive" from "this pid
    // now belongs to something else", so age is the only second signal.
    const { lockDir, probeRoot } = sandbox();
    plantLock(lockDir, process.pid, 20 * 60_000);

    let entered = false;
    withProbeLockAt(lockDir, probeRoot, () => {
      entered = true;
    });

    expect(entered).toBe(true);
  });

  it('does not steal a lock from a live holder', () => {
    // The other half of the contract: recovery must not become two processes in
    // the critical section, or the probes collide and results get flaky. A
    // short timeout keeps the case fast — the wait itself is the assertion.
    const { lockDir, probeRoot } = sandbox();
    const holder = spawn('sleep', ['30'], { detached: true, stdio: 'ignore' });
    holder.unref();
    // `ChildProcess.pid` is genuinely optional in @types/node, and this case is
    // only meaningful with a live holder: without one there is nothing to steal
    // from and the assertion below would pass for the wrong reason.
    if (holder.pid === undefined) throw new Error('lock holder did not spawn; case is void');
    plantLock(lockDir, holder.pid);

    let entered = false;
    const started = Date.now();
    try {
      expect(() => withProbeLockAt(lockDir, probeRoot, () => (entered = true), 400)).toThrow(
        /timed out after 400ms/,
      );
      expect(lockIsStale(lockDir)).toBe(false);
      expect(Date.now() - started).toBeGreaterThanOrEqual(400);
    } finally {
      try {
        holder.kill('SIGKILL');
      } catch {
        // Already gone.
      }
    }
    expect(entered).toBe(false);
    // Untouched, so the real holder is not harmed by another run's recovery.
    expect(existsSync(lockDir)).toBe(true);
  });

  it('records the current pid as the owner while it holds the lock', () => {
    const { lockDir, probeRoot } = sandbox();
    let ownerPid = 0;
    withProbeLockAt(lockDir, probeRoot, () => {
      // Read it the way a competing process would.
      ownerPid = JSON.parse(readFileSync(lockDir, 'utf8')).pid;
    });
    expect(ownerPid).toBe(process.pid);
  });

  it('does not remove a lock a concurrent acquirer has taken', () => {
    // The release must remove **its own** lock and nothing else.
    //
    // The lock used to be a directory, and removing a directory is not atomic:
    // it is a read, a set of unlinks and a final `rmdir`, so a release could
    // destroy a directory a concurrent acquirer had just created. The two were
    // then inside the critical section at once — and the cost of that is not a
    // crash but a *false violation*: the probe one of them holds is the
    // violation the other one reports.
    //
    // The lock is a file now, and the release removes it only when it is still
    // the same file (the inode) the holder created. Here the lock is taken
    // over from under the holder, and the takeover's lock must survive.
    const { lockDir, probeRoot } = sandbox();
    const takeover = JSON.stringify({ pid: process.pid + 1, at: Date.now() });
    let entered = false;
    withProbeLockAt(lockDir, probeRoot, () => {
      entered = true;
      // The interleaving: somebody else holds the lock by the time this
      // holder releases it. A takeover replaces the lock — a new file, and so a
      // new inode — which is exactly what the release has to notice.
      unlinkSync(lockDir);
      writeFileSync(lockDir, takeover);
    });
    expect(entered).toBe(true);
    expect(existsSync(lockDir)).toBe(true);
    expect(readFileSync(lockDir, 'utf8')).toBe(takeover);
  });
});

describe('the shipped timing constants', () => {
  it('keeps the owner grace above the write window and inside one test budget', () => {
    // The windows are not interchangeable, and a value that reads reasonable in
    // isolation breaks the run in a way nothing else catches.
    //
    // Upper: a grace at or above the 5s per-test limit turns a crashed run into
    // a test timeout, so recovery stops being recovery. This is the assertion
    // that catches a "make it more generous" edit.
    expect(LOCK_OWNER_GRACE_MS).toBeLessThan(5_000);
    //
    // Lower: the one the design actually rests on. The grace only has to outlast
    // the create→write gap, measured on this machine at p50 0.12ms,
    // p99 0.33ms and a worst case of 17ms over two runs of 20k samples, plus
    // whatever a loaded machine adds on top. 500ms is ~29x the worst
    // observation, so a later "tighten it" edit cannot land inside the window,
    // and the case above turns red well before the number could reach it.
    expect(LOCK_OWNER_GRACE_MS).toBeGreaterThanOrEqual(500);
    //
    // The pid-reuse cap cannot be bounded by a run: `process.kill(pid, 0)`
    // cannot tell "held" from "this pid now belongs to something else", so age
    // is the only second signal. It must stay several times above the wait, or
    // a slow-but-live holder is stolen while a waiter is still waiting, and a
    // crashed run becomes reclaimable within one wait.
    expect(LOCK_MAX_AGE_MS).toBeGreaterThanOrEqual(5 * LOCK_TIMEOUT_MS);
  });
});

describe('sweepStaleProbes', () => {
  it('removes probe files and the directories they needed', () => {
    const { probeRoot } = sandbox();
    const leaked = [
      plantProbe(probeRoot, 'src/__boundaryProbe__.ts'),
      plantProbe(probeRoot, 'dist/__boundaryProbe__.js'),
      plantProbe(probeRoot, 'build/__boundaryProbe__.js'),
      plantProbe(probeRoot, 'coverage/__boundaryProbe__.js'),
      plantProbe(probeRoot, 'node_modules/vendored-lib/__boundaryProbe__.js'),
    ];
    plantProbe(probeRoot, 'src/index.ts', 'export const real = 1;\n');

    const removed = sweepStaleProbes(probeRoot);

    expect(removed.map((p) => p.slice(probeRoot.length + 1)).sort()).toEqual(
      leaked.map((p) => p.slice(probeRoot.length + 1)).sort(),
    );
    for (const dir of ['dist', 'build', 'coverage', 'node_modules']) {
      expect(existsSync(join(probeRoot, dir))).toBe(false);
    }
    expect(existsSync(join(probeRoot, 'src/index.ts'))).toBe(true);
  });

  it('never removes a directory that still holds something real', () => {
    const { probeRoot } = sandbox();
    plantProbe(probeRoot, 'node_modules/vendored-lib/index.ts', 'export const vendored = 1;\n');

    sweepStaleProbes(probeRoot);

    expect(existsSync(join(probeRoot, 'node_modules/vendored-lib/index.ts'))).toBe(true);
  });
});
