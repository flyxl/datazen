import { describe, expect, it } from 'vitest';
import {
  buildKeyTreeRows,
  buildServerTreeRows,
  folderLabel,
  splitKeyNamespace,
} from '../key-browser/keyTree';
import type { ChildEntry } from '../shared/redisInvoke';
import type { KeyEntry } from '@datazen/driver-sdk';

function entry(key: string): KeyEntry {
  return { key, keyType: 'string', ttl: -1, size: 0, preview: '' };
}

function folder(prefix: string, count: number): ChildEntry {
  return { kind: 'folder', prefix, count };
}

function leaf(key: string): ChildEntry {
  return { kind: 'key', key, keyType: 'string', ttl: -1, logicalLen: 5, memBytes: null };
}

describe('splitKeyNamespace', () => {
  it('splits on colon by default', () => {
    expect(splitKeyNamespace('user:profile:1')).toEqual(['user', 'profile', '1']);
  });

  it('falls back to dot', () => {
    expect(splitKeyNamespace('a.b.c')).toEqual(['a', 'b', 'c']);
  });

  it('returns whole key when no separator', () => {
    expect(splitKeyNamespace('plain')).toEqual(['plain']);
  });
});

describe('buildKeyTreeRows', () => {
  it('builds collapsed folders and expands when path is open', () => {
    const keys = [entry('user:1'), entry('user:2'), entry('order:9')];
    const collapsed = buildKeyTreeRows(keys, new Set());
    expect(
      collapsed.filter((r) => r.kind === 'folder').map((r) => (r as { path: string }).path),
    ).toEqual(['order', 'user']);

    const expanded = buildKeyTreeRows(keys, new Set(['user']));
    const labels = expanded.map((r) =>
      r.kind === 'folder' ? `folder:${r.label}` : `key:${r.label}`,
    );
    expect(labels).toContain('folder:user');
    expect(labels).toContain('key:1');
    expect(labels).toContain('key:2');
  });
});

describe('folderLabel', () => {
  it('strips trailing separator and returns last segment', () => {
    expect(folderLabel('app:cache:')).toBe('cache');
    expect(folderLabel('app:')).toBe('app');
    expect(folderLabel('a.b.c.')).toBe('c');
  });

  it('returns whole prefix when no separator', () => {
    expect(folderLabel('bare')).toBe('bare');
  });
});

describe('buildServerTreeRows', () => {
  it('renders root level as folders + leaves at depth 0', () => {
    const levels = {
      '': { children: [folder('app:', 6), leaf('root-key')], done: true },
    };
    const rows = buildServerTreeRows(levels, new Set());
    expect(rows.map((r) => r.kind)).toEqual(['folder', 'key']);
    const f = rows[0];
    expect(f.kind === 'folder' && f.path).toBe('app:');
    expect(f.kind === 'folder' && f.label).toBe('app');
    const k = rows[1];
    expect(k.kind === 'key' && k.entry.key).toBe('root-key');
  });

  it('recurses into expanded folders at deeper depth', () => {
    const levels = {
      '': { children: [folder('app:', 2)], done: true },
      'app:': { children: [leaf('app:cache:1')], done: true },
    };
    const collapsed = buildServerTreeRows(levels, new Set());
    expect(collapsed).toHaveLength(1);

    const expanded = buildServerTreeRows(levels, new Set(['app:']));
    expect(expanded.map((r) => r.kind)).toEqual(['folder', 'key']);
    const leafRow = expanded[1];
    expect(leafRow.kind === 'key' && leafRow.depth).toBe(1);
    expect(leafRow.kind === 'key' && leafRow.label).toBe('1');
  });

  it('omits missing levels for un-expanded prefixes', () => {
    const levels = { '': { children: [folder('app:', 2)], done: false } };
    const rows = buildServerTreeRows(levels, new Set(['app:']));
    // 'app:' level not loaded yet → only the folder row renders
    expect(rows).toHaveLength(1);
    expect(rows[0].kind).toBe('folder');
  });
});
