import { DB_REGISTRY } from '../lib/databaseTypes';
import {
  buildPathHierarchyDatabasePin,
  namespaceRootsFrom,
  pathHierarchyConnectionRoot,
  pathHierarchyRelativeNamespacePath,
} from '../lib/queryContextPath';
import { useSchemaStore } from './schemaStore';
import type { SqlNamespace } from '../lib/sqlNamespace';
import type { QueryPanel } from './panelTypes';

/** One dot-separated identifier part of a qualified SQL name (quoted or bare). */
const QUALIFIED_NAME_PART = '(?:`[^`]+`|"[^"]+"|\\[[^\\]]+\\]|[A-Za-z_][\\w$]*)';
/** A qualified identifier: at least two parts joined by dots. */
const QUALIFIED_NAME = new RegExp(`${QUALIFIED_NAME_PART}(?:\\s*\\.\\s*${QUALIFIED_NAME_PART})+`);

function stripQuote(part: string): string {
  return part
    .trim()
    .replace(/^[`"[]+/, '')
    .replace(/[`"\]]+$/, '');
}

/**
 * The catalog/schema path implied by the SQL's first qualified table
 * reference, when its head segment is a known namespace root/alias/database
 * (`hive.snap.orders` → `['hive','snap']`; the trailing segment is the table).
 * Returns `null` when the SQL carries no such hint — the caller then keeps the
 * explicit panel selection.
 */
function sqlQualifiedNamespacePath(
  sql: string | undefined,
  databases: readonly string[],
  namespaceTree: SqlNamespace,
  pathAliases: Record<string, string>,
): string[] | null {
  if (!sql) return null;
  const match = sql.match(QUALIFIED_NAME);
  if (!match) return null;
  const parts = match[0].split('.').map(stripQuote);
  const [head, ...rest] = parts;
  const roots = namespaceRootsFrom(namespaceTree, pathAliases, databases);
  const headIsKnown = roots.includes(head) || databases.some((db) => db.split(':')[1] === head);
  if (!headIsKnown || rest.length === 0) return null;
  return [head, ...rest.slice(0, -1)];
}

/** Resolve the target database for query execution from panel + schema state. */
export function panelTargetDatabase(panel: QueryPanel, sql?: string): string | null {
  const schemaState = useSchemaStore.getState().schemas.get(panel.dbSessionId);
  const meta = DB_REGISTRY[panel.databaseType];
  if (meta?.namespaceEnsure === 'path-hierarchy') {
    const databases = schemaState?.databases ?? [];
    const pathAliases = schemaState?.pathAliases ?? {};
    const namespaceTree = schemaState?.namespaceTree ?? {};
    const namespaceRootNames = Object.keys(namespaceTree);
    // The explicit panel selection defines the base target. A qualified name
    // in the SQL may *complete* it: typing `hive` in the selectors and writing
    // `SELECT * FROM hive.snap.orders` targets `hive.snap`, so the deeper
    // shared-prefix SQL path extends the selection (never contradicts it).
    const namespacePath = panel.namespacePath ?? [];
    const selected = pathHierarchyRelativeNamespacePath(databases, namespaceTree, namespacePath);
    const fromSql = sqlQualifiedNamespacePath(sql, databases, namespaceTree, pathAliases);
    const extendsSelection =
      selected.length === 0 ||
      (fromSql !== null &&
        fromSql.length > selected.length &&
        selected.every((segment, index) => segment === fromSql[index]));
    const relativeNamespacePath =
      extendsSelection && fromSql !== null && fromSql.length > selected.length ? fromSql : selected;
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
