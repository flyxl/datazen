import { describe, expect, it } from 'vitest';
import { buildKeyTreeRows, splitKeyNamespace } from '../keyTree';
import type { KeyEntry } from '../../../../../src/types';

function entry(key: string): KeyEntry {
  return { key, keyType: 'string', ttl: -1, size: 0, preview: '' };
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
    expect(collapsed.filter((r) => r.kind === 'folder').map((r) => (r as { path: string }).path)).toEqual([
      'order',
      'user',
    ]);

    const expanded = buildKeyTreeRows(keys, new Set(['user']));
    const labels = expanded.map((r) =>
      r.kind === 'folder' ? `folder:${r.label}` : `key:${r.label}`,
    );
    expect(labels).toContain('folder:user');
    expect(labels).toContain('key:1');
    expect(labels).toContain('key:2');
  });
});
