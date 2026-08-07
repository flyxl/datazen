/** @vitest-environment node */
import { describe, expect, it } from 'vitest';
import {
  hasInjectedCargoContent,
  hasInjectedPluginInit,
  hasInjectedGeneratedTs,
  hasInjectedCapabilities,
  fileHasInjection,
  runPluginStashPrecommit,
} from '../plugin-stash-precommit.mjs';
import { createPluginFileStash, MANAGED_FILES } from '../plugin-file-stash.mjs';
import { mkdtempSync, existsSync, unlinkSync, readFileSync } from 'fs';
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
        `# <<plugin-dependencies>>\n# comment only\n# <</plugin-dependencies>>\n`,
      ),
    ).toBe(false);
  });

  it('detects plugin_init injection but not comment-only extern crate', () => {
    expect(hasInjectedPluginInit(CLEAN_CONTENTS['src-tauri/src/plugin_init.rs'])).toBe(
      false,
    );
    expect(hasInjectedPluginInit(INJECTED_CONTENTS['src-tauri/src/plugin_init.rs'])).toBe(
      true,
    );
  });

  it('detects generated.ts PluginDatabaseType != never', () => {
    expect(hasInjectedGeneratedTs(CLEAN_CONTENTS['src/plugins/generated.ts'])).toBe(
      false,
    );
    expect(hasInjectedGeneratedTs(INJECTED_CONTENTS['src/plugins/generated.ts'])).toBe(
      true,
    );
  });

  it('detects capabilities plugin ACL entries', () => {
    expect(
      hasInjectedCapabilities(CLEAN_CONTENTS['src-tauri/capabilities/default.json']),
    ).toBe(false);
    expect(
      hasInjectedCapabilities(INJECTED_CONTENTS['src-tauri/capabilities/default.json']),
    ).toBe(true);
    expect(hasInjectedCapabilities(`"permissions": ["olap:default"]`)).toBe(true);
    expect(hasInjectedCapabilities(`"permissions": ["superset:allow-login"]`)).toBe(
      true,
    );
  });

  it('fileHasInjection routes by path', () => {
    expect(fileHasInjection('Cargo.toml', INJECTED_CONTENTS['Cargo.toml'])).toBe(true);
    expect(
      fileHasInjection(
        'src-tauri/capabilities/default.json',
        CLEAN_CONTENTS['src-tauri/capabilities/default.json'],
      ),
    ).toBe(false);
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
      staged.add('src-tauri/capabilities/default.json');
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

  it('restores when only capabilities is injected (partial)', () => {
    const { root, opts, staged, cleanup } = setup();
    try {
      const stash = createPluginFileStash(root, { quiet: true });
      stash.stashManagedFiles();
      writeManagedFiles(root, {
        ...CLEAN_CONTENTS,
        'src-tauri/capabilities/default.json':
          INJECTED_CONTENTS['src-tauri/capabilities/default.json'],
      });
      staged.add('src-tauri/capabilities/default.json');

      const result = runPluginStashPrecommit(opts);
      expect(result.status).toBe(0);
      expect(result.restored).toBe(true);
      expect(readManaged(root, 'src-tauri/capabilities/default.json')).toBe(
        CLEAN_CONTENTS['src-tauri/capabilities/default.json'],
      );
      expect(readManaged(root, 'Cargo.toml')).toBe(CLEAN_CONTENTS['Cargo.toml']);
    } finally {
      cleanup();
    }
  });

  it('fails when injection present but stash missing', () => {
    const { root, opts, cleanup } = setup();
    try {
      writeManagedFiles(root, INJECTED_CONTENTS);
      const result = runPluginStashPrecommit(opts);
      expect(result.status).toBe(1);
      expect(result.reason).toBe('stash-missing');
      expect(readManaged(root, 'Cargo.toml')).toBe(INJECTED_CONTENTS['Cargo.toml']);
    } finally {
      cleanup();
    }
  });

  it('fails when stash is incomplete (one file deleted)', () => {
    const { root, opts, cleanup } = setup();
    try {
      const stash = createPluginFileStash(root, { quiet: true });
      stash.stashManagedFiles();
      writeManagedFiles(root, INJECTED_CONTENTS);
      unlinkSync(stash.stashPath('src-tauri/src/plugin_init.rs'));

      const result = runPluginStashPrecommit(opts);
      expect(result.status).toBe(1);
      expect(result.reason).toBe('stash-missing');
      expect(result.missing).toContain('src-tauri/src/plugin_init.rs');
    } finally {
      cleanup();
    }
  });

  it('restores when stash dir exists even if work looks clean', () => {
    const { root, opts, cleanup } = setup();
    try {
      const stash = createPluginFileStash(root, { quiet: true });
      stash.stashManagedFiles();
      writeManagedFiles(root, CLEAN_CONTENTS);

      const result = runPluginStashPrecommit(opts);
      expect(result.status).toBe(0);
      expect(result.restored).toBe(true);
      expect(existsSync(join(root, '.plugin-file-stash'))).toBe(false);
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

      const result = runPluginStashPrecommit(opts);
      expect(result.status).toBe(1);
      expect(result.reason).toBe('stash-missing');
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
