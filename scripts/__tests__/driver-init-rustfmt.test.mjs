// @vitest-environment node
/**
 * Regression guard: the generated `src-tauri/src/driver_init.rs` must be
 * byte-identical to what `rustfmt` would emit, for ANY driver set.
 *
 * HISTORY OF THE DEFECT THIS PINS
 * --------------------------------
 * `generateRustDriverInit` used to emit `extern crate` items in `--drivers`
 * order and to append a trailing blank line after every plugin block. Neither
 * matched rustfmt for any driver set measured:
 *
 *   - rustfmt's `reorder_modules` (default `true`) sorts a blank-line-delimited
 *     run of items alphabetically by crate path, and nothing else re-sorted
 *     them. The emitted order follows `--drivers` / registry order, which is
 *     never that order. Measured on the default 4-driver `basic` set
 *     (postgres, mysql, sqlite, redis) rustfmt wanted mysql before postgres
 *     and redis before sqlite; on the 15-driver set it produced 30 reordered
 *     lines. Sorting by the *crate ident* matters: `crateRustIdent` rewrites
 *     `-` to `_`, so driver-id order and crate order can disagree.
 *   - rustfmt collapses a run of blank lines to one. The template's
 *     `${body}\n\n    builder` plus the body's own trailing newline produced
 *     two blank lines whenever any driver declared a `tauriPlugin` — which
 *     `basic` does too, via redis.
 *
 * Measured `rustfmt --check` exit codes before the fix: 1 for the 4-driver set
 * (2 hunks) and 1 for the 15-driver set (3 hunks).
 *
 * WHY THIS TEST IS NOT SELF-SOURCED
 * ---------------------------------
 * The obvious wrong test here is `expect(template).toContain(...)` — asserting
 * the generator's output against a copy of the generator's intent, which can
 * never disagree with a broken template. Nothing below reads the template.
 *
 * Every assertion targets the *real artifact file* that the *real CLI* wrote:
 * `resolve-drivers.mjs --codegen-only` is spawned as a subprocess against a
 * throwaway copy of the repository, and the test reads back
 * `<staged>/src-tauri/src/driver_init.rs` from disk. The goldens under
 * `fixtures/` are frozen recorded artifacts, checked in independently of the
 * template; the three rustfmt-free property tests re-derive the rule from
 * rustfmt's documented behaviour rather than from this repo's code, and the
 * "comparison has teeth" test proves the comparison rejects the pre-fix shape.
 *
 * RUSTFMT AVAILABILITY
 * -------------------
 * The rustfmt cross-check runs only when a `rustfmt` binary is on PATH. The
 * absence is announced on stderr and is deliberately NOT turned into a silent
 * skip: the golden and property layers above do not use rustfmt at all, so the
 * guard still goes red on the defect without it. rustfmt is a *corroborating*
 * oracle here, not the gate.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { spawnSync } from 'child_process';
import {
  cpSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  unlinkSync,
} from 'fs';
import { tmpdir } from 'os';
import { basename, dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';

import { resolveDrivers } from '../resolve-drivers.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '../..');
const FIXTURE_DIR = join(HERE, 'fixtures');
const ARTIFACT_REL = 'src-tauri/src/driver_init.rs';

/**
 * Driver-set arguments under test.
 *
 * `all` is the trigger condition lead asked to be covered: it resolves to every
 * path driver in `drivers-registry.json` (15 today) and is the configuration
 * `--drivers=all` builds, where the pre-fix template produced the worst diff.
 * `basic` is pinned alongside it because it is the default set that `pnpm
 * tauri:dev` builds, and it was *also* fmt-dirty before the fix — via the
 * trailing blank line and a partial ordering miss. Guarding only the 15-driver
 * set would leave the default build unprotected.
 */
const SETS = {
  all: { driversArg: 'all', golden: 'driver-init-all.golden.txt' },
  basic: { driversArg: 'basic', golden: 'driver-init-basic.golden.txt' },
};

/** Repo subtrees the codegen subprocess does not need; copying them is pure cost. */
const SKIP_DIRS = new Set([
  'node_modules',
  'target',
  '.git',
  'posts',
  'dist',
  'coverage',
  '.codegraph',
  'e2e',
  'test',
]);

/**
 * Floor on the `all` driver set. The 15-driver case is the one lead asked to be
 * covered and the one where the pre-fix diff was largest; this floor exists so
 * that trimming `drivers-registry.json` cannot quietly shrink `all` into a set
 * too small to expose an ordering bug, turning the guard into a no-op.
 */
const MIN_TRIGGERING_DRIVERS = 5;

/* -------------------------------------------------------------------------- */
/* helpers                                                                     */
/* -------------------------------------------------------------------------- */

/**
 * First line on which two strings disagree, or null when identical.
 * Used so a failure names a location instead of dumping two large files.
 */
function firstDifference(actual, expected) {
  const a = actual.split('\n');
  const b = expected.split('\n');
  for (let i = 0; i < Math.max(a.length, b.length); i += 1) {
    if (a[i] !== b[i]) return { line: i + 1, actual: a[i], expected: b[i] };
  }
  return null;
}

function readGolden(name) {
  const path = join(FIXTURE_DIR, name);
  if (!existsSync(path)) {
    throw new Error(
      `missing golden fixture ${path}\n` +
        `Regenerate it by running, for a set S:\n` +
        `  node scripts/resolve-drivers.mjs --codegen-only --drivers=<S>\n` +
        `and copying src-tauri/src/driver_init.rs over the fixture.`,
    );
  }
  return readFileSync(path, 'utf8');
}

/** Assert a freshly generated artifact equals its golden, naming the line. */
function expectMatchesGolden(actual, goldenName, label) {
  const golden = readGolden(goldenName);
  const diff = firstDifference(actual, golden);
  const message = diff
    ? `${label} does not match fixtures/${goldenName}.\n` +
      `  first difference at line ${diff.line}\n` +
      `  generated: ${JSON.stringify(diff.actual)}\n` +
      `  expected:  ${JSON.stringify(diff.expected)}`
    : `${label} does not match fixtures/${goldenName}, but no differing line was found`;
  expect(diff, message).toBeNull();
}

/**
 * Copy the repo into a temp dir so the generator's writes cannot touch the
 * developer's worktree. `ROOT` in the build scripts is derived from the
 * script's own location (`resolve(__dirname, '..')`), so running the staged
 * copy keeps every output inside the temp dir. `node_modules` is symlinked
 * rather than copied — the same trick the repo's own worktrees use.
 */
function stageRepo() {
  const stage = mkdtempSync(join(tmpdir(), 'datazen-driver-init-guard-'));
  cpSync(REPO, stage, {
    recursive: true,
    filter: (src) => src === REPO || !SKIP_DIRS.has(basename(src)),
  });
  const nodeModules = join(REPO, 'node_modules');
  if (existsSync(nodeModules)) {
    symlinkSync(nodeModules, join(stage, 'node_modules'), 'dir');
  }
  return stage;
}

/** Run the real CLI in the staged copy and return the artifact it wrote. */
function generateDriverInit(stage, driversArg) {
  const result = spawnSync(
    process.execPath,
    ['scripts/resolve-drivers.mjs', '--codegen-only', `--drivers=${driversArg}`],
    { cwd: stage, encoding: 'utf8' },
  );

  if (result.status !== 0) {
    // Surfaced, never swallowed: a guard that cannot run must not report green.
    throw new Error(
      `resolve-drivers.mjs --codegen-only --drivers=${driversArg} failed ` +
        `(exit ${result.status}).\n` +
        `stderr: ${(result.stderr || '').trim()}\n` +
        `stdout: ${(result.stdout || '').trim()}`,
    );
  }

  const path = join(stage, ARTIFACT_REL);
  if (!existsSync(path)) {
    throw new Error(`generator exited 0 but wrote no ${ARTIFACT_REL} in ${stage}`);
  }
  return readFileSync(path, 'utf8');
}

function rustfmtVersion() {
  const probe = spawnSync('rustfmt', ['--version'], { encoding: 'utf8' });
  return probe.status === 0 ? (probe.stdout || '').trim() : null;
}

/** The `extern crate <ident>;` idents, in the order they appear in the artifact. */
function externCrateIdents(source) {
  return [...source.matchAll(/^extern crate ([A-Za-z0-9_]+);$/gm)].map((m) => m[1]);
}

/* -------------------------------------------------------------------------- */
/* one staged repo, shared by the generation-based cases                       */
/* -------------------------------------------------------------------------- */

describe('generated driver_init.rs is rustfmt-canonical', () => {
  let stage;
  let generated = {};
  let rustfmt;

  beforeAll(() => {
    stage = stageRepo();

    const registry = JSON.parse(readFileSync(join(REPO, 'drivers-registry.json'), 'utf8'));
    const allDrivers = resolveDrivers(SETS.all.driversArg, registry);

    // Prove the trigger condition rather than assuming it. A registry trimmed
    // to 4 drivers would silently turn the whole guard into a no-op.
    expect(
      allDrivers.length,
      `driver set "all" resolves to only ${allDrivers.length} drivers; ` +
        `this guard needs >= ${MIN_TRIGGERING_DRIVERS} to be able to fail`,
    ).toBeGreaterThanOrEqual(MIN_TRIGGERING_DRIVERS);

    // One staged copy, three generations. Each run rewrites the artifact, and
    // writeIfChanged would otherwise leave a stale file in place.
    for (const setName of ['all', 'basic']) {
      const artifact = join(stage, ARTIFACT_REL);
      if (existsSync(artifact)) unlinkSync(artifact);
      generated[setName] = generateDriverInit(stage, SETS[setName].driversArg);
    }

    // Order independence: the same driver set fed in reverse must still produce
    // the identical file. This is the assertion that cannot be satisfied by a
    // template which merely happens to match for one particular order.
    const reversed = resolveDrivers(SETS.all.driversArg, registry).reverse().join(',');
    const artifact = join(stage, ARTIFACT_REL);
    if (existsSync(artifact)) unlinkSync(artifact);
    generated.allReversed = generateDriverInit(stage, reversed);

    rustfmt = rustfmtVersion();
    if (!rustfmt) {
      console.warn(
        '[driver-init-rustfmt] rustfmt not on PATH: the cross-check in this ' +
          'file is skipped. The golden-fixture and property layers still ran ' +
          'and do not depend on rustfmt, so the guard is NOT vacuous.',
      );
    }
  }, 120_000);

  afterAll(() => {
    if (stage) rmSync(stage, { recursive: true, force: true });
  });

  /* --- primary: byte-for-byte against frozen, independently held goldens --- */

  it('emits the 15-driver artifact byte-identically to its golden', () => {
    expectMatchesGolden(generated.all, SETS.all.golden, 'the >=5-driver artifact');
  });

  it('emits the 4-driver artifact byte-identically to its golden', () => {
    expectMatchesGolden(generated.basic, SETS.basic.golden, 'the 4-driver artifact');
  });

  /* --- property layer: the rustfmt rule, derived, asserted on the artifact --- */

  it('orders `extern crate` idents ascending, as rustfmt reorder_modules requires', () => {
    for (const setName of ['all', 'basic']) {
      const idents = externCrateIdents(generated[setName]);
      expect(idents.length, `${setName}: no extern crate found in artifact`).toBeGreaterThan(0);
      const sorted = [...idents].sort();
      const offenders = idents
        .map((ident, i) => (ident === sorted[i] ? null : `pos ${i + 1}: ${ident}`))
        .filter(Boolean);
      expect(
        offenders,
        `${setName} artifact has \`extern crate\` items out of rustfmt order: ${offenders.join(', ')}`,
      ).toEqual([]);
    }
  });

  it('contains no consecutive blank lines, which rustfmt would collapse', () => {
    for (const setName of ['all', 'basic', 'allReversed']) {
      const lines = generated[setName].split('\n');
      const doubles = [];
      lines.forEach((line, i) => {
        if (line === '' && lines[i + 1] === '') doubles.push(i + 1);
      });
      expect(
        doubles,
        `${setName} artifact has consecutive blank lines starting at ${doubles.join(', ')}; ` +
          `rustfmt collapses each run to one`,
      ).toEqual([]);
    }
  });

  it('produces the same bytes whatever order the drivers are listed in', () => {
    const diff = firstDifference(generated.allReversed, generated.all);
    expect(
      diff,
      diff
        ? `reversing --drivers changed the artifact; first difference at line ${diff.line}\n` +
          `  forward: ${JSON.stringify(diff.expected)}\n` +
          `  reversed: ${JSON.stringify(diff.actual)}`
        : 'order independence could not be evaluated',
    ).toBeNull();
  });

  /* --- teeth check: prove the golden comparison rejects the pre-fix shape --- */

  it('rejects an artifact carrying the pre-fix defects', () => {
    const lines = generated.all.split('\n');

    // Defect 1: unsorted extern crates (reverse of the sorted order).
    const externStart = lines.findIndex((l) => l.startsWith('#[cfg(feature ='));
    const externEnd = lines.findIndex((l) => l === 'use tauri::Runtime;');
    const unsorted = [
      ...lines.slice(0, externStart),
      ...lines.slice(externStart, externEnd).reverse(),
      ...lines.slice(externEnd),
    ];

    // Defect 2: a doubled blank line before `builder`.
    const builderAt = unsorted.findIndex((l) => l === '    builder');
    const doubled = [
      ...unsorted.slice(0, builderAt),
      '',
      ...unsorted.slice(builderAt),
    ];

    const broken = doubled.join('\n');
    expect(
      broken,
      'the teeth-check fixture must not equal the golden to begin with',
    ).not.toBe(generated.all);

    const diff = firstDifference(broken, generated.all);
    expect(
      diff,
      'golden comparison failed to catch the reintroduced defect — the guard has no teeth',
    ).not.toBeNull();
  });

  /* --- corroborating oracle: rustfmt itself, when available ---------------- */

  it('passes rustfmt --check on the freshly generated 15-driver artifact', () => {
    if (!rustfmt) {
      // Surfaced above via console.warn and asserted here as a visible,
      // non-silent outcome. The other layers in this file carry the guard.
      expect(
        rustfmt,
        'rustfmt absence must be reported, not silently treated as a pass',
      ).toBeNull();
      return;
    }

    const path = join(stage, ARTIFACT_REL);
    const result = spawnSync('rustfmt', ['--edition', '2021', '--check', path], {
      encoding: 'utf8',
    });
    expect(
      result.status,
      `${result.stdout || result.stderr || ''}`.trim().slice(0, 2000) ||
        'rustfmt --check reported a diff with no output',
    ).toBe(0);
  });

  it('passes rustfmt --check on the golden fixtures themselves', () => {
    if (!rustfmt) {
      expect(rustfmt, 'rustfmt absence must be reported, not silently treated as a pass').toBeNull();
      return;
    }
    for (const setName of ['all', 'basic']) {
      const path = join(FIXTURE_DIR, SETS[setName].golden);
      const result = spawnSync('rustfmt', ['--edition', '2021', '--check', path], {
        encoding: 'utf8',
      });
      expect(
        result.status,
        `fixtures/${SETS[setName].golden} is not rustfmt-canonical, so it cannot be ` +
          `used as a golden:\n${(result.stdout || result.stderr || '').trim().slice(0, 2000)}`,
      ).toBe(0);
    }
  });
});