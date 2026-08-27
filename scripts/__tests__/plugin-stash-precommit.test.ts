/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import {
  hasInjectedCargoContent,
  hasInjectedDriverInit,
  hasInjectedGeneratedTs,
  hasInjectedGeneratedLocales,
  hasInjectedCapabilities,
  fileHasInjection,
  runPluginStashPrecommit,
} from '../plugin-stash-precommit.mjs';
import { createPluginFileStash, MANAGED_FILES } from '../plugin-file-stash.mjs';
import { mkdtempSync, existsSync, unlinkSync, readFileSync } from 'fs';
import { execSync } from 'child_process';
import { tmpdir } from 'os';
import { join } from 'path';
import {
  CLEAN_CONTENTS,
  INJECTED_CONTENTS,
  writeManagedFiles,
  readManaged,
  resetDir,
} from './fixture';

describe('injection detectors', () => {
  it('detects cargo marker injection and ignores empty markers', () => {
    expect(hasInjectedCargoContent(CLEAN_CONTENTS['Cargo.toml'])).toBe(false);
    expect(hasInjectedCargoContent(INJECTED_CONTENTS['Cargo.toml'])).toBe(true);
    expect(
      hasInjectedCargoContent(
        `# <<driver-dependencies>>\n# comment only\n# <</driver-dependencies>>\n`,
      ),
    ).toBe(false);
  });

  it('detects driver_init injection but not comment-only extern crate', () => {
    expect(hasInjectedDriverInit(CLEAN_CONTENTS['src-tauri/src/driver_init.rs'])).toBe(false);
    expect(hasInjectedDriverInit(INJECTED_CONTENTS['src-tauri/src/driver_init.rs'])).toBe(true);
  });

  it('detects generated.ts PluginDatabaseType != never', () => {
    expect(hasInjectedGeneratedTs(CLEAN_CONTENTS['src/plugins/generated.ts'])).toBe(false);
    expect(hasInjectedGeneratedTs(INJECTED_CONTENTS['src/plugins/generated.ts'])).toBe(true);
  });

  it('detects capabilities plugin ACL entries', () => {
    expect(hasInjectedCapabilities(`"permissions": ["core:default"]`)).toBe(false);
    expect(hasInjectedCapabilities(`"permissions": ["kiwi:default"]`)).toBe(true);
    expect(hasInjectedCapabilities(`"permissions": ["olap:default"]`)).toBe(true);
    expect(hasInjectedCapabilities(`"permissions": ["superset:allow-login"]`)).toBe(true);
  });

  it('fileHasInjection routes by path', () => {
    expect(fileHasInjection('Cargo.toml', INJECTED_CONTENTS['Cargo.toml'])).toBe(true);
    expect(
      fileHasInjection('src/plugins/generated.ts', INJECTED_CONTENTS['src/plugins/generated.ts']),
    ).toBe(true);
    expect(
      fileHasInjection(
        'src/plugins/generated-locales.ts',
        INJECTED_CONTENTS['src/plugins/generated-locales.ts'],
      ),
    ).toBe(true);
    expect(
      fileHasInjection(
        'src-tauri/src/driver_init.rs',
        INJECTED_CONTENTS['src-tauri/src/driver_init.rs'],
      ),
    ).toBe(true);
    expect(fileHasInjection('README.md', 'hello')).toBe(false);
  });

  it('treats empty or stub codegen as not injected', () => {
    expect(hasInjectedGeneratedTs('')).toBe(false);
    expect(hasInjectedGeneratedTs('export const x = 1;\n')).toBe(false);
    expect(hasInjectedGeneratedTs(CLEAN_CONTENTS['src/plugins/generated.ts'])).toBe(false);
    expect(hasInjectedGeneratedLocales('')).toBe(false);
    expect(hasInjectedGeneratedLocales(CLEAN_CONTENTS['src/plugins/generated-locales.ts'])).toBe(
      false,
    );
    expect(hasInjectedDriverInit('')).toBe(false);
    expect(hasInjectedCapabilities('')).toBe(false);
  });
});

describe('runPluginStashPrecommit', () => {
  function setup() {
    const root = mkdtempSync(join(tmpdir(), 'precommit-stash-'));
    writeManagedFiles(root, CLEAN_CONTENTS);
    const staged = new Set<string>();
    const stagedBlobs = new Map<string, string>();

    return {
      root,
      staged,
      stagedBlobs,
      cleanup: () => resetDir(root),
      opts: {
        root,
        quiet: true,
        isStaged: (f: string) => staged.has(f),
        getContent: (rel: string) => {
          if (stagedBlobs.has(rel)) return stagedBlobs.get(rel)!;
          const p = join(root, rel);
          if (!existsSync(p)) return null;
          return readFileSync(p, 'utf-8');
        },
        restage: (f: string) => {
          staged.add(f);
        },
        log: () => {},
      },
    };
  }

  it('uses git helpers when isStaged/getContent are omitted', () => {
    const { root, cleanup } = setup();
    try {
      execSync('git init', { cwd: root, stdio: 'pipe' });
      execSync('git add -A', { cwd: root, stdio: 'pipe' });
      const result = runPluginStashPrecommit({
        root,
        quiet: true,
        log: () => {},
      });
      expect(result.status).toBe(0);
      expect(result.restored).toBe(false);
    } finally {
      cleanup();
    }
  });

  it('no-ops when clean and no stash', () => {
    const { root, opts, cleanup } = setup();
    try {
      const result = runPluginStashPrecommit(opts);
      expect(result).toEqual({ status: 0, restored: false });
      for (const f of MANAGED_FILES) {
        expect(readManaged(root, f)).toBe(CLEAN_CONTENTS[f]);
      }
    } finally {
      cleanup();
    }
  });

  it('restores when working tree is fully injected and stash is complete', () => {
    const { root, opts, staged, cleanup } = setup();
    try {
      const stash = createPluginFileStash(root, { quiet: true });
      stash.stashManagedFiles();
      writeManagedFiles(root, INJECTED_CONTENTS);
      staged.add('Cargo.toml');

      const result = runPluginStashPrecommit(opts);
      expect(result.status).toBe(0);
      expect(result.restored).toBe(true);
      expect(existsSync(join(root, '.plugin-file-stash'))).toBe(false);
      for (const f of MANAGED_FILES) {
        expect(readManaged(root, f)).toBe(CLEAN_CONTENTS[f]);
      }
    } finally {
      cleanup();
    }
  });

  it('deinjects cargo/capabilities when injection present but stash missing', () => {
    const { root, opts, cleanup } = setup();
    try {
      writeManagedFiles(root, INJECTED_CONTENTS);
      const result = runPluginStashPrecommit(opts);
      expect(result.status).toBe(0);
      expect(result.restored).toBe(true);
      expect(hasInjectedCargoContent(readManaged(root, 'Cargo.toml'))).toBe(false);
    } finally {
      cleanup();
    }
  });

  it('deinjects cargo when stash is incomplete (one tracked stash file deleted)', () => {
    const { root, opts, cleanup } = setup();
    try {
      const stash = createPluginFileStash(root, { quiet: true });
      stash.stashManagedFiles();
      writeManagedFiles(root, INJECTED_CONTENTS);
      unlinkSync(stash.stashPath('src-tauri/Cargo.toml'));

      const result = runPluginStashPrecommit(opts);
      expect(result.status).toBe(0);
      expect(result.restored).toBe(true);
      expect(hasInjectedCargoContent(readManaged(root, 'Cargo.toml'))).toBe(false);
      expect(hasInjectedCargoContent(readManaged(root, 'src-tauri/Cargo.toml'))).toBe(false);
    } finally {
      cleanup();
    }
  });

  it('discards stash without overwriting when work looks clean', () => {
    const { root, opts, cleanup } = setup();
    try {
      const stash = createPluginFileStash(root, { quiet: true });
      stash.stashManagedFiles();

      const result = runPluginStashPrecommit(opts);
      expect(result.status).toBe(0);
      expect(result.restored).toBe(false);
      expect(result.discardedStash).toBe(true);
      expect(existsSync(join(root, '.plugin-file-stash'))).toBe(false);
    } finally {
      cleanup();
    }
  });

  it('deinjects cargo without stash when generated files are clean', () => {
    const { root, opts, cleanup } = setup();
    try {
      writeManagedFiles(root, {
        ...CLEAN_CONTENTS,
        'Cargo.toml': INJECTED_CONTENTS['Cargo.toml'],
      });

      const result = runPluginStashPrecommit(opts);
      expect(result.status).toBe(0);
      expect(result.restored).toBe(true);
      expect(readManaged(root, 'Cargo.toml')).toBe(CLEAN_CONTENTS['Cargo.toml']);
    } finally {
      cleanup();
    }
  });

  it('detects injection from staged blob even when worktree is clean', () => {
    const { root, opts, staged, stagedBlobs, cleanup } = setup();
    try {
      staged.add('Cargo.toml');
      stagedBlobs.set('Cargo.toml', INJECTED_CONTENTS['Cargo.toml']);
      expect(readManaged(root, 'Cargo.toml')).toBe(CLEAN_CONTENTS['Cargo.toml']);

      // Worktree is already clean; deinject is a no-op on disk, then restage
      // picks up the clean worktree (real `git add`). Staged blob must update.
      const result = runPluginStashPrecommit({
        ...opts,
        restage: (f) => {
          staged.add(f);
          stagedBlobs.set(f, readManaged(root, f));
        },
      });
      expect(result.status).toBe(0);
      expect(result.restored).toBe(true);
      expect(stagedBlobs.get('Cargo.toml')).toBe(CLEAN_CONTENTS['Cargo.toml']);
    } finally {
      cleanup();
    }
  });

  it('returns restore-failed when injected file has no work or stash', () => {
    const { root, opts, cleanup } = setup();
    try {
      unlinkSync(join(root, 'Cargo.toml'));
      const result = runPluginStashPrecommit({
        ...opts,
        getContent: (rel) =>
          rel === 'Cargo.toml' ? INJECTED_CONTENTS['Cargo.toml'] : opts.getContent(rel),
      });
      expect(result.status).toBe(1);
      expect(result.reason).toBe('restore-failed');
      expect(result.restored).toBe(false);
    } finally {
      cleanup();
    }
  });

  it('returns still-injected when getContent keeps reporting injection', () => {
    const { root, opts, cleanup } = setup();
    try {
      writeManagedFiles(root, INJECTED_CONTENTS);
      const result = runPluginStashPrecommit({
        ...opts,
        getContent: (rel) => INJECTED_CONTENTS[rel] ?? null,
      });
      expect(result.status).toBe(1);
      expect(result.reason).toBe('still-injected');
      expect(result.restored).toBe(true);
    } finally {
      cleanup();
    }
  });

  it('restages only previously staged managed files', () => {
    const { root, opts, staged, cleanup } = setup();
    try {
      const stash = createPluginFileStash(root, { quiet: true });
      stash.stashManagedFiles();
      writeManagedFiles(root, INJECTED_CONTENTS);
      staged.add('Cargo.toml');
      const restaged: string[] = [];
      const result = runPluginStashPrecommit({
        ...opts,
        restage: (f) => {
          restaged.push(f);
          staged.add(f);
        },
      });
      expect(result.status).toBe(0);
      expect(restaged).toEqual(['Cargo.toml']);
    } finally {
      cleanup();
    }
  });
});
