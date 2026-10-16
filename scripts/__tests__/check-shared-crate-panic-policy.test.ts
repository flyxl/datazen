/**
 * Tests for `scripts/check-shared-crate-panic-policy.mjs`.
 *
 * ## What the three required cases are, and what each one can actually fail on
 *
 *   1. a clean sample passes;
 *   2. a bare `unwrap()` in production code turns the gate red;
 *   3. a `unwrap()` inside a test module is **not** reported.
 *
 * Case 3 is the one that is easy to fake. A gate that scanned nothing would also
 * report "no violations" for the test-module sample, so the assertion there is
 * not "exit 0" — it is "exit 0 **and** the run says it located the test items and
 * skipped the calls inside them". Both numbers are printed by the gate itself,
 * and both are asserted.
 *
 * ## Why the CLI cases stub cargo
 *
 * `scripts/__tests__` belongs with the `frontend:` CI job, which has no Rust
 * toolchain. A suite that shelled out to a real `cargo metadata` would either
 * skip there or fail for a reason unrelated to the gate. The stub is a real
 * executable on `PATH`, so `runCargoMetadata`'s parsing, the shared
 * `classifyMemberDir` mapping and `runCli`'s return code all really run.
 *
 * The pure-function cases below need no cargo at all: they call the exported
 * analysis functions directly.
 */

import { spawnSync } from 'child_process';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { delimiter, dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import {
  RULE_ID,
  TARGET_CRATES,
  analyzeRustSource,
  findBareUnwraps,
  findTestRegions,
  maskRustSource,
  runCli,
  scanCrateSources,
} from '../check-shared-crate-panic-policy.mjs';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** The crate every fixture workspace declares, so it is the gate's subject. */
const CRATE_DIR = TARGET_CRATES[0];

describe('maskRustSource', () => {
  it('blanks comments so a documented unwrap() is not a call', () => {
    const src = [
      '// value.unwrap() is banned here',
      '/// doc: never call .unwrap()',
      'let x = 1;',
    ].join('\n');
    const masked = maskRustSource(src);

    expect(masked).not.toContain('unwrap');
    expect(masked).toContain('let x = 1;');
    expect(masked.split('\n')).toHaveLength(3); // line structure preserved
  });

  it('blanks nested block comments, which a flat scan would cut short', () => {
    const src = '/* outer /* inner .unwrap() */ still comment .unwrap() */ let y = 2;';
    const masked = maskRustSource(src);

    expect(masked).not.toContain('unwrap');
    expect(masked).toContain('let y = 2;');
  });

  it('blanks string literal bodies, including raw strings that contain quotes', () => {
    const src = [
      'let a = "call .unwrap() in a message";',
      'let re = r#"(?i)(\"password\"):[^\"]*"#;',
      'let real = value.unwrap();',
    ].join('\n');
    const masked = maskRustSource(src);
    const hits = findBareUnwraps(masked);

    expect(hits).toHaveLength(1);
    expect(hits[0].line).toBe(3);
  });

  it('treats a lifetime as code and a char literal as a literal', () => {
    // The trap: reading `&'static` as an opening quote would blank code until
    // the next apostrophe, and a missed violation is the one direction this gate
    // must never fail in.
    const src = [
      "fn f(x: &'static str) -> char { let q = '\"'; value.unwrap() }",
    ].join('\n');
    const masked = maskRustSource(src);

    expect(findBareUnwraps(masked).map((h) => h.line)).toEqual([1]);
  });
});

describe('findBareUnwraps', () => {
  it('flags only the argument-less call, never its typed relatives', () => {
    const src = [
      'let a = value.unwrap();',
      'let b = value.unwrap_or(0);',
      'let c = value.unwrap_or_else(|| 0);',
      'let d = result.unwrap_err();',
      'let e = value.expect("documented");',
    ].join('\n');
    const lines = findBareUnwraps(maskRustSource(src)).map((h) => h.line);

    expect(lines).toEqual([1]);
  });

  it('sees a call broken across lines and reports the line the call is on', () => {
    // The dot is on line 2, so line 2 is the line a reader opens. Asserted
    // rather than left implicit because the alternative (reporting the
    // receiver's line) is a plausible implementation nobody would notice.
    const src = ['let a = value', '    .unwrap();'].join('\n');

    expect(findBareUnwraps(maskRustSource(src)).map((h) => h.line)).toEqual([2]);
  });

  it('flags the qualified-path form and the raw-identifier spelling', () => {
    // Both were measured as real std panics by compiling them with rustc 1.90.0:
    // a qualified-path call *writes its argument in*, and `r#unwrap` is the same
    // method spelled as a raw identifier.
    //
    // The empty-argument rule belongs to the method-call branch only. It exists
    // to separate `.unwrap()` from `.unwrap_or(…)`, and on the `::` branch the
    // identifier text already does that — `unwrap` followed by `_` can never
    // reach the `(`. Extending the rule to `::` was the blind spot; these two
    // lines fail if it comes back.
    const src = [
      'let a = Option::unwrap(y);',
      'let b = y.r#unwrap();',
      'let c = Option::unwrap_or(y, 0);',
      'let d = y.unwrap_or(0);',
      'let e = y.unwrap_err();',
    ].join('\n');

    expect(findBareUnwraps(maskRustSource(src)).map((h) => h.line)).toEqual([1, 2]);
  });
});

describe('findTestRegions', () => {
  it('spans inline cfg(test) modules, #[test] fns and #[tokio::test] fns', () => {
    const src = [
      '#[cfg(test)]',
      'mod tests {',
      '    fn a() { x.unwrap(); }',
      '}',
      'fn production() -> u8 { 0 }',
      '#[test]',
      'fn b() { x.unwrap(); }',
      '#[tokio::test]',
      'async fn c() { x.unwrap(); }',
    ].join('\n');
    const regions = findTestRegions(maskRustSource(src));

    expect(regions).toHaveLength(3);
    expect(regions.map((r) => r.label)).toEqual(['#[cfg(test)]', '#[test]', '#[tokio::test]']);
    expect(regions[0].end).toBeLessThan(regions[1].start);
  });
});

describe('analyzeRustSource', () => {
  it('reports nothing for production code that propagates or documents', () => {
    const src = [
      'pub fn probe(value: Option<u32>) -> Result<u32, String> {',
      '    let v = value.ok_or_else(|| "missing".to_string())?;',
      '    let n: u32 = "42".parse().map_err(|e: std::num::ParseIntError| e.to_string())?;',
      '    let re = Regex::new(r"(?i)bearer\\s+\\S+").expect("compile-time literal");',
      '    drop(re);',
      '    Ok(v + n)',
      '}',
    ].join('\n');
    const result = analyzeRustSource(src, 'packages/platform-infra/src/clean.rs');

    expect(result.violations).toEqual([]);
    expect(result.exemptCalls).toBe(0);
  });

  it('reports a bare unwrap() on a production path with rule id and line', () => {
    const src = ['pub fn probe(v: Option<u32>) -> u32 {', '    v.unwrap()', '}'].join('\n');
    const result = analyzeRustSource(src, 'packages/platform-infra/src/dirty.rs');

    expect(result.violations).toEqual([
      { rule: RULE_ID, file: 'packages/platform-infra/src/dirty.rs', line: 2 },
    ]);
  });

  it('exempts the calls inside a test module but still counts them', () => {
    const src = [
      'pub fn probe(v: Option<u32>) -> Option<u32> {',
      '    v',
      '}',
      '',
      '#[cfg(test)]',
      'mod tests {',
      '    #[test]',
      '    fn works() {',
      '        assert_eq!(super::probe(Some(1)).unwrap(), 1);',
      '    }',
      '}',
    ].join('\n');
    const result = analyzeRustSource(src, 'packages/platform-infra/src/tested.rs');

    expect(result.violations).toEqual([]);
    // Not vacuous: the file really does contain a bare unwrap(), and the gate
    // really did decide to exempt it.
    expect(result.exemptCalls).toBe(1);
    expect(result.testItems).toBe(2);
  });
});

/**
 * A `cargo` stand-in that reports a workspace which really is on disk.
 *
 * CommonJS on purpose: it is executed through its own shebang by
 * `spawnSync('cargo', …)`, where Node parses an extensionless entry file as CJS.
 */
const CARGO_STUB_SOURCE = `#!/usr/bin/env node
'use strict';
const fs = require('fs');
const path = require('path');

// Every invocation is recorded, so the suite can show which cargo commands the
// gate reached rather than only what it returned.
if (process.env.STUB_CARGO_LOG) {
  fs.appendFileSync(process.env.STUB_CARGO_LOG, process.argv.slice(2).join(' ') + '\\n');
}

if (process.argv[2] !== 'metadata') process.exit(9);

const root = process.cwd();
const manifest = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8');
const block = /members\\s*=\\s*\\[([^\\]]*)\\]/.exec(manifest);
const dirs = (block ? block[1] : '')
  .split(',')
  .map((s) => s.trim().replace(/^["']|["']$/g, ''))
  .filter(Boolean);
const packages = dirs.map((dir) => {
  const abs = path.resolve(root, dir);
  const text = fs.readFileSync(path.join(abs, 'Cargo.toml'), 'utf8');
  const name = /name\\s*=\\s*"([^"]+)"/.exec(text)[1];
  return {
    id: 'path+file://' + abs + '#' + name,
    name: name,
    manifest_path: path.join(abs, 'Cargo.toml'),
  };
});
process.stdout.write(
  JSON.stringify({
    workspace_root: root,
    packages: packages,
    workspace_members: packages.map((p) => p.id),
    resolve: { root: null, nodes: packages.map((p) => ({ id: p.id, deps: [] })) },
  }),
);
process.exit(0);
`;

interface StubWorkspace {
  root: string;
  /** One line per cargo invocation the gate made against this workspace. */
  cargoLog: string;
}

interface CliRun {
  code: number;
  stdout: string;
  stderr: string;
}

const CLEAN_SOURCE = [
  '//! Production code that propagates or documents every fallible step.',
  'pub fn probe(value: Option<u32>) -> Result<u32, String> {',
  '    let v = value.ok_or_else(|| "missing".to_string())?;',
  '    let n: u32 = "42".parse().map_err(|e: std::num::ParseIntError| e.to_string())?;',
  '    let re = Regex::new(r"(?i)bearer\\s+\\S+").expect("compile-time literal");',
  '    drop(re);',
  '    Ok(v + n)',
  '}',
].join('\n');

const DIRTY_SOURCE = [
  '//! Production code that panics on a recoverable input.',
  'pub fn probe(value: Option<u32>) -> u32 {',
  '    value.unwrap()',
  '}',
].join('\n');

const TEST_ONLY_SOURCE = [
  'pub fn probe(value: Option<u32>) -> Option<u32> {',
  '    value',
  '}',
  '',
  '#[cfg(test)]',
  'mod tests {',
  '    #[test]',
  '    fn works() {',
  '        assert_eq!(super::probe(Some(1)).unwrap(), 1);',
  '    }',
  '}',
].join('\n');

let sandbox: string;
let cargoBin: string;
let clean: StubWorkspace;
let dirty: StubWorkspace;
let testOnly: StubWorkspace;

beforeAll(() => {
  sandbox = mkdtempSync(join(tmpdir(), 'datazen-shared-crate-panic-'));
  cargoBin = join(sandbox, 'bin');
  mkdirSync(cargoBin, { recursive: true });
  const stub = join(cargoBin, 'cargo');
  writeFileSync(stub, CARGO_STUB_SOURCE);
  chmodSync(stub, 0o755);

  clean = makeWorkspace('clean', { 'lib.rs': CLEAN_SOURCE });
  dirty = makeWorkspace('dirty', { 'lib.rs': DIRTY_SOURCE });
  testOnly = makeWorkspace('test-only', { 'lib.rs': TEST_ONLY_SOURCE });
});

afterAll(() => {
  if (sandbox !== undefined) rmSync(sandbox, { recursive: true, force: true });
});

/** Write one real crate: directory, `Cargo.toml`, and every source file given. */
function writeCrate(root: string, relDir: string, name: string, sources: Record<string, string>): void {
  const dir = join(root, relDir);
  mkdirSync(join(dir, 'src'), { recursive: true });
  writeFileSync(
    join(dir, 'Cargo.toml'),
    ['[package]', `name = "${name}"`, 'version = "0.0.1"', 'edition = "2021"', ''].join('\n'),
  );
  for (const [file, text] of Object.entries(sources)) {
    const full = join(dir, 'src', file);
    mkdirSync(dirname(full), { recursive: true });
    writeFileSync(full, text);
  }
}

function makeWorkspace(id: string, sources: Record<string, string>): StubWorkspace {
  const root = join(sandbox, id);
  mkdirSync(root, { recursive: true });
  writeFileSync(
    join(root, 'Cargo.toml'),
    ['[workspace]', 'resolver = "2"', `members = ["${CRATE_DIR}"]`, ''].join('\n'),
  );
  writeCrate(root, CRATE_DIR, 'datazen-platform-infra', sources);
  return { root, cargoLog: join(sandbox, `${id}.cargo-invocations`) };
}

function envFor(ws: StubWorkspace): NodeJS.ProcessEnv {
  return {
    ...process.env,
    PATH: `${cargoBin}${delimiter}${process.env.PATH ?? ''}`,
    STUB_CARGO_LOG: ws.cargoLog,
  };
}

/**
 * Run `runCli` with stdout/stderr captured instead of inherited.
 *
 * The cast is required, not a shortcut: `process.stdout.write` is an overload
 * set and a single arrow function cannot satisfy all of it.
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
    return { code: run(), stdout: outChunks.join(''), stderr: errChunks.join('') };
  } finally {
    outSpy.mockRestore();
    errSpy.mockRestore();
  }
}

function runIn(ws: StubWorkspace, args: readonly string[] = []): CliRun {
  rmSync(ws.cargoLog, { force: true });
  return captureCli(() => runCli({ argv: [`--root=${ws.root}`, ...args], env: envFor(ws) }));
}

/**
 * The same run, with the subject list overridden.
 *
 * `crateDirs` is the seam `runCli` forwards to `checkSharedCratePanicPolicy`;
 * overriding it is how the empty-subject-list case reaches a real exit code
 * instead of an assertion on an internal object.
 */
function runWithCrateDirs(ws: StubWorkspace, crateDirs: readonly string[]): CliRun {
  rmSync(ws.cargoLog, { force: true });
  return captureCli(() => runCli({ argv: [`--root=${ws.root}`], env: envFor(ws), crateDirs }));
}

describe('scanCrateSources', () => {
  it('walks the whole src tree and reports file count and path of each finding', () => {
    const root = join(sandbox, 'scan-tree');
    writeCrate(root, CRATE_DIR, 'datazen-platform-infra', {
      'lib.rs': CLEAN_SOURCE,
      'nested/deep.rs': DIRTY_SOURCE,
    });
    const result = scanCrateSources({ root, crateDir: CRATE_DIR });

    expect(result.files).toBe(2);
    expect(result.violations.map((v) => v.file)).toEqual([`${CRATE_DIR}/src/nested/deep.rs`]);
  });

  it('refuses to call a crate with no src tree clean', () => {
    const result = scanCrateSources({ root: join(sandbox, 'absent'), crateDir: CRATE_DIR });

    expect(result.files).toBe(0);
    expect(result.violations).toEqual([]);
    expect(result.errors).toHaveLength(1);
    expect(result.errors[0]).toContain('not found');
  });
});

describe('runCli exit codes', () => {
  it('exits 0 on a clean crate and prints the crate and file counts', () => {
    const run = runIn(clean);

    expect(run.code).toBe(0);
    expect(run.stdout).toContain('PASS');
    expect(run.stdout).toContain('ok datazen-platform-infra: packages/platform-infra (1 file(s))');
    expect(run.stdout).toContain('1 crate(s) scanned, 1 file(s)');
    expect(run.stderr).toBe('');
  });

  it('goes red on a bare unwrap() in production code and names file:line plus rule id', () => {
    const run = runIn(dirty);

    expect(run.code).toBe(1);
    expect(run.stdout).toContain('FAIL');
    expect(run.stdout).not.toContain('PASS');
    expect(run.stderr).toContain(`${CRATE_DIR}/src/lib.rs:3`);
    expect(run.stderr).toContain(RULE_ID);
    // The tally lives on the summary line (stdout); the finding lives on
    // stderr. Both are asserted, so a change that moved either would show.
    expect(run.stdout).toContain('1 violation(s)');
  });

  it('exempts a test module and proves the exemption was not a vacuous scan', () => {
    const run = runIn(testOnly);

    expect(run.code).toBe(0);
    expect(run.stdout).toContain('PASS');
    // The load-bearing assertions: the file really holds a bare unwrap(), the
    // gate located the test item, and it exempted exactly that call. "exit 0"
    // alone would also be produced by a gate that read nothing at all.
    expect(run.stdout).toContain('1 test-exempt unwrap() call(s)');
    const located = /(\d+) test item\(s\) excluded/.exec(run.stdout);
    expect(Number(located?.[1] ?? '0')).toBeGreaterThan(0);
  });

  it('rejects an unknown argument with 2, distinct from a gate failure', () => {
    expect(runIn(clean, ['--not-a-flag']).code).toBe(2);
  });

  it('errors, rather than passing, when the target crate is not a workspace member', () => {
    const root = join(sandbox, 'no-member');
    mkdirSync(root, { recursive: true });
    writeFileSync(
      join(root, 'Cargo.toml'),
      ['[workspace]', 'resolver = "2"', 'members = []', ''].join('\n'),
    );
    const ws: StubWorkspace = { root, cargoLog: join(sandbox, 'no-member.cargo-invocations') };
    const run = runIn(ws);

    expect(run.code).not.toBe(0);
    expect(run.stderr).toContain('not a classified workspace member');
    expect(run.stdout).not.toContain('PASS');
  });
});

describe('the gate refuses to pass on an empty scan', () => {
  it('errors when a resolved crate has a src directory with no Rust file in it', () => {
    // The case that used to print `ok … (0 file(s))` next to PASS. The crate
    // resolves, the directory exists, and the gate read nothing — which must not
    // be reportable as clean.
    const run = runIn(makeWorkspace('empty-src', {}));

    expect(run.code).not.toBe(0);
    expect(run.stdout).toContain('FAIL');
    expect(run.stdout).not.toContain('PASS');
    expect(run.stderr).toContain('0 Rust file(s) read');
    expect(run.stderr).toContain('refusing to report a clean scan');
  });

  it('errors when the subject list is empty, so a PASS would describe no code', () => {
    const run = runWithCrateDirs(clean, []);

    expect(run.code).not.toBe(0);
    expect(run.stdout).toContain('FAIL');
    expect(run.stdout).not.toContain('PASS');
    expect(run.stderr).toContain('TARGET_CRATES is empty');
  });
});

describe('the gate stays wired to reality', () => {
  it('actually reads Rust files for every crate the policy names', () => {
    // Two premises at once, and the second is the one F1 was about: every
    // fixture above is synthetic, so (a) a typo in TARGET_CRATES would leave the
    // suite green while the real gate scanned nothing, and (b) asserting that a
    // `src/` directory *exists* does not assert that anything is in it — a
    // directory holding no `.rs` is exactly the empty scan this suite now
    // refuses. So the assertion is on files actually read out of the real tree.
    expect(TARGET_CRATES.length).toBeGreaterThan(0);
    let total = 0;
    for (const dir of TARGET_CRATES) {
      expect(existsSync(join(REPO_ROOT, dir, 'Cargo.toml'))).toBe(true);
      const result = scanCrateSources({ root: REPO_ROOT, crateDir: dir });
      expect(result.errors).toEqual([]);
      total += result.files;
    }
    expect(total).toBeGreaterThan(0);
  });

  it('resolves the repo root this file claims to live in', () => {
    expect(existsSync(join(REPO_ROOT, 'scripts/check-shared-crate-panic-policy.mjs'))).toBe(true);
  });

  it('runs the CLI when the entry path is a symlink instead of silently exiting 0', () => {
    // The regression this pins: the usual `import.meta.url === 'file://' +
    // process.argv[1]` guard holds the **realpath** on the left and whatever was
    // typed on the right, so a symlinked invocation runs nothing, prints
    // nothing, and exits 0 — the same observable result as a clean pass. Only a
    // real subprocess can catch that, because in-process `runCli` calls never
    // consult the guard at all.
    const link = join(sandbox, 'linked-gate.mjs');
    symlinkSync(join(REPO_ROOT, 'scripts/check-shared-crate-panic-policy.mjs'), link);
    const proc = spawnSync(process.execPath, [link, `--root=${clean.root}`], {
      env: envFor(clean),
      encoding: 'utf8',
    });

    expect(proc.status).toBe(0);
    expect(proc.stdout).toContain('PASS');
    expect(proc.stdout).toContain('1 crate(s) scanned, 1 file(s)');
  });
});
