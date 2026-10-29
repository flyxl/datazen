#!/usr/bin/env node
import { isDirectInvocation } from './lib/isDirectInvocation.mjs';
/**
 * driver-file-stash.mjs
 *
 * Backup / restore managed files that resolve-drivers injects at build/dev time.
 *
 * Flow (cp + atomic rename — working paths always remain):
 *   1. `stash`  — copyFileSync clean working files → .driver-file-stash/<path>
 *                 (working tree stays in place so editors/git keep seeing the files)
 *   2. resolve-drivers overwrites working paths with injected content
 *   3. `restore` — deinject cargo/capabilities (keep user edits); then remove
 *                 stash copies. Gitignored codegen files (generated.ts /
 *                 driver_init.rs) are left as-is.
 *
 * Stash freshness: `stash` also writes `.driver-file-stash/.stash-meta.json`
 * recording the git HEAD it was taken at. Because `.driver-file-stash/` is
 * gitignored it survives `git checkout -- .` / `git reset --hard`, so a stash
 * left behind by an earlier session can outlive the managed-file edits it was
 * taken from — reusing it silently rolls those edits back. `evaluateStashFreshness`
 * turns (stashHead, currentHead, changed managed files) into an explicit verdict;
 * resolve-drivers.mjs refuses to reuse a stash it cannot prove fresh. See
 * docs/development/worktree-isolation.md.
 *
 * Usage:
 *   node scripts/driver-file-stash.mjs stash
 *   node scripts/driver-file-stash.mjs restore
 *   node scripts/driver-file-stash.mjs status
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
import { fileURLToPath } from 'url';
import { randomBytes } from 'crypto';
import { execFileSync } from 'child_process';
import { deinjectManagedContent } from './driver-deinject.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
export const ROOT = resolve(__dirname, '..');

/**
 * Tracked files that resolve-drivers injects. Codegen files are gitignored.
 *
 * `Cargo.lock` is included because enabling the injected driver features makes
 * cargo rewrite the workspace lockfile during the build. It carries no
 * injection markers, so it is restored wholesale from the stash rather than
 * deinjected in place — see `deinjectManagedContent`.
 */
export const MANAGED_FILES = [
  'Cargo.toml',
  'apps/desktop/src-tauri/Cargo.toml',
  'Cargo.lock',
];

/**
 * Metadata sidecar written into `.driver-file-stash/` by `stashManagedFiles`.
 *
 * Deliberately NOT part of `MANAGED_FILES`: it is provenance, not a file whose
 * content is injected/restored, so it must not take part in the
 * `MANAGED_FILES.every(existsSync)` completeness checks. It still dies with the
 * rest of the directory because `cleanupStashDir` removes the whole tree.
 */
export const STASH_META_FILE = '.stash-meta.json';

/**
 * @typedef {object} StashFreshnessInput
 * @property {string | null} [stashHead] git HEAD recorded when the stash was created
 * @property {string | null} [currentHead] git HEAD right now
 * @property {string[]} [managedFilesChangedBetween] managed files git reports changed
 *   between `stashHead` and `currentHead`
 */

/**
 * @typedef {object} StashFreshness
 * @property {boolean} stale whether reusing the stash would be unsafe
 * @property {string} reason machine-readable reason code
 * @property {string[]} changed managed files that changed since the stash was taken
 */

/**
 * Decide whether an existing `.driver-file-stash/` may be reused.
 *
 * Pure: no fs, no git, no logging. Both git answers arrive as arguments, so
 * the whole rule matrix is unit-testable without a repository, matching the
 * existing pure-fs fixture style. `gitHeadAt` / `gitManagedFilesChangedBetween`
 * below are the only git-touching code here; `resolve-drivers.mjs` owns when
 * they are called and what a `stale` verdict does.
 *
 * The default is deliberately conservative — an existing stash is reused only
 * when its content is *provably* still the content the managed files are
 * supposed to have:
 *
 *  - `missing-metadata`       — no usable metadata (legacy pre-fix stash): stale.
 *  - `unknown-current-head`    — no current HEAD (not a git work tree): stale.
 *  - `same-head`              — created at the current HEAD: fresh.
 *  - `managed-files-unchanged`— HEAD moved but no managed file did: fresh.
 *  - `managed-files-changed`  — HEAD moved *and* managed files did: stale.
 *
 * @param {StashFreshnessInput} input
 * @returns {StashFreshness}
 */
export function evaluateStashFreshness(input) {
  const stashHead = input?.stashHead ?? null;
  const currentHead = input?.currentHead ?? null;
  const changed = Array.isArray(input?.managedFilesChangedBetween)
    ? [...input.managedFilesChangedBetween]
    : [];

  if (!stashHead) {
    return { stale: true, reason: 'missing-metadata', changed: [] };
  }
  if (!currentHead) {
    return { stale: true, reason: 'unknown-current-head', changed: [] };
  }
  if (stashHead === currentHead) {
    return { stale: false, reason: 'same-head', changed: [] };
  }
  if (changed.length === 0) {
    return { stale: false, reason: 'managed-files-unchanged', changed: [] };
  }
  return { stale: true, reason: 'managed-files-changed', changed };
}

const SHA_RE = /^[0-9a-f]{7,40}$/;

/**
 * Current git HEAD of `root`, or null when it cannot be determined.
 * @param {string} root
 * @returns {string | null}
 */
export function gitHeadAt(root) {
  try {
    const out = execFileSync('git', ['rev-parse', 'HEAD'], {
      cwd: root,
      encoding: 'utf-8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const sha = out.trim();
    return SHA_RE.test(sha) ? sha : null;
  } catch {
    return null;
  }
}

/**
 * Managed files git reports as changed between two commits.
 *
 * Only `MANAGED_FILES` are considered: other tree changes cannot make a stash
 * copy wrong, because the stash content is compared against the current
 * *managed files*, not against the commit. Returns an empty list when the
 * query cannot run, which the caller must treat as "unchanged" only in
 * combination with the other freshness rules — never as proof of freshness on
 * its own.
 *
 * @param {string} root
 * @param {string} fromSha
 * @param {string} toSha
 * @returns {string[]} repo-relative POSIX paths, restricted to MANAGED_FILES
 */
export function gitManagedFilesChangedBetween(root, fromSha, toSha) {
  if (!SHA_RE.test(fromSha ?? '') || !SHA_RE.test(toSha ?? '')) return [];
  let out = '';
  try {
    out = execFileSync('git', ['diff', '--name-only', fromSha, toSha, '--', ...MANAGED_FILES], {
      cwd: root,
      encoding: 'utf-8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
  } catch {
    return [];
  }
  const managed = new Set(MANAGED_FILES);
  return out
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l.length > 0 && managed.has(l));
}

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
 * @param {{
 *   quiet?: boolean,
 *   resolveHead?: () => string | null,
 *   listManagedFilesChanged?: (fromSha: string, toSha: string) => string[],
 * }} [options]
 */
export function createDriverFileStash(root, options = {}) {
  const quiet = Boolean(options.quiet);
  const STASH_DIR = resolve(root, '.driver-file-stash');
  const resolveHead = options.resolveHead ?? (() => gitHeadAt(root));
  const listManagedFilesChanged =
    options.listManagedFilesChanged ?? ((fromSha, toSha) => gitManagedFilesChangedBetween(root, fromSha, toSha));

  function metaPath() {
    return resolve(STASH_DIR, STASH_META_FILE);
  }

  /**
   * Record which HEAD this stash was taken at.
   * @param {string | null} [head]
   */
  function writeStashMeta(head = resolveHead()) {
    mkdirSync(STASH_DIR, { recursive: true });
    const payload = {
      head: typeof head === 'string' && SHA_RE.test(head) ? head : null,
      createdAt: new Date().toISOString(),
    };
    writeFileSync(metaPath(), JSON.stringify(payload, null, 2) + '\n');
    return payload;
  }

  /**
   * Read the metadata sidecar. Returns null when absent, unreadable or not a
   * usable record — the caller must then treat freshness as unprovable.
   * @returns {{ head: string | null, createdAt: string | null } | null}
   */
  function readStashMeta() {
    const p = metaPath();
    if (!existsSync(p)) return null;
    try {
      const parsed = JSON.parse(readFileSync(p, 'utf-8'));
      if (!parsed || typeof parsed !== 'object') return null;
      const head = typeof parsed.head === 'string' && SHA_RE.test(parsed.head) ? parsed.head : null;
      const createdAt = typeof parsed.createdAt === 'string' ? parsed.createdAt : null;
      return { head, createdAt };
    } catch {
      return null;
    }
  }

  /**
   * Assess an existing stash against the current git state.
   * Read-only: never mutates.
   */
  function inspectFreshness() {
    const meta = readStashMeta();
    const stashHead = meta?.head ?? null;
    const currentHead = resolveHead();
    const changed =
      stashHead && currentHead && stashHead !== currentHead
        ? listManagedFilesChanged(stashHead, currentHead)
        : [];
    const verdict = evaluateStashFreshness({
      stashHead,
      currentHead,
      managedFilesChangedBetween: changed,
    });
    return {
      ...verdict,
      stashHead,
      currentHead,
      createdAt: meta?.createdAt ?? null,
    };
  }

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
        `[driver-file-stash] stash already exists for: ${existing.join(', ')}. ` +
          `Run \`node scripts/driver-file-stash.mjs restore\` first.`,
      );
    }

    const missing = missingWorkFiles();
    if (missing.length > 0) {
      throw new Error(
        `[driver-file-stash] cannot stash; missing working files: ${missing.join(', ')}`,
      );
    }

    for (const f of MANAGED_FILES) {
      const src = workPath(f);
      const dst = stashPath(f);
      mkdirSync(dirname(dst), { recursive: true });
      copyFileSync(src, dst);
    }

    const meta = writeStashMeta();

    if (!quiet) {
      console.log(
        `[driver-file-stash] copied ${MANAGED_FILES.length} file(s) → ${relative(root, STASH_DIR)}/ ` +
          `(head ${meta.head ?? 'unknown'})`,
      );
    }
  }

  /**
   * Write deinjected content to the working path.
   * Cargo / capabilities: strip injection, keep user edits.
   * @param {string} relPath
   */
  function restoreOneManagedFile(relPath) {
    const work = workPath(relPath);
    const stashed = stashPath(relPath);
    const hasStash = existsSync(stashed);
    const hasWork = existsSync(work);

    if (!hasWork && hasStash) {
      atomicReplaceWithCopy(stashed, work);
      return;
    }
    if (!hasWork) {
      throw new Error(
        `[driver-file-stash] cannot restore; missing work and stash for: ${relPath}`,
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
   * Restore tracked managed files: strip injection from work (keep user edits).
   * Gitignored codegen files are not touched.
   */
  function restoreManagedFiles() {
    const blocking = missingStashFiles().filter((f) => !existsSync(workPath(f)));
    if (blocking.length > 0) {
      throw new Error(
        `[driver-file-stash] cannot restore; stash missing for: ${blocking.join(', ')}. ` +
          `Injected files may be dirty — restore the clean versions manually.`,
      );
    }

    for (const f of MANAGED_FILES) {
      restoreOneManagedFile(f);
    }

    cleanupStashDir();
    if (!quiet) {
      console.log(
        `[driver-file-stash] restored ${MANAGED_FILES.length} file(s) (deinject + stash)`,
      );
    }
  }

  /**
   * Restore only the given tracked paths: deinject cargo/capabilities.
   * @param {string[]} relPaths
   */
  function restoreSelectedFiles(relPaths) {
    const missingOptional = relPaths.filter(
      (f) => !existsSync(workPath(f)) && !existsSync(stashPath(f)),
    );
    if (missingOptional.length > 0) {
      throw new Error(
        `[driver-file-stash] cannot restore selection; stash missing for: ${missingOptional.join(', ')}`,
      );
    }

    for (const f of relPaths) {
      restoreOneManagedFile(f);
    }

    cleanupStashDir();
    if (!quiet) {
      console.log(
        `[driver-file-stash] restored ${relPaths.length} injected file(s) via deinject; discarded remaining stash`,
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
    if (allStashed()) {
      const f = inspectFreshness();
      console.log(`  freshness: ${f.stale ? 'STALE' : 'fresh'} (reason=${f.reason})`);
      console.log(`  stash head: ${f.stashHead ?? 'unknown'} createdAt: ${f.createdAt ?? 'unknown'}`);
      console.log(`  current head: ${f.currentHead ?? 'unknown'}`);
      if (f.changed.length > 0) {
        console.log(`  managed files changed since stash: ${f.changed.join(', ')}`);
      }
    } else {
      console.log('  freshness: n/a (no complete stash present)');
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
    metaPath,
    readStashMeta,
    writeStashMeta,
    inspectFreshness,
  };
}

const defaultApi = createDriverFileStash(ROOT);

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
export const metaPath = defaultApi.metaPath;
export const readStashMeta = defaultApi.readStashMeta;
export const writeStashMeta = defaultApi.writeStashMeta;
export const inspectFreshness = defaultApi.inspectFreshness;

/**
 * @param {string[]} [argv]
 * @param {{
 *   api?: ReturnType<typeof createDriverFileStash>,
 *   error?: (...args: unknown[]) => void,
 * }} [options]
 * @returns {number}
 */
export function runDriverFileStashCli(argv = process.argv.slice(2), options = {}) {
  const api = options.api ?? defaultApi;
  const error = options.error ?? console.error.bind(console);
  const cmd = argv[0];
  try {
    if (cmd === 'stash') {
      api.stashManagedFiles();
    } else if (cmd === 'restore') {
      api.restoreManagedFiles();
    } else if (cmd === 'status') {
      api.printStatus();
    } else {
      error('Usage: node scripts/driver-file-stash.mjs <stash|restore|status>');
      return 1;
    }
    return 0;
  } catch (e) {
    error(e instanceof Error ? e.message : e);
    return 1;
  }
}

function main() {
  process.exit(runDriverFileStashCli());
}

if (isDirectInvocation(import.meta.url)) {
  main();
}
