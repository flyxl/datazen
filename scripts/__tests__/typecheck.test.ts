/** @vitest-environment node */
import { mkdtempSync, mkdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { basename, join } from 'path';
import { describe, expect, it } from 'vitest';
import {
  ENGINE_ORDER,
  TYPECHECK_ENGINES,
  TYPECHECK_PROGRAMS,
  engineBinPath,
  parseEngineSelection,
  planTypecheckCommands,
  probeEngine,
  resolveEngines,
} from '../typecheck.mjs';

/** A root that has no node_modules at all. */
function emptyRoot(): string {
  return mkdtempSync(join(tmpdir(), 'datazen-typecheck-empty-'));
}

/**
 * A root whose node_modules contains only the given engines' launchers.
 * The launcher is a stand-in file: the plan only needs it to exist, and the
 * probe is exercised separately.
 *
 * The package directory and the bin file name are read from the exported
 * engine table rather than spelled out here, so the fixture cannot drift
 * away from the mapping the implementation actually uses.
 */
function rootWithEngines(engines: string[]): string {
  const root = mkdtempSync(join(tmpdir(), 'datazen-typecheck-'));
  for (const engine of TYPECHECK_ENGINES) {
    if (!engines.includes(engine.id)) continue;
    const bin = join(root, 'node_modules', engine.pkg, 'bin');
    mkdirSync(bin, { recursive: true });
    writeFileSync(join(bin, basename(engine.bin)), '#!/usr/bin/env node\n');
  }
  return root;
}

describe('typecheck engine selection', () => {
  it('defaults to both engines', () => {
    expect(parseEngineSelection([])).toBe('both');
  });

  it('accepts a single engine', () => {
    expect(parseEngineSelection(['--engine=tsc'])).toBe('tsc');
    expect(parseEngineSelection(['--engine=tsrs'])).toBe('tsrs');
  });

  it('rejects an unknown engine instead of silently narrowing the gate', () => {
    expect(() => parseEngineSelection(['--engine=tsgo'])).toThrow(/unknown --engine=tsgo/);
  });
});

describe('typecheck command plan', () => {
  it('covers the three gated programs for each engine', () => {
    const root = rootWithEngines(['tsc', 'tsrs']);
    const commands = planTypecheckCommands(root, ENGINE_ORDER);
    expect(commands).toHaveLength(TYPECHECK_PROGRAMS.length * TYPECHECK_ENGINES.length);

    for (const engine of TYPECHECK_ENGINES) {
      const forEngine = commands.filter((command) => command.engine === engine.id);
      expect(forEngine.map((command) => command.program)).toEqual(
        TYPECHECK_PROGRAMS.map((program) => program.id),
      );
      for (const command of forEngine) {
        expect(command.args).toContain('--noEmit');
        expect(command.args).toContain('-p');
      }
    }
  });

  it('never shells out through a package-manager shim', () => {
    const root = rootWithEngines(['tsc', 'tsrs']);
    for (const command of planTypecheckCommands(root)) {
      // Invoked through process.execPath so the plan is identical on Windows.
      expect(command.cmd).toBe(process.execPath);
      expect(command.args.join(' ')).not.toContain('pnpm');
    }
  });

  it('skips an engine whose package is not installed', () => {
    const root = rootWithEngines(['tsc']);
    const commands = planTypecheckCommands(root, ENGINE_ORDER);
    expect(commands.every((command) => command.engine === 'tsc')).toBe(true);
    expect(engineBinPath(root, 'tsrs')).toBeNull();
  });
});

describe('typecheck engine probe', () => {
  it('reports an engine as unavailable when the package is absent', () => {
    const probe = probeEngine(emptyRoot(), 'tsc');
    expect(probe.available).toBe(false);
    expect(probe.reason).toMatch(/not installed/);
  });

  it('reports an engine as unavailable when the launcher cannot run', () => {
    const root = mkdtempSync(join(tmpdir(), 'datazen-typecheck-broken-'));
    const bin = join(root, 'node_modules', 'tsrs', 'bin');
    mkdirSync(bin, { recursive: true });
    // A launcher that exits non-zero stands in for tsrs on a platform with
    // no published binary.
    writeFileSync(join(bin, 'tsrs'), '#!/usr/bin/env node\nprocess.exit(1);\n');
    const probe = probeEngine(root, 'tsrs');
    expect(probe.available).toBe(false);
    expect(probe.reason).toMatch(/launcher failed/);
  });

  it('announces a skipped engine rather than dropping it silently', () => {
    const root = rootWithEngines(['tsc']);
    const { engines, skipped } = resolveEngines(root, 'both');
    expect(engines).toEqual(['tsc']);
    expect(skipped).toHaveLength(1);
    expect(skipped[0].engine).toBe('tsrs');
    expect(skipped[0].reason).toBeTruthy();
  });

  it('resolves both engines when both are installed', () => {
    const root = rootWithEngines(['tsc', 'tsrs']);
    const { engines, skipped } = resolveEngines(root, 'both');
    expect(engines).toEqual(['tsc', 'tsrs']);
    expect(skipped).toEqual([]);
  });
});
