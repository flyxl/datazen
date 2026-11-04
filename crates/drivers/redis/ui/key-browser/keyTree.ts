import type { KeyEntry } from '@datazen/driver-sdk';
import type { ChildEntry } from '../shared/redisInvoke';

/** Flat row or expandable namespace folder in the key browser. */
export type KeyTreeRow =
  | {
      kind: 'folder';
      path: string;
      label: string;
      depth: number;
      count: number;
      /**
       * `count` is a lower bound: the level that produced this folder has an
       * open scan cursor, so the subtree holds keys not scanned yet (I-4 / D-5
       * renders these as `(n+)` rather than presenting a partial subset as a
       * total).
       */
      partial?: boolean;
      /**
       * Rendered only as a **path breadcrumb** of a row the R2 pattern matched
       * (redis-tree-ui-BUG-001, see `keyTreeFilter.ts`): the folder does not
       * match the pattern itself — it only owns rows that do. Such a row is
       * decoration: not clickable, no checkbox, not counted by
       * `data-row-count`, not part of any selection set.
       */
      breadcrumb?: boolean;
    }
  | { kind: 'key'; entry: KeyEntry; depth: number; label: string };

/**
 * Separator the tree is currently built with (PRD §3.2 R3). One character, and
 * the *caller* supplies it — there is no implicit default any more, because a
 * per-connection preference that silently falls back to `:` makes "the tree
 * re-groups instantly when I change the separator" untestable and untraceable.
 */
export const SEPARATOR_CHOICES = [':', '.', '/'] as const;
export const DEFAULT_SEPARATOR = ':';

/**
 * True when `key` lives under `folderPrefix` — the folder-checkbox cascade and
 * the BUG-001 visible-key set both decide subtree membership with it.
 *
 * A prefix carries its trailing separator (`app:`), but it may have been folded
 * with a separator other than the one now configured (per-connection preference,
 * a level loaded before it changed), and a key may simply not repeat it. A bare
 * `startsWith` would let `app` swallow `apple`, so the rule is: the remainder
 * must be non-empty **and** the boundary must be a separator — the configured
 * one, or any of {@link SEPARATOR_CHOICES} for a foreign-folded prefix.
 */
export function keyUnderFolder(key: string, folderPrefix: string, sep?: string): boolean {
  if (!key.startsWith(folderPrefix)) return false;
  const rest = key.slice(folderPrefix.length);
  if (rest.length === 0) return false;
  if (ANY_SEPARATOR_CHAR.test(folderPrefix[folderPrefix.length - 1] ?? '')) return true;
  return rest[0] === sep || ANY_SEPARATOR_CHAR.test(rest[0] ?? '');
}

/** Strip every separator this module can be asked to group on. */
const ANY_SEPARATOR = /[:./]+$/;
const ANY_SEPARATOR_CHAR = /[:./]/;

/**
 * Last namespace segment of a full key or folder prefix (for row labels).
 *
 * `sep` is the configured grouping separator and gets the first say, but the
 * scan can hand back a prefix built with another separator (server default `:`,
 * per-connection override), so the fallback still recognises every char in
 * {@link SEPARATOR_CHOICES} rather than mis-labelling `a.b.c` as one segment.
 */
export function folderLabel(prefix: string, sep?: string): string {
  const trimmed = prefix.replace(ANY_SEPARATOR, '');
  const configured = sep && sep.length > 0 ? trimmed.lastIndexOf(sep) : -1;
  let last = configured;
  for (let i = trimmed.length - 1; i >= 0; i--) {
    if (ANY_SEPARATOR_CHAR.test(trimmed[i]!)) {
      last = Math.max(last, i);
      break;
    }
  }
  return last >= 0 ? trimmed.slice(last + 1) : trimmed;
}

function childToKeyEntry(child: Extract<ChildEntry, { kind: 'key' }>): KeyEntry {
  return {
    key: child.key,
    keyType: child.keyType,
    ttl: child.ttl,
    size: child.logicalLen,
    preview: '',
  };
}

/**
 * Flatten server-driven `list_children` levels into tree rows. Folders render
 * with an estimated count (`n+`) unless their level is `done`; expanded folders
 * recurse into their cached level.
 *
 * `partial` comes from the *parent* level's cursor, not the folder's own state:
 * an unexpanded folder's count is whatever the parent scan had accumulated when
 * it met the key, so a still-scanning parent can only ever under-report (I-4).
 */
export function buildServerTreeRows(
  levels: Record<string, { children: ChildEntry[]; done: boolean }>,
  expanded: Set<string>,
  sep?: string,
  prefix = '',
  depth = 0,
): KeyTreeRow[] {
  const level = levels[prefix];
  if (!level) return [];
  const rows: KeyTreeRow[] = [];
  const partial = !level.done;
  for (const child of level.children) {
    if (child.kind === 'folder') {
      rows.push({
        kind: 'folder',
        path: child.prefix,
        label: folderLabel(child.prefix, sep),
        depth,
        count: child.count,
        ...(partial ? { partial: true } : {}),
      });
      if (expanded.has(child.prefix)) {
        rows.push(...buildServerTreeRows(levels, expanded, sep, child.prefix, depth + 1));
      }
    } else {
      rows.push({
        kind: 'key',
        entry: childToKeyEntry(child),
        depth,
        label: folderLabel(child.key, sep),
      });
    }
  }
  return rows;
}

/**
 * Flat ("列表") rows for R3's view switch: every loaded key at depth 0 with its
 * *full* name as the label, i.e. no grouping at all. Same row spec and same
 * selection semantics as the tree — only the namespace folding is turned off,
 * which is what makes it the escape hatch for keys whose names contain the
 * configured separator.
 */
export function buildFlatTreeRows(keys: KeyEntry[]): KeyTreeRow[] {
  return keys.map((entry) => ({ kind: 'key', entry, depth: 0, label: entry.key }));
}

export const KEY_TYPE_FILTERS = [
  { value: 'all', labelKey: 'redis.typeAll' },
  { value: 'string', labelKey: 'redis.typeString' },
  { value: 'hash', labelKey: 'redis.typeHash' },
  { value: 'list', labelKey: 'redis.typeList' },
  { value: 'set', labelKey: 'redis.typeSet' },
  { value: 'zset', labelKey: 'redis.typeZset' },
  { value: 'stream', labelKey: 'redis.typeStream' },
] as const;
