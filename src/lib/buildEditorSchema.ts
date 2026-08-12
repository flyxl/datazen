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

export function buildEditorSchema({
  namespaceTree,
  tables,
  views,
  columnMap,
  currentDatabase,
}: BuildEditorSchemaInput): SqlNamespace {
  const base = isNamespaceEmpty(namespaceTree) ? flatFromTables(tables, views) : namespaceTree;
  return hoistNamespaceChild(overlayColumnMap(base, columnMap), currentDatabase);
}
