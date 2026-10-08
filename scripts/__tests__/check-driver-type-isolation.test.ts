/**
 * Fast, cargo-free tests for the driver type isolation guard.
 *
 * Everything here runs against synthetic trees in the system temp directory, so
 * `pnpm test:scripts` stays cheap. That means these tests cannot see the real
 * `cargo metadata` graph — the manifest-verified mutations recorded in
 * `docs/architecture/platform/shared-boundaries-and-ports.md` §8.2 are what
 * prove the guard works on this repository. This file's job is the part a
 * cargo-free guard can still get wrong: which rule fires, on which input, and
 * — the half most guard tests skip — which inputs must **not** fire.
 *
 * Assertions are on exit codes (`exitCodeFor`), never on message wording, so
 * rewording a diagnostic cannot turn the suite red or, worse, silently turn a
 * broken rule green.
 */

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { afterAll, describe, expect, it } from 'vitest';

import {
  DRIVER_CONTRACT_RE,
  checkDriverTypeIsolation,
  exitCodeFor,
  findFileIncludes,
  findWholeIdentifier,
  readRegistry,
  runCli,
  rustIdentOf,
  scanRust,
} from '../check-driver-type-isolation.mjs';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

const roots: string[] = [];
afterAll(() => {
  for (const root of roots) rmSync(root, { recursive: true, force: true });
});

/** A crate under `packages/drivers/*` that is not a driver: real code, no contract. */
const SUPPORT_SOURCE = 'pub fn value_display(v: &str) -> String {\n    v.to_string()\n}\n';

/**
 * A driver crate's minimum viable source: one file carrying the contract
 * marker the guard classifies on. `DRIVER_CONTRACT_RE` is imported rather than
 * restated so that these fixtures cannot quietly drift from the rule.
 */
function contractSource(id: string): string {
  const rustId = `datazen_driver_${id.replace(/-/g, '_')}`;
  return `use ${rustId}::DatabaseDriver;\n\nimpl ${rustId}::DatabaseDriver for ${id}Driver {}\n`;
}

type Member = {
  /** Directory under `packages/drivers/`, or `driver-api` for the shared API. */
  id: string;
  /** Write `src/<id>.rs` with the DatabaseDriver contract marker. */
  contract?: boolean;
  /** Extra files, keyed by path relative to the crate dir. */
  files?: Record<string, string>;
};

type Kind = null | 'normal' | 'build' | 'dev';

type RepoSpec = {
  members: Member[];
  /** `from -> [to]` over package names; unknown pairs are `normal`. */
  edges?: Record<string, string[]>;
  /** Edge kind override by `"from->to"`. */
  kinds?: Record<string, Kind>;
  /**
   * Ids registered as path drivers in `drivers-registry.json`. Omit the key
   * and every contract-carrying member is registered, which is the normal
   * state; pass a shorter list to model a crate nothing can ship.
   */
  registry?: string[];
  /** Non-member packages that still participate in the resolve graph. */
  external?: string[];
};

/**
 * Build a synthetic workspace on disk plus a matching `cargo metadata` document.
 *
 * The metadata is shaped after real cargo output, including the detail that
 * `resolve.nodes` are keyed by `id` and carry no `pkg` field. Files are real:
 * classification, T-02 and T-03 all read the filesystem, so a metadata-only
 * fixture would exercise the rules that happen not to touch disk and silently
 * skip the ones that do.
 */
function makeRepo(spec: RepoSpec): { root: string; metadata: object; registry: object } {
  const root = mkdtempSync(join(tmpdir(), 'dz-driver-types-'));
  roots.push(root);

  const dirOf = new Map<string, string>();
  for (const m of spec.members) {
    dirOf.set(m.id, m.id === 'driver-api' ? 'packages/driver-api' : `packages/drivers/${m.id}`);
  }
  for (const name of spec.external ?? []) dirOf.set(name, null as unknown as string);

  const idOf = (name: string) => `path+file://${dirOf.get(name)}#${name}`;
  const pkgName = (id: string) => `datazen-driver-${id}`;
  const nameOf = (id: string) => (id === 'driver-api' ? 'datazen-driver-api' : pkgName(id));

  for (const m of spec.members) {
    const dir = dirOf.get(m.id) as string;
    mkdirSync(join(root, dir, 'src'), { recursive: true });
    writeFileSync(join(root, dir, 'Cargo.toml'), `[package]\nname = "${nameOf(m.id)}"\nversion = "0.0.0"\n`);
    // A support crate is a real crate with real source that simply does not
    // implement the contract — an absent file would make it `reason:'empty'`,
    // which is a different condition and a different sentence.
    const body = m.contract === false ? SUPPORT_SOURCE : contractSource(m.id);
    writeFileSync(join(root, dir, 'src', `${m.id}.rs`), body);
    for (const [rel, content] of Object.entries(m.files ?? {})) {
      const target = join(root, dir, rel);
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, content);
    }
  }

  const names = [...dirOf.keys()];
  const metadata = {
    workspace_root: root,
    packages: names.map((name) => ({
      id: idOf(name),
      name: nameOf(name),
      manifest_path: join(root, dirOf.get(name) ?? '.', 'Cargo.toml'),
    })),
    workspace_members: spec.members.map((m) => idOf(m.id)),
    resolve: {
      root: null,
      nodes: names.map((name) => ({
        id: idOf(name),
        deps: (spec.edges?.[name] ?? []).map((target) => ({
          pkg: idOf(target),
          dep_kinds: [
            {
              kind: spec.kinds?.[`${name}->${target}`] === undefined ? null : spec.kinds[`${name}->${target}`],
              target: null,
            },
          ],
        })),
      })),
    },
  };

  const registry: Record<string, unknown> = {};
  for (const id of spec.registry ?? spec.members.filter((m) => m.contract !== false).map((m) => m.id)) {
    if (id === 'driver-api') continue;
    registry[id] = { source: 'path', path: `packages/drivers/${id}`, feature: `driver-${id}` };
  }

  return { root, metadata, registry };
}

/** Run the guard on a synthetic repo and hand back its exit code. */
function run(spec: RepoSpec) {
  const { root, metadata, registry } = makeRepo(spec);
  const result = checkDriverTypeIsolation({ root, metadata, registry });
  return { code: exitCodeFor(result), result };
}

/** The two-driver baseline every negative case is measured against. */
const BASE: RepoSpec = {
  members: [{ id: 'pg' }, { id: 'redis' }, { id: 'driver-api' }],
  edges: { pg: ['driver-api'], redis: ['driver-api'] },
};

// ---------------------------------------------------------------------------

describe('scanRust', () => {
  it('blanks line comments, block comments, strings and raw strings', () => {
    const { code } = scanRust(
      [
        '// datazen_driver_redis',
        '/* datazen_driver_redis /* nested datazen_driver_redis */ */',
        'let a = "datazen_driver_redis";',
        'let b = r#"datazen_driver_redis"#;',
        'let c = br##"datazen_driver_redis"##;',
        'let keep = datazen_driver_redis::Thing;',
      ].join('\n'),
    );
    expect(findWholeIdentifier(code, 'datazen_driver_redis')).toHaveLength(1);
    expect(code).toContain('let keep = datazen_driver_redis::Thing;');
  });

  it('keeps an identifier that shares a line with Rust lifetimes', () => {
    // The regression this scanner exists for. `scanCode` in
    // scripts/lib/scanSourceCode.mjs reads the first `'` as an unterminated
    // string and blanks everything up to the second one, which erases the
    // `use` between them — a violation reported as clean. Verified against the
    // real tokenizer: it drops this identifier.
    const line = "fn borrow<'a>(x: &datazen_driver_redis::Conn, y: &'a str) -> &'a str { y }";
    const { code } = scanRust(line);
    expect(findWholeIdentifier(code, 'datazen_driver_redis')).toHaveLength(1);
  });

  it('distinguishes a char literal from a lifetime', () => {
    const { code } = scanRust(
      [
        "let a: u8 = b'x';",
        "let b: u8 = b'\\n';",
        "let c = s.chars().next().unwrap_or(' ');",
        "fn shorten<'a>(s: &'a str) -> &'a str { s }",
      ].join('\n'),
    );
    // Char literals are consumed as tokens, so their bodies are not scanned for
    // identifiers…
    expect(code).not.toContain("b'x'");
    expect(code).not.toContain("b'\\n'");
    expect(code).not.toContain("' '");
    // …while a lifetime is ordinary code and stays visible.
    expect(code).toContain("shorten<'a>");
    expect(code).toContain("&'a str");
  });

  it('reports the line of a blanked literal so findings stay locatable', () => {
    const { strings } = scanRust('// x\nlet a = "one";\nlet b = r#"two"#;\n');
    expect(strings).toEqual([
      { value: 'one', line: 2, raw: false },
      { value: 'r#"two"#', line: 3, raw: true },
    ]);
  });
});

describe('findWholeIdentifier', () => {
  it('requires a right boundary, not only a left one', () => {
    // `findCodeNeedle` in scanSourceCode.mjs only checks the left edge, because
    // its needles end in `(`. Here the needles are crate names, so a longer
    // driver whose name starts the same must not match the shorter one.
    expect(findWholeIdentifier('datazen_driver_redis::T', 'datazen_driver_redis')).toHaveLength(1);
    expect(findWholeIdentifier('datazen_driver_redis_extra::T', 'datazen_driver_redis')).toHaveLength(0);
    expect(findWholeIdentifier('my_datazen_driver_redis::T', 'datazen_driver_redis')).toHaveLength(0);
  });
});

describe('findFileIncludes', () => {
  it('finds #[path] attributes and include! macros with line numbers', () => {
    const source = [
      '#[path = "tests.rs"]',
      'mod t;',
      'include!("../shared/contract.rs");',
      'let s = "#[path = \\"ignored.rs\\"]";',
    ].join('\n');
    expect(findFileIncludes(source).map((i) => [i.kind, i.value, i.line])).toEqual([
      ['path-attr', 'tests.rs', 1],
      ['include-macro', '../shared/contract.rs', 3],
    ]);
  });

  it('matches a #[path] spelled inside a raw string, and that is the known edge', () => {
    // findFileIncludes reads the *unblanked* text, which is what lets it see a
    // target that only exists as a string literal — the whole point of the
    // channel. The price is that a raw string mentioning one is also matched.
    // The exposure is bounded: T-03 reports only when the path resolves inside
    // another driver's directory, so a ghost path elsewhere resolves to a file
    // the guard has no opinion about. Asserted here so the edge is on the
    // record rather than discovered later.
    const source = 'let s = r"#[path = "ghost.rs"]";\n';
    expect(findFileIncludes(source).map((i) => [i.value, i.line])).toEqual([['ghost.rs', 1]]);
  });
});

describe('readRegistry', () => {
  it('takes path drivers and ignores git drivers, which have no path', () => {
    const parsed = {
      pg: { source: 'path', path: 'packages/drivers/pg/' },
      kiwi: { source: 'git', git: 'https://example.invalid/kiwi.git' },
      broken: { source: 'path' },
      notObject: 7,
    };
    expect(readRegistry(parsed)).toEqual([{ id: 'pg', path: 'packages/drivers/pg' }]);
  });
});

describe('rustIdentOf', () => {
  it('is what Cargo itself does to a package name', () => {
    expect(rustIdentOf('datazen-driver-postgres')).toBe('datazen_driver_postgres');
  });
});

// ---------------------------------------------------------------------------

describe('clean tree', () => {
  it('passes two drivers that share only driver-api', () => {
    expect(run(BASE).code).toBe(0);
  });

  it('passes drivers that share the http-support support library', () => {
    // The measured shape of this repository: 10 drivers depend on
    // datazen-driver-http-support. A shared implementation library is exactly
    // what the sanctioned workaround looks like, so it must not read as a
    // violation.
    const { code, result } = run({
      members: [
        { id: 'pg' },
        { id: 'redis' },
        { id: 'http-support', contract: false },
        { id: 'driver-api' },
      ],
      edges: { pg: ['driver-api', 'http-support'], redis: ['driver-api'], 'http-support': ['driver-api'] },
    });
    expect(code).toBe(0);
    expect(result.supportCrates.map((s: { name: string }) => s.name)).toEqual([
      'datazen-driver-http-support',
    ]);
  });
});

describe('T-01 dependency edges', () => {
  it('fails on a normal edge from one driver to another', () => {
    const { code, result } = run({ ...BASE, edges: { pg: ['driver-api', 'redis'], redis: ['driver-api'] } });
    expect(code).toBe(1);
    expect(result.violations).toHaveLength(1);
    expect(result.violations[0]).toContain('T-01');
  });

  it('fails on a dev edge from one driver to another', () => {
    // `docs/development/platform-development-plan.md:99` does not qualify the
    // clause, a dev-dependency compiles the other driver's types into the test
    // binary, and the remedy is the same http-support move the repo already made.
    const { code, result } = run({
      ...BASE,
      edges: { pg: ['driver-api', 'redis'], redis: ['driver-api'] },
      kinds: { 'pg->redis': 'dev' },
    });
    expect(code).toBe(1);
    expect(result.violations[0]).toContain('dev dependency');
  });

  it('does not follow a dependency library dev edge into another driver', () => {
    const { code, result } = run({
      members: [{ id: 'pg' }, { id: 'redis' }, { id: 'bridge', contract: false }, { id: 'driver-api' }],
      edges: { pg: ['bridge'], bridge: ['redis'], redis: ['driver-api'] },
      kinds: { 'bridge->redis': 'dev' },
    });
    expect(result.violations).toEqual([]);
    expect(code).toBe(0);
  });

  it('fails on transitive reach through a non-driver crate', () => {
    // pg does not name redis; it names a helper that names redis. pg still
    // links redis's implementation types, which is the failure `:99` names.
    const { code, result } = run({
      members: [
        { id: 'pg' },
        { id: 'redis' },
        { id: 'bridge', contract: false },
        { id: 'driver-api' },
      ],
      edges: { pg: ['driver-api', 'bridge'], bridge: ['driver-api', 'redis'], redis: ['driver-api'] },
    });
    expect(code).toBe(1);
    expect(result.violations[0]).toContain('transitive reach');
  });
});

describe('T-02 source text', () => {
  it('fails when a driver names another driver in its own source', () => {
    const { code, result } = run({
      ...BASE,
      members: [
        { id: 'pg', files: { 'src/use_redis.rs': 'use datazen_driver_redis::Conn;\n' } },
        { id: 'redis' },
        { id: 'driver-api' },
      ],
    });
    expect(code).toBe(1);
    expect(result.violations[0]).toContain('datazen_driver_redis');
    expect(result.violations[0]).toContain('src/use_redis.rs:1');
  });

  it('still fails when the name shares a line with lifetimes', () => {
    const { code } = run({
      ...BASE,
      members: [
        {
          id: 'pg',
          files: {
            'src/borrow.rs': "fn borrow<'a>(x: &datazen_driver_redis::Conn, y: &'a str) -> &'a str { y }\n",
          },
        },
        { id: 'redis' },
        { id: 'driver-api' },
      ],
    });
    expect(code).toBe(1);
  });

  it('passes when the other driver is only named in a comment or a string', () => {
    const { code } = run({
      ...BASE,
      members: [
        {
          id: 'pg',
          files: {
            'src/notes.rs':
              '// see datazen_driver_redis for the wire format\nlet doc = "datazen_driver_redis::Conn";\n',
          },
        },
        { id: 'redis' },
        { id: 'driver-api' },
      ],
    });
    expect(code).toBe(0);
  });

  it('passes on a longer identifier that merely starts with another driver', () => {
    const { code } = run({
      ...BASE,
      members: [
        { id: 'pg', files: { 'src/shape.rs': 'type Shape = datazen_driver_redis_extra::Config;\n' } },
        { id: 'redis' },
        { id: 'driver-api' },
      ],
    });
    expect(code).toBe(0);
  });

  it('passes when a driver names its own crate', () => {
    const { code } = run({
      ...BASE,
      members: [
        { id: 'pg', files: { 'src/self.rs': 'const SELF: &str = r#"datazen_driver_pg"#;\n' } },
        { id: 'redis' },
        { id: 'driver-api' },
      ],
    });
    expect(code).toBe(0);
  });
});

describe('T-03 path includes', () => {
  it('fails when #[path] resolves into another driver', () => {
    const { code, result } = run({
      ...BASE,
      members: [
        {
          id: 'pg',
          files: {
            'src/lib.rs': contractSource('pg') + '#[path = "../../redis/src/commands/mod.rs"]\nmod borrowed;\n',
            '../redis/src/commands/mod.rs': 'pub struct Conn;\n',
          },
        },
        { id: 'redis' },
        { id: 'driver-api' },
      ],
    });
    expect(code).toBe(1);
    expect(result.violations[0]).toContain('T-03');
  });

  it('passes on an intra-crate #[path]', () => {
    const { code } = run({
      ...BASE,
      members: [
        {
          id: 'pg',
          files: {
            'src/lib.rs': contractSource('pg') + '#[path = "tests.rs"]\nmod inner;\n',
            'src/tests.rs': 'pub fn t() {}\n',
          },
        },
        { id: 'redis' },
        { id: 'driver-api' },
      ],
    });
    expect(code).toBe(0);
  });

  it('passes on a #[path] into the shared support library', () => {
    // The idiom this repository actually uses:
    // packages/drivers/postgres/tests/real_driver_contract.rs:16 pulls in
    // ../../http-support/tests/support/real_driver_contract.rs.
    const { code } = run({
      members: [
        { id: 'pg', files: { 'tests/contract.rs': '#[path = "../../http-support/tests/support/t.rs"]\nmod c;\n' } },
        { id: 'redis' },
        { id: 'http-support', contract: false },
        { id: 'driver-api' },
      ],
      edges: { pg: ['driver-api', 'http-support'], redis: ['driver-api'], 'http-support': ['driver-api'] },
    });
    expect(code).toBe(0);
  });
});

// ---------------------------------------------------------------------------

describe('the guard refuses to report a green it cannot mean', () => {
  it('fails when fewer than two drivers resolve', () => {
    // Every rule here compares a driver against *another* driver. With one
    // driver the guard is arithmetically incapable of firing, so exit 0 would
    // be a green carrying no information. This is the direct answer to "edit
    // both hand-written arrays to the same wrong value": shrinking the subject
    // set stops the run instead of disabling it.
    const { code, result } = run({
      members: [{ id: 'pg' }, { id: 'driver-api' }],
      edges: { pg: ['driver-api'] },
    });
    expect(code).toBe(1);
    expect(result.errors.join('\n')).toContain('arithmetically incapable');
  });

  it('distinguishes a subject absent from disk from one present but empty', () => {
    // Both are ungreen-worthy facts about coverage, but they are different
    // facts and a single `no files` bucket would conflate "the crate is gone"
    // with "the crate is there and I read all of it".
    const { root, metadata, registry } = makeRepo({
      members: [{ id: 'pg' }, { id: 'redis' }, { id: 'driver-api' }],
    });
    const presentAndClean = checkDriverTypeIsolation({ root, metadata, registry });
    expect(presentAndClean.vacuous).toEqual([]);
    expect(exitCodeFor(presentAndClean)).toBe(0);

    // Now remove a driver's directory while cargo still lists it as a member and
    // the registry still advertises it.
    rmSync(join(root, 'packages/drivers/redis'), { recursive: true, force: true });
    const absent = checkDriverTypeIsolation({ root, metadata, registry });
    expect(absent.vacuous).toEqual([
      {
        rule: 'T-01..T-03',
        driver: 'datazen-driver-redis',
        relDir: 'packages/drivers/redis',
        reason: 'absent',
      },
    ]);
    // The vacuous note alone would still exit 0; it is S3 that promotes it, since
    // a registered driver the guard cannot read is a registration the build
    // would honour and the review would not.
    expect(exitCodeFor(absent)).toBe(1);
    expect(absent.errors.join('\n')).toContain('packages/drivers/redis');
  });

  it('reports reason=empty for a driver whose directory holds no .rs file', () => {
    const { root, metadata, registry } = makeRepo({
      members: [{ id: 'pg' }, { id: 'redis' }, { id: 'driver-api' }],
    });
    for (const id of ['pg', 'redis']) {
      rmSync(join(root, 'packages/drivers', id, 'src', `${id}.rs`), { force: true });
    }
    const result = checkDriverTypeIsolation({ root, metadata, registry });
    expect(result.vacuous.map((v: { driver: string; reason: string }) => [v.driver, v.reason])).toEqual([
      ['datazen-driver-pg', 'empty'],
      ['datazen-driver-redis', 'empty'],
    ]);
    expect(exitCodeFor(result)).toBe(1);
  });
});

describe('registry cross-check', () => {
  it('fails when the registry names a path driver cargo cannot see', () => {
    const { code, result } = run({
      members: [{ id: 'pg' }, { id: 'redis' }, { id: 'driver-api' }],
      edges: { pg: ['driver-api'], redis: ['driver-api'] },
      registry: ['pg', 'redis', 'kiwi'],
    });
    expect(code).toBe(1);
    expect(result.errors.join('\n')).toContain('not a Cargo workspace member');
  });

  it('fails when the registry calls a crate a driver but its source is not one', () => {
    // The guard's classification is a measurement, so the danger is not "wrong
    // measurement" but "measurement that silently stopped matching". S3 catches
    // that from the other side.
    const { code, result } = run({
      members: [{ id: 'pg' }, { id: 'redis', contract: false }, { id: 'driver-api' }],
      edges: { pg: ['driver-api', 'redis'], redis: ['driver-api'] },
    });
    expect(code).toBe(1);
    expect(result.errors.join('\n')).toContain('carries no');
  });

  it('fails when a driver by source is registered nowhere', () => {
    // resolve-drivers.mjs injects a feature per registered driver. An
    // unregistered crate is judged by every rule here and can never be linked.
    const { code, result } = run({ ...BASE, registry: ['pg'] });
    expect(code).toBe(1);
    expect(result.errors.join('\n')).toContain('never be linked');
  });

  it('does not require git drivers to exist in the workspace', () => {
    const { code } = run({ ...BASE, registry: ['pg', 'redis'] });
    expect(code).toBe(0);
  });
});

describe('DRIVER_CONTRACT_RE', () => {
  it('matches a real driver implementation', () => {
    expect(DRIVER_CONTRACT_RE.test('impl DatabaseDriver for PgDriver {}')).toBe(true);
    expect(DRIVER_CONTRACT_RE.test('impl datazen_driver_api::DatabaseDriver for PgDriver {}')).toBe(true);
  });

  it('does not promote a mere mention of the trait to a driver', () => {
    expect(DRIVER_CONTRACT_RE.test('pub use datazen_driver_api::DatabaseDriver;')).toBe(false);
    expect(DRIVER_CONTRACT_RE.test('fn make() -> Box<dyn DatabaseDriver> {}')).toBe(false);
    expect(DRIVER_CONTRACT_RE.test('/// implements DatabaseDriver for Pg')).toBe(false);
  });
});

describe('runCli', () => {
  it('rejects an unknown argument with exit 2, distinct from pass and fail', () => {
    expect(runCli({ argv: ['--nope'] })).toBe(2);
  });

  it('exits 0 on --help', () => {
    expect(runCli({ argv: ['--help'] })).toBe(0);
  });

  it('exits 2 when cargo cannot run, rather than reporting a clean tree', () => {
    const empty = mkdtempSync(join(tmpdir(), 'dz-no-cargo-'));
    roots.push(empty);
    expect(runCli({ argv: [`--root=${empty}`] })).toBe(2);
  });
});

describe('this repository', () => {
  it('has no cross-driver type edge in the committed tree', () => {
    // The one test here that is not synthetic. It costs a real `cargo metadata`
    // call, which is why it is the only one — but it is also the only assertion
    // that would notice the real 15-driver tree drifting into the shape the
    // fixtures assume.
    const result = checkDriverTypeIsolation({ root: REPO_ROOT });
    expect(result.violations).toEqual([]);
    expect(result.errors).toEqual([]);
    expect(exitCodeFor(result)).toBe(0);
    expect(result.evaluated.length).toBeGreaterThan(1);
  });
});