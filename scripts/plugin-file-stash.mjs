#!/usr/bin/env node
/**
 * plugin-file-stash.mjs
 *
 * Stash / restore managed files that resolve-plugins injects at build/dev time.
 *
 * Flow:
 *   1. `stash`  — rename clean working files → .plugin-file-stash/<path>
 *   2. resolve-plugins writes injected content at the original paths
 *   3. `restore` — rename stash back over working paths (errors if any stash missing)
 *
 * Usage:
 *   node scripts/plugin-file-stash.mjs stash
 *   node scripts/plugin-file-stash.mjs restore
 *   node scripts/plugin-file-stash.mjs status
 */

import {
  existsSync,
  mkdirSync,
  renameSync,
  unlinkSync,
  rmSync,
} from 'fs';
import { resolve, dirname, relative } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
export const ROOT = resolve(__dirname, '..');

/** Paths relative to repo root — keep in sync with resolve-plugins / pre-commit. */
export const MANAGED_FILES = [
  'Cargo.toml',
  'src-tauri/Cargo.toml',
  'src-tauri/src/plugin_init.rs',
  'src/plugins/generated.ts',
  'src-tauri/capabilities/default.json',
];

/**
 * @param {string} root
 * @param {{ quiet?: boolean }} [options]
 */
export function createPluginFileStash(root, options = {}) {
  const quiet = Boolean(options.quiet);
  const STASH_DIR = resolve(root, '.plugin-file-stash');

  function stashPath(relPath) {
    return resolve(STASH_DIR, relPath);
  }

  function workPath(relPath) {
    return resolve(root, relPath);
  }

  function managedReadPath(relPath) {
    const s = stashPath(relPath);
    if (existsSync(s)) return s;
    return workPath(relPath);
  }

  function stashExists() {
    return MANAGED_FILES.some((f) => existsSync(stashPath(f)));
  }

  function allStashed() {
    return MANAGED_FILES.every((f) => existsSync(stashPath(f)));
  }

  function missingStashFiles() {
    return MANAGED_FILES.filter((f) => !existsSync(stashPath(f)));
  }

  function missingWorkFiles() {
    return MANAGED_FILES.filter((f) => !existsSync(workPath(f)));
  }

  function cleanupStashDir() {
    if (!existsSync(STASH_DIR)) return;
    rmSync(STASH_DIR, { recursive: true, force: true });
  }

  function stashManagedFiles() {
    const existing = MANAGED_FILES.filter((f) => existsSync(stashPath(f)));
    if (existing.length > 0) {
      throw new Error(
        `[plugin-file-stash] stash already exists for: ${existing.join(', ')}. ` +
          `Run \`node scripts/plugin-file-stash.mjs restore\` first.`,
      );
    }

    const missing = missingWorkFiles();
    if (missing.length > 0) {
      throw new Error(
        `[plugin-file-stash] cannot stash; missing working files: ${missing.join(', ')}`,
      );
    }

    for (const f of MANAGED_FILES) {
      const src = workPath(f);
      const dst = stashPath(f);
      mkdirSync(dirname(dst), { recursive: true });
      renameSync(src, dst);
    }

    if (!quiet) {
      console.log(
        `[plugin-file-stash] stashed ${MANAGED_FILES.length} file(s) → ${relative(root, STASH_DIR)}/`,
      );
    }
  }

  function restoreManagedFiles() {
    const missing = missingStashFiles();
    if (missing.length > 0) {
      throw new Error(
        `[plugin-file-stash] cannot restore; stash missing for: ${missing.join(', ')}. ` +
          `Injected files may be dirty — restore the clean versions manually.`,
      );
    }

    for (const f of MANAGED_FILES) {
      const src = stashPath(f);
      const dst = workPath(f);
      mkdirSync(dirname(dst), { recursive: true });
      if (existsSync(dst)) {
        unlinkSync(dst);
      }
      renameSync(src, dst);
    }

    cleanupStashDir();
    if (!quiet) {
      console.log(`[plugin-file-stash] restored ${MANAGED_FILES.length} file(s) from stash`);
    }
  }

  function printStatus() {
    console.log(`stash dir: ${STASH_DIR}`);
    for (const f of MANAGED_FILES) {
      const s = existsSync(stashPath(f));
      const w = existsSync(workPath(f));
      console.log(`  ${f}: stash=${s ? 'yes' : 'no'} work=${w ? 'yes' : 'no'}`);
    }
  }

  return {
    root,
    STASH_DIR,
    MANAGED_FILES,
    stashPath,
    workPath,
    managedReadPath,
    stashExists,
    allStashed,
    missingStashFiles,
    missingWorkFiles,
    stashManagedFiles,
    restoreManagedFiles,
    printStatus,
    cleanupStashDir,
  };
}

const defaultApi = createPluginFileStash(ROOT);

export const STASH_DIR = defaultApi.STASH_DIR;
export const stashPath = defaultApi.stashPath;
export const workPath = defaultApi.workPath;
export const managedReadPath = defaultApi.managedReadPath;
export const stashExists = defaultApi.stashExists;
export const allStashed = defaultApi.allStashed;
export const missingStashFiles = defaultApi.missingStashFiles;
export const stashManagedFiles = defaultApi.stashManagedFiles;
export const restoreManagedFiles = defaultApi.restoreManagedFiles;

function main() {
  const cmd = process.argv[2];
  try {
    if (cmd === 'stash') {
      stashManagedFiles();
    } else if (cmd === 'restore') {
      restoreManagedFiles();
    } else if (cmd === 'status') {
      defaultApi.printStatus();
    } else {
      console.error('Usage: node scripts/plugin-file-stash.mjs <stash|restore|status>');
      process.exit(1);
    }
  } catch (e) {
    console.error(e instanceof Error ? e.message : e);
    process.exit(1);
  }
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] ?? '')).href) {
  main();
}
