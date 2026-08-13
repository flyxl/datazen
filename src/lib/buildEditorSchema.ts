import type { TableInfo } from '../types';
import { overlayColumnMap, type SqlNamespace } from './sqlNamespace';

export interface BuildEditorSchemaInput {
  namespaceTree: SqlNamespace;
  tables: TableInfo[];
  views: TableInfo[];
  columnMap: Record<string, string[]>;
  /** Hoist this database's children to the top level for unqualified completion. */
  currentDatabase?: string | null;
}

function isNamespaceEmpty(tree: SqlNamespace): boolean {
  return !Array.isArray(tree) && Object.keys(tree).length === 0;
}

function flatFromTables(tables: TableInfo[], views: TableInfo[]): SqlNamespace {
  const result: Record<string, SqlNamespace> = {};
  for (const t of [...tables, ...views]) {
    result[t.name] = [];
  }
  return result;
}

/** Copy `tree[childKey]` entries to the root without replacing existing keys. */
export function hoistNamespaceChild(
  tree: SqlNamespace,
  childKey: string | null | undefined,
): SqlNamespace {
  if (!childKey || Array.isArray(tree)) return tree;
  const child = tree[childKey];
  if (child == null) return tree;
  if (Array.isArray(child)) {
    const out: Record<string, SqlNamespace> = { ...tree };
    for (const name of child) {
      if (!(name in out)) out[name] = [];
    }
    return out;
  }
  const out: Record<string, SqlNamespace> = { ...tree };
  for (const [key, value] of Object.entries(child)) {
    if (!(key in out)) out[key] = value;
  }
  return out;
}

/**
 * PostgreSQL (and similar) nest tables under schema names. CodeMirror only
 * completes unqualified identifiers from the root, so copy table leaves up.
 */
export function hoistNestedTableLeaves(tree: SqlNamespace): SqlNamespace {
  if (Array.isArray(tree)) return tree;
  const out: Record<string, SqlNamespace> = { ...tree };
  for (const child of Object.values(tree)) {
    if (Array.isArray(child)) continue;
    for (const [name, value] of Object.entries(child)) {
      if (name in out) continue;
      if (Array.isArray(value)) out[name] = value;
    }
  }
  return out;
}

function mergeUnqualifiedTables(
  tree: SqlNamespace,
  tables: TableInfo[],
  views: TableInfo[],
): SqlNamespace {
  if (tables.length === 0 && views.length === 0) return tree;
  const out: Record<string, SqlNamespace> = Array.isArray(tree) ? {} : { ...tree };
  for (const item of [...tables, ...views]) {
    if (!(item.name in out)) out[item.name] = [];
  }
  return out;
}

export function buildEditorSchema({
  namespaceTree,
  tables,
  views,
  columnMap,
  currentDatabase,
}: BuildEditorSchemaInput): SqlNamespace {
  const base = isNamespaceEmpty(namespaceTree) ? flatFromTables(tables, views) : namespaceTree;
  const withDb = hoistNamespaceChild(base, currentDatabase);
  const withNested = hoistNestedTableLeaves(withDb);
  const withTables = mergeUnqualifiedTables(withNested, tables, views);
  return overlayColumnMap(withTables, columnMap);
}
