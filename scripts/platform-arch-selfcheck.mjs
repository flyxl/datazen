#!/usr/bin/env node
/**
 * Mutation proof for `check-platform-crate-boundaries.mjs`.
 *
 * Run: `pnpm test:platform-arch:mutations` (or `node scripts/platform-arch-selfcheck.mjs`).
 *
 * ## Why this is a standalone command and not a vitest file
 *
 * `boundaryMutation.ts` proves the source-level guards by writing probe files
 * into the real tree. Its analogues here would rewrite `Cargo.toml` manifests
 * and let cargo rewrite `Cargo.lock` — and `pnpm test:scripts` runs inside the
 * `frontend:` CI job, which has **no Rust toolchain and no cargo**. A suite
 * that rewrote manifests there would either no-op or fail for a reason that
 * has nothing to do with the guard. So this is an explicit, on-demand command
 * wired next to the `rust:` job's toolchain, not part of `test:scripts`.
 *
 * ## What it proves, and what it does not
 *
 * Each mutation is applied to the real tree, the guard is re-run against the
 * real `cargo metadata` graph, and the mutation is **reverted** — all before
 * the next one starts. A mutation that does not take effect is reported as a
 * failure of the *proof*, never silently counted as a red guard: a guard that
 * goes red for an unrelated reason (a broken manifest, a cargo error) would
 * otherwise "pass" this suite while proving nothing.
 *
 * The symmetric risk is a guard that is red on the clean tree and stays red
 * for a reason no mutation touched. That is the baseline check below, and it
 * is why the suite fails when `git status` is dirty on entry: it cannot tell
 * the difference, and neither can you.
 *
 * ## The lock
 *
 * `withProbeLockAt` in `scripts/__tests__/boundaryMutation.ts` is the right
 * pattern but is not reusable here: it is TypeScript under a vitest program,
 * and this file is run by `node` directly, so importing it would mean adding a
 * transpile step to the mutation command. The lock below also guards a
 * *different* resource — Cargo manifests and `Cargo.lock` rather than source
 * probes under `packages/ui/` — so it takes a different lock directory and
 * shares no state with the source-level suite.
 *
 * Nothing here is written inside the repository except the mutations
 * themselves, and those are reverted before the command exits. Stub crates go
 * to the system temp directory so no build artefact ever lands in the tree.
 */

import { spawnSync } from 'child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from 'fs';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const GUARD = join(REPO_ROOT, 'scripts/check-platform-crate-boundaries.mjs');
const LOCK_DIR = join(tmpdir(), 'datazen-platform-arch-selfcheck.lock');
const PREFIX = '[platform-arch-selfcheck]';

/** Keep the log readable without hiding which mutation produced a line. */
function out(line) {
  process.stdout.write(`${PREFIX} ${line}\n`);
}

// ---------------------------------------------------------------------------
// Tree snapshot: the only thing standing between this script and a worktree
// that silently differs from its commit.
// ---------------------------------------------------------------------------

class TreeSnapshot {
  #backups = [];
  #created = [];
  #dir = null;

  constructor(root) {
    this.root = root;
  }

  /** Copy `rel` aside so a later {@link revert} can put it back byte for byte. */
  backup(rel) {
    const abs = join(this.root, rel);
    this.#dir ??= mkdtempSync(join(tmpdir(), 'dz-arch-backup-'));
    const dest = join(this.#dir, String(this.#backups.length));
    copyFileSync(abs, dest);
    this.#backups.push({ rel, abs, dest });
  }

  /**
   * Record a path this run created, for recursive removal on revert.
   *
   * Refuses a path that already exists. A revert here is `rm -rf`, so
   * registering a pre-existing directory destroys tracked source: the files were
   * never in {@link backup}, so nothing can put them back. Two mutations used to
   * do exactly that — they were written before `packages/platform-api` and
   * `packages/backend-client` landed, and once those packages existed the
   * revert silently deleted 41 tracked files and reported `REVERT FAILED`
   * *after* the damage. A probe must never be able to delete what it is
   * probing; fail loudly at registration instead.
   */
  createdDir(rel) {
    if (existsSync(join(this.root, rel)))
      throw new Error(
        `createdDir(\`${rel}\`) would make the revert rm -rf a directory that already exists. ` +
          `A mutation that needs to touch a real package must back up the individual files it ` +
          `overwrites (snapshot.backup) and must not register the package directory for removal.`,
      );
    this.#created.push(rel);
  }

  revert() {
    for (const { abs, dest } of this.#backups) {
      copyFileSync(dest, abs);
      // Cargo compares manifests by mtime; a restored file with a stale one
      // can be skipped by the next build and hide a real change.
      const now = new Date();
      utimesSync(abs, now, now);
    }
    for (const rel of this.#created) rmSync(join(this.root, rel), { recursive: true, force: true });
    if (this.#dir) rmSync(this.#dir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// Probe-lock: mutual exclusion between two selfcheck runs on one worktree.
// ---------------------------------------------------------------------------

function withLock(fn) {
  mkdirSync(dirname(LOCK_DIR), { recursive: true });
  const deadline = Date.now() + 120_000;
  for (;;) {
    try {
      mkdirSync(LOCK_DIR);
      break;
    } catch (e) {
      if (e.code !== 'EEXIST') throw e;
      if (Date.now() > deadline) {
        rmSync(LOCK_DIR, { recursive: true, force: true });
        throw new Error(`another platform-arch selfcheck still holds ${LOCK_DIR}`);
      }
      sleepSync(250);
    }
  }
  try {
    writeFileSync(join(LOCK_DIR, 'owner.json'), JSON.stringify({ pid: process.pid, at: Date.now() }));
    return fn();
  } finally {
    rmSync(LOCK_DIR, { recursive: true, force: true });
  }
}

function sleepSync(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

// ---------------------------------------------------------------------------
// Mutation helpers.
// ---------------------------------------------------------------------------

/**
 * A throwaway crate outside the repository.
 *
 * Using a real crate named `tauri` / `axum` / `react` rather than a name like
 * `tauri-fake` is the point: the guard's rules are stated as crate *families*
 * and a stub named `tauri-fake` would let a family-matching bug hide.
 * Path-depending on it keeps the run offline and keeps the registry lockfile
 * churn out of the probe.
 */
function stubCrate(name, scratch) {
  const dir = join(scratch, name.replace(/[^\w.-]/g, '_'));
  mkdirSync(join(dir, 'src'), { recursive: true });
  writeFileSync(
    join(dir, 'Cargo.toml'),
    `[package]\nname = "${name}"\nversion = "0.0.0"\nedition = "2021"\n\n[dependencies]\n`,
  );
  writeFileSync(join(dir, 'src/lib.rs'), `// probe stub for ${name}\npub fn probe() {}\n`);
  return dir;
}

/** Create a workspace member directory; `manifest` is the whole Cargo.toml. */
function addMemberDir(snapshot, rel, name, manifestBody) {
  mkdirSync(join(REPO_ROOT, rel, 'src'), { recursive: true });
  snapshot.createdDir(rel);
  writeFileSync(
    join(REPO_ROOT, rel, 'Cargo.toml'),
    `[package]\nname = "${name}"\nversion = "0.0.1"\nedition = "2021"\n\n${manifestBody}`,
  );
  writeFileSync(join(REPO_ROOT, rel, 'src/lib.rs'), '// arch-guard probe\npub fn probe() {}\n');
}

/**
 * Append a path dependency to an existing manifest.
 *
 * `packages/runtime/Cargo.toml` is chosen over any driver manifest on
 * purpose: `scripts/resolve-drivers.mjs` owns the placeholder sections under
 * `packages/drivers/*` and rewrites them during every build, so a probe that
 * edited one would be racing a generator rather than testing the guard.
 */
function addDependency(snapshot, rel, line) {
  snapshot.backup(rel);
  const abs = join(REPO_ROOT, rel);
  writeFileSync(abs, `${readFileSync(abs, 'utf8').trimEnd()}\n${line}\n`);
}

/**
 * Append a path dependency into one named TOML table instead of the end of file.
 *
 * `addDependency` is only correct when `[dependencies]` is the last table.
 * `packages/platform-api/Cargo.toml` ends with `[dev-dependencies]`, so an
 * end-of-file append there would silently produce a dev-dependency — a weaker
 * probe than the rule intends, and one whose outcome depends on whether
 * `cargo metadata` happens to resolve dev-deps of a workspace member.
 */
function addDependencyTo(snapshot, rel, table, line) {
  snapshot.backup(rel);
  const abs = join(REPO_ROOT, rel);
  const text = readFileSync(abs, 'utf8');
  const out2 = text.replace(`\n[${table}]\n`, `\n[${table}]\n${line}\n`);
  if (out2 === text) throw new Error(`${rel} has no [\`${table}\`] table to extend`);
  writeFileSync(abs, out2);
}

/**
 * Temporarily add a workspace member path.
 *
 * Required for anything outside `packages/drivers/*`, which is the only member
 * entry written as a glob. Without this the crate is simply not a member,
 * `cargo metadata` never sees it, and the guard correctly reports the layer as
 * vacuous — which would look like a silent pass.
 */
function addMembers(snapshot, relPaths) {
  snapshot.backup('Cargo.toml');
  const abs = join(REPO_ROOT, 'Cargo.toml');
  const text = readFileSync(abs, 'utf8');
  const added = relPaths.map((p) => `    "${p}",`).join('\n');
  const out2 = text.replace(/(members = \[\n)/, `$1${added}\n`);
  if (out2 === text) throw new Error('root Cargo.toml has no `members = [` line to extend');
  writeFileSync(abs, out2);
}

// ---------------------------------------------------------------------------
// Running the guard.
// ---------------------------------------------------------------------------

function gitStatusPorcelain() {
  const r = spawnSync('git', ['status', '--porcelain'], { cwd: REPO_ROOT, encoding: 'utf8' });
  return (r.stdout ?? '').trim();
}

/** @returns {{code: number, stdout: string}} */
function runGuard(env = {}) {
  const r = spawnSync(process.execPath, [GUARD], {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, ...env },
  });
  return { code: r.status ?? -1, stdout: `${r.stdout ?? ''}${r.stderr ?? ''}` };
}

/**
 * Did the guard actually notice this mutation?
 *
 * Matching on the F-id alone would accept a run that failed for an unrelated
 * reason, so the line must also name the crate the mutation touched.
 */
function violationMentions(stdout, rule, crateName) {
  // The guard prefixes every line with `[check-platform-arch]` and separates the
  // rule id from the message with two spaces, so the match anchors on the
  // `VIOLATION <F-id> ` token rather than the start of the line.
  return stdout
    .split('\n')
    .some((l) => l.includes(`VIOLATION ${rule} `) && l.includes(crateName));
}

// ---------------------------------------------------------------------------
// The mutations.
// ---------------------------------------------------------------------------

/** Read a repo file, or '' when it is absent. */
function readOrEmpty(rel) {
  const abs = join(REPO_ROOT, rel);
  return existsSync(abs) ? readFileSync(abs, 'utf8') : '';
}

/**
 * Every mutation returns the check that proves it is on disk.
 *
 * Without this step a mutation that silently failed to apply — a typo in a
 * path, a manifest cargo never read — would be counted as a red guard, and the
 * suite would report "F-02 verified" for an F-02 that was never exercised.
 */
const MUTATIONS = [
  {
    id: 'M1',
    rule: 'F-01',
    title: 'a driver crate takes a dependency on datazen-runtime',
    crate: 'datazen-driver-arch-probe',
    apply(snapshot) {
      // `packages/drivers/*` is a glob, so a new directory is a member with no
      // edit to the root manifest at all.
      addMemberDir(
        snapshot,
        'packages/drivers/zz-arch-probe',
        'datazen-driver-arch-probe',
        `[dependencies]\ndatazen-runtime = { path = "${join(REPO_ROOT, 'packages/runtime')}" }\n`,
      );
      return () => readOrEmpty('packages/drivers/zz-arch-probe/Cargo.toml').includes('datazen-runtime');
    },
  },
  {
    id: 'M2',
    rule: 'F-02',
    title: 'the platform kernel takes a dependency on a tauri* crate',
    crate: 'datazen-runtime',
    apply(snapshot, scratch) {
      addDependency(snapshot, 'packages/runtime/Cargo.toml', `tauri = { path = "${stubCrate('tauri', scratch)}" }`);
      return () => /^tauri = \{ path = /m.test(readOrEmpty('packages/runtime/Cargo.toml'));
    },
  },
  {
    id: 'M3',
    rule: 'F-03',
    title: 'the platform kernel takes a dependency on an HTTP framework',
    crate: 'datazen-runtime',
    apply(snapshot, scratch) {
      addDependency(snapshot, 'packages/runtime/Cargo.toml', `axum = { path = "${stubCrate('axum', scratch)}" }`);
      return () => /^axum = \{ path = /m.test(readOrEmpty('packages/runtime/Cargo.toml'));
    },
  },
  {
    id: 'M4',
    rule: 'F-04',
    title: 'platform-api depends back on datazen-runtime (ports <-> use cases cycle)',
    crate: 'datazen-platform-api',
    apply(snapshot) {
      // `packages/platform-api` is a real package now, and already a workspace
      // member, so this only adds the back-edge. It must back up the manifest it
      // edits rather than register the package directory for removal: the revert
      // here is an `rm -rf`, and registering a real package deleted its 26
      // tracked files.
      addDependencyTo(
        snapshot,
        'packages/platform-api/Cargo.toml',
        'dependencies',
        `datazen-runtime = { path = "${join(REPO_ROOT, 'packages/runtime')}" }`,
      );
      return () =>
        /^\[dependencies\]$/m.test(readOrEmpty('packages/platform-api/Cargo.toml')) &&
        /datazen-runtime = \{ path = /m.test(readOrEmpty('packages/platform-api/Cargo.toml'));
    },
  },
  {
    id: 'M5',
    rule: 'F-05',
    title: 'the platform kernel takes a dependency on a UI-runtime crate',
    crate: 'datazen-runtime',
    apply(snapshot, scratch) {
      addDependency(snapshot, 'packages/runtime/Cargo.toml', `react = { path = "${stubCrate('react', scratch)}" }`);
      return () => /^react = \{ path = /m.test(readOrEmpty('packages/runtime/Cargo.toml'));
    },
  },
  {
    id: 'M6',
    rule: 'F-06',
    title: 'the server workspace takes a dependency on a tauri* crate',
    crate: 'datazen-server',
    apply(snapshot, scratch) {
      addMembers(snapshot, ['server']);
      addMemberDir(
        snapshot,
        'server',
        'datazen-server',
        `[dependencies]\ntauri = { path = "${stubCrate('tauri', scratch)}" }\n`,
      );
      return () =>
        readOrEmpty('Cargo.toml').includes('"server",') && readOrEmpty('server/Cargo.toml').includes('tauri =');
    },
  },
  {
    id: 'M7',
    rule: 'F-07',
    title: 'backend-client calls fetch() — a transport-agnostic contract gains a transport',
    crate: 'packages/backend-client',
    apply(snapshot) {
      // `packages/backend-client` is a real package now. Overwrite the one
      // source file and back it up; do not register the package directory for
      // removal — that deleted its 15 tracked files on revert.
      const rel = 'packages/backend-client/src/index.ts';
      snapshot.backup(rel);
      writeFileSync(
        join(REPO_ROOT, rel),
        'export async function probe(url: string) {\n  const res = await fetch(url);\n  return res.json();\n}\n',
      );
      return () => readOrEmpty(rel).includes('fetch(');
    },
  },
  {
    id: 'M8',
    rule: 'unclassified',
    title: 'a workspace member appears under a path no rule covers',
    crate: 'crates/arch-probe',
    apply(snapshot) {
      // The load-bearing negative: a crate nobody thought about must be an
      // error, not an unexamined green.
      addMembers(snapshot, ['crates/arch-probe']);
      addMemberDir(snapshot, 'crates/arch-probe', 'arch-probe', '[dependencies]\n');
      return () =>
        readOrEmpty('Cargo.toml').includes('"crates/arch-probe",') &&
        readOrEmpty('crates/arch-probe/Cargo.toml').includes('name = "arch-probe"');
    },
  },
];

// ---------------------------------------------------------------------------
// Driver.
// ---------------------------------------------------------------------------

function runMutation(mutation) {
  const snapshot = new TreeSnapshot(REPO_ROOT);
  const scratch = mkdtempSync(join(tmpdir(), 'dz-arch-mut-'));
  // Cargo rewrites this on every manifest change. Without the backup, a probe
  // leaves a lockfile diff behind that no revert path would notice.
  snapshot.backup('Cargo.lock');
  try {
    const assertApplied = mutation.apply(snapshot, scratch);
    if (!assertApplied()) {
      return {
        ok: false,
        code: 0,
        why: 'the mutation did not take effect on disk — the probe itself is broken, so the guard was never exercised',
      };
    }
    const { code, stdout } = runGuard();
    const mentions =
      mutation.rule === 'unclassified'
        ? stdout.split('\n').some((l) => l.includes('ERROR') && l.includes('crates/arch-probe'))
        : violationMentions(stdout, mutation.rule, mutation.crate);

    const cargoBroke = /could not parse|Could not read|failed to load|error: no matching package named/i.test(
      stdout,
    );
    if (cargoBroke && !mentions) {
      return { ok: false, code, why: `guard failed on a broken build, not on ${mutation.rule}` };
    }
    if (code === 0) return { ok: false, code, why: `guard stayed green (exit 0)` };
    if (!mentions) {
      return {
        ok: false,
        code,
        why: `guard exited ${code} but never named ${mutation.rule}/${mutation.crate}`,
      };
    }
    const line = stdout
      .split('\n')
      .find((l) => /VIOLATION |ERROR /.test(l) && l.includes(mutation.crate));
    return { ok: true, code, line };
  } finally {
    snapshot.revert();
    rmSync(scratch, { recursive: true, force: true });
  }
}

function main() {
  const before = gitStatusPorcelain();
  if (before !== '') {
    out('REFUSING TO RUN — the worktree has uncommitted changes, so a revert cannot be verified:');
    process.stdout.write(`${before}\n`);
    out('Commit or stash them, then re-run.');
    return 2;
  }

  const baseline = runGuard();
  if (baseline.code !== 0) {
    out('BASELINE FAILED — the guard is already red on the clean tree, so a red mutation proves nothing.');
    process.stdout.write(`${baseline.stdout.split('\n').slice(-20).join('\n')}\n`);
    return 1;
  }
  out(`baseline PASS (exit 0) — no false positive on the current tree`);

  const results = [];
  for (const mutation of MUTATIONS) {
    const r = runMutation(mutation);
    results.push({ ...r, mutation });
    out(
      `${r.ok ? 'ok  ' : 'FAIL'} ${mutation.id} ${mutation.rule.padEnd(12)} exit=${r.code}  ${mutation.title}` +
        (r.ok ? '' : `  — ${r.why}`),
    );
  }

  const after = gitStatusPorcelain();
  if (after !== '') {
    out('REVERT FAILED — the tree differs after the run. Inspect before doing anything else:');
    process.stdout.write(`${after}\n`);
    return 1;
  }
  out('revert verified — `git status --porcelain` is empty after all mutations');

  const failed = results.filter((r) => !r.ok);
  out(`${results.length - failed.length}/${results.length} mutation(s) turned the guard red`);
  if (failed.length > 0) return 1;
  for (const r of results) if (r.line) out(`  ${r.line.trim()}`);
  return 0;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  withLock(() => {
    process.exitCode = main();
  });
}