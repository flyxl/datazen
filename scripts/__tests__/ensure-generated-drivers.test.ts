/** @vitest-environment node */
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtempSync, mkdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { dirname, join } from 'path';
import {
  REQUIRED_CODEGEN_MARKERS,
  missingGeneratedFiles,
  shouldGenerate,
  staleGeneratedFiles,
  runEnsureGeneratedDrivers,
} from '../ensure-generated-drivers.mjs';
import { FULLY_GENERATED_MANAGED } from '../driver-deinject.mjs';
import { resetDir } from './fixture';

const ALL_GENERATED = [...FULLY_GENERATED_MANAGED, 'apps/desktop/src-tauri/capabilities/default.json'];

/** Content that satisfies every marker `generated.ts` is required to export. */
const CURRENT_GENERATED_TS = Object.entries(REQUIRED_CODEGEN_MARKERS)
  .flatMap(([, markers]) => markers)
  .map((marker) => `${marker} = 1;`)
  .join('\n');

/** Write a complete, current codegen tree, optionally overriding generated.ts. */
function writeCurrentTree(root: string, generatedTs = CURRENT_GENERATED_TS) {
  for (const rel of ALL_GENERATED) {
    const full = join(root, rel);
    mkdirSync(dirname(full), { recursive: true });
    writeFileSync(
      full,
      rel === 'packages/frontend/src/extensions/generated.ts' ? generatedTs : '// present\n',
    );
  }
}

describe('ensure-generated-drivers', () => {
  let root: string;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'ensure-gen-'));
  });

  afterEach(() => {
    resetDir(root);
  });

  it('reports all codegen files missing on a fresh tree', () => {
    expect(missingGeneratedFiles(root)).toEqual(ALL_GENERATED);
    expect(shouldGenerate(root)).toBe(true);
  });

  it('skips when all codegen files exist and are current', () => {
    writeCurrentTree(root);
    expect(missingGeneratedFiles(root)).toEqual([]);
    expect(staleGeneratedFiles(root)).toEqual([]);
    expect(shouldGenerate(root)).toBe(false);
    expect(shouldGenerate(root, true)).toBe(true);

    const calls: string[] = [];
    const result = runEnsureGeneratedDrivers({
      root,
      argv: [],
      log: () => {},
      runResolve: (args) => {
        calls.push(args);
      },
    });
    expect(result).toEqual({ generated: false, missing: [], stale: [] });
    expect(calls).toEqual([]);
  });

  it('regenerates a codegen file that predates a required export', () => {
    // Codegen files are gitignored and this script used to be a no-op whenever
    // they merely existed, so adding an export (DATAZEN_VARIANT /
    // DATAZEN_UPDATER_CHANNEL) left existing checkouts failing to typecheck with
    // "has no exported member" until the file was deleted by hand.
    const staleSource = 'export const DRIVER_PROTOCOL_VERSION = 1;\n';
    writeCurrentTree(root, staleSource);

    expect(missingGeneratedFiles(root)).toEqual([]);
    expect(staleGeneratedFiles(root)).toEqual(['packages/frontend/src/extensions/generated.ts']);
    expect(shouldGenerate(root)).toBe(true);

    const calls: string[] = [];
    const logged: string[] = [];
    const result = runEnsureGeneratedDrivers({
      root,
      argv: [],
      // `log` carries the console.log contract — variadic `unknown[]` — so record
      // the same space-joined line a real console would, instead of narrowing the
      // parameter to a single `string` the declaration never promised.
      log: (...args: unknown[]) => {
        logged.push(args.map(String).join(' '));
      },
      runResolve: (args) => {
        calls.push(args);
      },
    });
    expect(result.generated).toBe(true);
    expect(result.stale).toEqual(['packages/frontend/src/extensions/generated.ts']);
    expect(calls).toEqual(['--codegen-only']);
    expect(logged.some((m) => m.includes('stale'))).toBe(true);
  });

  it('ignores a missing file when reporting staleness', () => {
    // A missing file is already covered by missingGeneratedFiles; reporting it as
    // both missing and stale would make the log line lie about the cause.
    expect(staleGeneratedFiles(root)).toEqual([]);
  });

  it('runs --codegen-only when files are missing', () => {
    const calls: string[] = [];
    const result = runEnsureGeneratedDrivers({
      root,
      argv: ['--drivers=basic'],
      log: () => {},
      runResolve: (args) => {
        calls.push(args);
      },
    });
    expect(result.generated).toBe(true);
    expect(result.missing).toEqual(ALL_GENERATED);
    expect(calls).toEqual(['--codegen-only --drivers=basic']);
  });

  it('forwards --force even when files exist', () => {
    writeCurrentTree(root);
    const calls: string[] = [];
    const result = runEnsureGeneratedDrivers({
      root,
      argv: ['--force', '--drivers=all'],
      log: () => {},
      runResolve: (args) => {
        calls.push(args);
      },
    });
    expect(result.generated).toBe(true);
    expect(calls).toEqual(['--codegen-only --drivers=all']);
  });
});
