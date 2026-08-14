import type { TableInfo } from '../types';
import { isLeaf, overlayColumnMap, type SqlNamespace } from './sqlNamespace';

export interface BuildEditorSchemaInput {
  namespaceTree: SqlNamespace;
  tables: TableInfo[];
  views: TableInfo[];
  columnMap: Record<string, string[]>;
  /** Hoist this database's children to the top level for unqualified completion. */
  currentDatabase?: string | null;
  /** Catalog/schema path from the query toolbar — hoist each segment in order. */
  hoistPath?: readonly string[];
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
  if (!childKey || isLeaf(tree)) return tree;
  const child = tree[childKey];
  if (child == null) return tree;
  if (isLeaf(child)) {
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

/** Copy each path segment's children to the root (catalog → schema → tables). */
export function hoistNamespacePath(
  tree: SqlNamespace,
  path: readonly string[] | undefined,
): SqlNamespace {
  let out = tree;
  for (const seg of path ?? []) {
    out = hoistNamespaceChild(out, seg);
  }
  return out;
}

/**
 * Nest tables under schema / catalog / schema. CodeMirror only completes
 * unqualified identifiers from the root, so copy table leaves up at any depth.
 */
export function hoistNestedTableLeaves(tree: SqlNamespace): SqlNamespace {
  if (isLeaf(tree)) return tree;
  const out: Record<string, SqlNamespace> = { ...tree };
  const walk = (node: SqlNamespace) => {
    if (Array.isArray(node)) return;
    for (const [name, value] of Object.entries(node)) {
      if (Array.isArray(value)) {
        if (!(name in out)) out[name] = value;
      } else {
        walk(value);
      }
    }
  };
  walk(tree);
  return out;
}

function mergeUnqualifiedTables(
  tree: SqlNamespace,
  tables: TableInfo[],
  views: TableInfo[],
): SqlNamespace {
  if (tables.length === 0 && views.length === 0) return tree;
  const out: Record<string, SqlNamespace> = isLeaf(tree) ? {} : { ...tree };
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
  hoistPath,
}: BuildEditorSchemaInput): SqlNamespace {
  const base = isNamespaceEmpty(namespaceTree) ? flatFromTables(tables, views) : namespaceTree;
  const withDb = hoistNamespaceChild(base, currentDatabase);
  const withPath = hoistNamespacePath(withDb, hoistPath);
  const withNested = hoistNestedTableLeaves(withPath);
  const withTables = mergeUnqualifiedTables(withNested, tables, views);
  return overlayColumnMap(withTables, columnMap);
}
