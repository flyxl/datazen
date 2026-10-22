#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDirectInvocation } from './lib/isDirectInvocation.mjs';

/**
 * typecheck.mjs — the repository's typecheck gate, run under BOTH compilers.
 *
 * WHY TWO COMPILERS
 *
 * The repository is on TypeScript 7. Two implementations of that compiler are
 * installed, and they are not the same thing:
 *
 *   tsc    — `typescript` (npm `typescript@7.0.2`), the official Go port.
 *            This is the AUTHORITATIVE gate: it is the version that ships as
 *            `latest`, and the release pipeline's `union-typecheck` job runs
 *            exactly this binary (scripts/ci-union-typecheck.mjs). A PR that
 *            passes here cannot fail the release gate for a type error.
 *
 *   tsrs   — `tsrs` (npm `tsrs@0.18.0-ts7.1.0-dev.20260929`), the Rust port
 *            by oxc-project. It tracks the 7.1 development line, i.e. it is
 *            AHEAD of the released compiler, and it is roughly 2x faster and
 *            uses about a third of the memory on this repository.
 *
 * Running both is what makes the second one safe to keep: tsrs is a second
 * independent opinion on the same source, and it surfaces the next TypeScript
 * release's diagnostics before that release ships. It is NOT the authority —
 * `tsc` is — so a tsrs-only diagnostic can never turn a green release gate
 * red, and a tsc diagnostic can never be silently dropped by tsrs being
 * "more permissive".
 *
 * MEASURED (this repository, `main` at the time of writing, 1845 TS files)
 *
 *   tsc    5.9.3   24.8-27.0 s for the three programs   1.31 GB peak RSS
 *   tsc    7.0.2    2.4-2.6 s for the three programs   1.25 GB peak RSS
 *   tsrs   0.18.0   1.0-1.3 s for the three programs   0.35 GB peak RSS
 *
 * The 10x drop from 5.9.3 to 7.0.2 is the compiler rewrite, not this script.
 * tsrs's remaining 2x is why it is worth a second process; it is NOT worth
 * replacing the authoritative compiler with.
 *
 * PLATFORM
 *
 * tsrs 0.18.0 ships no win32 binary (the upstream project lists it as
 * "configured for the next release"). The gate therefore degrades to the
 * `tsc` engine alone on Windows rather than failing: the release matrix
 * builds and signs Windows artifacts, and a typecheck gate that cannot run
 * on the platform it ships to is a worse gate. The skip is announced on
 * stdout, never silent.
 *
 * The same probe makes the gate self-healing: when tsrs publishes a Windows
 * build, the next `pnpm install` picks it up and the tsrs engine starts
 * running without any change to this file.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * The programs the gate covers. These are exactly the three that
 * `pnpm typecheck` has always gated on — the root program, the scripts test
 * program, and the EP packaging program. Nothing else in the repository is
 * a gate: `tsconfig.scripts-checkjs.json` is an explicit backlog ("HONEST
 * BACKLOG, NOT A GATE"), `tsconfig.node.json` covers vite/tailwind configs
 * that no program builds, and the per-package `packages/<id>/tsconfig.json`
 * files are
 * editor-facing programs the root one supersedes.
 */
export const TYPECHECK_PROGRAMS = [
  { id: 'root', config: '.' },
  { id: 'scripts', config: 'tsconfig.scripts.json' },
  { id: 'pack-ep', config: 'tsconfig.pack-ep.json' },
];

/**
 * The compilers. `bin` is resolved through `process.execPath` rather than a
 * package-manager shim, so the command is byte-identical on Windows, macOS
 * and Linux — the same reasoning as scripts/ci-union-typecheck.mjs.
 */
export const TYPECHECK_ENGINES = [
  { id: 'tsc', bin: join('node_modules', 'typescript', 'bin', 'tsc'), pkg: 'typescript' },
  { id: 'tsrs', bin: join('node_modules', 'tsrs', 'bin', 'tsrs'), pkg: 'tsrs' },
];

/** The engine ids this script accepts, in the order they run. */
export const ENGINE_ORDER = ['tsc', 'tsrs'];

/**
 * Parse `--engine=tsc|tsrs|both`. Unknown values are an error rather than a
 * silent fallback: a typo in a gate's arguments must not quietly reduce the
 * gate to one compiler.
 */
export function parseEngineSelection(argv = []) {
  const flag = argv.find((arg) => arg.startsWith('--engine='));
  const value = flag ? flag.slice('--engine='.length) : 'both';
  if (!['tsc', 'tsrs', 'both'].includes(value)) {
    throw new Error(`unknown --engine=${value} (expected tsc, tsrs or both)`);
  }
  return value;
}

/** Absolute path of an engine's launcher, or null when the package is absent. */
export function engineBinPath(root, engineId) {
  const engine = TYPECHECK_ENGINES.find((candidate) => candidate.id === engineId);
  if (!engine) return null;
  const path = join(root, engine.bin);
  return existsSync(path) ? path : null;
}

/** Installed version of an engine's package, or null when it cannot be read. */
export function engineVersion(root, engineId) {
  const engine = TYPECHECK_ENGINES.find((candidate) => candidate.id === engineId);
  if (!engine) return null;
  try {
    const manifest = JSON.parse(
      readFileSync(join(root, 'node_modules', engine.pkg, 'package.json'), 'utf-8'),
    );
    return manifest.version ?? null;
  } catch {
    return null;
  }
}

/**
 * Probe whether an engine can actually run here.
 *
 * The package being installed is not the same question as the package being
 * usable: tsrs's launcher resolves a platform binary at runtime, and on
 * win32 that binary does not exist in 0.18.0. Probing with `--version` is
 * the only honest test — it exercises the same resolution path the real run
 * takes.
 */
export function probeEngine(root, engineId) {
  const bin = engineBinPath(root, engineId);
  if (!bin) return { available: false, reason: 'package not installed' };
  const probe = spawnSync(process.execPath, [bin, '--version'], {
    cwd: root,
    encoding: 'utf-8',
    shell: false,
  });
  if (probe.status !== 0) {
    const detail = (probe.stderr || '').trim().split('\n')[0] || `exit ${probe.status}`;
    return { available: false, reason: `launcher failed: ${detail}` };
  }
  return { available: true, reason: null };
}

/**
 * The full command list for the requested engines. Programs run in the
 * order above, engines in the order above: the authoritative compiler first
 * so its diagnostics are the first thing on screen.
 */
export function planTypecheckCommands(root = ROOT, engineIds = ENGINE_ORDER) {
  const commands = [];
  for (const engineId of engineIds) {
    const bin = engineBinPath(root, engineId);
    if (!bin) continue;
    for (const program of TYPECHECK_PROGRAMS) {
      commands.push({
        cmd: process.execPath,
        args: [bin, '-p', program.config, '--noEmit'],
        engine: engineId,
        program: program.id,
      });
    }
  }
  return commands;
}

/**
 * Run the gate. Fails at the first failing command — a typecheck gate that
 * prints three compilers' worth of the same errors is noise, not signal.
 */
export function runTypecheck({ root = ROOT, engineIds = ENGINE_ORDER, log = console.log } = {}) {
  const commands = planTypecheckCommands(root, engineIds);
  if (commands.length === 0) {
    throw new Error('no typecheck engine is installed — run `pnpm install` first');
  }

  for (const command of commands) {
    const version = engineVersion(root, command.engine);
    log(
      `[typecheck] ${command.engine}${version ? ` ${version}` : ''} — ${command.program}`,
    );
    const result = spawnSync(command.cmd, command.args, {
      cwd: root,
      stdio: 'inherit',
      shell: false,
    });
    if (result.status !== 0) {
      log(`[typecheck] FAILED in ${command.engine} (${command.program})`);
      return { status: result.status ?? 1, failed: command };
    }
  }
  log('[typecheck] OK');
  return { status: 0, failed: null };
}

/**
 * Resolve which engines will run, and say out loud which ones will not.
 * A skipped engine is announced, never silent: "tsrs was not run" is
 * information a reviewer needs.
 */
export function resolveEngines(root = ROOT, selection = 'both') {
  const requested = selection === 'both' ? ENGINE_ORDER : [selection];
  const engines = [];
  const skipped = [];
  for (const engineId of requested) {
    const probe = probeEngine(root, engineId);
    if (probe.available) {
      engines.push(engineId);
    } else {
      skipped.push({ engine: engineId, reason: probe.reason });
    }
  }
  return { engines, skipped };
}

function main() {
  let selection;
  try {
    selection = parseEngineSelection(process.argv.slice(2));
  } catch (err) {
    console.error('[typecheck]', err instanceof Error ? err.message : err);
    process.exit(1);
  }

  const { engines, skipped } = resolveEngines(ROOT, selection);
  for (const { engine, reason } of skipped) {
    console.log(`[typecheck] SKIPPING ${engine}: ${reason}`);
  }
  if (engines.length === 0) {
    console.error('[typecheck] no usable typecheck engine — run `pnpm install` first');
    process.exit(1);
  }

  let result;
  try {
    result = runTypecheck({ root: ROOT, engineIds: engines });
  } catch (err) {
    console.error('[typecheck]', err instanceof Error ? err.message : err);
    process.exit(1);
  }
  process.exit(result.status);
}

if (isDirectInvocation(import.meta.url)) {
  main();
}
