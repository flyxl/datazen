#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDirectInvocation } from './lib/isDirectInvocation.mjs';

/**
 * typecheck.mjs — the repository's typecheck gate, run under ONE compiler.
 *
 * WHY ONE COMPILER
 *
 * The repository is on TypeScript 7. Two implementations of that compiler are
 * installed, and they are not the same thing:
 *
 *   tsrs   — `tsrs` (npm `tsrs@0.18.0-ts7.1.0-dev.20260929`), the Rust port
 *            by oxc-project. It tracks the 7.1 development line, i.e. it is
 *            AHEAD of the released compiler, and it is roughly 2x faster and
 *            uses about a third of the memory on this repository.
 *
 *   tsc    — `typescript` (npm `typescript@7.0.2`), the official Go port.
 *            This is the version that ships as `latest`, and the release
 *            pipeline's `union-typecheck` job runs exactly this binary
 *            (scripts/ci-union-typecheck.mjs).
 *
 * **The gate runs `tsrs` alone.** It used to run both, on the reasoning that
 * a second compiler on the same source is a second opinion, and that the
 * authoritative one has to be in the local gate for a PR which passes here to
 * be unable to fail the release gate. That reasoning bought two things: the
 * tsrs diagnostics of the *next* TypeScript release, and the guarantee above.
 * The first is kept — `tsrs` alone already reports the 7.1 diagnostics. The
 * second is given up deliberately: the release gate still runs `tsc`
 * (`scripts/ci-union-typecheck.mjs`), so the authoritative compiler is not
 * gone from the pipeline, it is only gone from the *fast* local gate. The
 * honest consequence is that a PR can now pass `pnpm typecheck` and still
 * fail the release gate on a type error `tsc` reports and `tsrs` does not.
 * `pnpm typecheck:tsc` is kept as the escape hatch for anyone who needs the
 * authoritative compiler locally.
 *
 * MEASURED (this repository, `main` at the time of writing, 1845 TS files)
 *
 *   tsc    5.9.3   24.8-27.0 s for the three programs   1.31 GB peak RSS
 *   tsc    7.0.2    2.4-2.6 s for the three programs   1.25 GB peak RSS
 *   tsrs   0.18.0   1.0-1.3 s for the three programs   0.35 GB peak RSS
 *
 * The 10x drop from 5.9.3 to 7.0.2 is the compiler rewrite, not this script.
 * tsrs's remaining 2x is what the single-engine gate now takes.
 *
 * PLATFORM
 *
 * tsrs 0.18.0 ships no win32 binary (the upstream project lists it as
 * "configured for the next release"). The gate therefore falls back to the
 * `tsc` engine on Windows rather than failing: the release matrix builds and
 * signs Windows artifacts, and a typecheck gate that cannot run on the
 * platform it ships to is a worse gate. The fallback is announced on
 * stdout, never silent.
 *
 * The same probe makes the gate self-healing: when tsrs publishes a Windows
 * build, the next `pnpm install` picks it up and the tsrs engine starts
 * running there without any change to this file.
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
 * The engine the gate runs when nothing is asked for.
 *
 * `tsrs` is the one: it is the faster of the two, it is the one that reports
 * the next TypeScript release's diagnostics, and the authoritative compiler is
 * still in the pipeline through the release gate
 * (`scripts/ci-union-typecheck.mjs`). `pnpm typecheck:tsc` runs the other one
 * on demand.
 */
export const DEFAULT_ENGINE = 'tsrs';

/**
 * Parse `--engine=tsc|tsrs|both`. Unknown values are an error rather than a
 * silent fallback: a typo in a gate's arguments must not quietly reduce the
 * gate to one compiler.
 */
export function parseEngineSelection(argv = []) {
  const flag = argv.find((arg) => arg.startsWith('--engine='));
  const value = flag ? flag.slice('--engine='.length) : DEFAULT_ENGINE;
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
 * order above, engines in the order above: the requested compiler first so
 * its diagnostics are the first thing on screen.
 */
export function planTypecheckCommands(root = ROOT, engineIds = [DEFAULT_ENGINE]) {
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
export function runTypecheck({ root = ROOT, engineIds = [DEFAULT_ENGINE], log = console.log } = {}) {
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
 *
 * **The fallback.** A single requested engine that cannot run *here* falls
 * back to the other one. tsrs has no win32 binary yet, and a typecheck gate
 * that cannot run on the platform the product ships to is a worse gate than a
 * gate that runs the other compiler. The fallback is recorded on the skipped
 * entry (`fellBackTo`), so the caller announces it — it is never silent, and
 * it never pretends the requested engine ran.
 */
export function resolveEngines(root = ROOT, selection = DEFAULT_ENGINE) {
  const requested = selection === 'both' ? ENGINE_ORDER : [selection];
  const engines = [];
  /** @type {Array<{ engine: string, reason: string | null, fellBackTo?: string }>} */
  const skipped = [];
  for (const engineId of requested) {
    const probe = probeEngine(root, engineId);
    if (probe.available) {
      engines.push(engineId);
    } else {
      skipped.push({ engine: engineId, reason: probe.reason });
    }
  }
  if (engines.length === 0 && selection !== 'both') {
    const fallback = ENGINE_ORDER.find((id) => id !== selection);
    const probe = fallback ? probeEngine(root, fallback) : null;
    if (fallback && probe && probe.available) {
      engines.push(fallback);
      const entry = skipped.find((item) => item.engine === selection);
      if (entry) entry.fellBackTo = fallback;
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
  for (const { engine, reason, fellBackTo } of skipped) {
    console.log(
      fellBackTo
        ? `[typecheck] ${engine} is unavailable here (${reason}) — falling back to ${fellBackTo}`
        : `[typecheck] SKIPPING ${engine}: ${reason}`,
    );
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
