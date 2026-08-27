#!/usr/bin/env node
/**
 * plugin-stash-precommit.mjs
 *
 * Shared pre-commit logic for restoring plugin-injected managed files.
 * Invoked by `.husky/pre-commit`. Safe to unit-test with an injectable root.
 *
 * Strategy:
 *   - Detect tracked files that still carry injection (plugin compile ran).
 *   - For those files only: strip injection while keeping user edits
 *     (e.g. new window labels in capabilities).
 *   - Gitignored codegen (generated.ts / driver_init.rs) is left as-is.
 *
 * Exit codes:
 *   0 — clean / restored successfully
 *   1 — restore required but stash missing, or restore left injection behind
 */

import { existsSync, readFileSync } from 'fs';
import { resolve } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';
import { execSync } from 'child_process';
import {
  createPluginFileStash,
  MANAGED_FILES,
  ROOT,
} from './plugin-file-stash.mjs';
import { PLUGIN_ACL_IDS } from './plugin-deinject.mjs';

export { PLUGIN_ACL_IDS };

export function hasInjectedCargoContent(content) {
  if (!content) return false;
  const lines = content.split('\n');
  let inMarker = false;
  for (const line of lines) {
    if (/# <<driver-/.test(line)) {
      inMarker = true;
      continue;
    }
    if (/# <<\//.test(line)) {
      inMarker = false;
      continue;
    }
    if (inMarker && line.trim() && !line.trim().startsWith('#')) {
      return true;
    }
  }
  return false;
}

export function hasInjectedDriverInit(content) {
  if (!content) return false;
  return /^extern crate |^#\[cfg\(feature = "driver-/m.test(content);
}

export function hasInjectedGeneratedTs(content) {
  if (!content) return false;
  // Prefer DatabaseType (current stub/generated). Fall back to legacy PluginDatabaseType.
  const lines = content.split('\n');
  const typeLine =
    lines.find((l) => /^export type DatabaseType = /.test(l)) ||
    lines.find((l) => l.includes('PluginDatabaseType = '));
  if (!typeLine) return false;
  return !/=\s*never\b/.test(typeLine);
}

export function hasInjectedGeneratedLocales(content) {
  if (!content) return false;
  if (/from '\.\.\/\.\.\/packages\/drivers\//.test(content)) return true;
  const typeLine = content.split('\n').find((l) => /^export type PluginTranslationKey = /.test(l));
  if (!typeLine) return false;
  return !/=\s*never\b/.test(typeLine);
}

export function hasInjectedCapabilities(content, pluginIds = PLUGIN_ACL_IDS) {
  if (!content) return false;
  const re = new RegExp(`"(${pluginIds.map(escapeRegex).join('|')}):`);
  return re.test(content);
}

function escapeRegex(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * @param {string} relPath
 * @param {string} content
 * @param {string[]} [pluginIds]
 */
export function fileHasInjection(relPath, content, pluginIds = PLUGIN_ACL_IDS) {
  if (relPath.endsWith('Cargo.toml')) return hasInjectedCargoContent(content);
  if (relPath.endsWith('driver_init.rs')) return hasInjectedDriverInit(content);
  if (relPath.endsWith('generated-locales.ts')) return hasInjectedGeneratedLocales(content);
  if (relPath.endsWith('generated.ts')) return hasInjectedGeneratedTs(content);
  if (relPath.endsWith('capabilities/default.json')) {
    return hasInjectedCapabilities(content, pluginIds);
  }
  return false;
}

/**
 * @param {{
 *   root?: string,
 *   quiet?: boolean,
 *   getContent?: (relPath: string) => string | null,
 *   isStaged?: (relPath: string) => boolean,
 *   restage?: (relPath: string) => void,
 *   log?: (...args: unknown[]) => void,
 * }} [opts]
 * @returns {{ status: number, reason?: string, restored?: boolean, discardedStash?: boolean }}
 */
export function runPluginStashPrecommit(opts = {}) {
  const root = opts.root ?? ROOT;
  const log = opts.log ?? console.log;
  const stash = createPluginFileStash(root, { quiet: opts.quiet ?? false });

  const isStaged =
    opts.isStaged ??
    ((relPath) => {
      try {
        const out = execSync('git diff --cached --name-only', {
          cwd: root,
          encoding: 'utf-8',
        });
        return out.split('\n').includes(relPath);
      } catch {
        return false;
      }
    });

  const getContent =
    opts.getContent ??
    ((relPath) => {
      if (isStaged(relPath)) {
        try {
          return execSync(`git show :${relPath}`, {
            cwd: root,
            encoding: 'utf-8',
          });
        } catch {
          /* fall through */
        }
      }
      const p = stash.workPath(relPath);
      if (!existsSync(p)) return null;
      return readFileSync(p, 'utf-8');
    });

  const restage =
    opts.restage ??
    ((relPath) => {
      execSync(`git add -- ${relPath}`, { cwd: root, stdio: 'pipe' });
    });

  const injectedFiles = [];
  for (const f of MANAGED_FILES) {
    const content = getContent(f);
    if (content && fileHasInjection(f, content)) {
      injectedFiles.push(f);
    }
  }

  const stashDirPresent = existsSync(stash.STASH_DIR);
  if (injectedFiles.length === 0 && !stashDirPresent) {
    return { status: 0, restored: false };
  }

  if (injectedFiles.length === 0 && stashDirPresent) {
    // Stash leftover from a crashed/nested build, but working tree is clean
    // (or only has legitimate non-injection edits). Drop stash; do NOT overwrite.
    log(
      '[pre-commit] Stash present but no injected managed files — discarding stash, keeping working tree',
    );
    stash.cleanupStashDir();
    return { status: 0, restored: false, discardedStash: true };
  }

  log(
    `[pre-commit] Deinjecting ${injectedFiles.length} managed file(s) (keep user edits, strip plugin injection)...`,
  );

  try {
    stash.restoreSelectedFiles(injectedFiles);
  } catch (e) {
    log('[pre-commit] ERROR: deinject/restore failed');
    log(e instanceof Error ? e.message : e);
    return { status: 1, reason: 'restore-failed', restored: false };
  }

  for (const f of MANAGED_FILES) {
    if (isStaged(f)) {
      restage(f);
    }
  }

  for (const f of MANAGED_FILES) {
    const content = getContent(f);
    if (content && fileHasInjection(f, content)) {
      log('[pre-commit] ERROR: managed files still look injected after restore');
      return { status: 1, reason: 'still-injected', restored: true };
    }
  }

  if (existsSync(stash.STASH_DIR)) {
    log('[pre-commit] ERROR: .plugin-file-stash/ still present after restore');
    return { status: 1, reason: 'stash-dir-remains', restored: true };
  }

  log(
    '[pre-commit] Restored injected files: user edits kept, plugin injection removed.',
  );
  return { status: 0, restored: true };
}

function main() {
  const result = runPluginStashPrecommit({ root: ROOT });
  process.exit(result.status);
}

const __filename = fileURLToPath(import.meta.url);
if (import.meta.url === pathToFileURL(resolve(process.argv[1] ?? '')).href) {
  main();
}

void __filename;
