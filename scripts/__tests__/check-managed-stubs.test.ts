/** @vitest-environment node */
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtempSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { checkManagedStubs } from '../check-managed-stubs.mjs';
import { MANAGED_FILES } from '../plugin-file-stash.mjs';
import { CLEAN_CONTENTS, INJECTED_CONTENTS, writeManagedFiles, resetDir } from './fixture';

describe('checkManagedStubs', () => {
  let root: string;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'check-stubs-'));
  });

  afterEach(() => {
    resetDir(root);
  });

  it('returns 0 when tracked managed files are clean', () => {
    writeManagedFiles(root, CLEAN_CONTENTS);
    const logs: string[] = [];
    expect(
      checkManagedStubs({
        root,
        log: (msg) => logs.push(String(msg)),
        error: () => {},
      }),
    ).toBe(0);
    expect(logs).toHaveLength(MANAGED_FILES.length);
    expect(logs.every((l) => l.startsWith('[check-managed-stubs] ok '))).toBe(true);
  });

  it('returns 1 when a tracked file is missing', () => {
    writeManagedFiles(root, CLEAN_CONTENTS);
    const errors: string[] = [];
    expect(
      checkManagedStubs({
        root,
        files: ['Cargo.toml', 'does-not-exist.toml'],
        log: () => {},
        error: (msg) => errors.push(String(msg)),
      }),
    ).toBe(1);
    expect(errors.some((e) => /missing does-not-exist\.toml/.test(e))).toBe(true);
  });

  it('returns 1 when a tracked file looks injected', () => {
    writeManagedFiles(root, {
      ...CLEAN_CONTENTS,
      'Cargo.toml': INJECTED_CONTENTS['Cargo.toml'],
    });
    const errors: string[] = [];
    expect(
      checkManagedStubs({
        root,
        log: () => {},
        error: (msg) => errors.push(String(msg)),
      }),
    ).toBe(1);
    expect(errors.some((e) => /Cargo\.toml looks injected/.test(e))).toBe(true);
    expect(errors.some((e) => /plugin-file-stash\.mjs restore/.test(e))).toBe(true);
  });
});
