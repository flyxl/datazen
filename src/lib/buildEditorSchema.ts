import type { TableInfo } from '../types';
import { overlayColumnMap, type SqlNamespace } from './sqlNamespace';

export interface BuildEditorSchemaInput {
  namespaceTree: SqlNamespace;
  tables: TableInfo[];
  views: TableInfo[];
  columnMap: Record<string, string[]>;
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

export function buildEditorSchema({
  namespaceTree,
  tables,
  views,
  columnMap,
}: BuildEditorSchemaInput): SqlNamespace {
  const base = isNamespaceEmpty(namespaceTree)
    ? flatFromTables(tables, views)
    : namespaceTree;
  return overlayColumnMap(base, columnMap);
}
