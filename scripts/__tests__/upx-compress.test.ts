/** @vitest-environment node */
import { mkdtempSync, writeFileSync, mkdirSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { describe, expect, it } from 'vitest';
import {
  shouldCompressPlatform,
  findTargetExecutables,
  compressExecutable,
  runUpxCompression,
} from '../upx-compress.mjs';

describe('upx-compress platform policy', () => {
  it('allows Windows by default', () => {
    const res = shouldCompressPlatform('win32', {});
    expect(res.shouldRun).toBe(true);
    expect(res.reason).toContain('Windows');
  });

  it('skips macOS by default', () => {
    const res = shouldCompressPlatform('darwin', {});
    expect(res.shouldRun).toBe(false);
    expect(res.reason).toContain('macOS');
  });

  it('skips Linux by default to prevent patchelf conflict', () => {
    const res = shouldCompressPlatform('linux', {});
    expect(res.shouldRun).toBe(false);
    expect(res.reason).toContain('Linux skipped');
  });

  it('forces execution when DATAZEN_UPX_FORCE=1', () => {
    const res = shouldCompressPlatform('linux', { DATAZEN_UPX_FORCE: '1' });
    expect(res.shouldRun).toBe(true);
  });
});

describe('upx-compress findTargetExecutables', () => {
  it('finds windows executables in release folders', () => {
    const tmpRoot = mkdtempSync(join(tmpdir(), 'upx-test-'));
    const defaultRel = join(tmpRoot, 'target', 'release');
    const tripleRel = join(tmpRoot, 'target', 'x86_64-pc-windows-msvc', 'release');
    mkdirSync(defaultRel, { recursive: true });
    mkdirSync(tripleRel, { recursive: true });

    writeFileSync(join(defaultRel, 'DataZen.exe'), 'dummy');
    writeFileSync(join(defaultRel, 'DataZen.pdb'), 'pdb');
    writeFileSync(join(tripleRel, 'datazen.exe'), 'dummy');
    writeFileSync(join(tripleRel, 'other.exe'), 'dummy');

    const exes = findTargetExecutables(tmpRoot, 'win32');
    expect(exes.length).toBe(2);
    expect(exes.some((p) => p.includes('DataZen.exe'))).toBe(true);
    expect(exes.some((p) => p.includes('datazen.exe'))).toBe(true);
    expect(exes.some((p) => p.includes('other.exe'))).toBe(false);
  });

  it('returns empty array if target directory does not exist', () => {
    const tmpRoot = mkdtempSync(join(tmpdir(), 'upx-empty-'));
    const exes = findTargetExecutables(tmpRoot, 'win32');
    expect(exes).toEqual([]);
  });
});

describe('upx-compress run', () => {
  it('skips on darwin gracefully', () => {
    const logs: string[] = [];
    const res = runUpxCompression({
      platform: 'darwin',
      env: {},
      log: (msg: string) => logs.push(msg),
    });
    expect(res.skipped).toBe(true);
    expect(logs[0]).toContain('macOS skipped');
  });

  it('handles compressExecutable failure gracefully', () => {
    const tmpRoot = mkdtempSync(join(tmpdir(), 'upx-fail-'));
    const fakeExe = join(tmpRoot, 'datazen.exe');
    writeFileSync(fakeExe, 'binary content');

    const logs: string[] = [];
    const res = compressExecutable(fakeExe, {
      log: (msg: string) => logs.push(msg),
      exec: () => {
        throw new Error('upx: command not found');
      },
    });

    expect(res.success).toBe(false);
    expect(res.error).toContain('upx: command not found');
    expect(logs.some((l) => l.includes('warning: UPX compression failed'))).toBe(true);
  });
});
