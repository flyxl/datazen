/** @vitest-environment node */
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtempSync, mkdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { dirname, join } from 'path';
import {
  missingGeneratedFiles,
  shouldGenerate,
  runEnsureGeneratedDrivers,
} from '../ensure-generated-drivers.mjs';
import { FULLY_GENERATED_MANAGED } from '../driver-deinject.mjs';
import { resetDir } from './fixture';

const ALL_GENERATED = [...FULLY_GENERATED_MANAGED, 'src-tauri/capabilities/default.json'];

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

  it('skips when all codegen files exist', () => {
    for (const rel of ALL_GENERATED) {
      const full = join(root, rel);
      mkdirSync(dirname(full), { recursive: true });
      writeFileSync(full, '// present\n');
    }
    expect(missingGeneratedFiles(root)).toEqual([]);
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
    expect(result).toEqual({ generated: false, missing: [] });
    expect(calls).toEqual([]);
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
    for (const rel of ALL_GENERATED) {
      const full = join(root, rel);
      mkdirSync(dirname(full), { recursive: true });
      writeFileSync(full, '// present\n');
    }
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
