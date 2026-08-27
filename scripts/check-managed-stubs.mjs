#!/usr/bin/env node
/**
 * Fail if git-tracked managed files look injected.
 * Run on a clean checkout (before resolve-drivers) so CI catches accidental
 * commits of Cargo.toml / capabilities plugin injection.
 *
 * Driver codegen (generated.ts / driver_init.rs) is gitignored and not checked.
 */
import { readFileSync, existsSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';
import { MANAGED_FILES } from './driver-file-stash.mjs';
import { fileHasInjection } from './plugin-stash-precommit.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * @param {{
 *   root?: string,
 *   files?: string[],
 *   log?: (...args: unknown[]) => void,
 *   error?: (...args: unknown[]) => void,
 * }} [opts]
 * @returns {number} 0 if clean, 1 if any file is missing or injected
 */
export function checkManagedStubs(opts = {}) {
  const root = opts.root ?? ROOT;
  const files = opts.files ?? MANAGED_FILES;
  const log = opts.log ?? console.log.bind(console);
  const error = opts.error ?? console.error.bind(console);

  let failed = false;
  for (const rel of files) {
    const path = resolve(root, rel);
    if (!existsSync(path)) {
      error(`[check-managed-stubs] missing ${rel}`);
      failed = true;
      continue;
    }
    const content = readFileSync(path, 'utf-8');
    if (fileHasInjection(rel, content)) {
      error(
        `[check-managed-stubs] ${rel} looks injected (plugin deps / ACL present).`,
      );
      error('  Restore with: node scripts/driver-file-stash.mjs restore');
      failed = true;
      continue;
    }
    log(`[check-managed-stubs] ok ${rel}`);
  }
  return failed ? 1 : 0;
}

function main() {
  process.exit(checkManagedStubs());
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] ?? '')).href) {
  main();
}
