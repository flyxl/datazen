#!/usr/bin/env node
/**
 * Static guard for the invariant asserted in mutation-check-pane-focus.mjs:
 * no statement in its `finally` block may throw.
 *
 * Why the invariant needs a machine check. The block's promise is that a failure
 * there degrades the report instead of escaping the module — an escaping throw
 * prints a bare stack and makes node force exit 1 regardless of process.exitCode,
 * which is EXIT.UNCOVERED, and it does so *before* the exit-code fold at the
 * bottom, discarding every exitReason pushed so far. That promise is invisible to
 * review: a bare `unlinkSync(backup);` looks exactly as innocent as a guarded one.
 * It was in fact bare, unchanged since before that harness was extended, and the
 * first proof of the restore path found a *different* bare call in the same block.
 *
 * What it checks, and why each part is there:
 *   1. every `finally` block in the file, and there is at least one. Taking only
 *      the first one in the file was a bug in an earlier version of this checker:
 *      a longer, compliant `try/finally` in a helper above the real block
 *      satisfied the check on its behalf, and a bare `unlinkSync` in the actual
 *      restore block passed with "ok" and exit 0. All of them are checked now,
 *      and each finding names the block it came from so a decoy cannot mask the
 *      real one;
 *   2. every node:fs call in them sits inside a try/catch;
 *   3. none contains a `throw` statement. This is what makes (2) sufficient
 *      rather than merely necessary: a guarded call inside a try whose catch
 *      re-throws is protected by nothing, and depth counting alone would call
 *      that safe.
 *
 * Usage:
 *   node scripts/check-restore-block-guards.mjs           check the harness
 *   node scripts/check-restore-block-guards.mjs <file>    check some other file
 *   node scripts/check-restore-block-guards.mjs --selftest prove the check itself
 *   pnpm check:restore-guards                             the same, by name
 *
 * Exit 0 = invariant holds. Exit 1 = it does not, and every offending line is
 * printed. Exit 2 = the file could not be read or does not have the expected
 * shape. The selftest exits 0 only if the check FAILS on the fixtures it is
 * supposed to fail on AND passes on the real file; a check that cannot report a
 * failure is worse than no check, so that is the point of `--selftest`. It also
 * asserts the fixture inputs are pairwise distinct, since two identical inputs
 * test one path and inflate the count — the "depth bug" fixture used to be a
 * byte-for-byte copy of another one and was counted twice.
 *
 * It is a gate, not a courtesy. `pnpm typecheck` runs it first, ahead of the tsc
 * programs, and CI runs `pnpm typecheck` (.github/workflows/ci.yml), so a bare
 * call in the restore block fails the build. It was previously wired to nothing
 * at all, which made it worth exactly as much as remembering to run it; keeping
 * it in the typecheck chain costs about 35 ms per run, of which roughly 23 ms is
 * node startup, against tsc passes that take seconds. Putting it in
 * tsconfig.pack-ep.json's `files` list remains about its *types* only, and is not
 * what makes it a gate.
 *
 * Known limits, named because a tool that cannot see them is worse than one that
 * does not check:
 *   - `strip()` blanks comments and quoted strings but not regular-expression
 *     literals, so a regex containing a quote blanks the rest of the file and the
 *     run ends in "no `finally` block found" (exit 2). It refuses loudly rather
 *     than passing wrongly; tell/slash is deliberately not special-cased.
 *   - a call is identified by name, not by what it resolves to: a helper named
 *     `rethrow(` is correctly not read as a throw statement, but equally a helper
 *     that *does* re-throw is invisible. It reads the file, it does not run it.
 *   - a `try { … } catch { … }` written on one line is handled, because the brace
 *     stack is a separate pass; but the pairing pass looks ahead for a `catch`,
 *     so an unterminated `try` at end of file is read as having none.
 *
 * The `files` list of tsconfig.pack-ep.json still cannot force a *future* new .mjs
 * under scripts/ to be registered there: `files` plus `include: []` is a
 * whitelist, not a discovery set. That is a property of that program, not
 * something this file can fix from the inside.
 */

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const MIN_BLOCK_LINES = 5;
const HARNESS = 'scripts/mutation-check-pane-focus.mjs';
/** @type {readonly string[]} */
const FS_CALLS = [
  'unlinkSync',
  'writeFileSync',
  'readFileSync',
  'chmodSync',
  'renameSync',
  'rmSync',
];

/**
 * Blank out comments and string literals, so a call named inside prose or a
 * message is not mistaken for code. Returns the source padded with spaces and
 * newlines intact, so line numbers survive.
 * @param {string} src
 * @returns {string}
 */
function strip(src) {
  const out = src.split('');
  let i = 0;
  const n = src.length;
  while (i < n) {
    const two = src.slice(i, i + 2);
    if (two === '//') {
      while (i < n && src[i] !== '\n') out[i++] = ' ';
    } else if (two === '/*') {
      while (i < n && src.slice(i, i + 2) !== '*/') out[i++] = src[i] === '\n' ? '\n' : ' ';
      out[i] = ' ';
      i++;
      out[i] = ' ';
      i++;
    } else if (src[i] === "'" || src[i] === '"' || src[i] === '`') {
      const quote = src[i];
      out[i++] = ' ';
      while (i < n && src[i] !== quote) {
        // A backtick template may span lines; keep the newlines so numbering holds.
        if (src[i] === '\\') {
          out[i++] = ' ';
          if (i < n) out[i++] = ' ';
          continue;
        }
        out[i++] = src[i] === '\n' ? '\n' : ' ';
      }
      if (i < n) out[i++] = ' ';
    } else {
      i++;
    }
  }
  return out.join('');
}

/**
 * @typedef {{ ok: boolean, blockFound: boolean, blocksChecked: number, linesChecked: number, findings: string[] }} Verdict
 */

/**
 * The keyword that opened the block whose `{` is at index `i`, or '' if none.
 * Strings are already blank by the time this runs, so parens found here are real.
 * The paren-skip is load-bearing: in `catch (err) {` the keyword is not adjacent
 * to the brace, and without this the catch is read as an anonymous block, its
 * `hasCatch` never gets set, and every call inside a correctly-guarded try is
 * reported bare.
 * @param {string} text
 * @param {number} i
 * @returns {string}
 */
function keywordBefore(text, i) {
  let j = i - 1;
  while (j >= 0 && /\s/.test(text[j])) j--;
  if (text[j] === ')') {
    let depth = 0;
    while (j >= 0) {
      if (text[j] === ')') depth++;
      else if (text[j] === '(') {
        depth--;
        if (depth === 0) {
          j--;
          break;
        }
      }
      j--;
    }
    while (j >= 0 && /\s/.test(text[j])) j--;
  }
  const k = j;
  while (j >= 0 && /[A-Za-z0-9_$]/.test(text[j])) j--;
  return text.slice(j + 1, k + 1);
}

/**
 * Pass 1: map every `{` in the body to the keyword that opened it, and record
 * whether a `try` block is followed by a `catch`.
 *
 * This is a second pass rather than a running counter on purpose. A counter that
 * watches for `try` at the start of a line is wrong in two ways that both matter
 * here, and the selftest fixtures exist to keep them fixed:
 *   - `try { … } catch (e) { … }` on ONE line increments on `try` and never
 *     decrements, so every later call in the block looks guarded. Measured: that
 *     bug let a bare `unlinkSync` through.
 *   - `try { … } finally { … }` has no `catch` at all, so a throw from inside it
 *     propagates. Depth alone would call that protected.
 * Resolving try→catch needs to look ahead of the try's close brace, hence a pass.
 * @param {string} text
 * @param {number} from
 * @param {number} to
 * @returns {Map<number, { type: string, hasCatch: boolean }>}
 */
function blockTable(text, from, to) {
  /** @type {Map<number, { type: string, hasCatch: boolean }>} */
  const byStart = new Map();
  /** @type {{ type: string, hasCatch: boolean }[]} */
  const stack = [];
  /** @type {{ type: string, hasCatch: boolean } | null} */
  let lastClosedTry = null;
  for (let i = from; i < to; i++) {
    if (text[i] === '{') {
      const rec = { type: keywordBefore(text, i), hasCatch: false };
      byStart.set(i, rec);
      if (rec.type === 'catch' && lastClosedTry) lastClosedTry.hasCatch = true;
      stack.push(rec);
    } else if (text[i] === '}') {
      const done = stack.pop();
      if (done && done.type === 'try') lastClosedTry = done;
    }
  }
  return byStart;
}

/**
 * @param {string} ch
 * @returns {boolean}
 */
function isWordChar(ch) {
  return ch !== undefined && /[A-Za-z0-9_$]/.test(ch);
}

/**
 * The check for ONE `finally` block, delimited by its own open and close braces.
 * @param {string} text already stripped
 * @param {number} open index of the block's `{`
 * @param {number} close index of the block's matching `}`
 * @returns {{ findings: string[], linesChecked: number }}
 */
function scanBlock(text, open, close) {
  const lineOf = (/** @type {number} */ i) => text.slice(0, i).split('\n').length;
  const table = blockTable(text, open, close);
  const bodyStart = open + 1;
  const linesChecked = text.slice(bodyStart, close).split('\n').length;

  /** @type {string[]} */
  const findings = [];
  /** @type {{ type: string, hasCatch: boolean }[]} */
  const stack = [];
  for (let i = bodyStart; i < close; i++) {
    const ch = text[i];
    if (ch === '{') {
      stack.push(table.get(i) ?? { type: '', hasCatch: false });
      continue;
    }
    if (ch === '}') {
      stack.pop();
      continue;
    }
    // `startsWith` plus a lookbehind character test IS the word-boundary check;
    // an extra "am I inside a word" guard here would skip every identifier body.
    for (const fn of FS_CALLS) {
      if (!text.startsWith(fn, i)) continue;
      // Skip when the previous character IS a word char — that means we are inside
      // a longer identifier, e.g. the tail of `myUnlinkSync(`.
      if (isWordChar(text[i - 1])) continue;
      const after = text[i + fn.length];
      if (after !== '(') continue;
      // A declaration does not throw: `function unlinkSync(` is a definition.
      if (/\bfunction\s+$/.test(text.slice(Math.max(0, i - 20), i))) continue;
      if (!stack.some((b) => b.type === 'try' && b.hasCatch)) {
        findings.push(`line ${lineOf(i)}: bare ${fn}(…) — not inside a try/catch`);
      }
      i += fn.length;
    }
    // Both boundaries matter: the right one keeps `throwx` out, the left one keeps
    // `rethrow(` out — that is an identifier, and it is not a throw statement.
    if (text.startsWith('throw', i) && !isWordChar(text[i + 5]) && !isWordChar(text[i - 1])) {
      findings.push(
        `line ${lineOf(i)}: throw in the block — a re-throw escapes the try it sits in`,
      );
    }
  }
  if (linesChecked < MIN_BLOCK_LINES) {
    findings.push(
      `the block resolved to only ${linesChecked} line(s) — a truncated range would pass ` +
        `vacuously, so this is reported rather than trusted`,
    );
  }
  return { findings, linesChecked };
}

/**
 * The check itself, over a source string, so `--selftest` can feed it fixtures.
 *
 * Every `finally` block in the file is checked, and all of them must hold. Taking
 * only the first one is the bug this function used to have: the block it checked
 * was whichever came first in the file, so a longer, compliant `try/finally` in a
 * helper ABOVE the real one satisfied the check on its behalf and a bare
 * `unlinkSync` in the actual restore block passed as "ok". Requiring all of them
 * is stricter than the invariant, which is scoped to the harness's own block; a
 * helper with a deliberately bare fs call in a finally would be reported too. The
 * finding prints the line of the block it came from, so an unrelated helper
 * cannot mask the real one by being the one that trips.
 *
 * @param {string} src
 * @returns {Verdict}
 */
function checkSource(src) {
  const text = strip(src);
  const lineOf = (/** @type {number} */ i) => text.slice(0, i).split('\n').length;

  // Locate every `finally` block by its keyword, not by the shape of a line, so a
  // reformatted `} finally {` does not silently stop being checked.
  /** @type {number[]} */
  const opens = [];
  const re = /finally\s*\{/g;
  let m = re.exec(text);
  while (m !== null) {
    opens.push(m.index + m[0].length - 1);
    m = re.exec(text);
  }
  if (opens.length === 0) {
    return {
      ok: false,
      blockFound: false,
      blocksChecked: 0,
      linesChecked: 0,
      findings: ['no `finally` block found — the invariant is vacuous without one'],
    };
  }

  /** @type {string[]} */
  const findings = [];
  let linesChecked = 0;
  for (const open of opens) {
    // Recompute the matching close for this specific block, so a stray brace
    // later in the file cannot skew it.
    let depth = 0;
    let close = text.length;
    for (let i = open; i < text.length; i++) {
      if (text[i] === '{') depth++;
      else if (text[i] === '}') {
        depth--;
        if (depth === 0) {
          close = i;
          break;
        }
      }
    }
    const one = scanBlock(text, open, close);
    linesChecked += one.linesChecked;
    for (const f of one.findings) findings.push(`finally block at line ${lineOf(open)}: ${f}`);
  }
  return {
    ok: findings.length === 0,
    blockFound: true,
    blocksChecked: opens.length,
    linesChecked,
    findings,
  };
}

/**
 * Fixtures that the check MUST fail on, plus the real file that it must pass.
 * Without these a silently-broken check would report "invariant holds" forever.
 *
 * The selftest also asserts these inputs are pairwise distinct, because two
 * fixtures that are the same string test the same code path and inflate the
 * count. That is not hypothetical: the "depth bug" fixture used to be a
 * byte-identical copy of the "one bare unlinkSync" one, differing only in `name`,
 * so the regression it was named for was never independently pinned. It is now a
 * different input exercising a different path.
 * @returns {{ name: string, src: string, expect: boolean }[]}
 */
function fixtures() {
  const head = 'const STORE = "x";\ntry {\n  run();\n';
  const tail = '\n} catch (err) {\n  record(err);\n} finally {\n';
  const end = '\n}\nprocess.exitCode = 0;\n';
  // A helper whose finally is long and fully compliant — the exact decoy from the
  // bug this file was fixed for: while the check read only the FIRST finally in
  // the file, this block satisfied it on behalf of the real one below.
  const decoy =
    'function helper() {\n' +
    '  try {\n    doWork();\n  } finally {\n' +
    '    try { releaseLock(); } catch (e) { note(e); }\n' +
    '    try { flushMetrics(); } catch (e) { note(e); }\n' +
    '    try { closeSocket(); } catch (e) { note(e); }\n' +
    '  }\n' +
    '}\n';
  return [
    {
      name: 'all four calls guarded (must PASS)',
      src:
        head +
        tail +
        '  try { writeFileSync(STORE, a); } catch (e) { note(e); }\n' +
        '  try { chmodSync(STORE, 0o644); } catch (e) { note(e); }\n' +
        '  try { readFileSync(STORE); } catch (e) { note(e); }\n' +
        '  try { unlinkSync(STORE); } catch (e) { note(e); }\n' +
        end,
      expect: true,
    },
    {
      name: 'one bare unlinkSync — the bug this guard exists for (must FAIL)',
      src:
        head +
        tail +
        '  try { writeFileSync(STORE, a); } catch (e) { note(e); }\n' +
        '  // padding so this fixture fails for the bare call and nothing else\n' +
        '  // padding\n  unlinkSync(STORE);\n' +
        end,
      expect: false,
    },
    {
      name: 'every call bare (must FAIL)',
      src:
        head +
        tail +
        '  writeFileSync(STORE, a);\n  chmodSync(STORE, 0o644);\n  readFileSync(STORE);\n  unlinkSync(STORE);\n' +
        end,
      expect: false,
    },
    {
      // Different bytes from the "one bare unlinkSync" fixture, a different bare
      // call, and no writeFileSync to carry the earlier assertion: the earlier
      // single-line `try { … } catch { … }` leaks one level of try-depth, so
      // without the brace-stack pass every later call looks guarded.
      name: 'single-line try/catch leaking depth, then a bare chmodSync (must FAIL)',
      src:
        head +
        tail +
        '  try { readFileSync(STORE); } catch (e) { note(e); }\n' +
        '  // distinct from the unlinkSync fixture: different call, no earlier one to lean on\n' +
        '  // padding\n  // padding\n  // padding\n  chmodSync(STORE, 0o644);\n' +
        end,
      expect: false,
    },
    {
      name: 'try/finally with NO catch (must FAIL — a throw from it propagates)',
      src:
        head +
        tail +
        '  try { unlinkSync(STORE); } finally { note(1); }\n' +
        '  // padding\n  // padding\n  // padding\n' +
        end,
      expect: false,
    },
    {
      name: 'guarded, but the catch re-throws (must FAIL — depth alone is not enough)',
      src:
        head +
        tail +
        '  // padding so this fixture fails for the re-throw and nothing else\n' +
        '  // padding\n  try { unlinkSync(STORE); } catch (e) { throw e; }\n' +
        end,
      expect: false,
    },
    {
      name: 'a longer compliant finally ABOVE a bare unlinkSync (must FAIL — the decoy)',
      src: decoy + head + tail + '  unlinkSync(STORE);\n' + end,
      expect: false,
    },
    {
      // The positive twin of the decoy: adding a compliant helper must not turn
      // the check red. Without this, "require all blocks" could be satisfied by
      // failing on every file instead of by looking at the right one.
      name: 'the same compliant helper above a fully guarded block (must PASS)',
      src:
        decoy +
        head +
        tail +
        '  // padding\n  // padding\n  try { unlinkSync(STORE); } catch (e) { note(e); }\n' +
        end,
      expect: true,
    },
    {
      // Documents a limit rather than a fix: `rethrow(` is an identifier, so the
      // `throw` keyword is not there. But a wrapper that does re-throw is equally
      // invisible — the checker reads this file, it does not call it.
      name: 'a helper named rethrow( is not a throw statement (must PASS — known limit)',
      src:
        head +
        tail +
        '  // padding so this fixture stands or falls on the identifier alone\n' +
        '  // padding\n  try { unlinkSync(STORE); } catch (e) { rethrow(e); }\n' +
        end,
      expect: true,
    },
    {
      name: 'no finally block at all (must FAIL — vacuous truth is not a pass)',
      src: head + '  run();\n}\nprocess.exitCode = 0;\n',
      expect: false,
    },
  ];
}

/**
 * @param {string[]} args
 * @returns {number}
 */
function main(args) {
  if (args.includes('--selftest')) {
    let bad = 0;
    const cases = fixtures();
    // Distinctness first: a duplicate input tests nothing new, and the previous
    // version of this file counted one and called it a regression.
    const seen = new Map();
    for (const f of cases) {
      const dup = seen.get(f.src);
      if (dup !== undefined) {
        bad++;
        console.log(`  FAIL  duplicate fixture input — same bytes as "${dup}" — ${f.name}`);
      } else {
        seen.set(f.src, f.name);
      }
    }
    console.log(
      `  ${seen.size === cases.length ? 'ok   ' : 'FAIL '} ${cases.length} fixtures, ` +
        `${seen.size} distinct inputs\n`,
    );
    for (const f of cases) {
      const v = checkSource(f.src);
      const pass = v.ok === f.expect;
      if (!pass) bad++;
      console.log(
        `  ${pass ? 'ok  ' : 'FAIL'}  expected ${f.expect ? 'PASS' : 'FAIL'}, got ${v.ok ? 'PASS' : 'FAIL'}  — ${f.name}`,
      );
      for (const line of v.findings) console.log(`          ${line}`);
    }
    const real = checkSource(readFileSync(resolve(HARNESS), 'utf8'));
    const realOk = real.ok;
    if (!realOk) bad++;
    console.log(
      `  ${realOk ? 'ok  ' : 'FAIL'}  expected PASS, got ${real.ok ? 'PASS' : 'FAIL'}  — the real ${HARNESS} (${real.blocksChecked} finally block(s))`,
    );
    for (const line of real.findings) console.log(`          ${line}`);
    console.log(
      bad === 0
        ? '\nselftest: the check can report failure. good.'
        : `\nselftest: ${bad} case(s) wrong.`,
    );
    return bad === 0 ? 0 : 1;
  }

  const target = args.find((a) => !a.startsWith('--')) ?? HARNESS;
  const verdict = checkSource(readFileSync(resolve(target), 'utf8'));
  if (verdict.ok) {
    console.log(
      `ok: all ${verdict.blocksChecked} finally block(s) of ${target} are guarded, and nothing there throws.`,
    );
    return 0;
  }
  console.error(`✖ the finally block of ${target} can throw:`);
  for (const line of verdict.findings) console.error(`  ${line}`);
  console.error('  A throw there becomes a bare stack and a forced exit 1, i.e. EXIT.UNCOVERED.');
  return verdict.blockFound ? 1 : 2;
}

process.exitCode = main(process.argv.slice(2));

// Referenced so an unused-import lint does not fire; also documents the intent.
void dirname;
void fileURLToPath;
