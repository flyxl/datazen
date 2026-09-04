#!/usr/bin/env node
/**
 * ID terminology guard.
 *
 * Fails when deprecated connection-id identifiers appear in frontend / driver
 * / e2e sources. Terminology (see docs/architecture/naming.md):
 *   - `connectionId` / `connection_id` = persisted connection config id
 *     (formerly `configId`)
 *   - `dbSessionId`  / `db_session_id`  = runtime database session id
 *     (formerly the runtime meaning of `connectionId`) — never persisted
 *
 * Scans src/ packages/ e2e/ (skips node_modules and gitignored codegen such
 * as src/plugins/generated*.ts). Any forbidden hit outside the allow-list
 * below exits 1 with `file:line: content`.
 */
import { readdirSync, readFileSync } from 'fs';
import { join, relative, resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const SCAN_DIRS = ['src', 'packages', 'e2e'];
const SCAN_EXTENSIONS = new Set(['.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs', '.rs']);
const SKIP_DIR_NAMES = new Set(['node_modules', 'dist', 'coverage', '.git']);
// Gitignored codegen produced by resolve-drivers — never hand-written source.
const SKIP_FILES = new Set(['src/plugins/generated.ts', 'src/plugins/generated-locales.ts']);

/**
 * Forbidden token shapes. Every entry must carry a reason; hits are only
 * tolerated through the ALLOWLIST below (one justified entry each).
 */
const FORBIDDEN_PATTERNS = [
  // Old name of the persisted connection-config id (pre-rename). Any camelCase
  // occurrence is stale W1-W3 leftovers or a reintroduction.
  { regex: /\bconfigId\b/, reason: 'old persisted-config id name; use connectionId' },
  // snake_case variant of the same old name (Rust / payload keys).
  { regex: /\bconfig_id\b/, reason: 'old persisted-config id name; use connection_id' },
  // Specific old derived names that already caused silent regressions
  // (BUG-004: stale prop kept a test green while coverage was weakened).
  { regex: /\bactiveConfigId\b/, reason: 'old prop name; use activeConnectionId' },
  { regex: /\bsourceConfigId\b/, reason: 'old SyncTask/schema-diff field; see naming.md §5' },
  { regex: /\btargetConfigId\b/, reason: 'old SyncTask/schema-diff field; see naming.md §5' },
  { regex: /\bcatConfigId\b/, reason: 'old tree category key; use catConnectionId' },
  // Reversed form: stuffing the CONFIG id into what is now (or was) a session
  // key. Post-rename the session key is dbSessionId; `connectionId:` remains
  // in the list because pre-rename code used it as the session key.
  {
    regex: /\bconnectionId\s*:\s*["'`]?config\b/,
    reason: 'config id assigned into a connection/session key (reversed form)',
  },
  {
    regex: /\bdbSessionId\s*:\s*["'`]?config\b/,
    reason: 'config id assigned into the dbSessionId key (reversed form)',
  },
];

/**
 * Allow-list: legitimate occurrences, one entry per justification. A hit is
 * suppressed only when BOTH the file path matches AND the offending line
 * matches `line`. Anything else still fails, so edits inside these files stay
 * guarded.
 */
const ALLOWLIST = [
  {
    // Historical-format doc comment: documents that schema-diff v1 payloads
    // carrying the OLD `configId` key are rejected on import. The token only
    // describes the rejected legacy format; it is not a live identifier.
    file: 'src/commands/schemaDiff.ts',
    line: /v1 configs with configId are rejected/,
  },
  {
    // The `connect` IPC command takes the PERSISTED connection id by contract
    // (naming.md §3), so `{ connectionId: config.id }` here is the correct
    // post-rename usage, not the reversed form the pattern guards against.
    file: 'e2e/specs/bugfix-admin-commands.ts',
    line: /invokeBackend<string>\('connect',\s*\{\s*connectionId:\s*config\.id/,
  },
  {
    // Backward-compat shim: tries `connectionId` first, falls back to
    // legacy `configId` IPC arg for older builds.
    file: 'e2e/helpers.ts',
    line: /configId/,
  },
];

/** Recursively collect scannable files under one root directory. */
function walk(dir, root, out) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return; // missing scan dir (optional clone dir etc.) — nothing to scan
  }
  for (const entry of entries) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (!SKIP_DIR_NAMES.has(entry.name)) walk(full, root, out);
      continue;
    }
    if (!SCAN_EXTENSIONS.has(entry.name.slice(entry.name.lastIndexOf('.')))) continue;
    const rel = relative(root, full);
    if (SKIP_FILES.has(rel.split('\\').join('/'))) continue;
    out.push(full);
  }
}

/**
 * @param {{
 *   root?: string,
 *   dirs?: string[],
 *   log?: (...args: unknown[]) => void,
 *   error?: (...args: unknown[]) => void,
 * }} [opts]
 * @returns {number} 0 if clean, 1 if any non-allow-listed hit was found
 */
export function checkIdTerminology(opts = {}) {
  const root = opts.root ?? ROOT;
  const dirs = opts.dirs ?? SCAN_DIRS;
  const log = opts.log ?? console.log.bind(console);
  const error = opts.error ?? console.error.bind(console);

  const files = [];
  for (const dir of dirs) walk(resolve(root, dir), root, files);

  const violations = [];
  let allowed = 0;
  for (const file of files) {
    const rel = relative(root, file).split('\\').join('/');
    const lines = readFileSync(file, 'utf-8').split('\n');
    lines.forEach((text, i) => {
      for (const { regex } of FORBIDDEN_PATTERNS) {
        if (!regex.test(text)) continue;
        const exempt = ALLOWLIST.some(
          (entry) => rel === entry.file && entry.line.test(text),
        );
        if (exempt) {
          allowed += 1;
        } else {
          violations.push({ file: rel, line: i + 1, text: text.trim(), reason: FORBIDDEN_PATTERNS.find((p) => p.regex === regex)?.reason ?? '' });
        }
        break;
      }
    });
  }

  if (allowed > 0) log(`[check-id-terminology] ${allowed} allow-listed occurrence(s) skipped`);
  if (violations.length > 0) {
    error(`[check-id-terminology] ${violations.length} violation(s):`);
    for (const v of violations) {
      error(`  ${v.file}:${v.line}: ${v.text}\n    -> ${v.reason}`);
    }
    error('See docs/architecture/naming.md for the ID terminology rules.');
    return 1;
  }
  log(`[check-id-terminology] ok (${files.length} files scanned)`);
  return 0;
}

/* istanbul ignore next */
if (process.argv[1] && process.argv[1].endsWith('check-id-terminology.mjs')) {
  process.exitCode = checkIdTerminology();
}
