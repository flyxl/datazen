/** @vitest-environment node */
/**
 * End-to-end-ish: stash → write injected working copies → restore,
 * including mixed partial injection patterns.
 */
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtempSync, existsSync, writeFileSync, unlinkSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { createPluginFileStash, MANAGED_FILES } from '../plugin-file-stash.mjs';
import { runPluginStashPrecommit } from '../plugin-stash-precommit.mjs';
import {
  CLEAN_CONTENTS,
  INJECTED_CONTENTS,
  writeManagedFiles,
  readManaged,
  resetDir,
} from './fixture';

describe('stash inject restore workflow', () => {
  let root: string;
  let stash: ReturnType<typeof createPluginFileStash>;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'stash-flow-'));
    writeManagedFiles(root, CLEAN_CONTENTS);
    stash = createPluginFileStash(root, { quiet: true });
  });

  afterEach(() => {
    resetDir(root);
  });

  it('round-trip: stash → inject all → restore matches original', () => {
    const before = Object.fromEntries(
      MANAGED_FILES.map((f) => [f, readManaged(root, f)]),
    );
    stash.stashManagedFiles();
    writeManagedFiles(root, INJECTED_CONTENTS);
    stash.restoreManagedFiles();
    for (const f of MANAGED_FILES) {
      expect(readManaged(root, f)).toBe(before[f]);
    }
  });

  it('round-trip via precommit after partial inject (only 2 files written)', () => {
    stash.stashManagedFiles();
    writeFileSync(
      stash.workPath('Cargo.toml'),
      INJECTED_CONTENTS['Cargo.toml'],
    );
    writeFileSync(
      stash.workPath('src/plugins/generated.ts'),
      INJECTED_CONTENTS['src/plugins/generated.ts'],
    );
    // other work files intentionally absent

    const result = runPluginStashPrecommit({
      root,
      quiet: true,
      isStaged: () => false,
      getContent: (rel) => {
        const p = stash.workPath(rel);
        if (!existsSync(p)) return null;
        return readManaged(root, rel);
      },
      restage: () => {},
      log: () => {},
    });

    expect(result.status).toBe(0);
    expect(result.restored).toBe(true);
    for (const f of MANAGED_FILES) {
      expect(readManaged(root, f)).toBe(CLEAN_CONTENTS[f]);
    }
  });

  it('second stash without restore fails; after restore succeeds again', () => {
    stash.stashManagedFiles();
    writeManagedFiles(root, INJECTED_CONTENTS);
    expect(() => stash.stashManagedFiles()).toThrow(/stash already exists/);
    stash.restoreManagedFiles();
    expect(() => stash.stashManagedFiles()).not.toThrow();
    expect(stash.allStashed()).toBe(true);
    stash.restoreManagedFiles();
  });

  it('CI-like nest: outer stash stays while nested inject skips restore', async () => {
    const { INJECT_ACTIVE_ENV, runWithPluginInject } = await import(
      '../with-plugin-inject.mjs'
    );
    stash.stashManagedFiles();
    writeManagedFiles(root, INJECTED_CONTENTS);

    const result = runWithPluginInject({
      argv: ['--', 'true'],
      stashExistsFn: () => stash.stashExists(),
      env: { [INJECT_ACTIVE_ENV]: '1' },
      runResolve: () => {
        throw new Error('nested must not resolve');
      },
      runRestore: () => {
        throw new Error('nested must not restore');
      },
      runCommand: () => ({ status: 0 }),
      log: () => {},
    });

    expect(result.nested).toBe(true);
    expect(stash.allStashed()).toBe(true);
    for (const f of MANAGED_FILES) {
      expect(readManaged(root, f)).toBe(INJECTED_CONTENTS[f]);
    }
  });

  it('deleting generated stash mid-flight restores a git-safe stub', () => {
    stash.stashManagedFiles();
    writeManagedFiles(root, INJECTED_CONTENTS);
    unlinkSync(stash.stashPath('src/plugins/generated.ts'));

    stash.restoreManagedFiles();
    expect(readManaged(root, 'src/plugins/generated.ts')).toContain(
      'export type DatabaseType = never',
    );

    writeManagedFiles(root, INJECTED_CONTENTS);
    const result = runPluginStashPrecommit({
      root,
      quiet: true,
      isStaged: () => false,
      getContent: (rel) => readManaged(root, rel),
      restage: () => {},
      log: () => {},
    });
    expect(result.status).toBe(0);
    expect(result.restored).toBe(true);
    expect(readManaged(root, 'src/plugins/generated.ts')).toContain(
      'export type DatabaseType = never',
    );
  });
});
