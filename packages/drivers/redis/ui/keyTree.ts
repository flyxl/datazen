import type { KeyEntry } from '../../../../src/types';
import type { ChildEntry } from './redisInvoke';

/** Flat row or expandable namespace folder in the key browser. */
export type KeyTreeRow =
  | { kind: 'folder'; path: string; label: string; depth: number; count: number }
  | { kind: 'key'; entry: KeyEntry; depth: number; label: string };

const DEFAULT_SEPARATORS = [':', '.'];

/** Split a key into namespace segments using the first matching separator. */
export function splitKeyNamespace(
  key: string,
  separators: string[] = DEFAULT_SEPARATORS,
): string[] {
  for (const sep of separators) {
    if (key.includes(sep)) {
      return key.split(sep).filter((s) => s.length > 0);
    }
  }
  return [key];
}

/**
 * Build a flat list of tree rows from key entries.
 * Folders are collapsed unless their path is in `expanded`.
 */
export function buildKeyTreeRows(
  keys: KeyEntry[],
  expanded: Set<string>,
  separators: string[] = DEFAULT_SEPARATORS,
): KeyTreeRow[] {
  type Node = {
    label: string;
    path: string;
    children: Map<string, Node>;
    entry?: KeyEntry;
  };

  const root: Node = { label: '', path: '', children: new Map() };

  for (const entry of keys) {
    const parts = splitKeyNamespace(entry.key, separators);
    let node = root;
    let path = '';
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i]!;
      path = path ? `${path}${separators[0] ?? ':'}${part}` : part;
      const isLeaf = i === parts.length - 1;
      if (!node.children.has(part)) {
        node.children.set(part, { label: part, path, children: new Map() });
      }
      const child = node.children.get(part)!;
      if (isLeaf) {
        child.entry = entry;
      }
      node = child;
    }
  }

  const rows: KeyTreeRow[] = [];

  function walk(node: Node, depth: number) {
    const folders = [...node.children.values()].sort((a, b) => a.label.localeCompare(b.label));
    for (const child of folders) {
      const hasChildren = child.children.size > 0;
      if (hasChildren) {
        const count = countLeaves(child);
        rows.push({
          kind: 'folder',
          path: child.path,
          label: child.label,
          depth,
          count,
        });
        if (expanded.has(child.path)) {
          walk(child, depth + 1);
        }
      } else if (child.entry) {
        rows.push({
          kind: 'key',
          entry: child.entry,
          depth,
          label: child.label,
        });
      }
    }
  }

  walk(root, 0);
  return rows;
}

function countLeaves(node: { children: Map<string, unknown>; entry?: KeyEntry }): number {
  let n = node.entry ? 1 : 0;
  for (const child of node.children.values()) {
    n += countLeaves(child as { children: Map<string, unknown>; entry?: KeyEntry });
  }
  return n;
}

/** Last namespace segment of a full key or folder prefix (for row labels). */
export function folderLabel(prefix: string): string {
  const trimmed = prefix.replace(/[:.]+$/, '');
  const i = Math.max(trimmed.lastIndexOf(':'), trimmed.lastIndexOf('.'));
  return i >= 0 ? trimmed.slice(i + 1) : trimmed;
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
 * with an estimated count (`~`) unless their level is `done`; expanded folders
 * recurse into their cached level.
 */
export function buildServerTreeRows(
  levels: Record<string, { children: ChildEntry[]; done: boolean }>,
  expanded: Set<string>,
  prefix = '',
  depth = 0,
): KeyTreeRow[] {
  const level = levels[prefix];
  if (!level) return [];
  const rows: KeyTreeRow[] = [];
  for (const child of level.children) {
    if (child.kind === 'folder') {
      rows.push({
        kind: 'folder',
        path: child.prefix,
        label: folderLabel(child.prefix),
        depth,
        count: child.count,
      });
      if (expanded.has(child.prefix)) {
        rows.push(...buildServerTreeRows(levels, expanded, child.prefix, depth + 1));
      }
    } else {
      rows.push({
        kind: 'key',
        entry: childToKeyEntry(child),
        depth,
        label: folderLabel(child.key),
      });
    }
  }
  return rows;
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
