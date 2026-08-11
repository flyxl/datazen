#!/usr/bin/env node
/**
 * plugin-file-stash.mjs
 *
 * Backup / restore managed files that resolve-drivers injects at build/dev time.
 *
 * Flow (cp + atomic rename — working paths always remain):
 *   1. `stash`  — copyFileSync clean working files → .plugin-file-stash/<path>
 *                 (working tree stays in place so editors/git keep seeing the files)
 *   2. resolve-drivers overwrites working paths with injected content
 *   3. `restore` — deinject cargo/capabilities (keep user edits); stash-restore
 *                 fully-generated files; then remove stash copies
 *
 * Usage:
 *   node scripts/plugin-file-stash.mjs stash
 *   node scripts/plugin-file-stash.mjs restore
 *   node scripts/plugin-file-stash.mjs status
 */

import {
  existsSync,
  mkdirSync,
  copyFileSync,
  renameSync,
  unlinkSync,
  rmSync,
  readFileSync,
  writeFileSync,
} from 'fs';
import { resolve, dirname, relative, basename } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';
import { randomBytes } from 'crypto';
import {
  cleanFullyGeneratedContent,
  deinjectManagedContent,
  isFullyGeneratedManagedFile,
} from './plugin-deinject.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
export const ROOT = resolve(__dirname, '..');

/** Paths relative to repo root — keep in sync with resolve-drivers / pre-commit. */
export const MANAGED_FILES = [
  'Cargo.toml',
  'src-tauri/Cargo.toml',
  'src-tauri/src/plugin_init.rs',
  'src/plugins/generated.ts',
  'src/plugins/generated-locales.ts',
  'src-tauri/capabilities/default.json',
];

/**
 * Atomically replace `dest` with contents of `src` via temp file + rename.
 * Works across the common case where src/dest are on the same volume.
 * @param {string} src
 * @param {string} dest
 */
export function atomicReplaceWithCopy(src, dest) {
  mkdirSync(dirname(dest), { recursive: true });
  const token = randomBytes(6).toString('hex');
  const tmp = resolve(dirname(dest), `.${basename(dest)}.${token}.tmp`);
  try {
    copyFileSync(src, tmp);
    renameSync(tmp, dest);
  } catch (e) {
    try {
      if (existsSync(tmp)) unlinkSync(tmp);
    } catch {
      /* ignore */
    }
    throw e;
  }
}

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

  /**
   * Copy clean working files into the stash dir. Working paths stay put.
   */
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
      copyFileSync(src, dst);
    }

    if (!quiet) {
      console.log(
        `[plugin-file-stash] copied ${MANAGED_FILES.length} file(s) → ${relative(root, STASH_DIR)}/`,
      );
    }
  }

  /**
   * Write deinjected (or stash baseline) content to the working path.
   * Cargo / capabilities: strip injection, keep user edits.
   * Fully generated files: replace from stash.
   * @param {string} relPath
   */
  function restoreOneManagedFile(relPath) {
    const work = workPath(relPath);
    const stashed = stashPath(relPath);
    const hasStash = existsSync(stashed);
    const hasWork = existsSync(work);

    if (isFullyGeneratedManagedFile(relPath)) {
      if (hasStash) {
        atomicReplaceWithCopy(stashed, work);
        return;
      }
      // Stash missing (e.g. injected generated.ts was committed): write git-safe stub.
      const token = randomBytes(6).toString('hex');
      const tmp = resolve(dirname(work), `.${basename(work)}.${token}.tmp`);
      try {
        writeFileSync(tmp, cleanFullyGeneratedContent(relPath));
        renameSync(tmp, work);
      } catch (e) {
        try {
          if (existsSync(tmp)) unlinkSync(tmp);
        } catch {
          /* ignore */
        }
        throw e;
      }
      return;
    }

    if (!hasWork && hasStash) {
      atomicReplaceWithCopy(stashed, work);
      return;
    }
    if (!hasWork) {
      throw new Error(
        `[plugin-file-stash] cannot restore; missing work and stash for: ${relPath}`,
      );
    }

    const workContent = readFileSync(work, 'utf-8');
    const next = deinjectManagedContent(relPath, workContent, {
      stashContent: hasStash ? readFileSync(stashed, 'utf-8') : null,
    });
    if (next === workContent) return;
    const token = randomBytes(6).toString('hex');
    const tmp = resolve(dirname(work), `.${basename(work)}.${token}.tmp`);
    try {
      writeFileSync(tmp, next);
      renameSync(tmp, work);
    } catch (e) {
      try {
        if (existsSync(tmp)) unlinkSync(tmp);
      } catch {
        /* ignore */
      }
      throw e;
    }
  }

  /**
   * Restore managed files: strip injection from work (keep user edits),
   * restore fully-generated files from stash (or canonical stub if stash missing).
   */
  function restoreManagedFiles() {
    const missingNonGenerated = missingStashFiles().filter(
      (f) => !isFullyGeneratedManagedFile(f),
    );
    // Cargo/capabilities can deinject without stash when work copies exist.
    const blocking = missingNonGenerated.filter(
      (f) => !existsSync(workPath(f)),
    );
    if (blocking.length > 0) {
      throw new Error(
        `[plugin-file-stash] cannot restore; stash missing for: ${blocking.join(', ')}. ` +
          `Injected files may be dirty — restore the clean versions manually.`,
      );
    }

    for (const f of MANAGED_FILES) {
      restoreOneManagedFile(f);
    }

    cleanupStashDir();
    if (!quiet) {
      console.log(
        `[plugin-file-stash] restored ${MANAGED_FILES.length} file(s) (deinject + stash)`,
      );
    }
  }

  /**
   * Restore only injected paths: deinject cargo/capabilities (keep user edits),
   * stash-restore fully generated files (canonical stub if stash missing).
   * @param {string[]} relPaths
   */
  function restoreSelectedFiles(relPaths) {
    const missingOptional = relPaths.filter(
      (f) =>
        !isFullyGeneratedManagedFile(f) &&
        !existsSync(workPath(f)) &&
        !existsSync(stashPath(f)),
    );
    if (missingOptional.length > 0) {
      throw new Error(
        `[plugin-file-stash] cannot restore selection; stash missing for: ${missingOptional.join(', ')}`,
      );
    }

    for (const f of relPaths) {
      restoreOneManagedFile(f);
    }

    cleanupStashDir();
    if (!quiet) {
      console.log(
        `[plugin-file-stash] restored ${relPaths.length} injected file(s) via deinject; discarded remaining stash`,
      );
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
    restoreSelectedFiles,
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
export const restoreSelectedFiles = defaultApi.restoreSelectedFiles;

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
