/** @vitest-environment node */
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtempSync, writeFileSync, mkdirSync, existsSync, readFileSync, unlinkSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import {
  createDriverFileStash,
  MANAGED_FILES,
  runDriverFileStashCli,
} from '../driver-file-stash.mjs';
import {
  CLEAN_CONTENTS,
  INJECTED_CONTENTS,
  writeManagedFiles,
  readManaged,
  resetDir,
} from './fixture';

describe('createDriverFileStash', () => {
  let root: string;
  let stash: ReturnType<typeof createDriverFileStash>;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'driver-stash-'));
    writeManagedFiles(root, CLEAN_CONTENTS);
    stash = createDriverFileStash(root, { quiet: true });
  });

  afterEach(() => {
    resetDir(root);
  });

  it('stashes all managed files (working paths remain, stash present)', () => {
    stash.stashManagedFiles();
    expect(stash.allStashed()).toBe(true);
    for (const f of MANAGED_FILES) {
      expect(existsSync(stash.workPath(f))).toBe(true);
      expect(existsSync(stash.stashPath(f))).toBe(true);
      expect(readFileSync(stash.stashPath(f), 'utf-8')).toBe(CLEAN_CONTENTS[f]);
      expect(readFileSync(stash.workPath(f), 'utf-8')).toBe(CLEAN_CONTENTS[f]);
    }
  });

  it('restore brings back exact clean bytes after inject simulation', () => {
    stash.stashManagedFiles();
    // simulate resolve-plugins writing injected copies
    writeManagedFiles(root, INJECTED_CONTENTS);
    for (const f of MANAGED_FILES) {
      expect(readManaged(root, f)).toBe(INJECTED_CONTENTS[f]);
    }

    stash.restoreManagedFiles();
    expect(existsSync(stash.STASH_DIR)).toBe(false);
    for (const f of MANAGED_FILES) {
      expect(readManaged(root, f)).toBe(CLEAN_CONTENTS[f]);
    }
  });

  it('errors when stash already exists', () => {
    stash.stashManagedFiles();
    writeManagedFiles(root, CLEAN_CONTENTS); // recreate work files
    expect(() => stash.stashManagedFiles()).toThrow(/stash already exists/);
  });

  it('errors when a working file is missing before stash', () => {
    const victim = stash.workPath('Cargo.toml');
    unlinkSync(victim);
    expect(() => stash.stashManagedFiles()).toThrow(/missing working files.*Cargo\.toml/);
  });

  it('leaves gitignored codegen files in place on restore', () => {
    stash.stashManagedFiles();
    mkdirSync(join(root, 'src/plugins'), { recursive: true });
    writeFileSync(
      stash.workPath('src/plugins/generated.ts'),
      INJECTED_CONTENTS['src/plugins/generated.ts'],
    );
    writeManagedFiles(root, INJECTED_CONTENTS);
    stash.restoreManagedFiles();
    expect(readManaged(root, 'src/plugins/generated.ts')).toBe(
      INJECTED_CONTENTS['src/plugins/generated.ts'],
    );
    expect(readManaged(root, 'Cargo.toml')).toBe(CLEAN_CONTENTS['Cargo.toml']);
  });

  it('deinjects when stash dir is gone but working copies remain', () => {
    stash.stashManagedFiles();
    writeManagedFiles(root, INJECTED_CONTENTS);
    stash.cleanupStashDir();
    stash.restoreManagedFiles();
    expect(readManaged(root, 'Cargo.toml')).toBe(CLEAN_CONTENTS['Cargo.toml']);
  });

  it('managedReadPath prefers stash when present', () => {
    stash.stashManagedFiles();
    writeManagedFiles(root, INJECTED_CONTENTS);
    expect(readFileSync(stash.managedReadPath('Cargo.toml'), 'utf-8')).toBe(
      CLEAN_CONTENTS['Cargo.toml'],
    );
    expect(readFileSync(stash.workPath('Cargo.toml'), 'utf-8')).toBe(
      INJECTED_CONTENTS['Cargo.toml'],
    );
  });

  it('double restore after successful restore is idempotent', () => {
    stash.stashManagedFiles();
    writeManagedFiles(root, INJECTED_CONTENTS);
    stash.restoreManagedFiles();
    expect(() => stash.restoreManagedFiles()).not.toThrow();
    expect(readManaged(root, 'Cargo.toml')).toBe(CLEAN_CONTENTS['Cargo.toml']);
  });

  it('stashExists is true if any file is stashed', () => {
    expect(stash.stashExists()).toBe(false);
    stash.stashManagedFiles();
    expect(stash.stashExists()).toBe(true);
    for (const f of MANAGED_FILES.slice(1)) {
      unlinkSync(stash.stashPath(f));
    }
    expect(stash.stashExists()).toBe(true);
    expect(stash.allStashed()).toBe(false);
  });

  it('restores a missing work file from stash', () => {
    stash.stashManagedFiles();
    unlinkSync(stash.workPath('Cargo.toml'));
    stash.restoreManagedFiles();
    expect(readManaged(root, 'Cargo.toml')).toBe(CLEAN_CONTENTS['Cargo.toml']);
  });

  it('errors when restore has neither work nor stash for a tracked file', () => {
    unlinkSync(stash.workPath('Cargo.toml'));
    expect(() => stash.restoreManagedFiles()).toThrow(
      /cannot restore; stash missing for: Cargo\.toml/,
    );
  });

  it('restoreSelectedFiles errors when both work and stash are missing', () => {
    unlinkSync(stash.workPath('Cargo.toml'));
    expect(() => stash.restoreSelectedFiles(['Cargo.toml'])).toThrow(/cannot restore selection/);
  });

  it('printStatus reports stash and work presence', () => {
    const lines: string[] = [];
    const orig = console.log;
    console.log = (...args: unknown[]) => {
      lines.push(args.map(String).join(' '));
    };
    try {
      stash.printStatus();
    } finally {
      console.log = orig;
    }
    expect(lines.some((l) => l.includes('stash dir:'))).toBe(true);
    expect(lines.some((l) => /Cargo\.toml: stash=no work=yes/.test(l))).toBe(true);
  });

  it('runDriverFileStashCli dispatches stash / restore / status / usage', () => {
    const errors: string[] = [];
    expect(
      runDriverFileStashCli(['status'], {
        api: stash,
        error: (msg) => errors.push(String(msg)),
      }),
    ).toBe(0);
    expect(
      runDriverFileStashCli(['stash'], {
        api: stash,
        error: (msg) => errors.push(String(msg)),
      }),
    ).toBe(0);
    expect(
      runDriverFileStashCli(['restore'], {
        api: stash,
        error: (msg) => errors.push(String(msg)),
      }),
    ).toBe(0);
    expect(
      runDriverFileStashCli(['nope'], {
        api: stash,
        error: (msg) => errors.push(String(msg)),
      }),
    ).toBe(1);
    expect(errors.some((e) => /Usage:/.test(e))).toBe(true);
  });

  it('runDriverFileStashCli returns 1 when the command throws', () => {
    const errors: string[] = [];
    expect(
      runDriverFileStashCli(['stash'], {
        api: stash,
        error: (msg) => errors.push(String(msg)),
      }),
    ).toBe(0);
    expect(
      runDriverFileStashCli(['stash'], {
        api: stash,
        error: (msg) => errors.push(String(msg)),
      }),
    ).toBe(1);
    expect(errors.some((e) => /stash already exists/.test(e))).toBe(true);
  });
});
