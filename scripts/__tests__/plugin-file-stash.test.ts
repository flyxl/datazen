/** @vitest-environment node */
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import {
  mkdtempSync,
  writeFileSync,
  mkdirSync,
  existsSync,
  readFileSync,
  unlinkSync,
} from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { createPluginFileStash, MANAGED_FILES } from '../plugin-file-stash.mjs';
import {
  CLEAN_CONTENTS,
  INJECTED_CONTENTS,
  writeManagedFiles,
  readManaged,
  resetDir,
} from './fixture';

describe('createPluginFileStash', () => {
  let root: string;
  let stash: ReturnType<typeof createPluginFileStash>;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'plugin-stash-'));
    writeManagedFiles(root, CLEAN_CONTENTS);
    stash = createPluginFileStash(root, { quiet: true });
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

  it('restore keeps user capability edits while stripping plugin ACL', () => {
    stash.stashManagedFiles();
    const caps = INJECTED_CONTENTS['src-tauri/capabilities/default.json'].replace(
      '"connection-*"',
      '"connection-*", "docs-singleton"',
    );
    writeManagedFiles(root, {
      ...INJECTED_CONTENTS,
      'src-tauri/capabilities/default.json': caps,
    });
    stash.restoreManagedFiles();
    const after = readManaged(root, 'src-tauri/capabilities/default.json');
    expect(after).toContain('docs-singleton');
    expect(after).not.toContain('kiwi:');
    expect(readManaged(root, 'src/plugins/generated.ts')).toBe(
      CLEAN_CONTENTS['src/plugins/generated.ts'],
    );
  });

  it('restore succeeds when only some working files were modified', () => {
    stash.stashManagedFiles();
    // partially overwrite: only capabilities + generated injected
    mkdirSync(join(root, 'src-tauri/capabilities'), { recursive: true });
    mkdirSync(join(root, 'src/plugins'), { recursive: true });
    writeFileSync(
      stash.workPath('src-tauri/capabilities/default.json'),
      INJECTED_CONTENTS['src-tauri/capabilities/default.json'],
    );
    writeFileSync(
      stash.workPath('src/plugins/generated.ts'),
      INJECTED_CONTENTS['src/plugins/generated.ts'],
    );

    stash.restoreManagedFiles();
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

  it('errors when any stash backup is missing on restore', () => {
    stash.stashManagedFiles();
    writeManagedFiles(root, INJECTED_CONTENTS);
    unlinkSync(stash.stashPath('src/plugins/generated.ts'));
    expect(() => stash.restoreManagedFiles()).toThrow(
      /cannot restore; stash missing.*generated\.ts/,
    );
  });

  it('errors when entire stash dir is gone but restore is requested', () => {
    stash.stashManagedFiles();
    writeManagedFiles(root, INJECTED_CONTENTS);
    stash.cleanupStashDir();
    expect(() => stash.restoreManagedFiles()).toThrow(/cannot restore; stash missing/);
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

  it('double restore after successful restore fails (no stash)', () => {
    stash.stashManagedFiles();
    writeManagedFiles(root, INJECTED_CONTENTS);
    stash.restoreManagedFiles();
    expect(() => stash.restoreManagedFiles()).toThrow(/cannot restore; stash missing/);
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
});
