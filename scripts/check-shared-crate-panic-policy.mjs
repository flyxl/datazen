#!/usr/bin/env node
/**
 * Shared-crate panic policy gate — no bare `unwrap()` on production paths.
 *
 * ## Why this is a second script and not one more rule in
 * ## `check-platform-crate-boundaries.mjs`
 *
 * That gate carries a recorded R-phase conclusion line (`PASS — 29 workspace
 * member(s) classified, 10 rule(s) evaluated over 31 crate(s), …`). Adding a
 * rule to it would invalidate that record, so the new policy gets a gate whose
 * blast radius is exactly the crates it names. The decision was option (c) 3:
 * constrain **new** shared crates, never retro-fit the existing tree.
 *
 * ## What is enforced, and why `expect(…)` is deliberately left alone
 *
 * AGENTS.md 代码风格 and docs/development/panic-policy.md say 生产路径禁止裸
 * `unwrap()` / `expect()`（`#[cfg(test)]` 除外；确需 panic 须注释说明）. The
 * load-bearing word is 裸 ("bare"): `expect("…")` carries a message and is the
 * documented escape hatch for a path that genuinely cannot fail, so this gate
 * forbids only the message-less `unwrap()`. That is what makes the rule
 * **mechanically decidable** — a syntactic property of the call. A rule that
 * needed the guard to judge whether a comment is a good enough justification
 * would be a rule nobody could satisfy reliably, and the guard would be deleted
 * rather than obeyed.
 *
 * ## Test code is excluded by syntax, never by filename
 *
 * `#[cfg(test)]` items, `#[test]` functions and `#[tokio::test]`-style
 * attribute macros are skipped by locating the annotated item and dropping its
 * span. A test helper parked in production code is therefore still covered:
 * only what the compiler excludes from the shipped build is exempt, which is the
 * boundary panic-policy.md 2 draws (Tests are exempt).
 *
 * Known limits, stated rather than hidden — all of them fail **red**, never
 * green:
 *
 *   * an out-of-line test module (`#[cfg(test)] mod tests;` with the body in
 *     `src/tests.rs`) is a *separate file*; the declaration is skipped here and
 *     the other file is scanned, so its `unwrap()`s are reported;
 *   * `#[cfg(any(test, …))]` is not recognised as a test attribute;
 *   * a qualified path that only *names* an item called `unwrap` reads the same
 *     as one that calls the std helper. Measured instances, in ascending
 *     implausibility: `use foo::unwrap;` (an item), `my::unwrap::<u8>(x)`
 *     (a generic call), `my::unwrap!(x)` (a **macro**, not a function or a
 *     method) and the non-generic `Option::<u8>::unwrap::<u8>(y)`. Every one
 *     turns red, and that is the price of catching `Option::unwrap` in value
 *     position (`.map(Option::unwrap)` panics on the first `None` exactly as
 *     a call does). Separating them would take type information this guard does
 *     not have, and err-red is the direction that gets noticed and fixed.
 *   * a `#[cfg(test)]` item whose *signature* contains a `{` before the body —
 *     `fn helper() -> [u8; { 1 }] { … }` — ends its span at that brace, so the
 *     body is not exempt and its `unwrap()`s are reported. Pre-existing
 *     behaviour of `itemEnd`, left alone on purpose: a brace inside a function
 *     signature is vanishingly rare, and a brace-matching rewrite of `itemEnd`
 *     risks more than this false positive costs.
 *
 * A missed exemption is a false positive, and a false positive gets fixed. A
 * missed violation would be a silent hole, which is the failure this design is
 * arranged to avoid.
 */

import { existsSync, readFileSync, readdirSync, realpathSync } from 'fs';
import { join, relative, resolve, sep } from 'path';
import { pathToFileURL } from 'url';

import { buildWorkspaceIndex, runCargoMetadata } from './lib/cargoWorkspace.mjs';
import { lineAtOffset } from './lib/scanSourceCode.mjs';

export const PREFIX = '[check-shared-crate-panic]';

/** The rule id every violation line carries. */
export const RULE_ID = 'PN-01';

/**
 * Crates this gate covers, as workspace-relative POSIX directories.
 *
 * Deliberately an explicit list, and deliberately **not** derived from
 * `LAYERS`: the policy applies to new shared infrastructure, so adopting a
 * crate must be a deliberate edit here. A target that is absent from the
 * workspace — or present on disk but unclassifiable — is an **error**, so the
 * list can never silently degrade into a green scan over nothing.
 */
export const TARGET_CRATES = Object.freeze(['packages/infra']);

/**
 * Attributes whose annotated item is test-only code.
 *
 * `#[test]` covers the bare form; the `…::test` alternative covers
 * `#[tokio::test]`, which is exactly as test-only as `#[test]` itself.
 */
const TEST_ATTR_RE =
  /#\s*\[\s*(?:cfg\s*\(\s*test\s*\)|test|[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*::test)\s*\]/g;

/**
 * A bare `unwrap()`: a method or qualified-path call.
 *
 * `unwrap_or(…)`, `unwrap_or_else(…)`, `unwrap_err()` and a `fn unwrap()`
 * *definition* are all left alone, and so is every `expect("…")`. The leading
 * `.`/`::` is what keeps the definition out while staying blind to whitespace
 * and line breaks.
 *
 * The two branches carry **different** trailing rules, and the asymmetry is
 * deliberate:
 *
 *   * a method call must have an EMPTY argument list. `value.unwrap()` is the
 *     panic and `value.unwrap_or(0)` is not; the identifier boundary rules out
 *     the typed relatives and the parens rule out the `_or` family for good
 *     measure. A method is only ever *called* through a `.`, so this branch has
 *     no value position to cover.
 *   * a qualified path needs neither, because `Option::unwrap` is the same std
 *     panic helper whether it is called or handed over as a value:
 *     `Option::unwrap(y)` writes its argument in, and `map(Option::unwrap)`
 *     panics on the first `None` exactly as the call does. So this branch stops
 *     at the end of the identifier — `(?![A-Za-z0-9_])` is what keeps
 *     `unwrap_or` / `unwrap_err` / `clippy::unwrap_used` out. Requiring `(`
 *     here was the blind spot; both shapes were compiled with rustc and panic
 *     at runtime.
 *
 * `(?:r#)?` covers the raw-identifier spelling `r#unwrap`, which is the same
 * method.
 */
const BARE_UNWRAP_RE = /(?:\.\s*(?:r#)?unwrap\s*\(\s*\)|::\s*(?:r#)?unwrap(?![A-Za-z0-9_]))/g;

/** Build output is not authored source. */
const RUST_SKIP_DIR_NAMES = new Set(['target', 'node_modules', '.git', 'dist']);

const isIdentChar = (c) => c !== undefined && /[A-Za-z0-9_]/.test(c);

/**
 * Blank comments and literal bodies, preserving every byte offset and newline.
 *
 * Rust needs its own masker. `lib/scanSourceCode.mjs` is a JS/TS tokenizer: it
 * would read `&'static Regex` as a char literal and `r#"(?i)…"#` as a plain
 * string, and either mis-read shifts the scan so that real code gets blanked —
 * for a panic gate that means a silently missed violation, the one direction
 * this gate must never fail in.
 *
 * Handled: line comments, **nested** block comments, `"…"` / `b"…"` strings
 * with escapes, raw strings (`r"…"`, `r#"…"#`, `br#"…"#`) and char literals
 * (`'a'`, `'\\n'`, a unicode escape, `b'\\xFF'`). A lifetime (`&'a str`) and
 * a loop label are left as code, because that is what they are.
 *
 * @param {string} text
 * @returns {string} same length, comments/literals replaced by spaces
 */
export function maskRustSource(text) {
  const out = text.split('');
  const n = text.length;
  const blank = (from, to) => {
    for (let k = from; k < to && k < n; k += 1) if (out[k] !== '\n') out[k] = ' ';
  };
  let i = 0;
  while (i < n) {
    const ch = text[i];
    if (ch === '/' && text[i + 1] === '/') {
      const start = i;
      while (i < n && text[i] !== '\n') i += 1;
      blank(start, i);
      continue;
    }
    if (ch === '/' && text[i + 1] === '*') {
      // Rust block comments nest; a flat scan would end at the inner `*/` and
      // leave the outer half of the comment looking like code.
      const start = i;
      let depth = 0;
      while (i < n) {
        if (text[i] === '/' && text[i + 1] === '*') {
          depth += 1;
          i += 2;
          continue;
        }
        if (text[i] === '*' && text[i + 1] === '/') {
          depth -= 1;
          i += 2;
          if (depth === 0) break;
          continue;
        }
        i += 1;
      }
      blank(start, i);
      continue;
    }
    const end = literalEnd(text, i);
    if (end !== -1) {
      blank(i, end);
      i = end;
      continue;
    }
    i += 1;
  }
  return out.join('');
}

/**
 * End offset (exclusive) of the literal starting at `i`, or -1 when `i` does
 * not start one. A lifetime or loop label returns -1 and stays in the code
 * channel, which is correct: it is code.
 */
function literalEnd(text, i) {
  const ch = text[i];
  const next = text[i + 1];
  if (ch === '"') return quotedStringEnd(text, i);
  if (ch === "'") return charLiteralEnd(text, i);
  // A prefix is a prefix only when it is not the tail of an identifier:
  // `bar"…"` is not a byte string, and the `cr` in `scram` is not a C raw
  // string.
  if (isIdentChar(text[i - 1])) return -1;
  if (ch === 'r') {
    const hashes = hashRun(text, i + 1);
    return text[i + 1 + hashes] === '"' ? rawStringEnd(text, i + 1 + hashes, hashes) : -1;
  }
  if (ch === 'b' || ch === 'c') {
    if (next === '"') return quotedStringEnd(text, i + 1);
    if (next === "'") return charLiteralEnd(text, i + 1);
    if (next === 'r') {
      const hashes = hashRun(text, i + 2);
      return text[i + 2 + hashes] === '"' ? rawStringEnd(text, i + 2 + hashes, hashes) : -1;
    }
  }
  return -1;
}

function hashRun(text, from) {
  let k = from;
  while (text[k] === '#') k += 1;
  return k - from;
}

/** Offset just past the closing quote. Rust strings may span lines. */
function quotedStringEnd(text, quoteIndex) {
  let i = quoteIndex + 1;
  while (i < text.length) {
    const c = text[i];
    if (c === '\\') {
      i += 2;
      continue;
    }
    if (c === '"') return i + 1;
    i += 1;
  }
  return text.length;
}

/** Offset just past the closing hash-quote run of a raw string. */
function rawStringEnd(text, quoteIndex, hashes) {
  const close = '"' + '#'.repeat(hashes);
  const at = text.indexOf(close, quoteIndex + 1);
  return at === -1 ? text.length : at + close.length;
}

/** Offset just past a char literal, or -1 when `'` opens a lifetime instead. */
function charLiteralEnd(text, i) {
  const n = text.length;
  if (i + 1 >= n) return -1;
  if (text[i + 1] === '\\') {
    let k = i + 2;
    if (text[k] === 'u' && text[k + 1] === '{') {
      k += 2;
      while (k < n && text[k] !== '}') k += 1;
      k += 1;
    } else if (text[k] === 'x') {
      k += 3;
    } else {
      k += 1;
    }
    return text[k] === "'" ? k + 1 : -1;
  }
  // `'a'` is a char literal; the `'a` in `&'a str` is a lifetime. The
  // difference is the closing quote two units along, and that is a syntactic
  // fact, not a guess.
  return i + 2 < n && text[i + 2] === "'" ? i + 3 : -1;
}

/**
 * Spans of test-only items: `[start, end)` over the **masked** source.
 *
 * Locating the annotated *item* rather than the module keyword is what makes
 * `#[cfg(test)] fn helper() { … }` and `#[cfg(test)] use super::*;` work as
 * well as the usual `#[cfg(test)] mod tests { … }`.
 *
 * @param {string} masked comment/literal-blanked source
 * @returns {Array<{start: number, end: number, label: string}>}
 */
export function findTestRegions(masked) {
  const regions = [];
  TEST_ATTR_RE.lastIndex = 0;
  for (let m = TEST_ATTR_RE.exec(masked); m !== null; m = TEST_ATTR_RE.exec(masked)) {
    const itemStart = skipAttributes(masked, m.index + m[0].length);
    regions.push({
      start: m.index,
      end: itemEnd(masked, itemStart),
      label: m[0].replace(/\s+/g, ''),
    });
  }
  return regions;
}

/** Skip whitespace and any further `#[…]` attributes before the item itself. */
function skipAttributes(text, from) {
  let i = from;
  for (;;) {
    while (i < text.length && /\s/.test(text[i])) i += 1;
    if (text[i] !== '#' || text[i + 1] !== '[') return i;
    let depth = 0;
    while (i < text.length) {
      const c = text[i];
      if (c === '[') depth += 1;
      else if (c === ']') depth -= 1;
      i += 1;
      if (depth === 0) break;
    }
  }
}

/**
 * End of the item that follows a test attribute.
 *
 * First `;` or first `{` wins, whichever comes first: that is the whole
 * difference between `mod tests;` / `use super::*;` and
 * `mod tests { … }` / `fn helper() { … }`.
 */
function itemEnd(text, from) {
  for (let i = from; i < text.length; i += 1) {
    if (text[i] === ';') return i + 1;
    if (text[i] === '{') return blockEnd(text, i);
  }
  return text.length;
}

function blockEnd(text, open) {
  let depth = 0;
  for (let i = open; i < text.length; i += 1) {
    if (text[i] === '{') depth += 1;
    else if (text[i] === '}') {
      depth -= 1;
      if (depth === 0) return i + 1;
    }
  }
  return text.length;
}

/**
 * Every bare `unwrap()` in masked source, with its 1-based line.
 *
 * @param {string} masked
 * @returns {Array<{offset: number, line: number}>}
 */
export function findBareUnwraps(masked) {
  const hits = [];
  BARE_UNWRAP_RE.lastIndex = 0;
  for (let m = BARE_UNWRAP_RE.exec(masked); m !== null; m = BARE_UNWRAP_RE.exec(masked)) {
    hits.push({ offset: m.index, line: lineAtOffset(masked, m.index) });
  }
  return hits;
}

/**
 * Audit one Rust source file.
 *
 * @param {string} text
 * @param {string} file repo-relative POSIX path, used verbatim in findings
 * @returns {{violations: Array<{rule: string, file: string, line: number}>, exemptCalls: number, testItems: number}}
 */
export function analyzeRustSource(text, file) {
  const masked = maskRustSource(text);
  const regions = findTestRegions(masked);
  const violations = [];
  let exemptCalls = 0;
  for (const hit of findBareUnwraps(masked)) {
    if (regions.some((r) => hit.offset >= r.start && hit.offset < r.end)) {
      exemptCalls += 1;
      continue;
    }
    violations.push({ rule: RULE_ID, file, line: hit.line });
  }
  return { violations, exemptCalls, testItems: regions.length };
}

/**
 * Walk one crate's `src` tree and audit every Rust file in it.
 *
 * A crate whose `src/` directory does not exist is an **error**, not a clean
 * scan: "nothing was read" and "nothing was wrong" must never print the same
 * line.
 *
 * @param {{root: string, crateDir: string}} options
 */
export function scanCrateSources({ root, crateDir }) {
  const srcDir = resolve(root, crateDir, 'src');
  const violations = [];
  const errors = [];
  let files = 0;
  let exemptCalls = 0;
  let testItems = 0;
  if (!existsSync(srcDir)) {
    errors.push(crateDir + '/src not found — nothing to scan, refusing to report a clean crate');
    return { violations, errors, files, exemptCalls, testItems };
  }
  const walk = (dir) => {
    const entries = readdirSync(dir, { withFileTypes: true }).sort((a, b) =>
      a.name < b.name ? -1 : a.name > b.name ? 1 : 0,
    );
    for (const entry of entries) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) {
        if (RUST_SKIP_DIR_NAMES.has(entry.name)) continue;
        walk(full);
        continue;
      }
      if (!entry.name.endsWith('.rs')) continue;
      files += 1;
      const rel = relative(root, full).split(sep).join('/');
      const result = analyzeRustSource(readFileSync(full, 'utf8'), rel);
      violations.push(...result.violations);
      exemptCalls += result.exemptCalls;
      testItems += result.testItems;
    }
  };
  walk(srcDir);
  return { violations, errors, files, exemptCalls, testItems };
}

/**
 * Run the gate.
 *
 * @param {{
 *   root: string,
 *   metadata?: object,
 *   env?: NodeJS.ProcessEnv,
 *   crateDirs?: readonly string[],
 *   log?: (s: string) => void,
 *   error?: (s: string) => void,
 * }} options
 */
export function checkSharedCratePanicPolicy(options) {
  const { root, log = () => {}, error = console.error, crateDirs = TARGET_CRATES } = options;
  const violations = [];
  const errors = [];
  const scanned = [];

  const metadata = options.metadata ?? runCargoMetadata({ cwd: root, env: options.env, log, error });
  const index = buildWorkspaceIndex(metadata);
  const byDir = new Map();
  for (const member of index.members.values()) byDir.set(member.relDir, member);

  let files = 0;
  let exemptCalls = 0;
  let testItems = 0;

  for (const crateDir of crateDirs) {
    const member = byDir.get(crateDir);
    if (!member) {
      const why = index.unclassified.find((u) => u.relDir === crateDir);
      errors.push(
        'target crate ' + crateDir + ' is not a classified workspace member' +
          (why ? ' — ' + why.reason : '') +
          '. TARGET_CRATES and the workspace have diverged; fix one before trusting this gate.',
      );
      continue;
    }
    const result = scanCrateSources({ root, crateDir: member.relDir });
    violations.push(...result.violations);
    for (const line of result.errors) errors.push(member.name + ': ' + line);
    files += result.files;
    exemptCalls += result.exemptCalls;
    testItems += result.testItems;
    scanned.push({ name: member.name, dir: member.relDir, files: result.files });
  }

  // The non-empty precondition. Any branch above can decline to read a file
  // without that being a defect in itself, so every one of them leaves `files`
  // at 0 — and a gate that prints PASS next to "0 file(s)" is the single failure
  // mode this script exists to avoid. Resolving a member and reading its sources
  // are different events; only the second is evidence.
  if (crateDirs.length === 0) {
    errors.push(
      'TARGET_CRATES is empty, so this gate has no subject at all — a PASS here would ' +
        'describe no code. List the crate(s) this policy applies to.',
    );
  } else if (files === 0) {
    errors.push(
      '0 Rust file(s) read across ' + scanned.length + ' of ' + crateDirs.length +
        ' target crate(s) — refusing to report a clean scan. "Nothing was read" and ' +
        '"nothing was wrong" must never print the same line.',
    );
  }

  const summary =
    scanned.length + ' crate(s) scanned, ' + files + ' file(s), ' + testItems +
    ' test item(s) excluded, ' + exemptCalls + ' test-exempt unwrap() call(s), ' +
    violations.length + ' violation(s), ' + errors.length + ' error(s)';
  return { violations, errors, scanned, files, exemptCalls, testItems, summary };
}

function parseArgs(argv) {
  const args = { root: process.cwd() };
  for (const arg of argv) {
    if (arg.startsWith('--root=')) args.root = resolve(arg.slice('--root='.length));
    else if (arg === '--help') args.help = true;
    else throw new Error('unknown argument: ' + arg);
  }
  return args;
}

/**
 * `crateDirs` is not a CLI flag: it is the same seam `checkSharedCratePanicPolicy`
 * already exposes, forwarded so the suite can drive the *empty subject list*
 * through the real exit code instead of asserting on an internal object.
 */
export function runCli({ argv = process.argv.slice(2), env = process.env, crateDirs = TARGET_CRATES } = {}) {
  const out = (s) => process.stdout.write(s + '\n');
  const err = (s) => process.stderr.write(s + '\n');

  let args;
  try {
    args = parseArgs(argv);
  } catch (cause) {
    err(PREFIX + ' ' + cause.message);
    return 2;
  }
  if (args.help) {
    out(PREFIX + ' usage: node scripts/check-shared-crate-panic-policy.mjs [--root=<dir>]');
    out(PREFIX + ' crates: ' + TARGET_CRATES.map((c) => c + ' (src/**/*.rs)').join(', '));
    return 0;
  }

  let result;
  try {
    result = checkSharedCratePanicPolicy({ ...args, env, crateDirs, log: out, error: err });
  } catch (cause) {
    err(PREFIX + ' ' + cause.message);
    return 2;
  }

  for (const v of result.violations) {
    err(
      PREFIX + ' VIOLATION  ' + v.rule + '  ' + v.file + ':' + v.line +
        '  bare `unwrap()` on a production path — return the error with `?` / `map_err`, or use' +
        ' `expect("…")` with a comment saying why panicking is acceptable (docs/development/panic-policy.md).',
    );
  }
  for (const line of result.errors) err(PREFIX + ' ERROR    ' + line);
  for (const crate of result.scanned) {
    out(PREFIX + ' ok ' + crate.name + ': ' + crate.dir + ' (' + crate.files + ' file(s))');
  }

  const failed = result.violations.length + result.errors.length;
  out(PREFIX + ' ' + (failed === 0 ? 'PASS' : 'FAIL') + ' — ' + result.summary);
  return failed === 0 ? 0 : 1;
}

/**
 * Is this module the process entry point?
 *
 * `import.meta.url === 'file://' + process.argv[1]` is the repo's usual spelling
 * and it is wrong in two ways, each of which makes the CLI body silently not
 * run — zero output, exit 0, indistinguishable from a clean pass:
 *
 *   * `import.meta.url` holds the **realpath** (Node resolves symlinks for the
 *     main module) while `process.argv[1]` is whatever was typed, so invoking
 *     the script through a symlink never matches;
 *   * a path with a space or a non-ASCII byte is percent-encoded in a file URL,
 *     and string concatenation does not encode it.
 */
function isDirectInvocation() {
  const entry = process.argv[1];
  if (entry === undefined) return false;
  let resolved = entry;
  try {
    resolved = realpathSync(entry);
  } catch {
    // Keep the literal path: an unreadable entry still gets its one comparison.
  }
  return (
    import.meta.url === pathToFileURL(resolved).href ||
    import.meta.url === pathToFileURL(entry).href
  );
}

if (isDirectInvocation()) {
  process.exitCode = runCli();
}
