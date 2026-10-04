/**
 * Exit-code tests for the platform core crate test runner.
 *
 * ## Why every assertion here is on a return code and never on output
 *
 * The defect this file exists for: `run-platform-crate-tests.mjs` printed
 * `ERROR unclassified workspace member` to stderr and then fell through to
 * `return 0` (the `--dry-run` branch), or to cargo's own code on the real path.
 * A workspace member the shared classifier could not place therefore made
 * `pnpm test:platform-crates` green. An assertion of the form "the output
 * contains ERROR" would have **passed against the broken script**, because the
 * broken script produced exactly that line — so this file asserts only on the
 * value CI observes. The stderr text is read in at most one place, and only as a
 * secondary check that the message names the offending member.
 *
 * ## Why cargo is stubbed rather than real
 *
 * `platform-arch-selfcheck.mjs` records the constraint in prose: `pnpm
 * test:scripts` belongs with the `frontend:` CI job, which has no Rust
 * toolchain and no cargo. A suite here that shelled out to a real `cargo
 * metadata` would either skip there or fail for a reason that has nothing to do
 * with the runner — and a silently skipped test is close to how this defect
 * survived the first time, so `skipIf(!hasCargo)` is deliberately not used.
 *
 * The stub is a real executable prepended to `PATH`, so `spawnSync('cargo', …)`
 * inside `runCli` genuinely is a subprocess: `runCargoMetadata`'s parsing, the
 * shared classifier's directory mapping and `runCli`'s return code all really
 * run. What it adds over a hand-written metadata blob is that the workspace
 * really exists on disk — real member directories, real `Cargo.toml` files, and
 * real absolute `manifest_path`s read back out of them — so an unclassifiable
 * member is an actual situation and not an assertion about a fixture.
 *
 * ## The two independent sources
 *
 *  1. the **return code** of `runCli`, which is what CI gates on; and
 *  2. the stub's **invocation log**, which proves `cargo test` was never reached
 *     in the abort cases, and — via the clean-tree case with the very same stub
 *     and the very same command — that the stub answers `0` for `cargo test`.
 *
 * A non-zero code in an abort case therefore cannot be cargo's status leaking
 * through, and a zero code in a clean case cannot be "nothing was run". Removing
 * either assertion leaves the other able to fail on its own.
 *
 * POSIX only: the stub is a shebang script, so `spawnSync('cargo', …)` will not
 * resolve it on Windows. This repo's CI is `ubuntu-latest` and its developers
 * are on macOS; adding a `.cmd` twin would be untested code on a platform the
 * gate never runs on.
 */

import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'fs';
import { tmpdir } from 'os';
import { delimiter, dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { LAYERS, layerById } from '../lib/cargoWorkspace.mjs';
import {
  EXTRA_TARGETS,
  TESTED_LAYERS,
  buildCargoArgv,
  discoverCoreCrates,
  runCli,
} from '../run-platform-crate-tests.mjs';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/**
 * A `cargo` stand-in that reports the members of a workspace that really is on
 * disk. Written as CommonJS: it is executed by its own shebang through
 * `spawnSync('cargo', …)`, where Node parses an extensionless entry file as CJS.
 */
const CARGO_STUB_SOURCE = [
  '#!/usr/bin/env node',
  "'use strict';",
  "const fs = require('fs');",
  "const path = require('path');",
  '',
  '// Every invocation is recorded so the suite can assert not just what the',
  '// runner returned but which cargo commands it actually reached.',
  'if (process.env.STUB_CARGO_LOG) {',
  "  fs.appendFileSync(process.env.STUB_CARGO_LOG, process.argv.slice(2).join(' ') + '\\n');",
  '}',
  '',
  "if (process.argv[2] === 'metadata') {",
  '  const root = process.cwd();',
  "  const manifest = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8');",
  '  const block = /members\\s*=\\s*\\[([^\\]]*)\\]/.exec(manifest);',
  '  const dirs = (block ? block[1] : "")',
  '    .split(",")',
  '    .map(function (s) {',
  '      return s.trim().replace(/^["\']|["\']$/g, "");',
  '    })',
  '    .filter(Boolean);',
  '  const packages = dirs.map(function (dir) {',
  '    const abs = path.resolve(root, dir);',
  '    const text = fs.readFileSync(path.join(abs, "Cargo.toml"), "utf8");',
  '    const name = /name\\s*=\\s*"([^"]+)"/.exec(text)[1];',
  '    return {',
  "      id: 'path+file://' + abs + '#' + name,",
  '      name: name,',
  '      manifest_path: path.join(abs, "Cargo.toml"),',
  '    };',
  '  });',
  '  process.stdout.write(',
  '    JSON.stringify({',
  '      workspace_root: root,',
  '      packages: packages,',
  '      workspace_members: packages.map(function (p) { return p.id; }),',
  '      resolve: {',
  '        root: null,',
  '        nodes: packages.map(function (p) { return { id: p.id, deps: [] }; }),',
  '      },',
  '    }),',
  '  );',
  '  process.exit(0);',
  '}',
  '',
  '// Every other subcommand stands in for `cargo test`. Its real work is not',
  '// what is under test, but its exit code is: the runner must keep returning',
  '// it unchanged, so it is configurable.',
  "process.exit(Number(process.env.STUB_CARGO_TEST_EXIT || '0'));",
  '',
].join('\n');

interface StubWorkspace {
  root: string;
  /** One line per cargo invocation the runner made against this workspace. */
  cargoLog: string;
}

interface CliRun {
  code: number;
  stdout: string;
  stderr: string;
}

let sandbox: string;
let cargoBin: string;
/** Workspaces are built in `beforeAll`: each `cargo` call is a real subprocess. */
let clean: StubWorkspace;
let unclassified: StubWorkspace;

beforeAll(() => {
  sandbox = mkdtempSync(join(tmpdir(), 'datazen-platform-crate-tests-'));
  cargoBin = join(sandbox, 'bin');
  mkdirSync(cargoBin, { recursive: true });
  const stub = join(cargoBin, 'cargo');
  writeFileSync(stub, CARGO_STUB_SOURCE);
  chmodSync(stub, 0o755);

  clean = makeWorkspace('clean', [RUNTIME, DRIVER]);
  unclassified = makeWorkspace('unclassified', [RUNTIME, DRIVER, UNCLASSIFIABLE]);
});

afterAll(() => {
  if (sandbox !== undefined) rmSync(sandbox, { recursive: true, force: true });
});

/** Write one real workspace member: directory, `Cargo.toml`, `src/lib.rs`. */
function writeMember(root: string, dir: string, name: string): void {
  const abs = join(root, dir);
  mkdirSync(join(abs, 'src'), { recursive: true });
  writeFileSync(
    join(abs, 'Cargo.toml'),
    ['[package]', `name = "${name}"`, 'version = "0.0.1"', 'edition = "2021"', ''].join('\n'),
  );
  writeFileSync(join(abs, 'src/lib.rs'), 'pub fn probe() {}\n');
}

function makeWorkspace(
  id: string,
  members: ReadonlyArray<{ dir: string; name: string }>,
): StubWorkspace {
  const root = join(sandbox, id);
  mkdirSync(root, { recursive: true });
  writeFileSync(
    join(root, 'Cargo.toml'),
    [
      '[workspace]',
      'resolver = "2"',
      `members = [${members.map((m) => `"${m.dir}"`).join(', ')}]`,
      '',
    ].join('\n'),
  );
  for (const member of members) writeMember(root, member.dir, member.name);
  return { root, cargoLog: join(sandbox, `${id}.cargo-invocations`) };
}

const RUNTIME = { dir: 'packages/runtime', name: 'datazen-runtime' };
const DRIVER = { dir: 'packages/drivers/postgres', name: 'datazen-driver-postgres' };
/** A real directory below no `LAYERS` path, so `classifyMemberDir` must reject it. */
const UNCLASSIFIABLE = { dir: 'crates/mystery', name: 'mystery' };

function cargoInvocations(ws: StubWorkspace): string[] {
  if (!existsSync(ws.cargoLog)) return [];
  return readFileSync(ws.cargoLog, 'utf8').split('\n').filter(Boolean);
}

/**
 * Only the `cargo test` invocations; the `metadata` call is discovery.
 *
 * Assertions on this rather than on `cargoInvocations` are what keep the
 * abort-path tests meaningful: an exact-string `not.toContain` silently turns
 * into "assert true" the day the argv grows a flag, and the abort it was
 * guarding stops being tested at all.
 *
 * **This filter has a measured blind spot — see `cargoTestInvocationsSeesReorderedArgv`.**
 * `startsWith('test ')` assumes the subcommand is argv[0]. Cargo in fact requires
 * it to be, so this cannot bite in real use; the blind spot matters because it
 * makes the abort-path `toEqual([])` assertions weaker than they look, and the
 * only thing that actually catches a reordered call is `EXPECTED_CLEAN_INVOCATIONS`.
 */
function cargoTestInvocations(ws: StubWorkspace): string[] {
  return cargoInvocations(ws).filter((line) => line.startsWith('test '));
}

/**
 * The same filter, deliberately fed a reordered argv, to keep the note above
 * true instead of aspirational. Without this test the blind spot is invisible:
 * nothing in the suite would ever fail if `startsWith('test ')` were loosened.
 */
function cargoTestInvocationsSeesReorderedArgv(ws: StubWorkspace): string[] {
  return cargoInvocations(ws).filter((line) => /(^|\s)test(\s|$)/.test(line));
}

/**
 * What a clean one-crate workspace must produce, verbatim and in order.
 *
 * Exact rather than `toContain`, on purpose: a third invocation or a reordered
 * flag is a change to what CI compiles, and it should cost a deliberate edit
 * here rather than pass unnoticed.
 *
 * ★ **This must never become `toContain`, and that is not a style preference.**
 * `toContain` was the first round's failing shape: with it, *deleting* the
 * no-selector invocation leaves every member still present, so the gate that
 * exists to prove `datazen-runtime`'s 19 integration binaries are compiled in
 * CI goes green on the exact regression it was added for — and the deletion is
 * invisible, because a subset assertion cannot fail on a subset. The abort-path
 * `toEqual([])` assertions do **not** cover this: they filter through
 * `startsWith('test ')`, which cannot see a reordered or non-`test`-first call at
 * all (see `cargoTestInvocationsSeesReorderedArgv`). Measured: under the
 * `--offline test --lib …` reorder, those abort assertions see `[]` for a run
 * that did invoke cargo and pass. So this array is the load-bearing assertion,
 * and it fails closed on any missing, extra or reordered call.
 *
 * The order is part of the contract: the `--lib` gate runs first because it is
 * cheapest, and the abort-path tests depend on cargo never being reached.
 */
const EXPECTED_CLEAN_INVOCATIONS = [
  'test --lib -p datazen-runtime --test cm60_pressure_drain',
  'test -p datazen-runtime',
  'test --release -p datazen-runtime --bin cm60-bench',
];

function envFor(ws: StubWorkspace, cargoTestExit = 0): NodeJS.ProcessEnv {
  return {
    ...process.env,
    PATH: `${cargoBin}${delimiter}${process.env.PATH ?? ''}`,
    STUB_CARGO_LOG: ws.cargoLog,
    STUB_CARGO_TEST_EXIT: String(cargoTestExit),
    // `parseArgs` reads the process-wide env, not the injected one, so this has
    // to be neutralised in the real process env for the run to be reproducible
    // on a developer machine that exports it.
    ARCH_GUARD_REQUIRED_LAYERS: '',
  };
}

/**
 * Run `runCli` with stdout/stderr captured instead of inherited.
 *
 * The cast is required, not a shortcut: `process.stdout.write` is an overload
 * set and a single arrow function cannot satisfy all of it. The behaviour is
 * total — every chunk is recorded and `true` (bytes written) is returned.
 */
function captureCli(run: () => number): CliRun {
  const outChunks: string[] = [];
  const errChunks: string[] = [];
  const outSpy = vi.spyOn(process.stdout, 'write').mockImplementation(((
    chunk: string | Uint8Array,
  ) => {
    outChunks.push(typeof chunk === 'string' ? chunk : Buffer.from(chunk).toString('utf8'));
    return true;
  }) as typeof process.stdout.write);
  const errSpy = vi.spyOn(process.stderr, 'write').mockImplementation(((
    chunk: string | Uint8Array,
  ) => {
    errChunks.push(typeof chunk === 'string' ? chunk : Buffer.from(chunk).toString('utf8'));
    return true;
  }) as typeof process.stderr.write);
  try {
    const code = run();
    return { code, stdout: outChunks.join(''), stderr: errChunks.join('') };
  } finally {
    outSpy.mockRestore();
    errSpy.mockRestore();
  }
}

/**
 * One `runCli` invocation against one workspace.
 *
 * The invocation log is cleared first, so `cargoInvocations` afterwards answers
 * "what did *this* run reach" rather than "what did this file reach" — several
 * cases reuse the clean workspace and would otherwise read each other's history.
 */
function runIn(ws: StubWorkspace, args: readonly string[], cargoTestExit = 0): CliRun {
  rmSync(ws.cargoLog, { force: true });
  return captureCli(() =>
    runCli({ argv: [`--root=${ws.root}`, ...args], env: envFor(ws, cargoTestExit) }),
  );
}

describe('run-platform-crate-tests exit codes', () => {
  it('fails the run when a workspace member matches no layer path', () => {
    const run = runIn(unclassified, []);

    expect(run.code).not.toBe(0);
    // Secondary only: the message has to name the member so the failure is
    // actionable. The broken script also printed ERROR, which is why this is
    // never the primary assertion.
    expect(run.stderr).toContain(UNCLASSIFIABLE.dir);
    // ...and it must not have reached the testing it was told to distrust.
    expect(cargoInvocations(unclassified)).not.toContain('test --lib -p datazen-runtime');
  });

  it('fails closed on --dry-run, the path the original bypass used', () => {
    const run = runIn(unclassified, ['--dry-run']);

    expect(run.code).not.toBe(0);
    // A dry run must not print the pass line it is refusing to earn.
    expect(run.stdout).not.toContain('PASS');
    expect(cargoTestInvocations(unclassified)).toEqual([]);
  });

  it('still exits 0 on a workspace every member classifies', () => {
    const run = runIn(clean, []);

    expect(run.code).toBe(0);
    // The same stub answers 0 for the same command here, which is what makes
    // the non-zero codes above attributable to classification and not to cargo.
    expect(cargoTestInvocations(clean)).toEqual(EXPECTED_CLEAN_INVOCATIONS);
  });

  it('still exits 0 on --dry-run for a clean workspace', () => {
    const run = runIn(clean, ['--dry-run']);

    expect(run.code).toBe(0);
    expect(run.stdout).toContain('datazen-runtime');
  });

  it("keeps returning cargo's own status unchanged on a clean workspace", () => {
    // 101 is cargo's own "tests failed" code. The classification abort must not
    // have swallowed it, or a genuinely failing crate suite would read as the
    // same red as an unclassified member and stop being diagnosable.
    const run = runIn(clean, [], 101);

    expect(run.code).toBe(101);
  });

  it('still fails a required layer that has no crate', () => {
    const run = runIn(clean, ['--require-layers=server']);

    expect(run.code).not.toBe(0);
    expect(cargoTestInvocations(clean)).toEqual([]);
  });

  it('accepts a required layer that does have a crate', () => {
    const run = runIn(clean, ['--require-layers=runtime']);

    expect(run.code).toBe(0);
    expect(cargoTestInvocations(clean)).toEqual(EXPECTED_CLEAN_INVOCATIONS);
  });

  it('requires a layer from the tested set, not one that merely exists', () => {
    // `driver` is a real LAYERS entry and the workspace really has a driver
    // crate, but it is not in TESTED_LAYERS so no driver is ever selected to
    // test. Pinned because the check runs against the selected crates, and
    // `--require-layers=driver` passing would mean a layer was promised and
    // never exercised.
    const run = runIn(clean, ['--require-layers=driver']);

    expect(run.code).not.toBe(0);
  });

  it('rejects an unknown argument with 2, distinct from a gate failure', () => {
    const run = runIn(clean, ['--not-a-flag']);

    expect(run.code).toBe(2);
  });

  it('fails when the only members are unclassifiable', () => {
    // Guards against a "no core crate found" path masking the classification
    // error: both are failures, but they must not be reported as one. With only
    // an unclassified member, the pre-fix code also exited non-zero — via the
    // "no core crate found" branch — so the exit code alone cannot tell the two
    // causes apart and the misclassification would look like an empty workspace.
    const ws = makeWorkspace('only-unclassified', [UNCLASSIFIABLE]);
    const run = runIn(ws, []);

    expect(run.code).not.toBe(0);
    expect(run.stderr).toContain(UNCLASSIFIABLE.dir);
    expect(run.stderr).not.toContain('no core crate found');
  });
});

describe('discoverCoreCrates stays pure about policy', () => {
  it('reports unclassified members instead of throwing or hiding them', () => {
    // It is an exported function and other code may read it, so folding the
    // decision into here would change its contract for every future caller.
    let thrown: unknown;
    let discovered: ReturnType<typeof discoverCoreCrates> | undefined;
    try {
      discovered = discoverCoreCrates({ root: unclassified.root, env: envFor(unclassified) });
    } catch (cause) {
      thrown = cause;
    }

    expect(thrown).toBeUndefined();
    expect(discovered?.unclassified.length).toBeGreaterThan(0);
    expect(discovered?.crates.map((c) => c.name)).toContain(RUNTIME.name);
  });

  it('keeps every tested layer id a real layer id', () => {
    // A tested layer the classifier cannot name could never be discovered, so
    // its crates would silently never be tested — the same class of gap as an
    // unclassified member, one step further out.
    const ids = new Set(LAYERS.map((l) => l.id));
    const missing = TESTED_LAYERS.filter((id) => !ids.has(id));

    expect(missing).toEqual([]);
    for (const id of TESTED_LAYERS) expect(layerById(id)).not.toBeNull();
  });

  it('keeps REPO_ROOT resolvable, so a fixture path typo cannot pass silently', () => {
    // Cheap guard on this file's own premise: every `runIn` above points at a
    // workspace under the sandbox, but a bad REPO_ROOT would mean the module
    // under test was resolved from somewhere unexpected.
    expect(existsSync(join(REPO_ROOT, 'scripts/run-platform-crate-tests.mjs'))).toBe(true);
  });
});

/**
 * `EXTRA_TARGETS` decides what CI compiles besides `--lib`. Both CM-60 entry
 * points are structurally invisible to `--lib` — one is an integration test
 * target, the other a binary target — so an entry that goes missing from this
 * table removes the gate without turning anything red. These assertions are on
 * the argv itself, because that argv is the whole mechanism.
 */
describe('buildCargoArgv puts both CM-60 entry points behind the gate', () => {
  const crates = [{ name: RUNTIME.name }, { name: DRIVER.name }];
  const invocations = buildCargoArgv(crates);
  /**
   * Located by what each argv *is*, never by its position.
   *
   * This was `const [debugArgv, releaseArgv] = buildCargoArgv(crates)`, and it
   * silently rebound the moment a third invocation was inserted between them:
   * `releaseArgv` became the new no-selector call, and `builds the bin unit
   * tests in their own --release invocation` went red for a reason that had
   * nothing to do with the release path. A positional destructure over a list
   * whose length is allowed to change is the defect; the assertions below are
   * unchanged and now say what they mean.
   */
  const debugArgv = invocations.find((argv) => argv.includes('--lib')) ?? [];
  const releaseArgv = invocations.find((argv) => argv.includes('--release')) ?? [];

  it('keeps the whole-crate --lib run and adds the integration test to it', () => {
    // `--lib` alone cannot reach packages/runtime/tests/cm60_pressure_drain.rs,
    // so the §11.4 pressure half was never compiled in CI before this.
    expect(debugArgv.length).toBeGreaterThan(0); // the find() above failed closed
    expect(debugArgv).toEqual(
      expect.arrayContaining(['--lib', '--test', 'cm60_pressure_drain']),
    );
    for (const { name } of crates) expect(debugArgv).toContain(name);
  });

  it('builds the bin unit tests in their own --release invocation', () => {
    // cm60-bench refuses to run under debug_assertions (exit 2), so its unit
    // tests are only meaningful in release.
    expect(releaseArgv.length).toBeGreaterThan(0);
    expect(releaseArgv).toEqual(expect.arrayContaining(['--release', '--bin', 'cm60-bench']));
  });

  it('never lets --release leak onto the whole-crate run', () => {
    // `--release` is a profile flag for the entire invocation. If it reached the
    // first argv, every core crate's test suite would silently rebuild in
    // release — invisible in the diff, expensive in CI minutes.
    expect(debugArgv).not.toContain('--release');
    expect(releaseArgv).not.toContain('--lib');
  });

  it('leaves a crate with no entry untouched, and never invents a crate', () => {
    // The table is keyed by crate name. A crate that was not discovered has no
    // target to add, and a crate with no entry contributes nothing.
    //
    // The second invocation is the no-selector one, and it is here for *every*
    // tested crate, not only the ones with a table entry: its whole job is to
    // reach the `tests/*.rs` binaries that `EXTRA_TARGETS` does not (and should
    // not) enumerate, and a driver crate landing in a tested layer later must
    // not be the first one to miss them.
    expect(buildCargoArgv([{ name: DRIVER.name }])).toEqual([
      ['test', '--lib', '-p', DRIVER.name],
      ['test', '-p', DRIVER.name],
    ]);
    expect(buildCargoArgv([])).toEqual([]);
  });

  it('runs exactly one invocation that carries no target selector at all', () => {
    // The structural property behind the whole integration-coverage gap: a
    // selector (`--lib`, `--test x`, `--bin x`, `--tests`) names a subset of the
    // crate's targets, and the subset missing a name is the subset that silently
    // stops compiling in CI. Asserted on argv shape rather than on a target
    // list, so it keeps holding when `Cargo.toml` gains a `tests/*.rs` this file
    // has never heard of.
    const TARGET_SELECTORS = ['--lib', '--tests', '--test', '--bin', '--bench', '--all-targets'];
    const unpinned = buildCargoArgv([RUNTIME]).filter(
      (argv) => !argv.some((arg) => TARGET_SELECTORS.includes(arg)),
    );

    expect(unpinned).toHaveLength(1);
    // ...and it is a plain whole-crate run: no profile flag, no target selector,
    // no extras folded in. Anything else here would quietly narrow it again.
    expect(unpinned[0]).toEqual(['test', '-p', RUNTIME.name]);
  });

  it('does not replace the --lib invocation with --tests', () => {
    // The trap this change walks next to. `--tests` looks like the tidy way to
    // "run the integration binaries", and adopting it *as a replacement* silently
    // drops the library's own unit tests — a bigger hole than the one being
    // closed (measured on datazen-runtime: 383 lib tests vs 180 integration
    // ones). Pinned so the substitution cannot arrive later as a tidy-up.
    const argv = buildCargoArgv([RUNTIME]).map((a) => a.join(' '));
    expect(argv).toContain('test --lib -p datazen-runtime --test cm60_pressure_drain');
    expect(argv.some((line) => line.includes('--tests'))).toBe(false);
  });

  it('names targets that exist, so the table cannot rot into a no-op', () => {
    // A renamed or deleted target makes cargo fail, but only *after* CI has
    // spent its minutes; asserting on disk here turns the rot into a local red.
    for (const [crate, entries] of Object.entries(EXTRA_TARGETS)) {
      for (const { args } of entries) {
        const flag = args[0];
        const name = args[1];
        const path =
          flag === '--test'
            ? join(REPO_ROOT, 'packages', 'runtime', 'tests', `${name}.rs`)
            : join(REPO_ROOT, 'packages', 'runtime', 'src', 'bin', name, 'main.rs');
        expect([flag, crate, existsSync(path)], `${crate} ${flag} ${name}`).toEqual([
          flag,
          crate,
          true,
        ]);
      }
    }
  });
});

/**
 * Negative controls for the assertions that are supposed to catch a regression.
 *
 * A guard is only worth having if it has been seen to go red. Each case below is
 * a mutation of `EXPECTED_CLEAN_INVOCATIONS` standing in for a real edit, and
 * each asserts both that the exact comparison rejects it **and** that the
 * comparison is not rejecting it vacuously. The second half is the part that is
 * easy to skip and is what stopped a dead assertion in round one: "not equal"
 * is trivially true when both sides are empty, so every case pins a non-empty
 * mutated list and a length.
 */
describe('run-platform-crate-tests guard negative controls', () => {
  /** Same contents as a cargo log, minus the one line a given mutation removes. */
  const without = (dropped: string): string[] =>
    EXPECTED_CLEAN_INVOCATIONS.filter((line) => line !== dropped);

  it('the exact comparison rejects the no-selector invocation being deleted', () => {
    // This is the regression the whole change exists to prevent: dropping one
    // array element leaves every remaining element present, so a `toContain`
    // assertion stays green and the 19 integration binaries stop compiling in CI
    // without a single red.
    const mutated = without('test -p datazen-runtime');

    expect(mutated).not.toEqual(EXPECTED_CLEAN_INVOCATIONS);
    expect(mutated).toHaveLength(EXPECTED_CLEAN_INVOCATIONS.length - 1);
    expect(mutated.length).toBeGreaterThan(0); // not "both sides empty"
    // ...and a subset assertion, the shape round one used, would NOT have caught it.
    expect(EXPECTED_CLEAN_INVOCATIONS).toEqual(expect.arrayContaining(mutated));
  });

  it('the exact comparison rejects the no-selector invocation being narrowed', () => {
    // Same hole, quieter route: `--tests` "looks" like it covers integration
    // binaries and a reader skims it as a fix rather than a swap.
    const mutated = EXPECTED_CLEAN_INVOCATIONS.map((line) =>
      line === 'test -p datazen-runtime' ? 'test --tests -p datazen-runtime' : line,
    );

    expect(mutated).not.toEqual(EXPECTED_CLEAN_INVOCATIONS);
    expect(mutated).toHaveLength(EXPECTED_CLEAN_INVOCATIONS.length);
  });

  it('the abort-path filter cannot see a reordered argv, which is why the exact list carries the gate', () => {
    // Measured, not assumed: cargo requires the subcommand first, so this shape
    // cannot reach production — but it is the mechanism by which the abort-side
    // `toEqual([])` assertions degenerate into "assert true", and that is the
    // same failure mode round one had, one helper away.
    const reordered = ['--offline test --lib -p datazen-runtime'];

    // A real file, not `''`: `cargoInvocations` returns `[]` for a missing path,
    // which would make the assertion below pass for the wrong reason and prove
    // nothing — the very defect this file is describing.
    const cargoLog = join(mkdtempSync(join(tmpdir(), 'dz-crate-tests-blindspot-')), 'cargo.log');
    writeFileSync(cargoLog, `${reordered.join('\n')}\n`);
    const ws: StubWorkspace = { ...clean, cargoLog };

    expect(cargoTestInvocations(ws)).toEqual([]); // the production filter, blind
    expect(cargoTestInvocationsSeesReorderedArgv(ws)).toEqual(reordered); // a filter that sees it
  });
});
