/**
 * Fast, cargo-free tests for the platform crate boundary guard.
 *
 * These do NOT replace `pnpm test:platform-arch:mutations`. A guard that has
 * never seen a real `cargo metadata` graph can pass for entirely the wrong
 * reason — the failure mode `boundaryMutation.ts` exists to prevent, applied
 * here to the crate graph. The mutation script is the proof; this file is what
 * keeps `pnpm test:scripts` cheap enough to run on every change.
 *
 * The spec cases read the real §2.4 table rather than a transcription of it.
 * A copy would drift the moment the doc is edited, and the drift would show up
 * as a test failure that says nothing about the guard.
 */

import { readFileSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import {
  SPEC_PATH,
  checkPlatformCrateBoundaries,
  checkSpecConsistency,
  parseSpecRow,
} from '../check-platform-crate-boundaries.mjs';
import { classifyMemberDir, matchesCrateFamily } from '../lib/cargoWorkspace.mjs';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const SPEC = readFileSync(join(REPO_ROOT, SPEC_PATH), 'utf8');

function specRow(id: string): string {
  const row = SPEC.split('\n').find((l) => l.startsWith(`| ${id} `));
  if (row === undefined) throw new Error(`no ${id} row in ${SPEC_PATH} §2.4`);
  return row;
}

/**
 * A minimal but structurally faithful `cargo metadata` document.
 *
 * Shaped after cargo 1.90 output, including the detail that `resolve.nodes`
 * are keyed by `id` and carry no `pkg` field — an earlier draft of this guard
 * keyed nodes by `node.pkg` and therefore resolved every closure to exactly
 * the member itself, which reads as a perfectly green guard.
 */
function fixture(
  members: Array<{ name: string; dir: string }>,
  edges: Record<string, string[]>,
  external: Array<{ name: string }> = [],
  /** Edge kinds by `from->to`; anything unlisted is a normal dependency. */
  kinds: Record<string, 'normal' | 'build' | 'dev'> = {},
): object {
  const all = [
    ...members.map((m) => ({ name: m.name, dir: m.dir as string | null })),
    ...external.map((e) => ({ name: e.name, dir: null })),
  ];
  const dirOf = new Map(all.map((p) => [p.name, p.dir]));
  const idOf = (name: string) => `path+file://${dirOf.get(name)}#${name}`;
  return {
    workspace_root: REPO_ROOT,
    packages: all.map((p) => ({
      id: idOf(p.name),
      name: p.name,
      manifest_path: join(REPO_ROOT, p.dir ?? '.', 'Cargo.toml'),
    })),
    workspace_members: members.map((m) => idOf(m.name)),
    resolve: {
      root: null,
      nodes: all.map((p) => ({
        id: idOf(p.name),
        deps: (edges[p.name] ?? []).map((target) => ({
          pkg: idOf(target),
          dep_kinds: [{ kind: kinds[`${p.name}->${target}`] ?? null, target: null }],
        })),
      })),
    },
  };
}

const CORE = [
  { name: 'datazen', dir: 'src-tauri' },
  { name: 'datazen-driver-api', dir: 'packages/driver-api' },
  { name: 'datazen-runtime', dir: 'packages/runtime' },
  { name: 'datazen-driver-redis', dir: 'packages/drivers/redis' },
];

// `classifyMemberDir` returns a discriminated union; `.id` only exists on the
// classified branch. Narrowing here keeps the assertion honest — a path §2.4
// names that stops classifying must fail loudly, not yield `undefined`.
function layerIdOf(dir: string): string {
  const result = classifyMemberDir(dir);
  if (!('id' in result))
    throw new Error(`expected \`${dir}\` to classify, got ${result.unclassified}`);
  return result.id;
}

function run(metadata: object, specText = SPEC) {
  return checkPlatformCrateBoundaries({ root: REPO_ROOT, metadata, specText });
}

describe('classifyMemberDir', () => {
  it('maps every path §2.4 names to a layer', () => {
    for (const dir of [
      'src-tauri',
      'server',
      'packages/application',
      'packages/runtime',
      'packages/platform-api',
    ]) {
      expect(classifyMemberDir(dir).unclassified).toBeUndefined();
    }
    expect(layerIdOf('packages/application')).toBe('application');
    expect(layerIdOf('packages/drivers/redis')).toBe('driver');
  });

  it('respects segment boundaries so a sibling directory is not a layer', () => {
    // `packages/drivers-archive` shares a string prefix with `packages/drivers`
    // but is a different directory; matching naively would classify an
    // unrelated crate as a driver and hand it F-01's rules.
    expect(classifyMemberDir('packages/drivers-archive/foo')).toHaveProperty('unclassified');
  });

  it('reports an unknown member as an error rather than a silent pass', () => {
    const result = run(fixture([...CORE, { name: 'mystery', dir: 'crates/mystery' }], {}));
    expect(result.errors.join('\n')).toContain('crates/mystery');
  });
});

describe('matchesCrateFamily', () => {
  it('matches the family itself and its sub-crates', () => {
    for (const name of ['tauri', 'tauri-plugin', 'tauri-plugin-fs', 'tauri_utils']) {
      expect(matchesCrateFamily(name, 'tauri')).toBe(true);
    }
  });

  it('does not match a crate that merely contains the family name', () => {
    // `notauri` must not read as `tauri`, or F-02 would fail on an unrelated
    // dependency and the team would learn to ignore the guard.
    expect(matchesCrateFamily('notauri', 'tauri')).toBe(false);
    expect(matchesCrateFamily('tari', 'tauri')).toBe(false);
  });
});

describe('parseSpecRow', () => {
  it('F-06 crates keep the spec family glob, and the guard strips it', () => {
    const f06 = parseSpecRow(specRow('F-06'));
    expect(f06.paths).toEqual(['server']);
    // `tauri*` is a family glob in the doc; `matchesCrateFamily` is what turns
    // it into the `tauri-plugin`/`tauri-utils` edges a closure check needs.
    expect(f06.crates).toEqual(['tauri*', 'datazen']);
    expect(matchesCrateFamily('tauri-plugin', f06.crates[0].replace(/\*$/, ''))).toBe(true);
  });

  it('reads F-03 as two subject paths and four crate names', () => {
    const f03 = parseSpecRow(specRow('F-03'));
    expect(f03.paths).toEqual(['packages/application', 'packages/runtime']);
    expect(f03.crates).toEqual(['axum', 'actix-web', 'warp', 'tonic']);
  });

  it('classifies a scoped npm token as frontend, not as a crate name', () => {
    // `@tauri-apps/api` contains a `/`, which is also the separator in
    // workspace paths; reading it as either the wrong one would make F-05's
    // crate set silently empty.
    expect(parseSpecRow(specRow('F-05')).frontend).toContain('@tauri-apps/api');
  });
});

describe('checkSpecConsistency', () => {
  it('accepts the real §2.4 table', () => {
    expect(checkSpecConsistency(SPEC)).toEqual([]);
  });

  it('fails when the spec drops a crate family the guard enforces', () => {
    // Scope the edit to the F-03 row. A bare `SPEC.replace('`tonic`', …)` only
    // rewrites the FIRST occurrence, so the moment §2's prose mentions the same
    // crate the mutation lands on the paragraph instead of the table row and
    // this test silently stops probing the rule it names.
    const weakened = SPEC.split('\n')
      .map((l) => (l.startsWith('| F-03 ') ? l.replace('`tonic`', '`其他`') : l))
      .join('\n');
    expect(weakened).not.toBe(SPEC);
    expect(checkSpecConsistency(weakened).join('\n')).toContain('F-03');
  });

  it('fails when the spec adds a subject the guard does not check', () => {
    const widened = SPEC.replace(
      '| F-03 | `packages/application`、`packages/runtime` ',
      '| F-03 | `packages/application`、`packages/runtime`、`packages/server` ',
    );
    expect(checkSpecConsistency(widened).join('\n')).toContain('F-03');
  });

  it('fails when an F-row disappears entirely', () => {
    const withoutF06 = SPEC.split('\n')
      .filter((l) => !l.startsWith('| F-06 '))
      .join('\n');
    expect(checkSpecConsistency(withoutF06).join('\n')).toContain('F-06');
  });
});

describe('checkPlatformCrateBoundaries', () => {
  const clean = { 'datazen-runtime': ['datazen-driver-api'] };

  it('passes a workspace that respects every rule', () => {
    const result = run(fixture(CORE, clean));
    expect(result.violations).toEqual([]);
    expect(result.errors).toEqual([]);
  });

  it('F-01: a driver reaching the host crate is a violation', () => {
    const result = run(fixture(CORE, { ...clean, 'datazen-driver-redis': ['datazen'] }));
    expect(result.violations.join('\n')).toMatch(/F-01.*datazen-driver-redis.*→ datazen/);
  });

  it('F-01: a driver reaching datazen-runtime is a violation', () => {
    const result = run(fixture(CORE, { ...clean, 'datazen-driver-redis': ['datazen-runtime'] }));
    expect(result.violations.join('\n')).toMatch(/F-01.*datazen-driver-redis.*→ datazen-runtime/);
  });

  it('F-01: a build-kind edge is walked, not just a normal one', () => {
    // The redis defect that motivated the advisory: a `[build-dependencies]`
    // entry drags Tauri into a driver's build graph exactly as a normal one
    // would, so a normal-only check would have missed it.
    const result = run(
      fixture(CORE, { ...clean, 'datazen-driver-redis': ['datazen'] }, [], {
        'datazen-driver-redis->datazen': 'build',
      }),
    );
    expect(result.violations.join('\n')).toMatch(/F-01.*datazen-driver-redis/);
  });

  it('F-02: a tauri crate in a core crate closure is a violation', () => {
    const result = run(
      fixture(CORE, { ...clean, 'datazen-runtime': ['tauri-plugin'] }, [{ name: 'tauri-plugin' }]),
    );
    expect(result.violations.join('\n')).toMatch(/F-02.*datazen-runtime.*→ tauri-plugin/);
  });

  it('F-02: a tauri crate reached transitively is still a violation', () => {
    // The check is over the *closure*, not the declared edge: a core crate
    // that never names Tauri but pulls it in through a helper is exactly the
    // case F-02 exists for.
    const members = [
      ...CORE,
      { name: 'datazen-driver-postgres', dir: 'packages/drivers/postgres' },
    ];
    const result = run(
      fixture(
        members,
        { 'datazen-runtime': ['datazen-driver-postgres'], 'datazen-driver-postgres': ['tauri'] },
        [{ name: 'tauri' }],
      ),
    );
    expect(result.violations.join('\n')).toMatch(/F-02.*datazen-runtime.*→ tauri/);
  });

  it('F-02: a dev-dependency does not count as a shippable edge', () => {
    const result = run(
      fixture(CORE, { ...clean, 'datazen-runtime': ['tauri'] }, [{ name: 'tauri' }], {
        'datazen-runtime->tauri': 'dev',
      }),
    );
    expect(result.violations).toEqual([]);
  });

  it('F-03: an HTTP framework in the core is a violation', () => {
    const result = run(
      fixture(CORE, { ...clean, 'datazen-runtime': ['axum'] }, [{ name: 'axum' }]),
    );
    expect(result.violations.join('\n')).toMatch(/F-03.*axum/);
  });

  it('F-05: a UI-runtime crate in the core is a violation', () => {
    const result = run(
      fixture(CORE, { ...clean, 'datazen-runtime': ['react'] }, [{ name: 'react' }]),
    );
    expect(result.violations.join('\n')).toMatch(/F-05.*react/);
  });

  it('F-04: platform-api depending on datazen-runtime is a violation', () => {
    const members = [...CORE, { name: 'datazen-platform-api', dir: 'packages/platform-api' }];
    const result = run(fixture(members, { ...clean, 'datazen-platform-api': ['datazen-runtime'] }));
    expect(result.violations.join('\n')).toMatch(/F-04.*datazen-platform-api.*→ datazen-runtime/);
  });

  it('F-04: platform-api depending on driver-api is allowed (§2.3)', () => {
    const members = [...CORE, { name: 'datazen-platform-api', dir: 'packages/platform-api' }];
    const result = run(
      fixture(members, { ...clean, 'datazen-platform-api': ['datazen-driver-api'] }),
    );
    expect(result.violations).toEqual([]);
  });

  it('F-06: a tauri crate in the server closure is a violation', () => {
    const members = [...CORE, { name: 'datazen-server', dir: 'server' }];
    const result = run(
      fixture(members, { ...clean, 'datazen-server': ['tauri'] }, [{ name: 'tauri' }]),
    );
    expect(result.violations.join('\n')).toMatch(/F-06.*datazen-server.*→ tauri/);
  });

  it('reports a not-yet-created layer as vacuous rather than failing', () => {
    const result = run(fixture(CORE, clean));
    expect(result.vacuous.map((v) => v.rule)).toContain('F-06');
    expect(result.violations).toEqual([]);
  });

  it('--require-layers turns a vacuous rule into an error', () => {
    const result = checkPlatformCrateBoundaries({
      root: REPO_ROOT,
      metadata: fixture(CORE, clean),
      requireLayers: ['server'],
    });
    expect(result.errors.join('\n')).toContain('server');
  });
});
