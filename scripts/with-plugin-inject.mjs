#!/usr/bin/env node
/**
 * with-plugin-inject.mjs — resolve-drivers once, run command, restore stash.
 *
 * Intended as the single inject boundary for packaging (`tauri:build`, CI).
 * `pnpm build` / beforeBuildCommand must NOT call this — they only compile the
 * frontend against already-injected managed files.
 *
 * Safety: if stash already exists, skip resolve/restore (nested / re-entrant).
 *
 * Usage:
 *   node scripts/with-plugin-inject.mjs [--drivers=...] -- <cmd> [args...]
 *   node scripts/with-plugin-inject.mjs -- tauri build
 */

import { execSync, spawnSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';
import { stashExists } from './plugin-file-stash.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

/**
 * Decide whether this wrapper owns stash lifecycle.
 * @param {() => boolean} [exists]
 * @returns {{ ownStash: boolean, nested: boolean }}
 */
export function planPluginInjectLifecycle(exists = stashExists) {
  const nested = exists();
  return { ownStash: !nested, nested };
}

/**
 * @param {{
 *   argv?: string[],
 *   root?: string,
 *   stashExistsFn?: () => boolean,
 *   runResolve?: (args: string) => void,
 *   runRestore?: () => void,
 *   runCommand?: (cmd: string, args: string[]) => { status: number | null },
 *   log?: (msg: string) => void,
 * }} [options]
 */
export function runWithPluginInject(options = {}) {
  const argv = options.argv ?? process.argv.slice(2);
  const root = options.root ?? ROOT;
  const existsFn = options.stashExistsFn ?? stashExists;
  const log = options.log ?? console.log.bind(console);

  const sep = argv.indexOf('--');
  const ahead = sep === -1 ? argv : argv.slice(0, sep);
  const behind = sep === -1 ? [] : argv.slice(sep + 1);
  if (
    ahead.some((a) => a === '--plugins' || a.startsWith('--plugins=')) ||
    process.env.DATAZEN_PLUGINS
  ) {
    console.error(
      '[with-plugin-inject] --plugins / DATAZEN_PLUGINS are no longer supported. Use --drivers=... or DATAZEN_DRIVERS.',
    );
    process.exit(1);
  }

  const driversArgs = ahead.filter((a) => a.startsWith('--drivers'));
  const resolveArgs = driversArgs.join(' ');

  const runResolve =
    options.runResolve ??
    ((args) => {
      execSync(`node scripts/resolve-drivers.mjs ${args}`, {
        cwd: root,
        stdio: 'inherit',
      });
    });

  const runRestore =
    options.runRestore ??
    (() => {
      try {
        execSync('node scripts/plugin-file-stash.mjs restore', {
          cwd: root,
          stdio: 'inherit',
        });
      } catch {
        console.error('[with-plugin-inject] stash restore failed');
      }
    });

  const runCommand =
    options.runCommand ??
    ((cmd, args) => {
      // On Windows, spawnSync(cmd, args, { shell: true }) mangles args (e.g. bash -c SCRIPT).
      // Prefer shell:false; fall back to a single command line only when needed for .cmd shims.
      if (process.platform === 'win32' && cmd !== 'bash' && cmd !== 'sh') {
        return spawnSync(cmd, args, {
          cwd: root,
          stdio: 'inherit',
          shell: true,
          env: process.env,
        });
      }
      return spawnSync(cmd, args, {
        cwd: root,
        stdio: 'inherit',
        shell: false,
        env: process.env,
      });
    });

  const { ownStash } = planPluginInjectLifecycle(existsFn);

  if (ownStash) {
    runResolve(resolveArgs);
  } else {
    log(
      '[with-plugin-inject] stash already present; skipping resolve/restore (nested)',
    );
  }

  if (behind.length === 0) {
    if (ownStash) runRestore();
    return { status: 0, ownStash, nested: !ownStash };
  }

  const result = runCommand(behind[0], behind.slice(1));
  if (ownStash) runRestore();
  return {
    status: result.status ?? 1,
    ownStash,
    nested: !ownStash,
  };
}

function main() {
  const result = runWithPluginInject();
  process.exit(result.status);
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] ?? '')).href) {
  main();
}
