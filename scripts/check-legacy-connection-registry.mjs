#!/usr/bin/env node
/**
 * Legacy connection-registry guard.
 *
 * The P6 migration retired the old `ConnectionManager` (and its snake_case
 * Rust twin `connection_manager`): every connection/session path now goes
 * through the shared runtime (`packages/runtime` — SessionRegistry / Runtime),
 * and the old module is gone from the tree.
 *
 * A deletion has no mechanical proof of its own, so this guard is that proof:
 * a re-introduced reference in shipped source fails the build instead of
 * quietly reviving a second connection lifecycle next to the runtime one.
 *
 * Scope: `src/`, `packages/`, `src-tauri/`, `e2e/` — Rust included, because
 * the snake_case twin is the most likely re-introduction site. Prose is not
 * scanned: `docs/` and `site/` describe the migration itself and legitimately
 * name the old manager.
 */
import { readdirSync } from 'fs';
import { join, relative, resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import {
  SCAN_EXTENSIONS as SHARED_SCAN_EXTENSIONS,
  SKIP_DIR_NAMES,
  isSkippedPath,
  readScannedIfPresent,
} from './lib/scanTargets.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const SCAN_DIRS = ['src', 'packages', 'src-tauri', 'e2e'];
const SCAN_EXTENSIONS = new Set([...SHARED_SCAN_EXTENSIONS, '.rs']);

/**
 * Forbidden token shapes. Every entry carries the reason it is forbidden.
 */
const FORBIDDEN_PATTERNS = [
  {
    regex: /\bConnectionManager\b/,
    reason: 'retired legacy connection manager; use the runtime session path',
  },
  {
    regex: /\bconnection_manager\b/,
    reason: 'retired legacy connection manager (Rust twin); use the runtime session path',
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
      const relDir = relative(root, full).split('\\').join('/');
      if (isSkippedPath(relDir)) continue;
      if (!SKIP_DIR_NAMES.has(entry.name)) walk(full, root, out);
      continue;
    }
    if (!SCAN_EXTENSIONS.has(entry.name.slice(entry.name.lastIndexOf('.')))) continue;
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
 * @returns {number} 0 if clean, 1 if a retired-manager reference was found
 */
export function checkLegacyConnectionRegistry(opts = {}) {
  const root = opts.root ?? ROOT;
  const dirs = opts.dirs ?? SCAN_DIRS;
  const log = opts.log ?? console.log.bind(console);
  const error = opts.error ?? console.error.bind(console);

  const files = [];
  for (const dir of dirs) walk(resolve(root, dir), root, files);

  const violations = [];
  for (const file of files) {
    const rel = relative(root, file).split('\\').join('/');
    const content = readScannedIfPresent(file);
    if (content === null) continue;
    content.split('\n').forEach((text, i) => {
      for (const { regex, reason } of FORBIDDEN_PATTERNS) {
        if (!regex.test(text)) continue;
        violations.push({ file: rel, line: i + 1, text: text.trim(), reason });
        break;
      }
    });
  }

  if (violations.length > 0) {
    error(`[check-legacy-connection-registry] ${violations.length} violation(s):`);
    for (const v of violations) {
      error(`  ${v.file}:${v.line}: ${v.text}\n    -> ${v.reason}`);
    }
    error(
      'The legacy ConnectionManager was removed by the P6 runtime migration; ' +
        'route connection/session work through packages/runtime instead.',
    );
    return 1;
  }
  log(`[check-legacy-connection-registry] ok (${files.length} files scanned)`);
  return 0;
}

/* istanbul ignore next */
if (process.argv[1] && process.argv[1].endsWith('check-legacy-connection-registry.mjs')) {
  process.exitCode = checkLegacyConnectionRegistry();
}
