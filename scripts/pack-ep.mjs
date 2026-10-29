#!/usr/bin/env node
/**
 * pack-ep.mjs — build, rewrite, sign, and package privileged extensions (.dzx)
 * or stage them as Tauri builtin static resources (track B).
 *
 * The staged ESM bundle is loaded at runtime from a blob: URL with no module
 * resolution context, so every bare import (react, @codemirror/*, host
 * packages) is rewritten to the host shared-singleton table
 * (`globalThis.__DATAZEN_HOST__`, populated by the host entry) BEFORE signing —
 * signatures always cover the exact bytes that ship.
 *
 * Two gates guard the rewrite, and they are NOT equivalent:
 *   1. `rewriteEpImportsToHostGlobals` is an INPUT gate — it only sees imports
 *      still bare when it runs. An extension whose own build (the Pro
 *      `renderChunk` plugin) rewrote them first leaves it nothing to reject, so
 *      named/default/namespace imports of an unmapped package sail through with
 *      an empty `rewrote bare imports:` log (BUG-002).
 *   2. `assertHostGlobalKeysAllowed` is the ARTIFACT gate and the real one. It
 *      reads the bytes about to be signed, so no earlier rewrite can hide a key
 *      from it, and it fails the build before any signature is issued.
 *
 * Usage:
 *   node scripts/pack-ep.mjs --extension=sql-editor-pro
 *   node scripts/pack-ep.mjs --extension=sql-editor-pro --mode=dzx --out=artifacts/
 *   node scripts/pack-ep.mjs --extension=sql-editor-pro --mode=stage
 *   node scripts/pack-ep.mjs --extension=sql-editor-pro --mode=both
 *   node scripts/pack-ep.mjs --dir=/path/to/extension --mode=dzx --skip-build
 */

import { execSync } from 'child_process';
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'fs';
import { basename, dirname, join, relative, resolve } from 'path';
import { fileURLToPath } from 'url';
import { zipSync } from 'fflate';
import { signEpPackage } from './sign-ep.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
export const ROOT = resolve(__dirname, '..');
export const DEFAULT_PRO_DEST = resolve(ROOT, 'packages/pro-extensions/sql-editor-pro');
export const DEFAULT_BUILTIN_EP_ROOT = resolve(ROOT, 'apps/desktop/src-tauri/resources/builtin-ep');
export const DEFAULT_DZX_OUT_DIR = resolve(ROOT, 'artifacts');

/** Relative paths that must exist in a staged / packed extension tree. */
export const REQUIRED_PACKAGE_PATHS = [
  'manifest.json',
  'dist/index.esm.js',
  'signature.sig',
];

/** Optional locale sources copied into locales/ when present. */
export const LOCALE_SOURCE_DIR = 'src/locales';

/**
 * Sink for progress output. Every caller passes `console.log`; tests pass a
 * no-op, so the signature stays `(...args: unknown[])` rather than
 * `(...args: any[])`.
 *
 * @typedef {(...args: unknown[]) => void} LogFn
 */

/**
 * Options for the two scanner entry points
 * (`collectHostGlobalKeys` / `countUnverifiableHostRefs`).
 *
 * @typedef {{ modules?: string[], globalName?: string }} HostRefScanOptions
 */

/**
 * Options for `assertHostGlobalKeysAllowed` and the tree/archive wrappers that
 * forward to it. `hostTable` defaults to the keys parsed out of
 * `apps/desktop/src/main.tsx`; `label` only decorates the failure message.
 *
 * @typedef {{
 *   modules?: string[],
 *   hostTable?: string[],
 *   globalName?: string,
 *   label?: string,
 * }} HostKeyAssertOptions
 */

/** @typedef {{ log?: LogFn }} LogOptions */

/** @typedef {{ log?: LogFn, rewriteImports?: boolean }} StageTreeOptions */

/**
 * Shape returned by `packEp` / `packEpInner`.
 *
 * `dzxPath` is null for a stage-only run: the field exists so callers can
 * report "no archive written" without re-deriving it from `mode`.
 *
 * @typedef {{
 *   extension: string,
 *   extensionDir: string,
 *   stageDir: string,
 *   workDir: string,
 *   manifestVersion: string,
 *   dzxPath: string | null,
 *   staged: boolean,
 * }} PackEpResult
 */

/**
 * Named capture groups of `scanHostGlobalRefs`' regex, as handed to a
 * `String.prototype.replace` replacer. A group that did not participate in the
 * match reads as `undefined` — never as a positional sibling.
 *
 * @typedef {{ q?: string, raw?: string, dot?: string }} HostRefGroups
 */

/**
 * Sibling marker recording that a staging run did NOT finish.
 *
 * A staged tree is only trustworthy as a whole: `manifest.json` +
 * `dist/index.esm.js` + `signature.sig` are one signed unit, and the signature
 * covers exactly those bytes. `packEp` rewrites `builtin-ep/<ext>` as its very
 * last step, so a run that dies earlier — vite build, or the artifact-level
 * host-key gate — leaves the *previous* tree in place, still complete and still
 * signed. Nothing about it says "this does not match the source you just
 * built". `resolve-pro`'s "already staged" short-circuit then reuses it and
 * exits 0, which is how a failed build turns into a green run.
 *
 * The marker makes that state explicit. It is written *before* the build starts
 * and removed only after the tree has been staged, so a tree is trusted exactly
 * when the last staging run over it finished. A crash or a kill leaves the
 * marker behind and the tree is ignored, which is the same answer as a clean
 * failure — rebuild, never "verify with yesterday's bytes".
 *
 * It is a sibling of the tree (not a file inside it) because `stagePackageTree`
 * `rmSync`s its target on entry and would take an in-tree marker with it, and
 * because CI ships the tree alone as an artifact: a variant job that downloads
 * `pro-extension` into `builtin-ep/` sees no marker, and the "build once, share
 * with every variant" short-circuit is preserved exactly.
 *
 * @param {string} stageDir absolute path of the staged tree
 * @returns {string} absolute path of its sibling `<tree>.incomplete` marker
 */
export function stagingMarkerPath(stageDir) {
  const target = resolve(stageDir);
  return join(dirname(target), `${basename(target)}.incomplete`);
}

/**
 * @param {string} stageDir absolute path of the staged tree
 * @param {string} reason free-form text stored in the marker
 * @returns {string} the marker path just written
 */
export function markStagingIncomplete(stageDir, reason) {
  const marker = stagingMarkerPath(stageDir);
  mkdirSync(dirname(marker), { recursive: true });
  writeFileSync(marker, `${reason}\n`, 'utf-8');
  return marker;
}

/**
 * @param {string} stageDir absolute path of the staged tree
 * @returns {void}
 */
export function clearStagingIncomplete(stageDir) {
  rmSync(stagingMarkerPath(stageDir), { force: true });
}

/**
 * Whether the last staging run over `stageDir` finished.
 *
 * @param {string} stageDir absolute path of the staged tree
 * @returns {boolean}
 */
export function stagedTreeComplete(stageDir) {
  return !existsSync(stagingMarkerPath(stageDir));
}


/**
 * Host shared-singleton table consumed by staged EP bundles at runtime
 * (track B: `builtin-ep` loaded from a blob: URL with no module resolution).
 *
 * Keys must cover the extension build's `external` list plus any bare
 * specifier used in dynamic `import()` inside the bundle. The host entry
 * (`apps/desktop/src/main.tsx`) populates `globalThis.__DATAZEN_HOST__` with THE SAME keys —
 * keep both lists in sync or the blob-loaded bundle crashes on a missing key.
 * A pack-time throw on unmapped bare imports is intentional: fail here, not
 * in a user's WebView.
 */
export const HOST_GLOBAL_NAME = '__DATAZEN_HOST__';
/**
 * Narrow, explicit allow-list. Deliberately narrower than the extension's
 * build-time externalize rule (a wide `/^@codemirror\//` regex in the Pro
 * `vite.config.ts`): the wide rule only guarantees a new bare specifier gets
 * *checked*, while this list is the place where it gets *confirmed* to have a
 * host singleton. Never replace an entry with that regex — a regexp here would
 * silently admit a module the host table does not provide, shipping a second
 * copy into the bundle (cross-realm identity split, no error at load).
 * Keep in sync with the `__DATAZEN_HOST__` table in `apps/desktop/src/main.tsx`;
 * `scripts/__tests__/pack-ep.test.ts` fails when the two drift apart.
 */
export const HOST_SHARED_MODULES = [
  'react',
  'react-dom',
  'react/jsx-runtime',
  '@datazen/ui',
  '@datazen/extension-points',
  '@codemirror/state',
  '@codemirror/view',
  '@codemirror/lint',
  '@codemirror/autocomplete',
  '@codemirror/language',
  '@codemirror/commands',
];

/**
 * @param {string} spec bare module specifier
 * @param {string} [globalName] host singleton table name
 * @returns {string} the source text that reads the singleton off the host table
 */
function hostGlobalRef(spec, globalName = HOST_GLOBAL_NAME) {
  return `globalThis.${globalName}[${JSON.stringify(spec)}]`;
}

/**
 * @param {string} spec bare module specifier
 * @returns {string} an identifier-safe local name for the import
 */
function tmpVarFor(spec) {
  return `__host_${spec.replace(/[^A-Za-z0-9_$]/g, '_')}`;
}

/**
 * @param {string} spec module specifier
 * @returns {boolean} true for a bare specifier (needs a host singleton)
 */
function isBareSpecifier(spec) {
  return !(
    spec.startsWith('.') ||
    spec.startsWith('/') ||
    spec.startsWith('http') ||
    spec.startsWith('blob:') ||
    spec.startsWith('data:')
  );
}

/**
 * @param {string} kind the import form that carried `spec` (for the message)
 * @param {string} spec the unmapped bare specifier
 * @returns {Error}
 */
function unmappedError(kind, spec) {
  return new Error(
    `[pack-ep] unmapped bare ${kind} "${spec}" — add it to HOST_SHARED_MODULES ` +
      `and the host ${HOST_GLOBAL_NAME} table in src/main.tsx`,
  );
}

/**
 * Split `D, {...}` / `D, * as ns` / `{...}` / `* as ns` / `D`.
 *
 * @param {string} clause the import clause text, braces and/or `* as ns` included
 * @returns {{ def: string | null, rest: string | null }} `rest` is null when the
 *   clause carried nothing after the default binding
 */
function splitDefault(clause) {
  const text = clause.trim();
  if (text.startsWith('*') || text.startsWith('{')) return { def: null, rest: text };
  const comma = text.indexOf(',');
  if (comma === -1) return { def: text, rest: null };
  return { def: text.slice(0, comma).trim(), rest: text.slice(comma + 1).trim() };
}

/**
 * Parse `{ a, b as c }` (braces included) into [{ orig, alias }].
 *
 * @param {string} braced the named-import clause, braces included
 * @returns {{ orig: string, alias: string }[]}
 */
function parseNamedItems(braced) {
  const inner = braced.trim().replace(/^\{/, '').replace(/\}$/, '');
  return inner
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean)
    .map((item) => {
      const m = item.match(/^([A-Za-z_$][\w$]*|default)\s+as\s+([A-Za-z_$][\w$]*)$/);
      return m ? { orig: m[1], alias: m[2] } : { orig: item, alias: item };
    });
}

/**
 * @param {{ orig: string, alias: string }[]} named
 * @returns {string} a destructuring pattern body
 */
function destructureNamed(named) {
  return named
    .map((n) => (n.alias === n.orig ? n.orig : `${n.orig}: ${n.alias}`))
    .join(', ');
}

/**
 * Rewrite one static `import <clause> from "<spec>"` into const bindings off
 * the host singleton table. Default imports get `??` interop because the
 * host namespace object is the source of truth for `.default`.
 *
 * @param {string} clauseRaw the raw import clause text
 * @param {string} spec the bare specifier being imported
 * @param {string} globalName host singleton table name
 * @returns {string} replacement source text
 */
function rewriteStaticImport(clauseRaw, spec, globalName) {
  const clause = clauseRaw.trim();
  const G = hostGlobalRef(spec, globalName);
  const tmp = tmpVarFor(spec);
  const { def, rest } = splitDefault(clause);

  if (rest !== null) {
    const nsMatch = rest.match(/^\*\s+as\s+([A-Za-z_$][\w$]*)$/);
    if (nsMatch) {
      if (def === null) return `const ${nsMatch[1]} = ${G};`;
      return [
        `const ${tmp} = ${G};`,
        `const ${def} = ${tmp}.default ?? ${tmp};`,
        `const ${nsMatch[1]} = ${tmp};`,
      ].join('\n');
    }
    if (rest.startsWith('{')) {
      const named = parseNamedItems(rest);
      const defaultItems = named.filter((n) => n.orig === 'default');
      const nonDefault = named.filter((n) => n.orig !== 'default');
      if (def === null && defaultItems.length === 0) {
        return `const { ${destructureNamed(named)} } = ${G};`;
      }
      const lines = [`const ${tmp} = ${G};`];
      if (def !== null) lines.push(`const ${def} = ${tmp}.default ?? ${tmp};`);
      for (const n of defaultItems) lines.push(`const ${n.alias} = ${tmp}.default ?? ${tmp};`);
      if (nonDefault.length > 0) lines.push(`const { ${destructureNamed(nonDefault)} } = ${tmp};`);
      return lines.join('\n');
    }
  }
  if (rest === null && def !== null) {
    return `const ${tmp} = ${G};\nconst ${def} = ${tmp}.default ?? ${tmp};`;
  }
  throw new Error(`[pack-ep] cannot parse import clause from "${spec}": ${clauseRaw}`);
}

/**
 * Rewrite `export { a, b as c } from "<spec>"` — destructure behind temp
 * locals, then re-export with the original exported names preserved.
 *
 * @param {string} namesRaw the comma-separated exported names
 * @param {string} spec the bare specifier being re-exported from
 * @param {string} globalName host singleton table name
 * @param {number} counter per-call counter keeping the temp locals unique
 * @returns {string} replacement source text
 */
function rewriteExportFrom(namesRaw, spec, globalName, counter) {
  const G = hostGlobalRef(spec, globalName);
  const items = namesRaw
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  /** @type {string[]} destructuring pattern fragments, one per exported item */
  const destructured = [];
  /** @type {string[]} `local as exported` fragments, one per exported item */
  const exported = [];
  items.forEach((item, i) => {
    const local = `__e${counter}_${i}`;
    const m = item.match(/^([A-Za-z_$][\w$]*|default)\s+as\s+([A-Za-z_$][\w$]*)$/);
    if (m) {
      destructured.push(`${m[1]}: ${local}`);
      exported.push(`${local} as ${m[2]}`);
    } else {
      destructured.push(`${item}: ${local}`);
      exported.push(`${local} as ${item}`);
    }
  });
  return `const { ${destructured.join(', ')} } = ${G};\nexport { ${exported.join(', ')} };`;
}

/**
 * Rewrite every bare import in a staged EP bundle to the host
 * shared-singleton table. Idempotent (a rewritten bundle has no bare imports
 * left, so a second pass is a no-op) and strict (unknown bare specifiers
 * throw instead of shipping a bundle that crashes at load).
 *
 * Handles: static imports (default/named/namespace), side-effect imports,
 * `export ... from`, and dynamic `import("...")`. Relative/absolute/URL
 * specifiers pass through untouched.
 *
 * @param {string} code the bundle source
 * @param {HostRefScanOptions} [options]
 * @returns {{ code: string, rewritten: string[] }} the rewritten source and the
 *   bare specifiers that were mapped onto the host table
 */
export function rewriteEpImportsToHostGlobals(
  code,
  { modules = HOST_SHARED_MODULES, globalName = HOST_GLOBAL_NAME } = {},
) {
  const allowed = new Set(modules);
  const rewritten = new Set();
  let exportCounter = 0;

  // export * as ns from "bare"
  code = code.replace(
    /^[ \t]*export\s+\*\s+as\s+([A-Za-z_$][\w$]*)\s+from\s*["']([^"']+)["'][ \t]*;?[ \t]*$/gm,
    (full, ns, spec) => {
      if (!isBareSpecifier(spec)) return full;
      if (!allowed.has(spec)) throw unmappedError('re-export from', spec);
      rewritten.add(spec);
      return `const ${ns} = ${hostGlobalRef(spec, globalName)};\nexport { ${ns} };`;
    },
  );
  // export * from "bare" — cannot be expanded statically, always an error.
  code = code.replace(
    /^[ \t]*export\s+\*\s+from\s*["']([^"']+)["'][ \t]*;?[ \t]*$/gm,
    (full, spec) => {
      if (!isBareSpecifier(spec)) return full;
      throw new Error(
        `[pack-ep] cannot rewrite 'export * from "${spec}"' to host globals — ` +
          `enumerate named exports in the extension source instead`,
      );
    },
  );
  // export { ... } from "bare"
  code = code.replace(
    /^[ \t]*export\s+([^*;][^;]*?)\s+from\s*["']([^"']+)["'][ \t]*;?[ \t]*$/gm,
    (full, names, spec) => {
      if (!isBareSpecifier(spec)) return full;
      if (!allowed.has(spec)) throw unmappedError('re-export from', spec);
      rewritten.add(spec);
      return rewriteExportFrom(names, spec, globalName, exportCounter++);
    },
  );
  // static import ... from "bare"
  code = code.replace(
    /^[ \t]*import\s+([^"';]+?)\s+from\s*["']([^"']+)["'][ \t]*;?[ \t]*$/gm,
    (full, clause, spec) => {
      if (!isBareSpecifier(spec)) return full;
      if (!allowed.has(spec)) throw unmappedError('import from', spec);
      rewritten.add(spec);
      return rewriteStaticImport(clause, spec, globalName);
    },
  );
  // side-effect import "bare" — host already holds the singleton; elide.
  code = code.replace(
    /^[ \t]*import\s*["']([^"']+)["'][ \t]*;?[ \t]*$/gm,
    (full, spec) => {
      if (!isBareSpecifier(spec)) return full;
      if (!allowed.has(spec)) throw unmappedError('side-effect import', spec);
      rewritten.add(spec);
      return `/* [pack-ep] side-effect import "${spec}" elided — host provides the shared singleton */\nvoid 0;`;
    },
  );
  // dynamic import("bare") — e.g. lazily invoked Tauri/core bridges.
  code = code.replace(/import\(\s*["']([^"']+)["']\s*\)/g, (full, spec) => {
    if (!isBareSpecifier(spec)) return full;
    if (!allowed.has(spec)) throw unmappedError('dynamic import of', spec);
    rewritten.add(spec);
    return `(Promise.resolve(${hostGlobalRef(spec, globalName)}))`;
  });

  // Drop the stale sourcemap ref: the map is not staged (see stagePackageTree)
  // because rewriting invalidates its mappings.
  code = code.replace(/^[ \t]*\/\/#\s*sourceMappingURL=.*$/gm, '');

  return { code, rewritten: [...rewritten] };
}

/**
 * Read → rewrite → write an EP bundle file in place. Returns rewritten specs.
 *
 * @param {string} bundlePath path of the bundle to rewrite in place
 * @param {HostRefScanOptions} [opts] forwarded to `rewriteEpImportsToHostGlobals`
 * @returns {{ bundlePath: string, rewritten: string[] }}
 */
export function rewriteEpBundleFile(bundlePath, opts = {}) {
  const code = readFileSync(bundlePath, 'utf8');
  const { code: rewrittenCode, rewritten } = rewriteEpImportsToHostGlobals(code, opts);
  writeFileSync(bundlePath, rewrittenCode, 'utf8');
  return { bundlePath, rewritten };
}

export const HOST_TABLE_SOURCE = resolve(ROOT, 'apps/desktop/src/main.tsx');

/**
 * @param {string} text literal text to embed in a RegExp source
 * @returns {string}
 */
function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Read the key set of the `__DATAZEN_HOST__` table from the host entry module.
 *
 * This is the **runtime truth** the invariant is measured against. The table has
 * two entry shapes — quoted specifiers (`'@codemirror/view': cmView`) and bare
 * identifiers (`react: reactAll`) — so a quoted-only parser would silently drop
 * `react` and go blind in exactly the case it exists to catch. An entry line
 * matching neither shape throws: a parser that shrugs off what it cannot read
 * is not a guard. A missing table is likewise fatal, never a soft "assume
 * everything is allowed".
 *
 * @param {string} [sourcePath] host entry module to read the table from
 * @returns {string[]} the published key set, quoted and bare-identifier entries alike
 */
export function readHostGlobalTableKeys(sourcePath = HOST_TABLE_SOURCE) {
  const source = readFileSync(sourcePath, 'utf8');
  const table = source.match(/__DATAZEN_HOST__\s*=\s*\{([\s\S]*?)\n\};/);
  if (!table) {
    throw new Error(
      `[pack-ep] ${HOST_GLOBAL_NAME} table literal not found in ${sourcePath} — ` +
        `the artifact key invariant cannot be verified`,
    );
  }
  const keys = [];
  for (const rawLine of table[1].split('\n')) {
    const line = rawLine.trim();
    if (!line || line.startsWith('//') || line.startsWith('/*') || line.startsWith('*')) {
      continue;
    }
    const entry = line.match(/^(?:(['"])((?:[^'"\\]|\\.)*)\1|([A-Za-z_$][\w$]*))\s*:\s*.+?,?$/);
    if (!entry) {
      throw new Error(
        `[pack-ep] unparsable ${HOST_GLOBAL_NAME} table entry "${line}" in ${sourcePath} — ` +
          `the artifact key invariant cannot be verified`,
      );
    }
    keys.push(entry[3] ?? entry[2]);
  }
  return keys;
}

/**
 * @param {string} raw the captured key text, quotes stripped but escapes intact
 * @returns {string} the unescaped key, or `raw` unchanged when the escape is
 *   exotic enough that JSON cannot round-trip it
 */
function unquoteHostKey(raw) {
  try {
    return JSON.parse(`"${raw.replace(/"/g, '\\"')}"`);
  } catch {
    // An exotic escape the allow-list will not match, so the check below fails
    // it loudly. Never guess a key and let it through.
    return raw;
  }
}

/**
 * Every `__DATAZEN_HOST__` key the shipped bytes claim, in **any** form.
 *
 * Two forms occur in real artifacts and BOTH must be recognised, because a
 * scanner blind to either reports "0 keys" on a bundle that uses it and passes
 * vacuously:
 *   - `__DATAZEN_HOST__["react"]` — bracket form, in both quote styles, because
 *     the pack-time rewriter emits `JSON.stringify` output (double) while the
 *     Pro `renderChunk` emits single.
 *   - `__DATAZEN_HOST__.react` — dot form, present in the shipped Pro bundle for
 *     `react`: a bracket-only scanner would not even see the React binding the
 *     bundle actually loads, and a future `__DATAZEN_HOST__.search` would ship
 *     signed and crash on load exactly like the bracket-form bypass.
 *
 * @param {string} code the shipped bytes
 * @param {string} [globalName] host singleton table name
 * @returns {{ keys: string[], unverifiable: number }} `keys` is sorted and
 *   de-duplicated; `unverifiable` counts references left after masking that name
 *   the table without a statically known key
 */
function scanHostGlobalRefs(code, globalName = HOST_GLOBAL_NAME) {
  const name = escapeRegExp(globalName);
  // Named groups, not positional ones: a replacer's 3rd positional argument is
  // the match *offset* the moment a capture disappears, so reshaping this regex
  // would silently feed indexes in as keys. An absent named group is undefined.
  const reference = new RegExp(
    `${name}\\s*\\[\\s*(?<q>["'])(?<raw>(?:[^"'\\\\]|\\\\.)*)\\k<q>\\s*\\]` +
      `|${name}\\s*\\.\\s*(?<dot>[A-Za-z_$][\\w$]*)`,
    'g',
  );
  const keys = new Set();
  const masked = code.replace(
    reference,
    /**
     * Replacer for a RegExp that declares **only** named groups, so its trailing
     * argument is the groups object. It is read by position-from-the-end rather
     * than by a fixed parameter index, because the offset/whole-string arguments
     * would silently slide into the wrong slot if a numbered capture were ever
     * added. Anything that is not an object is treated as "no groups", which is
     * what reading `.dot`/`.raw` off a number or string already produced.
     *
     * @param {string} full the whole match, replaced by spaces of equal length
     * @param {...(number | string | HostRefGroups | undefined)} rest
     */
    (full, ...rest) => {
      const tail = rest[rest.length - 1];
      const groups = tail !== null && typeof tail === 'object' ? tail : {};
      keys.add(groups.dot ?? unquoteHostKey(groups.raw ?? ''));
      return ' '.repeat(full.length);
    },
  );
  const unverifiable = (masked.match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;
  return { keys: [...keys].sort(), unverifiable };
}

/**
 * @param {string} code the shipped bytes
 * @param {HostRefScanOptions} [options]
 * @returns {string[]} sorted, de-duplicated host table keys the bytes claim
 */
export function collectHostGlobalKeys(code, { globalName = HOST_GLOBAL_NAME } = {}) {
  return scanHostGlobalRefs(code, globalName).keys;
}

/**
 * Count references to the host table that name no statically known key (a
 * computed subscript or a whole-table read). Such a reference cannot be checked
 * against the host table by construction — the gate fails closed on it rather
 * than waving it through.
 *
 * @param {string} code the shipped bytes
 * @param {HostRefScanOptions} [options]
 * @returns {number}
 */
export function countUnverifiableHostRefs(code, { globalName = HOST_GLOBAL_NAME } = {}) {
  return scanHostGlobalRefs(code, globalName).unverifiable;
}

/**
 * Compose the build-failing Error for a violated host-key invariant. This is
 * the enforcement point of the BUG-002 artifact contract, so every branch it
 * reports is named explicitly in the message.
 *
 * @param {{ unmapped: string[], allowListedOnly: string[], unverifiable: number }} violations
 * @param {{ globalName: string, label: string }} context
 * @returns {Error}
 */
function hostKeyError(violations, { globalName, label }) {
  const lines = [
    `[pack-ep] artifact host-key invariant violated in ${label}:`,
    `[pack-ep] the shipped bytes resolve shared modules through ${globalName}[...],`,
    `[pack-ep] and at least one key is not a host singleton. Refusing to sign.`,
  ];
  for (const spec of violations.unmapped) {
    lines.push(
      `[pack-ep]   - unmapped host table key "${spec}" (absent from HOST_SHARED_MODULES ` +
        `and from the ${globalName} table in src/main.tsx)`,
    );
  }
  for (const spec of violations.allowListedOnly) {
    lines.push(
      `[pack-ep]   - host table key "${spec}" is in HOST_SHARED_MODULES but the ` +
        `${globalName} table in src/main.tsx never publishes it`,
    );
  }
  if (violations.unverifiable > 0) {
    lines.push(
      `[pack-ep]   - ${violations.unverifiable} non-literal ${globalName} access(es) ` +
        `(computed key or whole-table read) cannot be verified against the host table`,
    );
  }
  lines.push(
    `[pack-ep] fix: add the specifier to HOST_SHARED_MODULES (scripts/pack-ep.mjs) AND to ` +
      `the ${globalName} table in src/main.tsx, or drop the import.`,
  );
  return new Error(lines.join('\n'));
}

/**
 * BUG-002 — the artifact-level host key invariant.
 *
 * The narrow allow-list gate above is an *input* gate: it only sees what is
 * still a bare import when the pack-time rewrite runs. The Pro build's own
 * `renderChunk` runs first and already rewrites every `/^@codemirror\//`
 * import — named, default and namespace alike — into `__DATAZEN_HOST__['…']`
 * without consulting the allow-list, so by the time the input gate runs there
 * is nothing left to reject and its `rewrote bare imports:` log is permanently
 * empty. This gate reads the shipped bytes instead, so no earlier rewrite can
 * hide a key from it.
 *
 * Measured against two lists, because they answer different questions:
 * `HOST_SHARED_MODULES` is the declared intent, and the `apps/desktop/src/main.tsx` table is
 * what actually exists at runtime. Membership is **exact** — never a prefix
 * rule, which would let `react-anything` through and turn the allow-list into
 * decoration while the enumerated `react/jsx-runtime` sub-path keeps working.
 * No silent degradation: a violation is a build failure.
 *
 * @param {string} code the shipped bytes
 * @param {HostKeyAssertOptions} [options]
 * @returns {string[]} the host table keys the bytes claim (empty on success)
 * @throws {Error} `hostKeyError` on any violation
 */
export function assertHostGlobalKeysAllowed(
  code,
  {
    modules = HOST_SHARED_MODULES,
    hostTable = readHostGlobalTableKeys(),
    globalName = HOST_GLOBAL_NAME,
    label = 'bundle',
  } = {},
) {
  const allowList = new Set(modules);
  const host = new Set(hostTable);
  const keys = collectHostGlobalKeys(code, { globalName });
  const unmapped = [];
  const allowListedOnly = [];
  for (const key of keys) {
    if (!allowList.has(key) && !host.has(key)) {
      unmapped.push(key);
    } else if (allowList.has(key) && !host.has(key)) {
      allowListedOnly.push(key);
    }
  }
  const unverifiable = countUnverifiableHostRefs(code, { globalName });
  if (unmapped.length > 0 || allowListedOnly.length > 0 || unverifiable > 0) {
    throw hostKeyError({ unmapped, allowListedOnly, unverifiable }, { globalName, label });
  }
  return keys;
}

/**
 * Assert the invariant on the staged/packed bundle of a package tree.
 *
 * @param {string} packageDir root of the package tree
 * @param {HostKeyAssertOptions} [opts] forwarded to `assertHostGlobalKeysAllowed`;
 *   `label` defaults to the bundle path so the message names the actual artifact
 * @returns {string[]} the host table keys the bundle claims
 */
export function assertHostGlobalKeysInTree(packageDir, opts = {}) {
  const bundle = join(packageDir, 'dist/index.esm.js');
  if (!existsSync(bundle)) {
    throw new Error(`[pack-ep] cannot verify host keys: ${bundle} does not exist`);
  }
  return assertHostGlobalKeysAllowed(readFileSync(bundle, 'utf8'), {
    ...opts,
    label: opts.label ?? bundle,
  });
}

/**
 * Parse `--flag=value` / `--flag` command line into packEp options.
 *
 * `mode` is deliberately typed `string` rather than the three-value union
 * `packEp` accepts: this parser happily returns an unrecognised `--mode=…`, and
 * narrowing the type here would make that unrepresentable while `packEp` still
 * has to reject it at runtime.
 *
 * @param {string[]} [argv] arguments after the script name
 * @returns {{
 *   extension: string,
 *   extensionDir: string,
 *   mode: string,
 *   outDir: string,
 *   stageDir: string,
 *   skipBuild: boolean,
 * }} all paths resolved to absolute
 */
export function parsePackArgs(argv = process.argv.slice(2)) {
  let extension = 'sql-editor-pro';
  let extensionDir = null;
  let mode = 'both';
  let outDir = DEFAULT_DZX_OUT_DIR;
  let stageDir = null;
  let skipBuild = false;

  for (const arg of argv) {
    if (arg.startsWith('--extension=')) {
      extension = arg.slice('--extension='.length);
    } else if (arg.startsWith('--dir=')) {
      extensionDir = arg.slice('--dir='.length);
    } else if (arg.startsWith('--mode=')) {
      mode = arg.slice('--mode='.length);
    } else if (arg.startsWith('--out=')) {
      outDir = arg.slice('--out='.length);
    } else if (arg.startsWith('--stage-dir=')) {
      stageDir = arg.slice('--stage-dir='.length);
    } else if (arg === '--skip-build') {
      skipBuild = true;
    }
  }

  const resolvedExtensionDir =
    extensionDir ?? (extension === 'sql-editor-pro' ? DEFAULT_PRO_DEST : join(ROOT, 'packages/pro-extensions', extension));
  const resolvedStageDir =
    stageDir ?? join(DEFAULT_BUILTIN_EP_ROOT, extension);

  return {
    extension,
    extensionDir: resolve(resolvedExtensionDir),
    mode,
    outDir: resolve(outDir),
    stageDir: resolve(resolvedStageDir),
    skipBuild,
  };
}

/**
 * @param {string} manifestPath path of the package's manifest.json
 * @returns {string} the declared version
 * @throws {Error} when `version` is missing or is not a string
 */
export function readManifestVersion(manifestPath) {
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  if (!manifest.version || typeof manifest.version !== 'string') {
    throw new Error(`[pack-ep] manifest.json missing string "version" at ${manifestPath}`);
  }
  return manifest.version;
}

/**
 * Build the extension's distributable library with its own vite config.
 *
 * @param {string} extensionDir extension package root
 * @param {LogOptions} [options]
 * @returns {void}
 * @throws {Error} when package.json or the built bundle is missing
 */
export function buildExtensionLibrary(extensionDir, { log = console.log } = {}) {
  if (!existsSync(join(extensionDir, 'package.json'))) {
    throw new Error(`[pack-ep] extension package.json not found under ${extensionDir}`);
  }
  log(`[pack-ep] building extension library in ${extensionDir}`);
  execSync('npx vite build', {
    cwd: extensionDir,
    stdio: 'inherit',
    env: process.env,
  });
  const bundle = join(extensionDir, 'dist/index.esm.js');
  if (!existsSync(bundle)) {
    throw new Error(`[pack-ep] build did not produce dist/index.esm.js under ${extensionDir}`);
  }
}

/**
 * Copy locale sources into the staged tree's `locales/`.
 *
 * An explicit `locales/` directory in the extension wins outright; otherwise
 * the TypeScript/JSON sources under `src/locales` are copied one by one so
 * build scratch files never reach the staged tree.
 *
 * @param {string} extensionDir extension package root
 * @param {string} targetRoot staged tree root
 * @param {LogOptions} [options]
 * @returns {void}
 */
export function syncLocales(extensionDir, targetRoot, { log = console.log } = {}) {
  const localesOut = join(targetRoot, 'locales');
  mkdirSync(localesOut, { recursive: true });

  const explicitLocalesDir = join(extensionDir, 'locales');
  if (existsSync(explicitLocalesDir)) {
    cpSync(explicitLocalesDir, localesOut, { recursive: true });
    log(`[pack-ep] copied locales/ from ${explicitLocalesDir}`);
    return;
  }

  const srcLocales = join(extensionDir, LOCALE_SOURCE_DIR);
  if (!existsSync(srcLocales)) {
    return;
  }

  for (const name of readdirSync(srcLocales)) {
    if (!/\.(ts|js|json)$/.test(name)) continue;
    cpSync(join(srcLocales, name), join(localesOut, name));
  }
  log(`[pack-ep] copied locale sources from ${srcLocales}`);
}

/**
 * Copy manifest + bundle into `targetDir`, rewrite bare imports, enforce the
 * artifact host-key invariant, then sign. The invariant runs *before* signing on
 * purpose: an unsatisfiable bundle must never acquire a signature.
 *
 * @param {string} sourceDir built extension package root
 * @param {string} targetDir staged tree root (replaced wholesale)
 * @param {StageTreeOptions} [options] `rewriteImports: false` keeps the bundle bytes as built
 * @returns {void}
 * @throws {Error} from the host-key gate, `signEpPackage`, or a missing source file
 */
export function stagePackageTree(sourceDir, targetDir, { log = console.log, rewriteImports = true } = {}) {
  if (existsSync(targetDir)) {
    rmSync(targetDir, { recursive: true, force: true });
  }
  mkdirSync(targetDir, { recursive: true });

  cpSync(join(sourceDir, 'manifest.json'), join(targetDir, 'manifest.json'));
  mkdirSync(join(targetDir, 'apps/desktop/dist'), { recursive: true });
  cpSync(join(sourceDir, 'dist/index.esm.js'), join(targetDir, 'dist/index.esm.js'));
  if (rewriteImports) {
    // Track B: blob-loaded bundles cannot resolve bare imports — rewrite to
    // the host singleton table BEFORE signing (the signature covers shipped bytes).
    const stagedBundle = join(targetDir, 'dist/index.esm.js');
    const { rewritten } = rewriteEpBundleFile(stagedBundle);
    log(`[pack-ep] rewrote bare imports to ${HOST_GLOBAL_NAME}: ${rewritten.sort().join(', ')}`);
  }
  // BUG-002: the input gate above only sees imports still bare at rewrite time.
  // The extension's own build may have rewritten them first, leaving nothing to
  // reject — so verify the bytes we are about to sign, not the bytes we read.
  // Runs BEFORE signEpPackage on purpose: an unsatisfiable bundle must never
  // acquire a signature, or the crash becomes unauditable downstream.
  const shippedKeys = assertHostGlobalKeysInTree(targetDir, {
    label: `staged bundle ${targetDir}/dist/index.esm.js`,
  });
  log(
    `[pack-ep] verified ${shippedKeys.length} ${HOST_GLOBAL_NAME} key(s) against the host ` +
      `table + allow-list: ${shippedKeys.join(', ') || '(none)'}`,
  );
  // NOTE: dist/index.esm.js.map is intentionally NOT staged — the import
  // rewrite invalidates its mappings. Debug against extension sources instead.
  syncLocales(sourceDir, targetDir, { log });
  signEpPackage({ packageDir: targetDir });
}

/**
 * @param {string} packageDir root of the package tree
 * @returns {void}
 * @throws {Error} listing every missing required path
 */
export function assertPackageLayout(packageDir) {
  const missing = REQUIRED_PACKAGE_PATHS.filter((rel) => !existsSync(join(packageDir, rel)));
  if (missing.length > 0) {
    throw new Error(`[pack-ep] incomplete package at ${packageDir}; missing: ${missing.join(', ')}`);
  }
}

/**
 * Walk a directory tree into a path → bytes map for the archiver.
 *
 * @param {string} rootDir tree root that relative keys are computed against
 * @param {string} [currentDir] recursion cursor, always a descendant of `rootDir`
 * @param {Record<string, Buffer>} [acc] accumulator, threaded through the recursion
 * @returns {Record<string, Buffer>} keys are POSIX-separated, relative to `rootDir`
 */
export function listZipEntries(rootDir, currentDir = rootDir, acc = {}) {
  for (const name of readdirSync(currentDir, { withFileTypes: true })) {
    const abs = join(currentDir, name.name);
    const rel = relative(rootDir, abs).split('\\').join('/');
    if (name.isDirectory()) {
      listZipEntries(rootDir, abs, acc);
    } else {
      acc[rel] = readFileSync(abs);
    }
  }
  return acc;
}

/**
 * Re-check the host-key invariant, zip the whole tree, and write the `.dzx`.
 *
 * @param {string} packageDir root of the package tree
 * @param {string} outFile absolute path of the archive to write
 * @param {HostKeyAssertOptions} [opts] forwarded to `assertHostGlobalKeysInTree`
 * @returns {string} `outFile`
 */
export function createDzxArchive(packageDir, outFile, opts = {}) {
  assertPackageLayout(packageDir);
  // Defence in depth for the CI/prebuilt handoff: a tree that reached the
  // archiver by any other route still must not ship a key the host lacks.
  assertHostGlobalKeysInTree(packageDir, {
    label: `archive source ${packageDir}/dist/index.esm.js`,
    ...opts,
  });
  const entries = listZipEntries(packageDir);
  const zipped = zipSync(entries, { level: 9 });
  mkdirSync(dirname(outFile), { recursive: true });
  writeFileSync(outFile, Buffer.from(zipped));
  return outFile;
}

/**
 * @param {string} extensionId extension id, e.g. `sql-editor-pro`
 * @param {string} version manifest version
 * @returns {string}
 */
export function dzxFileName(extensionId, version) {
  return `${extensionId}-${version}.dzx`;
}

/**
 * @param {{
 *   extension?: string,
 *   extensionDir?: string,
 *   mode?: 'dzx' | 'stage' | 'both',
 *   outDir?: string,
 *   stageDir?: string,
 *   skipBuild?: boolean,
 *   log?: LogFn,
 * }} [opts]
 * @returns {PackEpResult}
 * @throws {Error} from the build, the host-key gate, or an unknown `mode`
 */
export function packEp(opts = {}) {
  const parsed = parsePackArgs();
  const extension = opts.extension ?? parsed.extension;
  const extensionDir = resolve(opts.extensionDir ?? parsed.extensionDir);
  const mode = opts.mode ?? parsed.mode;
  const outDir = resolve(opts.outDir ?? parsed.outDir);
  const stageDir = resolve(opts.stageDir ?? join(DEFAULT_BUILTIN_EP_ROOT, extension));
  const skipBuild = opts.skipBuild ?? parsed.skipBuild;
  const log = opts.log ?? console.log.bind(console);

  // Only a run that is going to rewrite the staged tree may invalidate it: a
  // dzx-only pack never touches `stageDir` and must leave the CI-shared tree
  // exactly as usable as it found it.
  const stagesTree = mode === 'stage' || mode === 'both';
  if (stagesTree) {
    markStagingIncomplete(stageDir, `pack-ep started at ${new Date().toISOString()} (${extension})`);
  }

  try {
    return packEpInner({
      extension,
      extensionDir,
      mode,
      outDir,
      stageDir,
      skipBuild,
      log,
      stagesTree,
    });
  } catch (err) {
    if (stagesTree) {
      // Left in place on purpose: the tree under `stageDir` still carries the
      // PREVIOUS build's bytes and signature, and must not be reusable as if it
      // were this build's. (The work dir under `artifacts/` is deliberately
      // kept too — it is the on-disk evidence of what this run produced.)
      //
      // The thrown value is narrowed instead of assumed, because JS lets a
      // non-Error be thrown. Measured by driving the real markStagingIncomplete,
      // stagedTreeComplete and stagedTreeUsable: against the previous unguarded
      // `${err.message}`, `throw null` and `throw undefined` each made THIS call
      // throw a TypeError, so neither the FAILED marker nor the log() below ran.
      //
      // That cost DIAGNOSTICS, not safety. The marker is on disk either way: the
      // markStagingIncomplete above the `try`, under the same `stagesTree` guard,
      // already wrote it, and the only clear is the clearStagingIncomplete on
      // packEpInner's success path — unreachable once this catch fires. Measured
      // in both cases: marker present, stagedTreeComplete() false, resolve-pro
      // does not reuse the stale tree. No security property was at risk. What was
      // lost is the real thrown value (the TypeError stood in for it), the
      // marker's reason text, and this log() line.
      //
      // The narrowing can itself fail, which is why it was measured rather than
      // assumed: it does not. The mark executes and the reason becomes
      // `pack-ep FAILED for <extension> at <timestamp>: null`. So the
      // discriminator between the old and new behaviour is the marker's REASON
      // TEXT — not whether a marker is on disk, which is true in both cases.
      markStagingIncomplete(
        stageDir,
        `pack-ep FAILED for ${extension} at ${new Date().toISOString()}: ${
          err instanceof Error ? err.message : String(err)
        }`,
      );
      log(
        `[pack-ep] staging at ${stageDir} marked incomplete after a failed run; ` +
          `resolve-pro will not reuse it. (markers: ${stagingMarkerPath(stageDir)})`,
      );
    }
    throw err;
  }
}

/**
 * @param {{
 *   extension: string,
 *   extensionDir: string,
 *   mode: string,
 *   outDir: string,
 *   stageDir: string,
 *   skipBuild: boolean,
 *   log: LogFn,
 *   stagesTree: boolean,
 * }} ctx every field already resolved and defaulted by `packEp`
 * @returns {PackEpResult}
 */
function packEpInner({ extension, extensionDir, mode, outDir, stageDir, skipBuild, log, stagesTree }) {
  let effectiveExtensionDir = extensionDir;
  if (!existsSync(effectiveExtensionDir)) {
    if (extension === 'sql-editor-pro' && existsSync(DEFAULT_PRO_DEST)) {
      log(`[pack-ep] specified dir not found: ${effectiveExtensionDir}, falling back to ${DEFAULT_PRO_DEST}`);
      effectiveExtensionDir = DEFAULT_PRO_DEST;
    } else {
      throw new Error(`[pack-ep] extension directory not found: ${effectiveExtensionDir}`);
    }
  }

  if (!skipBuild) {
    buildExtensionLibrary(effectiveExtensionDir, { log });
  } else if (!existsSync(join(effectiveExtensionDir, 'dist/index.esm.js'))) {
    throw new Error(
      `[pack-ep] --skip-build requires existing dist/index.esm.js under ${effectiveExtensionDir}`,
    );
  }

  const workDir = join(outDir, `.pack-ep-staging-${extension}`);
  stagePackageTree(effectiveExtensionDir, workDir, { log });
  assertPackageLayout(workDir);

  const manifestVersion = readManifestVersion(join(workDir, 'manifest.json'));
  const dzxName = dzxFileName(extension, manifestVersion);
  const dzxPath = join(outDir, dzxName);

  /** @type {PackEpResult} */
  const result = {
    extension,
    extensionDir,
    stageDir,
    workDir,
    manifestVersion,
    dzxPath: null,
    staged: false,
  };

  if (mode === 'stage' || mode === 'both') {
    stagePackageTree(workDir, stageDir, { log });
    result.staged = true;
    log(`[pack-ep] staged signed extension to ${stageDir}`);
  }

  if (mode === 'dzx' || mode === 'both') {
    createDzxArchive(workDir, dzxPath);
    result.dzxPath = dzxPath;
    log(`[pack-ep] wrote ${dzxPath}`);
  }

  if (mode !== 'stage' && mode !== 'dzx' && mode !== 'both') {
    throw new Error(`[pack-ep] unknown mode "${mode}" (expected dzx, stage, or both)`);
  }

  rmSync(workDir, { recursive: true, force: true });
  if (stagesTree) {
    // Only now is the tree a faithful, signed image of this build.
    clearStagingIncomplete(stageDir);
  }
  return result;
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1]);
if (isMain) {
  try {
    packEp();
  } catch (err) {
    console.error(err);
    process.exit(1);
  }
}
