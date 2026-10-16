/** @vitest-environment node */
import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtempSync, existsSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import {
  createDriverFileStash,
  evaluateStashFreshness,
  MANAGED_FILES,
  STASH_META_FILE,
} from '../driver-file-stash.mjs';
import {
  assertStashReusable,
  staleStashMessage,
  StaleDriverFileStashError,
} from '../resolve-drivers.mjs';
import { CLEAN_CONTENTS, writeManagedFiles, resetDir } from './fixture';

const SHA_A = 'a'.repeat(40);
const SHA_B = 'b'.repeat(40);

/** Verdict shape as thrown by assertStashReusable. */
type Verdict = { stale: boolean; reason: string; changed: string[] };

function verdictOf(err: unknown): Verdict {
  return (err as StaleDriverFileStashError & { verdict: Verdict }).verdict;
}

describe('evaluateStashFreshness (pure decision)', () => {
  it('reuses when the stash HEAD equals the current HEAD', () => {
    expect(
      evaluateStashFreshness({
        stashHead: SHA_A,
        currentHead: SHA_A,
        managedFilesChangedBetween: ['Cargo.toml'],
      }),
    ).toEqual({ stale: false, reason: 'same-head', changed: [] });
  });

  it('reuses when HEAD moved but no managed file changed', () => {
    expect(
      evaluateStashFreshness({
        stashHead: SHA_A,
        currentHead: SHA_B,
        managedFilesChangedBetween: [],
      }),
    ).toEqual({ stale: false, reason: 'managed-files-unchanged', changed: [] });
  });

  it('is stale when HEAD moved and a managed file changed', () => {
    expect(
      evaluateStashFreshness({
        stashHead: SHA_A,
        currentHead: SHA_B,
        managedFilesChangedBetween: ['Cargo.toml', 'Cargo.lock'],
      }),
    ).toEqual({
      stale: true,
      reason: 'managed-files-changed',
      changed: ['Cargo.toml', 'Cargo.lock'],
    });
  });

  it('is stale without metadata (legacy stash created before the sidecar existed)', () => {
    expect(
      evaluateStashFreshness({
        stashHead: null,
        currentHead: SHA_B,
        managedFilesChangedBetween: [],
      }),
    ).toEqual({ stale: true, reason: 'missing-metadata', changed: [] });
  });

  it('is stale when the current HEAD cannot be resolved', () => {
    expect(
      evaluateStashFreshness({
        stashHead: SHA_A,
        currentHead: null,
        managedFilesChangedBetween: [],
      }),
    ).toEqual({ stale: true, reason: 'unknown-current-head', changed: [] });
  });
});

describe('staleStashMessage (pure text)', () => {
  const message = staleStashMessage({
    reason: 'managed-files-changed',
    stashHead: SHA_A,
    currentHead: SHA_B,
    createdAt: '2026-01-01T00:00:00.000Z',
    changed: ['Cargo.toml', 'Cargo.lock'],
  });

  it('names both shas, the changed files and the recovery command', () => {
    expect(message).toContain(SHA_A.slice(0, 12));
    expect(message).toContain(SHA_B.slice(0, 12));
    expect(message).toContain('2026-01-01T00:00:00.000Z');
    expect(message).toContain('- Cargo.toml');
    expect(message).toContain('- Cargo.lock');
    expect(message).toContain('rm -rf .driver-file-stash');
    expect(message).toContain('git checkout HEAD -- Cargo.toml src-tauri/Cargo.toml Cargo.lock');
  });

  it('explains why restore is unsafe and why the outcome would be silent', () => {
    expect(message).toMatch(/Do NOT run .*driver-file-stash\.mjs restore/);
    expect(message).toContain('silent rollback');
    expect(message).toContain('managedReadPath');
  });

  it('gives a distinct explanation for a legacy stash with no metadata', () => {
    const legacy = staleStashMessage({
      reason: 'missing-metadata',
      stashHead: null,
      currentHead: SHA_B,
      createdAt: null,
      changed: [],
    });
    expect(legacy).toContain('missing-metadata');
    expect(legacy).toContain('.stash-meta.json');
    expect(legacy).toContain('unknown');
    expect(legacy).toContain('rm -rf .driver-file-stash');
    // No changed-file list is fabricated when we never got to compare HEADs.
    expect(legacy).not.toContain('    - ');
  });
});

describe('assertStashReusable', () => {
  it('reuses the stash when HEAD is unchanged', () => {
    expect(
      assertStashReusable({
        readMeta: () => ({ head: SHA_A, createdAt: '2026-01-01T00:00:00.000Z' }),
        resolveHead: () => SHA_A,
        listChanged: () => ['Cargo.toml'],
      }),
    ).toEqual({ stale: false, reason: 'same-head', changed: [] });
  });

  it('does not even query managed files when HEAD is unchanged (nested re-run)', () => {
    let queried = false;
    const verdict = assertStashReusable({
      readMeta: () => ({ head: SHA_A, createdAt: null }),
      resolveHead: () => SHA_A,
      listChanged: () => {
        queried = true;
        return ['Cargo.toml'];
      },
    });
    expect(verdict.stale).toBe(false);
    expect(queried).toBe(false);
  });

  it('reuses the stash when HEAD moved but managed files are untouched', () => {
    expect(
      assertStashReusable({
        readMeta: () => ({ head: SHA_A, createdAt: null }),
        resolveHead: () => SHA_B,
        listChanged: () => [],
      }),
    ).toEqual({ stale: false, reason: 'managed-files-unchanged', changed: [] });
  });

  it('hard-fails listing the changed files when a managed file moved under the stash', () => {
    let thrown: unknown = null;
    try {
      assertStashReusable({
        readMeta: () => ({ head: SHA_A, createdAt: '2026-01-01T00:00:00.000Z' }),
        resolveHead: () => SHA_B,
        listChanged: () => ['Cargo.toml'],
      });
    } catch (e) {
      thrown = e;
    }
    expect(thrown).toBeInstanceOf(StaleDriverFileStashError);
    expect(verdictOf(thrown)).toEqual({
      stale: true,
      reason: 'managed-files-changed',
      changed: ['Cargo.toml'],
    });
    const message = (thrown as Error).message;
    expect(message).toContain('- Cargo.toml');
    expect(message).toContain('rm -rf .driver-file-stash');
  });

  it('hard-fails a legacy stash that carries no metadata, never reusing it silently', () => {
    let thrown: unknown = null;
    try {
      assertStashReusable({
        readMeta: () => null,
        resolveHead: () => SHA_B,
        listChanged: () => [],
      });
    } catch (e) {
      thrown = e;
    }
    expect(thrown).toBeInstanceOf(StaleDriverFileStashError);
    expect(verdictOf(thrown).reason).toBe('missing-metadata');
  });
});

describe('stash metadata sidecar', () => {
  let root: string;
  let stash: ReturnType<typeof createDriverFileStash>;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'driver-stash-fresh-'));
    writeManagedFiles(root, CLEAN_CONTENTS);
    stash = createDriverFileStash(root, { quiet: true, resolveHead: () => SHA_A });
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  it('is deliberately not a managed file', () => {
    expect(STASH_META_FILE).toBe('.stash-meta.json');
    expect(MANAGED_FILES).not.toContain(STASH_META_FILE);
  });

  it('records the creating HEAD and a timestamp when stashing', () => {
    stash.stashManagedFiles();
    expect(existsSync(stash.metaPath())).toBe(true);
    const meta = stash.readStashMeta();
    expect(meta?.head).toBe(SHA_A);
    expect(typeof meta?.createdAt).toBe('string');
    expect(String(meta?.createdAt)).not.toBe('');
  });

  it('treats its own fresh stash as reusable', () => {
    stash.stashManagedFiles();
    expect(stash.inspectFreshness()).toMatchObject({
      stale: false,
      reason: 'same-head',
      stashHead: SHA_A,
      currentHead: SHA_A,
    });
  });

  it('goes stale once the managed file changes under it', () => {
    stash.stashManagedFiles();
    const moved = createDriverFileStash(root, {
      quiet: true,
      resolveHead: () => SHA_B,
      listManagedFilesChanged: () => ['Cargo.toml'],
    });
    expect(moved.inspectFreshness()).toMatchObject({
      stale: true,
      reason: 'managed-files-changed',
      changed: ['Cargo.toml'],
    });
  });

  it('dies with the stash directory, so a fresh stash starts with no metadata', () => {
    stash.stashManagedFiles();
    stash.restoreManagedFiles();
    expect(existsSync(stash.metaPath())).toBe(false);
    expect(stash.readStashMeta()).toBeNull();
  });

  it('reports freshness through printStatus without touching managed files', () => {
    stash.stashManagedFiles();
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
    expect(lines.some((l) => /freshness: fresh \(reason=same-head\)/.test(l))).toBe(true);
    expect(lines.some((l) => l.includes(SHA_A))).toBe(true);
    for (const f of MANAGED_FILES) {
      expect(existsSync(stash.stashPath(f))).toBe(true);
      expect(existsSync(stash.workPath(f))).toBe(true);
    }
  });

  it('reports n/a freshness when no complete stash is present', () => {
    const lines: string[] = [];
    const orig = console.log;
    console.log = (...args: unknown[]) => {
      lines.push(args.map(String).join(' '));
    };
    try {
      resetDir(root);
      writeManagedFiles(root, CLEAN_CONTENTS);
      stash.printStatus();
    } finally {
      console.log = orig;
    }
    expect(lines.some((l) => l.includes('freshness: n/a'))).toBe(true);
  });
});
