#!/usr/bin/env node
/**
 * plugin-stash-precommit.mjs
 *
 * Shared pre-commit logic for restoring plugin-injected managed files.
 * Invoked by `.husky/pre-commit`. Safe to unit-test with an injectable root.
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

export const PLUGIN_ACL_IDS = ['kiwi', 'olap', 'superset'];

export function hasInjectedCargoContent(content) {
  if (!content) return false;
  const lines = content.split('\n');
  let inMarker = false;
  for (const line of lines) {
    if (/# <<plugin-/.test(line)) {
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

export function hasInjectedPluginInit(content) {
  if (!content) return false;
  return /^extern crate |^#\[cfg\(feature = "plugin-/m.test(content);
}

export function hasInjectedGeneratedTs(content) {
  if (!content) return false;
  const typeLine = content
    .split('\n')
    .find((l) => l.includes('PluginDatabaseType = '));
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
  if (relPath.endsWith('plugin_init.rs')) return hasInjectedPluginInit(content);
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
 * @returns {{ status: number, reason?: string, restored?: boolean }}
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

  let injectionDetected = false;
  for (const f of MANAGED_FILES) {
    const content = getContent(f);
    if (content && fileHasInjection(f, content)) {
      injectionDetected = true;
      break;
    }
  }

  const stashDirPresent = existsSync(stash.STASH_DIR);
  if (!injectionDetected && !stashDirPresent) {
    return { status: 0, restored: false };
  }

  log('[pre-commit] Restoring managed files from .plugin-file-stash/ ...');

  if (!stash.allStashed()) {
    const missing = stash.missingStashFiles();
    log(
      '[pre-commit] ERROR: injected/plugin-stash state detected but stash backups are missing.',
    );
    log(
      '[pre-commit] Cannot safely restore. Re-checkout clean managed files or recreate the stash.',
    );
    if (missing.length) {
      log(`[pre-commit] missing stash: ${missing.join(', ')}`);
    }
    return {
      status: 1,
      reason: 'stash-missing',
      missing,
      restored: false,
    };
  }

  try {
    stash.restoreManagedFiles();
  } catch (e) {
    log('[pre-commit] ERROR: stash restore failed');
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

  log('[pre-commit] Restored and re-staged managed plugin files.');
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
