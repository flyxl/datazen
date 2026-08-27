#!/usr/bin/env node
/**
 * with-driver-inject.mjs — resolve-drivers once, run command, restore stash.
 *
 * Intended as the single inject boundary for packaging (`tauri:build`, CI).
 * `pnpm build` / beforeBuildCommand must NOT call this — they only compile the
 * frontend against already-injected Cargo.toml / already-generated codegen.
 *
 * Nesting: set DATAZEN_DRIVER_INJECT_ACTIVE=1 on child processes. Inner
 * with-driver-inject sees that and skips resolve/restore. A leftover
 * `.driver-file-stash/` without that env is treated as orphaned and cleaned
 * before this wrapper takes ownership (avoids leaving Cargo.toml injected).
 *
 * Usage:
 *   node scripts/with-driver-inject.mjs [--drivers=...] -- <cmd> [args...]
 *   node scripts/with-driver-inject.mjs -- tauri build
 */

import { execSync, spawnSync } from 'child_process';
import { resolve, dirname } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';
import { stashExists } from './driver-file-stash.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

/** Child processes of an owning inject wrapper set this so nested wrappers skip. */
export const INJECT_ACTIVE_ENV = 'DATAZEN_DRIVER_INJECT_ACTIVE';

/**
 * Decide whether this wrapper owns stash lifecycle.
 * @param {{
 *   exists?: () => boolean,
 *   env?: NodeJS.ProcessEnv,
 * }} [opts]
 * @returns {{ ownStash: boolean, nested: boolean, orphanStash: boolean }}
 */
export function planDriverInjectLifecycle(opts = {}) {
  const exists = typeof opts === 'function' ? opts : (opts.exists ?? stashExists);
  const env = typeof opts === 'function' ? process.env : (opts.env ?? process.env);
  const markedNested = env[INJECT_ACTIVE_ENV] === '1';
  const stashPresent = exists();

  if (markedNested) {
    return { ownStash: false, nested: true, orphanStash: false };
  }
  if (stashPresent) {
    // Leftover from bare resolve-drivers / crashed wrapper — take ownership.
    return { ownStash: true, nested: false, orphanStash: true };
  }
  return { ownStash: true, nested: false, orphanStash: false };
}

/**
 * @param {{
 *   argv?: string[],
 *   root?: string,
 *   stashExistsFn?: () => boolean,
 *   env?: NodeJS.ProcessEnv,
 *   runResolve?: (args: string) => void,
 *   runRestore?: () => void,
 *   runCommand?: (cmd: string, args: string[], env: NodeJS.ProcessEnv) => { status: number | null },
 *   log?: (msg: string) => void,
 * }} [options]
 */
export function runWithDriverInject(options = {}) {
  const argv = options.argv ?? process.argv.slice(2);
  const root = options.root ?? ROOT;
  const existsFn = options.stashExistsFn ?? stashExists;
  const baseEnv = options.env ?? process.env;
  const log = options.log ?? console.log.bind(console);

  const sep = argv.indexOf('--');
  const ahead = sep === -1 ? argv : argv.slice(0, sep);
  const behind = sep === -1 ? [] : argv.slice(sep + 1);
  if (
    ahead.some((a) => a === '--plugins' || a.startsWith('--plugins=')) ||
    baseEnv.DATAZEN_PLUGINS
  ) {
    console.error(
      '[with-driver-inject] --plugins / DATAZEN_PLUGINS are no longer supported. Use --drivers=... or DATAZEN_DRIVERS.',
    );
    return { status: 1, ownStash: false, nested: false, orphanStash: false };
  }

  const driversArgs = ahead.filter((a) => a.startsWith('--drivers'));
  const resolveArgs = driversArgs.join(' ');

  const runResolve =
    options.runResolve ??
    ((args) => {
      execSync(`node scripts/resolve-drivers.mjs ${args}`, {
        cwd: root,
        stdio: 'inherit',
        env: baseEnv,
      });
    });

  const runRestore =
    options.runRestore ??
    (() => {
      try {
        execSync('node scripts/driver-file-stash.mjs restore', {
          cwd: root,
          stdio: 'inherit',
          env: baseEnv,
        });
      } catch {
        console.error('[with-driver-inject] stash restore failed');
      }
    });

  const runCommand =
    options.runCommand ??
    ((cmd, args, env) => {
      if (process.platform === 'win32' && cmd !== 'bash' && cmd !== 'sh') {
        return spawnSync(cmd, args, {
          cwd: root,
          stdio: 'inherit',
          shell: true,
          env,
        });
      }
      return spawnSync(cmd, args, {
        cwd: root,
        stdio: 'inherit',
        shell: false,
        env,
      });
    });

  const { ownStash, nested, orphanStash } = planDriverInjectLifecycle({
    exists: existsFn,
    env: baseEnv,
  });

  if (nested) {
    log(
      `[with-driver-inject] ${INJECT_ACTIVE_ENV}=1; skipping resolve/restore (nested)`,
    );
  } else if (orphanStash) {
    log(
      '[with-driver-inject] orphan .driver-file-stash/ detected; restoring before resolve',
    );
    runRestore();
  }

  if (ownStash) {
    runResolve(resolveArgs);
  }

  const childEnv = ownStash
    ? { ...baseEnv, [INJECT_ACTIVE_ENV]: '1' }
    : { ...baseEnv };

  if (behind.length === 0) {
    if (ownStash) runRestore();
    return { status: 0, ownStash, nested, orphanStash };
  }

  const result = runCommand(behind[0], behind.slice(1), childEnv);
  if (ownStash) runRestore();
  return {
    status: result.status ?? 1,
    ownStash,
    nested,
    orphanStash,
  };
}

function main() {
  const result = runWithDriverInject();
  process.exit(result.status);
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] ?? '')).href) {
  main();
}
