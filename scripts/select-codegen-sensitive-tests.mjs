#!/usr/bin/env node
import { isDirectInvocation } from './lib/isDirectInvocation.mjs';
/**
 * Select the Host-suite test files whose behaviour can depend on the injected
 * driver set.
 *
 * ## Why this exists
 *
 * `pnpm test:unit` and `pnpm test:unit:driver-set` both collect the same files
 * (the `include` globs of vitest.config.ts); the second run exists to judge
 * the suite under a driver set other than `basic`. The only artifact that
 * differs between the two runs is `packages/frontend/src/extensions/generated.ts` (gitignored
 * codegen): `builtinLocales.ts` is also regenerated, but its bytes do not
 * depend on `--drivers`, and nothing else on the test path reads the set.
 *
 * A test can therefore only behave differently under another driver set if its
 * module graph **executes** `packages/frontend/src/extensions/generated.ts`. That is a narrower
 * predicate than "appears in the same suite" and a wider one than "imports it
 * directly": both are wrong in opposite directions.
 *
 * ## The criterion, stated exactly
 *
 * A test file is selected iff `packages/frontend/src/extensions/generated.ts` is reachable from
 * it through **value imports**: import forms that make the bundler load and
 * execute the target module. Type-only forms are excluded because esbuild
 * (and therefore vitest) erases them before execution:
 *
 *   - `import type … from '…'` / `export type … from '…'` — erased whole;
 *   - `import { type A, type B } from '…'` — erased when EVERY named binding
 *     carries the inline `type` marker;
 *   - `import { A } from '…'` where `A` is only used in type positions —
 *     also erased by esbuild's unused-binding elimination (this repo compiles
 *     with `isolatedModules` but not `verbatimModuleSyntax`, so this form is
 *     legal). The scanner cannot see usage sites, so it counts the statement
 *     as a value edge. That over-selects; it never under-selects. The cost of
 *     one over-selected file is one test file executed twice; the cost of one
 *     under-selected file is a driver-set regression that CI structurally
 *     cannot see. The asymmetry decides the direction.
 *
 * Dynamic `import('…')`, side-effect `import '…'` and namespace
 * `import * as … from '…'` all execute the target and count as value edges.
 *
 * `packages/frontend/src/locales/builtinLocales.ts` is deliberately NOT a target: it is
 * regenerated identically for every driver set, so no test can behave
 * differently because of it.
 *
 * ## Fail-closed, not fail-cheap
 *
 * Every degenerate input is an error, never a silent fallback:
 *
 *   - vitest.config.ts unparsable (no `include` block, or no block whose
 *     entries all look like test-file patterns) → throw;
 *   - an `include` glob matching zero files on disk → throw (a stale pattern
 *     would otherwise quietly shrink the driver-set run);
 *   - a `setupFiles` entry reaching generated.ts → throw: setup modules run
 *     for every test, so the driver set would be observable everywhere and
 *     narrowing would be unsound rather than merely unprofitable;
 *   - an empty selection → throw (an empty vitest filter is vacuously green).
 *
 * @example
 * node scripts/select-codegen-sensitive-tests.mjs
 */

import { existsSync, readFileSync, readdirSync, statSync } from 'fs';
import { dirname, join, posix, resolve } from 'path';
import { fileURLToPath } from 'url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));
const ROOT = resolve(__dirname, '..');

export const PREFIX = '[codegen-sensitive]';

/**
 * The gitignored codegen files whose *content* depends on `--drivers`, and
 * which the Host suite can import. `driver_init.rs` is Rust-only and never
 * imported from the suite, so it is not listed.
 */
export const CODEGEN_TARGETS = Object.freeze(['packages/frontend/src/extensions/generated.ts']);

/** Directories that can neither be collected by `include` nor imported. */
const SKIP_DIRS = new Set(['node_modules', 'apps/desktop/dist', 'pro-extensions', '.git', 'target']);

/**
 * Regex over import/export statements. The clause group is everything between
 * the keyword and `from`, so a statement's kind can be decided without parsing
 * TS. The alternation also picks up side-effect and dynamic imports, which are
 * value edges by definition.
 */
const IMPORT_STATEMENT_RE =
  /(?:import|export)\s+([^'";]*?)\s*from\s*['"]([^'"]+)['"]|import\s*['"]([^'"]+)['"]|import\(\s*['"]([^'"]+)['"]/g;

/**
 * Decide whether an import/export clause is a value edge.
 *
 * @param {string} clause - text between `import`/`export` and `from`, verbatim
 * @returns {boolean} true when the bundler must load and execute the target
 */
export function isValueImportClause(clause) {
  const m = clause.trim();
  if (!m) return false; // `from '…'` alone is not a statement this scanner owns
  if (m === '*' || m.startsWith('* as')) return true; // namespace: value
  if (/^type[\s(]/.test(m) || m === 'type') return false; // import type: erased
  const brace = /\{([^}]*)\}/.exec(m);
  const beforeBrace = brace ? m.slice(0, brace.index).trim().replace(/,$/, '') : m;
  if (beforeBrace) return true; // default or bare binding: value
  if (!brace) return true; // unrecognized shape: assume value (over-select)
  const bindings = brace[1].split(',').map((s) => s.trim()).filter(Boolean);
  if (bindings.length === 0) return true; // `import {} from` still executes it
  return !bindings.every((b) => /^type[\s]/.test(b)); // all-marked-type: erased
}

/**
 * Extract the `test.include` glob list and `resolve.alias` map from
 * vitest.config.ts **as text**. Importing the config would need a TS loader;
 * duplicating the lists would need a second owner. Reading the text keeps the
 * config the single owner and turns any reformatting into a loud error here
 * instead of a silent divergence.
 *
 * @param {string} configText
 * @returns {{ include: string[], aliases: Record<string, string>, setupFiles: string[] }}
 */
export function parseVitestTestConfig(configText) {
  const includeBlocks = [...configText.matchAll(/include:\s*\[([\s\S]*?)\]/g)].map((m) => m[1]);
  const testBlocks = includeBlocks.filter((block) => {
    const entries = [...block.matchAll(/'([^']+)'/g)].map((m) => m[1]);
    return entries.length > 0 && entries.every((e) => e.includes('.test.'));
  });
  if (testBlocks.length !== 1) {
    throw new Error(
      `expected exactly one test-file include block in vitest.config.ts, found ${testBlocks.length}` +
        ` — a second block whose entries all contain '.test.' or the removal of the real one both need a human`,
    );
  }
  const include = [...testBlocks[0].matchAll(/'([^']+)'/g)].map((m) => m[1]);

  const aliases = {};
  const aliasBlock = /alias:\s*\{([\s\S]*?)\n\s*\}/.exec(configText);
  if (aliasBlock) {
    for (const m of aliasBlock[1].matchAll(/'([^']+)':\s*resolve\(__dirname,\s*'([^']+)'\)/g)) {
      aliases[m[1]] = m[2];
    }
    if (Object.keys(aliases).length === 0) {
      // The block exists but no entry parsed: the extraction pattern is stale,
      // and a silent empty map would quietly stop resolving alias imports.
      throw new Error('resolve.alias block found but no entry parsed — extraction pattern is stale');
    }
  }
  // No `alias:` key at all is a legitimate config shape: nothing to resolve
  // through, so the walk simply sees no alias edges.

  const setupFiles = [];
  if (/\bsetupFiles\b/.test(configText)) {
    const setupMatch = /setupFiles:\s*\[([^\]]*)\]/.exec(configText);
    if (!setupMatch) {
      throw new Error('vitest.config.ts mentions setupFiles but the array cannot be parsed');
    }
    for (const m of setupMatch[1].matchAll(/'([^']+)'/g)) setupFiles.push(m[1]);
  }
  return { include, aliases, setupFiles };
}

/**
 * @param {string} root
 * @returns {Set<string>} posix-relative paths of every candidate source file
 */
export function collectSourceFiles(root) {
  const out = new Set();
  const walk = (dirRel) => {
    let entries;
    try {
      entries = readdirSync(join(root, dirRel));
    } catch {
      return; // a root that does not exist contributes nothing
    }
    for (const entry of entries) {
      if (SKIP_DIRS.has(entry)) continue;
      const rel = dirRel === '' ? entry : posix.join(dirRel, entry);
      let st;
      try {
        st = statSync(join(root, rel));
      } catch {
        continue;
      }
      if (st.isDirectory()) walk(rel);
      else if (/\.(ts|tsx|mjs)$/.test(rel)) out.add(rel.split('\\').join('/'));
    }
  };
  // F6 layout: shared sources live under packages/frontend/src, the desktop
  // app under apps/desktop/src. Walking 'packages' and 'apps' covers both and
  // any future sibling packages/apps; 'scripts' completes the module graph
  // roots a test file can import from.
  walk('packages');
  walk('apps');
  walk('scripts');
  return out;
}

/** Minimal glob → RegExp for the shapes vitest.config.ts actually uses. */
export function globToRegExp(pattern) {
  let re = '';
  let i = 0;
  while (i < pattern.length) {
    const ch = pattern[i];
    if (ch === '*') {
      if (pattern.slice(i, i + 3) === '**/') {
        re += '(?:[^/]+/)*';
        i += 3;
      } else {
        re += '[^/]*';
        i += 1;
      }
    } else if (ch === '{') {
      const end = pattern.indexOf('}', i);
      if (end === -1) throw new Error(`unbalanced brace in include pattern: ${pattern}`);
      re += '(?:' + pattern.slice(i + 1, end).split(',').join('|') + ')';
      i = end + 1;
    } else if ('\\.^$|()[]+?'.includes(ch)) {
      re += '\\' + ch;
      i += 1;
    } else {
      re += ch;
      i += 1;
    }
  }
  return new RegExp('^' + re + '$');
}

/**
 * Value-import edges of one file, resolved to posix-relative paths.
 *
 * @param {string} root
 * @param {string} rel - posix-relative file path
 * @param {Record<string, string>} aliases
 * @param {Set<string>} known - every candidate source file (resolution oracle)
 * @returns {string[]}
 */
export function valueImportsOf(root, rel, aliases, known) {
  let text;
  try {
    text = readFileSync(join(root, rel), 'utf-8');
  } catch {
    return [];
  }
  const specs = new Set();
  IMPORT_STATEMENT_RE.lastIndex = 0;
  let m;
  while ((m = IMPORT_STATEMENT_RE.exec(text))) {
    if (m[2] !== undefined) {
      if (isValueImportClause(m[1])) specs.add(m[2]);
    } else if (m[3] !== undefined) specs.add(m[3]);
    else if (m[4] !== undefined) specs.add(m[4]);
  }
  const resolved = [];
  for (const spec of specs) {
    const bare = spec.split('?')[0];
    let base = null;
    if (bare.startsWith('./') || bare.startsWith('../')) {
      base = posix.normalize(posix.join(dirname(rel), bare));
    } else if (aliases[bare]) {
      base = aliases[bare];
    } else {
      continue; // external package: not part of the repo graph
    }
    for (const cand of [
      base,
      base + '.ts',
      base + '.tsx',
      base + '.mjs',
      base + '.js',
      posix.join(base, 'index.ts'),
      posix.join(base, 'index.tsx'),
      posix.join(base, 'index.mjs'),
    ]) {
      if (known.has(cand)) {
        resolved.push(cand);
        break;
      }
    }
  }
  return resolved;
}

/**
 * Compute the codegen-sensitive test files.
 *
 * @param {{ root?: string, log?: (s: string) => void }} [options]
 * @returns {{ files: string[], testFileCount: number, sensitiveModuleCount: number }}
 * @throws on every degenerate input — see the module doc. Never returns an
 *   empty `files`.
 */
export function selectCodegenSensitiveTests(options = {}) {
  const root = options.root ?? ROOT;
  const log = options.log ?? (() => {});

  const configPath = join(root, 'vitest.config.ts');
  if (!existsSync(configPath)) throw new Error(`missing vitest.config.ts at ${configPath}`);
  const { include, aliases, setupFiles } = parseVitestTestConfig(readFileSync(configPath, 'utf-8'));

  const sources = collectSourceFiles(root);
  if (sources.size === 0) throw new Error('no candidate source files found under packages/apps/scripts');

  // Forward edges once per file, then reverse them: the dependents of
  // generated.ts are a much smaller subgraph than the whole suite, and one
  // reverse walk answers for every test file at once.
  const forward = new Map();
  for (const rel of sources) forward.set(rel, valueImportsOf(root, rel, aliases, sources));
  const reverse = new Map();
  for (const [file, deps] of forward) {
    for (const dep of deps) {
      if (!reverse.has(dep)) reverse.set(dep, []);
      reverse.get(dep).push(file);
    }
  }

  const dependents = new Set(CODEGEN_TARGETS);
  const queue = [...CODEGEN_TARGETS];
  for (const target of CODEGEN_TARGETS) {
    if (!sources.has(target)) throw new Error(`codegen target missing from the tree: ${target}`);
  }
  while (queue.length) {
    const cur = queue.shift();
    for (const importer of reverse.get(cur) ?? []) {
      if (!dependents.has(importer)) {
        dependents.add(importer);
        queue.push(importer);
      }
    }
  }
  log(`${PREFIX} ${dependents.size} source file(s) transitively execute the codegen targets`);

  for (const setup of setupFiles) {
    const rel = setup.replace(/^\.\//, '');
    if (dependents.has(rel)) {
      throw new Error(
        `setup file ${rel} reaches the codegen targets, so every test observes the driver set — ` +
          `narrowing is unsound here and must not be attempted`,
      );
    }
  }

  const testFiles = new Set();
  for (const pattern of include) {
    const re = globToRegExp(pattern);
    let hits = 0;
    for (const rel of sources) {
      if (re.test(rel)) {
        testFiles.add(rel);
        hits++;
      }
    }
    if (hits === 0) {
      throw new Error(`include pattern matched no file on disk: ${pattern}`);
    }
  }
  if (testFiles.size === 0) throw new Error('the vitest include patterns collected no test files');

  const files = [...testFiles].filter((f) => dependents.has(f)).sort();
  if (files.length === 0) {
    throw new Error(
      'no test file reaches the codegen targets — either the walk is broken or the suite ' +
        'genuinely stopped importing them; both need a human, not an empty green run',
    );
  }
  log(`${PREFIX} ${files.length} of ${testFiles.size} collected test file(s) are driver-set sensitive`);
  return { files, testFileCount: testFiles.size, sensitiveModuleCount: dependents.size };
}

function main() {
  const { files, testFileCount } = selectCodegenSensitiveTests({ log: console.log });
  for (const file of files) console.log(file);
  console.log(`${PREFIX} ${files.length}/${testFileCount} test file(s)`);
}

if (isDirectInvocation(import.meta.url)) {
  main();
}
