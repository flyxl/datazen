import { DB_REGISTRY } from '../lib/databaseTypes';
import {
  buildPathHierarchyDatabasePin,
  pathHierarchyConnectionRoot,
  pathHierarchyRelativeNamespacePath,
} from '../lib/queryContextPath';
import { useSchemaStore } from './schemaStore';
import type { QueryPanel } from './panelTypes';

/** Resolve the target database for query execution from panel + schema state. */
export function panelTargetDatabase(panel: QueryPanel, _sql?: string): string | null {
  const schemaState = useSchemaStore.getState().schemas.get(panel.dbSessionId);
  const meta = DB_REGISTRY[panel.databaseType];
  if (meta?.namespaceEnsure === 'path-hierarchy') {
    const databases = schemaState?.databases ?? [];
    const pathAliases = schemaState?.pathAliases ?? {};
    const namespaceTree = schemaState?.namespaceTree ?? {};
    const namespaceRootNames = Object.keys(namespaceTree);
    // Only explicit panel selection defines the execution target. Qualified
    // names in unfinished SQL remain completion/browsing hints.
    const namespacePath = panel.namespacePath ?? [];
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
