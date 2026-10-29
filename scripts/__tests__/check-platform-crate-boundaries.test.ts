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

import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { describe, expect, it } from 'vitest';

import {
  DEFAULT_REQUIRED_LAYERS,
  RULES,
  SPEC_PATH,
  checkPlatformCrateBoundaries,
  checkSpecConsistency,
  parseSpecRow,
  ruleVerdict,
} from '../check-platform-crate-boundaries.mjs';
import {
  classifyMemberDir,
  declaredDependencies,
  matchesCrateFamily,
} from '../lib/cargoWorkspace.mjs';
import { withProbeLock } from './boundaryMutation';

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
 *
 * `root` is the tree the member manifests live in. It defaults to the real
 * repository, which is what most cases want: the declared-edge scan reads
 * `Cargo.toml` **from disk**, so a case that needs a manifest with a specific
 * dependency in it points `root` at a throwaway tree and writes the manifest
 * there.
 */
function fixture(
  members: Array<{ name: string; dir: string }>,
  edges: Record<string, string[]>,
  external: Array<{ name: string }> = [],
  /** Edge kinds by `from->to`; anything unlisted is a normal dependency. */
  kinds: Record<string, 'normal' | 'build' | 'dev'> = {},
  root = REPO_ROOT,
): object {
  const all = [
    ...members.map((m) => ({ name: m.name, dir: m.dir as string | null })),
    ...external.map((e) => ({ name: e.name, dir: null })),
  ];
  const dirOf = new Map(all.map((p) => [p.name, p.dir]));
  const idOf = (name: string) => `path+file://${dirOf.get(name)}#${name}`;
  return {
    workspace_root: root,
    packages: all.map((p) => ({
      id: idOf(p.name),
      name: p.name,
      manifest_path: join(root, p.dir ?? '.', 'Cargo.toml'),
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
  { name: 'datazen_desktop', dir: 'apps/desktop/src-tauri' },
  { name: 'datazen-driver-api', dir: 'packages/driver-api' },
  { name: 'datazen', dir: 'modules' },
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

/**
 * Run the guard against the **real** working tree, under the shared probe lock.
 *
 * This suite does not use throwaway trees for the crate-graph rules: `root` is
 * the repository itself, so F-07 reads the real `packages/backend-client`
 * sources and the declared-edge scan reads the real `Cargo.toml` files. That is
 * the point of the suite — a guard proven only against a synthetic tree can be
 * green for the wrong reason.
 *
 * It is also the point of the lock. `check-module-layers.test.ts` proves its
 * port rules by planting a **real violating file** in the very same directory —
 * `packages/backend-client/src/__boundaryProbe__.ts` — and the file lives in the
 * tree for as long as that suite's critical section lasts. A scan of the real
 * tree that runs at that moment reads the probe as a genuine F-07 violation:
 *
 *     F-07  packages/backend-client/src:1 contains `fetch(` — packages/backend-client
 *     must reach the backend through the platform client, not through Tauri IPC or
 *     the network directly.
 *
 * The two suites are in different worker **processes** (Vitest's default pool
 * is `forks`, and `fileParallelism` is on), so a per-process mutex would not
 * have helped; the shared lock is an on-disk file created with `O_EXCL`, which
 * is atomic across processes, and every operation that observes or mutates the
 * real tree takes it. Without it the failure is a coin flip on timing — a green
 * suite locally,
 * a red one in CI — and it moves to whichever unguarded scan runs next.
 *
 * The lock is not re-entrant, so it is taken exactly once per call here and
 * never around a call that already holds it.
 */
function run(metadata: object, specText = SPEC, options: { requireLayers?: string[] } = {}) {
  return withProbeLock(() =>
    checkPlatformCrateBoundaries({ root: REPO_ROOT, metadata, specText, ...options }),
  );
}

describe('classifyMemberDir', () => {
  it('maps every path §2.4 names to a layer', () => {
    for (const dir of [
      'apps/desktop/src-tauri',
      'server',
      'modules',
      'packages/infra',
      'packages/driver-api',
      'packages/drivers/redis',
      'packages/backend-client',
    ]) {
      expect(classifyMemberDir(dir).unclassified).toBeUndefined();
    }
    expect(layerIdOf('modules')).toBe('modules');
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

  it('reads F-03 as the modules subject and four crate names', () => {
    const f03 = parseSpecRow(specRow('F-03'));
    expect(f03.paths).toEqual(['modules']);
    expect(f03.crates).toEqual(['axum', 'actix-web', 'warp', 'tonic']);
  });

  it('classifies a scoped npm token as frontend, not as a crate name', () => {
    // `@tauri-apps/api` contains a `/`, which is also the separator in
    // workspace paths; reading it as either the wrong one would make F-05's
    // crate set silently empty.
    expect(parseSpecRow(specRow('F-05')).frontend).toContain('@tauri-apps/api');
  });
});

/**
 * The per-rule summary line's verdict word.
 *
 * B3: the loop that printed one summary line per evaluated rule hardcoded
 * `ok`, so a run could print `VIOLATION F-11  …` and `ok F-11: …` a few lines
 * apart and still exit 1 — the output contradicting itself at exactly the point
 * a reader looks for the per-rule story. Measured at bc36cd121.
 */
describe('ruleVerdict', () => {
  it('attributes rule scan errors without blaming other rules or prefixes', () => {
    expect(ruleVerdict('F-07', [], ['F-07: could not scan source'])).toBe('fail');
    expect(ruleVerdict('F-07', [], ['F-070: another rule', 'F-01: another rule'])).toBe('ok');
  });
  it('says fail for a rule one of whose violations is attributed to it', () => {
    // Both producer shapes: the closure walk and the F-07 scan open with
    // `<id>  `, the spec cross-check with `<id>: `.
    expect(ruleVerdict('F-11', ['F-11  datazen-infra (packages/infra/Cargo.toml) → x'])).toBe(
      'fail',
    );
    expect(ruleVerdict('F-11', ['F-11: subjects disagree with §2.4'])).toBe('fail');
  });

  it('says ok for a rule with no violation of its own', () => {
    expect(ruleVerdict('F-11', [])).toBe('ok');
    // Another rule being red must not smear onto this one.
    expect(
      ruleVerdict('F-11', ['F-01  datazen-driver-redis → datazen', 'F-11x: not a rule id']),
    ).toBe('ok');
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
    const widened = SPEC.replace('| F-03 | `modules` ', '| F-03 | `modules`、`packages/server` ');
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
  const clean = { datazen: ['datazen-driver-api'] };

  it('passes a workspace that respects every rule', () => {
    const result = run(fixture(CORE, clean));
    expect(result.violations).toEqual([]);
    expect(result.errors).toEqual([]);
  });

  it('marks the rule that carries the violation as fail, and no other', () => {
    // The two halves of B3 joined up: every producer must open its violation
    // with the rule id, and the summary line must read that prefix. Before the
    // fix this run printed `VIOLATION F-01 …` and `ok F-01: …` in the same
    // output, so the per-rule line disagreed with the line above it.
    const result = run(fixture(CORE, { ...clean, 'datazen-driver-redis': ['datazen'] }));
    expect(result.violations.length).toBeGreaterThan(0);
    expect(result.violations.every((line) => line.startsWith('F-01'))).toBe(true);
    expect(ruleVerdict('F-01', result.violations)).toBe('fail');
    // F-02 is evaluated in the same run and is clean, so the tally is not a
    // blanket fail the moment anything is red.
    expect(ruleVerdict('F-02', result.violations)).toBe('ok');
  });

  it('F-01: a driver reaching the host crate is a violation', () => {
    const result = run(fixture(CORE, { ...clean, 'datazen-driver-redis': ['datazen'] }));
    expect(result.violations.join('\n')).toMatch(/F-01.*datazen-driver-redis.*→ datazen/);
  });

  it('F-14: modules reaching the desktop host are rejected', () => {
    const result = run(fixture(CORE, { ...clean, datazen: ['datazen_desktop'] }), SPEC, {
      requireLayers: ['modules'],
    });
    expect(result.violations.join('\n')).toMatch(/F-14.*datazen.*datazen_desktop/);
  });

  it('F-01: a driver reaching datazen is a violation', () => {
    const result = run(fixture(CORE, { ...clean, 'datazen-driver-redis': ['datazen'] }));
    expect(result.violations.join('\n')).toMatch(/F-01.*datazen-driver-redis.*→ datazen/);
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

  it('F-01: a tauri crate in a driver build graph is a violation', () => {
    // RED FIRST, then corrected — twice. The first version of this test was
    // written against the ORIGINAL behaviour: it asserted `violations` and was
    // red at base 2530161ac, which is what proved the hole was real rather
    // than assumed. It was then rewritten to assert the **advisory** instead,
    // because at that point F-01's `forbiddenCrates` was `[]` and the guard
    // could not make the call without the spec adopting it. The spec adopted it
    // (§2.4's F-01 row already said `tauri*`; the archive's precondition table
    // in `docs/development/ci-test-matrix.md` §2.3.1 listed the two remaining
    // steps), so the honest form of the claim is a violation again.
    //
    // Neither matcher is at fault. `normalBuildClosure` follows `kind ===
    // "build"` edges and `matchesCrateFamily("tauri-plugin", "tauri")` is true
    // (measured: `tauri` matches `tauri-plugin`, `tauri-build`, `tauri_utils`;
    // `taurium` does not).
    const result = run(
      fixture(
        CORE,
        { ...clean, 'datazen-driver-redis': ['tauri-plugin'] },
        [{ name: 'tauri-plugin' }],
        { 'datazen-driver-redis->tauri-plugin': 'build' },
      ),
    );
    expect(result.violations.join('\n')).toMatch(/F-01.*datazen-driver-redis.*→ tauri-plugin/);
    // The rule's own summary line must agree with the line above it.
    expect(ruleVerdict('F-01', result.violations)).toBe('fail');
  });

  it('F-01: a dormant tauri declaration in a driver is a violation', () => {
    // The second half of the rule, and the reason the manifest scan exists at
    // all. The resolve graph is feature-resolved, so an optional dependency
    // behind a feature nobody enables is not in `resolve.deps[]` and no closure
    // walk can reach it. §2.4's note under the table says both sides count.
    //
    // Hermetic: the driver manifest is written into a throwaway tree, because
    // the declared-edge scan reads `Cargo.toml` **from disk** — a fixture that
    // only faked the graph could not exercise this half at all.
    const root = mkdtempSync(join(tmpdir(), 'datazen-f01-declared-'));
    try {
      const dir = join(root, 'packages/drivers/probe');
      mkdirSync(dir, { recursive: true });
      writeFileSync(
        join(dir, 'Cargo.toml'),
        '[package]\nname = "datazen-driver-probe"\nversion = "0.0.1"\n\n' +
          '[dependencies]\ntauri = { version = "2", optional = true }\n',
      );
      const result = run(
        fixture(
          [{ name: 'datazen-driver-probe', dir: 'packages/drivers/probe' }],
          {},
          [],
          {},
          root,
        ),
      );
      expect(result.violations.join('\n')).toMatch(
        /F-01.*datazen-driver-probe.*declares tauri in \[dependencies\]/,
      );
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("F-01: the host's own dormant tauri declaration is not adjudicated by F-01", () => {
    // The trap this rule must not fall into. `apps/desktop/src-tauri/Cargo.toml` declares
    // `tauri-plugin-webdriver` for the macOS silent-E2E feature, and the host
    // is *supposed* to depend on Tauri — F-01's subjects are the `driver`
    // layer, so that edge is outside the rule. Reading it as a violation would
    // be the gate punishing the one member that is allowed to have it.
    //
    // The real `apps/desktop/src-tauri/Cargo.toml` is read here, so this case is about the
    // real tree's host manifest, not a synthetic one.
    const result = run(fixture(CORE, { ...clean }));
    expect(result.violations.filter((v) => v.startsWith('F-01'))).toEqual([]);
    expect(result.advisories.join('\n')).toMatch(
      /datazen_desktop \(apps\/desktop\/src-tauri\/Cargo\.toml\) declares .*tauri-plugin-webdriver/,
    );
    expect(result.advisories.join('\n')).toMatch(/Not blocking: F-01's subjects are/);
  });

  it('a declared edge the resolve graph drops is reported once, by the rule that owns it', () => {
    // Declared-minus-resolved, not declared-in-full: an edge that resolved is
    // the closure walk's to report, and printing it twice would make the count
    // read as two findings when there is one. The fixture resolves
    // `tauri-plugin`, so the declared-edge side must stay quiet about it.
    const result = run(
      fixture(
        CORE,
        { ...clean, 'datazen-driver-redis': ['tauri-plugin'] },
        [{ name: 'tauri-plugin' }],
        { 'datazen-driver-redis->tauri-plugin': 'build' },
      ),
    );
    expect(result.violations.filter((v) => v.includes('tauri-plugin')).length).toBe(1);
    expect(result.advisories.join('\n')).not.toMatch(/datazen-driver-redis/);
  });

  it('F-02: a tauri crate in a core crate closure is a violation', () => {
    const result = run(
      fixture(CORE, { ...clean, datazen: ['tauri-plugin'] }, [{ name: 'tauri-plugin' }]),
    );
    expect(result.violations.join('\n')).toMatch(/F-02.*datazen.*→ tauri-plugin/);
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
        { datazen: ['datazen-driver-postgres'], 'datazen-driver-postgres': ['tauri'] },
        [{ name: 'tauri' }],
      ),
    );
    expect(result.violations.join('\n')).toMatch(/F-02.*datazen.*→ tauri/);
  });

  it('a manifest that cannot be read is an error, not a silent skip', () => {
    // The declared-edge scan used to `advisories.push` and `continue` on a
    // read failure — the one non-fail-closed asymmetry in the gate. Now that
    // F-01 adjudicates declaration edges, that same hole would swallow a real
    // violation, so the read failure became an **error**.
    //
    // The member's manifest is pointed at a path that does not exist. Cargo
    // would have failed before reaching this point in the real world; the
    // fixture is what makes the branch reachable.
    const root = mkdtempSync(join(tmpdir(), 'datazen-f01-unreadable-'));
    try {
      const result = run(
        fixture(
          [{ name: 'datazen-driver-ghost', dir: 'packages/drivers/ghost' }],
          {},
          [],
          {},
          root,
        ),
      );
      expect(result.errors.join('\n')).toMatch(/read\?.*datazen-driver-ghost/);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('F-02: a dev-dependency does not count as a shippable edge', () => {
    const result = run(
      fixture(CORE, { ...clean, datazen: ['tauri'] }, [{ name: 'tauri' }], {
        'datazen->tauri': 'dev',
      }),
    );
    expect(result.violations).toEqual([]);
  });

  it('F-03: an HTTP framework in the core is a violation', () => {
    const result = run(fixture(CORE, { ...clean, datazen: ['axum'] }, [{ name: 'axum' }]));
    expect(result.violations.join('\n')).toMatch(/F-03.*axum/);
  });

  it('F-05: a UI-runtime crate in the core is a violation', () => {
    const result = run(fixture(CORE, { ...clean, datazen: ['react'] }, [{ name: 'react' }]));
    expect(result.violations.join('\n')).toMatch(/F-05.*react/);
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
    const result = run(fixture(CORE, clean), SPEC, { requireLayers: ['server'] });
    expect(result.errors.join('\n')).toContain('server');
  });
});

/**
 * F-01's exemption mechanism.
 *
 * The rule table has no implicit way for a driver to keep a `tauri*` edge: an
 * exemption is a **written-down decision** — member + family + reason — and
 * the guard prints it on every run. Two properties are load-bearing and are
 * asserted here:
 *
 *   * an exemption that names a member the rule does not cover, or a family
 *     the rule does not forbid, is an **error**: a stale entry protects
 *     nothing while looking like it does;
 *   * an exemption covers the **family** side only — a driver allowed to
 *     depend on `tauri*` is still forbidden from depending on the host.
 */
describe('F-01 exemptions', () => {
  const clean = { datazen: ['datazen-driver-api'] };

  /** The production rule table with F-01's exemption list replaced. */
  function withExemptions(exemptions: Array<{ member: string; family: string; reason: string }>) {
    return RULES.map((rule) => (rule.id === 'F-01' ? { ...rule, exemptions } : rule));
  }

  function runWith(
    exemptions: Array<{ member: string; family: string; reason: string }>,
    metadata: object,
  ) {
    // Real tree, so the same lock as `run` — see the note there.
    return withProbeLock(() =>
      checkPlatformCrateBoundaries({
        root: REPO_ROOT,
        metadata,
        specText: SPEC,
        rules: withExemptions(exemptions),
      }),
    );
  }

  it('suppresses the violation and says so out loud', () => {
    const result = runWith(
      [{ member: 'datazen-driver-redis', family: 'tauri', reason: 'probe exemption' }],
      fixture(CORE, { ...clean, 'datazen-driver-redis': ['tauri-plugin'] }, [
        { name: 'tauri-plugin' },
      ]),
    );
    expect(result.violations.filter((v) => v.includes('tauri-plugin'))).toEqual([]);
    expect(result.exemptions.join('\n')).toMatch(
      /datazen-driver-redis.*tauri-plugin.*probe exemption/,
    );
  });

  it('does not let an exemption cover the host side', () => {
    // The exemption names the `tauri` family, so it says nothing about the
    // `datazen` host crate — that edge is still a violation.
    const result = runWith(
      [{ member: 'datazen-driver-redis', family: 'tauri', reason: 'probe exemption' }],
      fixture(CORE, { ...clean, 'datazen-driver-redis': ['datazen'] }),
    );
    expect(result.violations.join('\n')).toMatch(/F-01.*datazen-driver-redis.*→ datazen/);
  });

  it('rejects an exemption whose member is not a subject of the rule', () => {
    const result = runWith(
      [{ member: 'datazen-driver-nobody', family: 'tauri', reason: 'probe exemption' }],
      fixture(CORE, clean),
    );
    expect(result.errors.join('\n')).toMatch(/F-01.*datazen-driver-nobody.*not a subject/);
  });

  it('rejects an exemption whose family the rule does not forbid', () => {
    const result = runWith(
      [{ member: 'datazen-driver-redis', family: 'axum', reason: 'probe exemption' }],
      fixture(CORE, clean),
    );
    expect(result.errors.join('\n')).toMatch(/F-01.*family.*axum/);
  });
});

/**
 * F-07's TypeScript half, exercised against a throwaway tree.
 *
 * The guard resolves every scanned path from `root`, so pointing it at a temp
 * directory keeps these cases honest — they cannot accidentally read the real
 * `packages/backend-client`, and a failing assertion cannot leave a stray file
 * behind in the repo.
 */
describe('F-07 TypeScript literals', () => {
  const BACKEND_CLIENT = { name: 'datazen-backend-client', dir: 'packages/backend-client' };
  // Same shape as the `clean` map inside the crate-graph describe: runtime is
  // the only member with an edge, and it is a legal one.
  const CLEAN: Record<string, string[]> = { datazen: ['datazen-driver-api'] };

  function scanWith(source: string, filename = 'probe.ts') {
    const root = mkdtempSync(join(tmpdir(), 'datazen-f07-'));
    const dir = join(root, 'packages/backend-client/src');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, filename), source);
    try {
      return checkPlatformCrateBoundaries({
        root,
        metadata: fixture([...CORE, BACKEND_CLIENT], CLEAN),
        specText: SPEC,
      });
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }

  it('accepts a client that reaches the backend only through its transport', () => {
    const result = scanWith('export function probe(url: string): void {\n  void url;\n}\n');
    expect(result.violations).toEqual([]);
  });

  it('catches a Tauri import', () => {
    const result = scanWith("import { invoke } from '@tauri-apps/api/core';\n");
    expect(result.violations.join('\n')).toMatch(/F-07.*@tauri-apps\//);
  });

  it('catches a direct network call', () => {
    const result = scanWith('export function probe(u: string) { return fetch(u); }\n');
    expect(result.violations.join('\n')).toMatch(/F-07.*fetch\(/);
  });

  it('catches `new XMLHttpRequest()`, not just the token inside a string', () => {
    // The matcher used to consult string literals only, so the one form the
    // rule exists to forbid slipped through and the guard read as green.
    const result = scanWith('export function probe() { return new XMLHttpRequest(); }\n');
    expect(result.violations.join('\n')).toMatch(/F-07.*XMLHttpRequest/);
  });

  it('catches the Tauri token when it only appears as a string', () => {
    const result = scanWith("export const NAME = '@tauri-apps/api/core';\n");
    expect(result.violations.join('\n')).toMatch(/F-07.*@tauri-apps\//);
  });

  it('does not fire on prose that merely mentions a forbidden token', () => {
    const result = scanWith(
      '/** We never use fetch( or XMLHttpRequest here; the transport owns the wire. */\nexport const ok = 1;\n',
      'probe.ts',
    );
    expect(result.violations).toEqual([]);
  });

  it('scans __tests__ too, since a test can smuggle the same call in', () => {
    // `SKIP_DIR_NAMES` deliberately does not exclude `__tests__`; if that ever
    // changes, this is the assertion that notices.
    const root = mkdtempSync(join(tmpdir(), 'datazen-f07-tests-'));
    const dir = join(root, 'packages/backend-client/__tests__');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'probe.test.ts'), 'export const load = (u: string) => fetch(u);\n');
    try {
      const result = checkPlatformCrateBoundaries({
        root,
        metadata: fixture([...CORE, BACKEND_CLIENT], CLEAN),
        specText: SPEC,
      });
      expect(result.violations.join('\n')).toMatch(/F-07.*fetch\(/);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

/**
 * Whether a rule's subject counts as present, and what `--require-layers` does
 * about it.
 *
 * `packages/backend-client` has no `Cargo.toml`, so the real `cargo metadata`
 * never lists it. Resolving a `ts` layer out of cargo's index could therefore
 * only ever answer "absent" — which is how a clean package ended up reported as
 * `VACUOUS F-07: no packages/backend-client yet` by the same run that scanned
 * its 13 files, and how `--require-layers=backend-client` exited 1 on a layer
 * that was present, clean, and fully evaluated.
 *
 * The fixtures below therefore declare `backend-client` **absent from the cargo
 * graph** on purpose: that is the real shape of the workspace, and a test that
 * quietly added it as a member would keep passing after the fix were reverted.
 */
describe('subject presence', () => {
  const CLEAN: Record<string, string[]> = { datazen: ['datazen-driver-api'] };

  /** Guard run against a throwaway tree. `files` is what lands under `packages/backend-client/src`. */
  function withTree(files: Record<string, string> | null, requireLayers: string[] = []) {
    const root = mkdtempSync(join(tmpdir(), 'datazen-presence-'));
    try {
      if (files) {
        const dir = join(root, 'packages/backend-client/src');
        mkdirSync(dir, { recursive: true });
        for (const [name, source] of Object.entries(files)) writeFileSync(join(dir, name), source);
      }
      return checkPlatformCrateBoundaries({
        root,
        // No `backend-client` member: `fixture` is given CORE alone.
        metadata: fixture(CORE, CLEAN),
        specText: SPEC,
        requireLayers,
      });
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }

  const vacuousF07 = (r: ReturnType<typeof withTree>) =>
    r.vacuous.filter((v) => v.rule === 'F-07').map((v) => v.reason);

  it('evaluates a present ts layer instead of calling it absent', () => {
    // The whole point: present and clean is a PASS, and the run must not also
    // claim the rule found nothing to check.
    const result = withTree({ 'probe.ts': 'export const ok = 1;\n' });
    expect(vacuousF07(result)).toEqual([]);
    expect(result.evaluated.map((e) => e.rule)).toContain('F-07');
    expect(result.violations).toEqual([]);
    expect(result.errors).toEqual([]);
  });

  it('still calls a genuinely missing ts layer vacuous', () => {
    // The control for the case above. If this ever empties out, `--require-layers`
    // has lost the ability to reject a layer that is not on disk.
    expect(vacuousF07(withTree(null))).toEqual(['absent']);
  });

  it('tells an empty directory apart from a missing one', () => {
    // `packages/backend-client/` exists but holds no scannable source. "No
    // package yet" would be false; "nothing to read" is true.
    const root = mkdtempSync(join(tmpdir(), 'datazen-empty-'));
    try {
      mkdirSync(join(root, 'packages/backend-client'), { recursive: true });
      const result = checkPlatformCrateBoundaries({
        root,
        metadata: fixture(CORE, CLEAN),
        specText: SPEC,
        requireLayers: [],
      });
      expect(result.vacuous.filter((v) => v.rule === 'F-07').map((v) => v.reason)).toEqual([
        'empty',
      ]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('--require-layers accepts a present ts layer', () => {
    const result = withTree({ 'probe.ts': 'export const ok = 1;\n' }, ['backend-client']);
    expect(result.errors).toEqual([]);
    expect(result.evaluated.map((e) => e.rule)).toContain('F-07');
  });

  it('--require-layers rejects a ts layer that is not on disk', () => {
    const result = withTree(null, ['backend-client']);
    expect(result.errors.join('\n')).toContain('packages/backend-client');
    expect(result.errors.join('\n')).toContain('no source directory');
  });

  it('arms backend-client by default, so deleting it fails the gate', () => {
    // `DEFAULT_REQUIRED_LAYERS` is what makes a *present* F-07 subject
    // non-optional. This asserts the arming survives, and that it is the arming —
    // not luck — that turns a deletion into an error.
    expect(DEFAULT_REQUIRED_LAYERS).toContain('backend-client');
    const result = withTree(null, DEFAULT_REQUIRED_LAYERS);
    expect(result.errors.join('\n')).toContain('packages/backend-client');
    // ...and the unarmed tree still passes.
    expect(withTree(null).errors).toEqual([]);
  });
});

/**
 * F-07's code channel, which matches tokens by identifier boundary.
 *
 * A bare substring search turns `store.refetch()` and `cache.prefetch()` into
 * "a direct network call" — a false positive on correct code, reported at the
 * line of whatever happened to contain the letters `fetch` first. A guard that
 * reports correct code as broken is a guard the team learns to delete.
 */
describe('F-07 transport tokens respect identifier boundaries', () => {
  function scan(source: string) {
    const root = mkdtempSync(join(tmpdir(), 'datazen-boundary-'));
    const dir = join(root, 'packages/backend-client/src');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'probe.ts'), source);
    try {
      return checkPlatformCrateBoundaries({
        root,
        metadata: fixture(
          [
            { name: 'datazen_desktop', dir: 'apps/desktop/src-tauri' },
            { name: 'datazen', dir: 'modules' },
          ],
          {},
        ),
        specText: SPEC,
        requireLayers: [],
      });
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }

  it('does not read `refetch()` as a transport call', () => {
    const result = scan(
      'export type Q = { refetch(): void };\nexport function reload(q: Q): void {\n  q.refetch();\n}\n',
    );
    expect(result.violations).toEqual([]);
  });

  it('blames the call, not an earlier line that merely spells the token', () => {
    // Line 1 spells `fetch(` inside a method signature. The substring search
    // matched there, so the guard reported line 1 — a line with no call on it —
    // and the developer was left hunting for a transport the file never made.
    // Line 4 is the actual call, and it is what the boundary matcher finds.
    const result = scan(
      'export type Q = { refetch(): void };\nexport function load(u: string): Promise<unknown> {\n  const q = {} as Q;\n  return fetch(u).then(q.refetch);\n}\n',
    );
    expect(result.violations.join('\n')).toMatch(/backend-client\/src:4/);
    expect(result.violations.join('\n')).not.toMatch(/backend-client\/src:1/);
  });
});

describe('declaredDependencies — the manifest-side scan', () => {
  // The resolve graph is feature-resolved and therefore structurally blind to
  // optional edges (measured at 2530161ac: redis declares `tauri` optional on
  // line 27 and it is absent from `resolve.deps[]`). These tests are hermetic —
  // synthetic manifest text, no fixture, no real repo — because the failure mode
  // they lock down is a parsing failure that a synthetic fixture would hide.

  it('reads a plain [dependencies] declaration', () => {
    expect(declaredDependencies('[dependencies]\nredis = "0.27"\n')).toEqual([
      { name: 'redis', kind: 'dependencies', target: null, form: 'inline', renamed: false },
    ]);
  });

  it('reads [build-dependencies] as shippable, which the resolve graph confirms', () => {
    expect(
      declaredDependencies('[build-dependencies]\ntauri-plugin = { version = "2" }\n'),
    ).toEqual([
      {
        name: 'tauri-plugin',
        kind: 'build-dependencies',
        target: null,
        form: 'inline',
        renamed: false,
      },
    ]);
  });

  it('reads a target-scoped table — the side door', () => {
    // `[target.'cfg(windows)'.dependencies]` is common in cross-platform drivers
    // and was the open question: a scanner that only matches a literal
    // `[dependencies]` header misses it, and a door that misses the side door is
    // not a door. The real occurrence is `apps/desktop/src-tauri/Cargo.toml`.
    expect(
      declaredDependencies(`[target.'cfg(target_os = "macos")'.dependencies]\nobjc2 = "0.6.4"\n`),
    ).toEqual([
      {
        name: 'objc2',
        kind: 'dependencies',
        target: 'cfg(target_os = "macos")',
        form: 'inline',
        renamed: false,
      },
    ]);
  });

  it('reads a target-scoped build-dependencies table', () => {
    // Same door, harder hinge: the kind is not the section it looks like, and the
    // `cfg()` predicate may carry any number of segments.
    expect(
      declaredDependencies(`[target.'cfg(windows)'.build-dependencies]\ntauri-build = "2"\n`),
    ).toEqual([
      {
        name: 'tauri-build',
        kind: 'build-dependencies',
        target: 'cfg(windows)',
        form: 'inline',
        renamed: false,
      },
    ]);
  });

  it('reads a dotted cfg predicate without splitting inside the quotes', () => {
    expect(
      declaredDependencies(
        `[target.'cfg(all(target_os = "macos", target_arch = "aarch64"))'.dependencies]\nobjc2 = "0.6"\n`,
      ),
    ).toEqual([
      {
        name: 'objc2',
        kind: 'dependencies',
        target: 'cfg(all(target_os = "macos", target_arch = "aarch64"))',
        form: 'inline',
        renamed: false,
      },
    ]);
  });

  it('reads the [dependencies.name] sub-table form', () => {
    expect(
      declaredDependencies('[dependencies.tauri]\nversion = "2"\nfeatures = ["tray-icon"]\n'),
    ).toEqual([
      { name: 'tauri', kind: 'dependencies', target: null, form: 'table', renamed: false },
    ]);
  });

  it('resolves a renamed dependency to the crate that would actually be linked', () => {
    // The key is an alias; `package` is the real crate. Matching the key would let
    // `tui = { package = "tauri" }` walk straight past a tauri-family check.
    expect(
      declaredDependencies('[dependencies]\ntui = { package = "tauri", version = "2" }\n'),
    ).toEqual([
      { name: 'tauri', kind: 'dependencies', target: null, form: 'inline', renamed: true },
    ]);
  });

  it('does not read a non-dependency table as a dependency', () => {
    // `[features]` is where `tauri-plugin = ["dep:tauri"]` lives in redis's
    // manifest; a feature is not an edge, and reading it would report the same
    // name twice for different reasons.
    const found = declaredDependencies('[[features]]\ntauri-plugin = ["dep:tauri"]\n');
    expect(found).toEqual([]);
  });

  it('ignores commented-out declarations', () => {
    expect(declaredDependencies('[dependencies]\n# tauri = "2"\nredis = "0.27"\n')).toEqual([
      { name: 'redis', kind: 'dependencies', target: null, form: 'inline', renamed: false },
    ]);
  });
});
