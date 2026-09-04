import { DB_REGISTRY } from '../lib/databaseTypes';
import {
  buildPathHierarchyDatabasePin,
  inferSqlRelationPath,
  namespaceRootsFrom,
  pathHierarchyConnectionRoot,
  pathHierarchyRelativeNamespacePath,
  resolveQueryContextPath,
} from '../lib/queryContextPath';
import { useSchemaStore } from './schemaStore';
import type { QueryPanel } from './panelTypes';

/** Resolve the target database for query execution from panel + schema state. */
export function panelTargetDatabase(panel: QueryPanel, sql?: string): string | null {
  const schemaState = useSchemaStore.getState().schemas.get(panel.dbSessionId);
  const meta = DB_REGISTRY[panel.databaseType];
  if (meta?.namespaceEnsure === 'path-hierarchy') {
    const databases = schemaState?.databases ?? [];
    const pathAliases = schemaState?.pathAliases ?? {};
    const namespaceTree = schemaState?.namespaceTree ?? {};
    const roots = namespaceRootsFrom(namespaceTree, pathAliases, databases);
    const namespaceRootNames = Object.keys(namespaceTree);
    const rootSet = new Set(roots);
    let fromSql = sql?.trim()
      ? resolveQueryContextPath(sql, { databases, namespaceRoots: roots })
      : null;
    if (sql?.trim()) {
      const relation = inferSqlRelationPath(sql);
      if (relation.length >= 3) {
        const inferred = relation.slice(0, -1);
        if (!fromSql || inferred.length > fromSql.length) fromSql = inferred;
      } else if (relation.length >= 2) {
        const inferred = relation.slice(0, -1);
        if (inferred[0] && rootSet.has(inferred[0])) {
          if (!fromSql || inferred.length > fromSql.length) fromSql = inferred;
        }
      }
    }
    const namespacePath = fromSql ?? panel.namespacePath ?? [];
    const relativeNamespacePath = pathHierarchyRelativeNamespacePath(
      databases,
      namespaceTree,
      namespacePath,
    );
    const root = pathHierarchyConnectionRoot(
      databases,
      panel.database,
      schemaState?.currentDatabase ?? null,
      pathAliases,
      namespaceRootNames,
    );
    if (!root && relativeNamespacePath.length === 0) return null;
    if (!root) return relativeNamespacePath.join('/');
    return buildPathHierarchyDatabasePin(root, relativeNamespacePath);
  }
  if (panel.database?.trim()) return panel.database;
  return schemaState?.currentDatabase ?? null;
}

/** Resolve the PG-family schema envelope field for query execution. */
export function panelTargetSchema(panel: QueryPanel): string | null {
  if (panel.schema?.trim()) return panel.schema;
  return useSchemaStore.getState().schemas.get(panel.dbSessionId)?.currentSchema ?? null;
}
