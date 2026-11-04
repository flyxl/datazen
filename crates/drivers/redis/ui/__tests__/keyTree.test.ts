import { describe, expect, it } from 'vitest';
import {
  DEFAULT_SEPARATOR,
  SEPARATOR_CHOICES,
  buildFlatTreeRows,
  buildServerTreeRows,
  folderLabel,
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

/** `kind:identity` per row — compact, order-explicit shape of a folded tree. */
function shape(rows: ReturnType<typeof buildServerTreeRows>): string[] {
  return rows.map((r) =>
    r.kind === 'folder' ? `folder:${r.path}@${r.depth}` : `key:${r.entry.key}@${r.depth}`,
  );
}

describe('separator plumbing (R3 / D-3)', () => {
  it('offers exactly the three documented separators and defaults to colon', () => {
    // The choices are the ones `list_children` is actually sent as `sep`, so a
    // fourth entry here without server support would be a silent no-op.
    expect(SEPARATOR_CHOICES).toEqual([':', '.', '/']);
    expect(DEFAULT_SEPARATOR).toBe(':');
  });

  it('labels a prefix with the configured separator but survives a foreign one', () => {
    expect(folderLabel('app:cache:', ':')).toBe('cache');
    expect(folderLabel('app/cache/', '/')).toBe('cache');
    expect(folderLabel('a.b.c.', '.')).toBe('c');
    expect(folderLabel('bare')).toBe('bare');
    // A level folded before the preference changed still gets a sane label.
    expect(folderLabel('a:b:c', '.')).toBe('c');
  });
});

describe('buildServerTreeRows re-groups when the separator changes (R3 state machine)', () => {
  /*
   * The live path (D-3), rewritten in coder round 1 to stop testing dead code.
   * Until now this "state machine" case exercised `buildKeyTreeRows`, a
   * client-side fold that has no production caller: the tree is grouped
   * *server-side* by `list_children(prefix, sep)`, and `buildServerTreeRows`
   * renders the levels that came back. So the machine under test is
   * `(levels, expanded, sep) → rows`, and its three elements are:
   *
   *  - enter: a different `sep` means a different level record came back, and the
   *    rows are re-derived from it immediately — no cached grouping to go stale;
   *  - in-state: a folder row's label is cut on the separator that folded it,
   *    with a documented fallback for a level folded under another separator;
   *  - exit: expanded prefixes belong to the previous separator. Until the new
   *    level arrives, that subtree renders as *nothing* rather than resurrecting
   *    stale children (and the parent folder row survives either way).
   */
  const byColon = { '': { children: [folder('app:', 1), folder('cache:', 1)], done: true } };
  const byDot = { '': { children: [folder('app.', 2)], done: true } };

  it('enter: the same keys, two separators, two different top-level folder sets', () => {
    // Colon grouping sees `app` and `cache` as namespaces; dot grouping folds
    // everything under one `app` namespace holding both keys.
    expect(shape(buildServerTreeRows(byColon, new Set(), ':'))).toEqual([
      'folder:app:@0',
      'folder:cache:@0',
    ]);
    const dotRows = buildServerTreeRows(byDot, new Set(), '.');
    expect(shape(dotRows)).toEqual(['folder:app.@0']);
    expect(dotRows[0]!.kind === 'folder' && dotRows[0]!.count).toBe(2);
  });

  it('in-state: labels follow the separator that folded the level, with a fallback', () => {
    expect(buildServerTreeRows(byColon, new Set(), ':').map((r) => r.label)).toEqual([
      'app',
      'cache',
    ]);
    expect(buildServerTreeRows(byDot, new Set(), '.').map((r) => r.label)).toEqual(['app']);
    // A level folded before the preference changed keeps a sane label rather than
    // a whole-path one: `app:user:` still labels as `user` under `.` because the
    // label cut recognises every separator this module can be asked to group on.
    const foreign = { '': { children: [folder('app:user:', 2)], done: true } };
    expect(buildServerTreeRows(foreign, new Set(), '.')[0]!.label).toBe('user');
  });

  it('exit: a still-expanded prefix from the old separator renders nothing', () => {
    // `app:` was expanded under `:`; the dot-folded scan has no such level, so no
    // children may be painted for it — but the folder row itself stays.
    expect(shape(buildServerTreeRows(byColon, new Set(['app:']), '.'))).toEqual([
      'folder:app:@0',
      'folder:cache:@0',
    ]);
    // Once its own level loads, the expansion is honoured again at depth + 1.
    const withLevel = { ...byColon, 'app:': { children: [leaf('app:user:1')], done: true } };
    expect(shape(buildServerTreeRows(withLevel, new Set(['app:']), ':'))).toEqual([
      'folder:app:@0',
      'key:app:user:1@1',
      'folder:cache:@0',
    ]);
  });

  it('an unfinished level marks its folders `(n+)`; a finished one does not', () => {
    const open = { '': { children: [folder('app:', 2)], done: false } };
    const closed = { '': { children: [folder('app:', 2)], done: true } };
    expect(buildServerTreeRows(open, new Set())[0]).toMatchObject({ partial: true });
    expect(buildServerTreeRows(closed, new Set())[0]).not.toHaveProperty('partial');
  });
});

describe('buildFlatTreeRows', () => {
  it('turns off grouping entirely: depth 0 and the full key name', () => {
    const rows = buildFlatTreeRows([entry('user:profile:1'), entry('plain')]);
    expect(rows.map((r) => (r.kind === 'key' ? `${r.depth}:${r.label}` : 'folder'))).toEqual([
      '0:user:profile:1',
      '0:plain',
    ]);
    expect(rows.every((r) => r.kind === 'key')).toBe(true);
  });

  it('is empty, not undefined, for an unpopulated database', () => {
    expect(buildFlatTreeRows([])).toEqual([]);
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

  it('a child-level leaf keeps its absolute key but shows the last segment', () => {
    // Basis for BUG-001's "fork on kind, not on field presence": the pattern has
    // to be matched against `entry.key` (absolute), never against `label`.
    const levels = {
      '': { children: [folder('app:', 1)], done: true },
      'app:': { children: [leaf('app:user:1')], done: true },
    };
    const rows = buildServerTreeRows(levels, new Set(['app:']), ':');
    const leafRow = rows[1]!;
    expect(leafRow.kind === 'key' && leafRow.entry.key).toBe('app:user:1');
    expect(leafRow.kind === 'key' && leafRow.label).toBe('1');
  });
});
