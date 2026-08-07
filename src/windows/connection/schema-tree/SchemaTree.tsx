import { DB_REGISTRY } from '../../../lib/databaseTypes';
import type { DatabaseTypeMeta } from '../../../lib/databaseMeta';
import { getPluginSchemaTree } from '../../../plugins/generated';
import type { DatabaseType } from '../../../types';
import { MultiDatabaseSchemaTree } from './MultiDatabaseSchemaTree';
import { StandardSchemaTree } from './StandardSchemaTree';

export interface SchemaTreeProps {
  connectionId: string;
  databaseType: DatabaseType;
  initialDatabase?: string;
  selectedTable: string | null;
  searchQuery: string;
  onSelectTable: (table: string, schema?: string) => void;
  onTableContextMenu?: (tableName: string, x: number, y: number) => void;
}

/**
 * Multi-DB tree when the driver supports it, unless connection.database is a
 * *logical* DB name that should lock the sidebar.
 *
 * Kiwi (`databaseFieldType: 'domain'`) stores the instance domain in
 * `connection.database` — that must not force StandardSchemaTree.
 */
export function shouldUseMultiDatabaseTree(
  meta: Pick<DatabaseTypeMeta, 'hasMultiDatabase' | 'databaseFieldType'> | undefined,
  initialDatabase?: string,
): boolean {
  if (!meta?.hasMultiDatabase) return false;
  if (meta.databaseFieldType === 'domain') return true;
  return !initialDatabase?.trim();
}

export function SchemaTree(props: SchemaTreeProps) {
  const meta = DB_REGISTRY[props.databaseType];

  if (meta?.schemaTreeMode === 'custom') {
    const PluginTree = getPluginSchemaTree(props.databaseType);
    if (PluginTree) {
      return (
        <PluginTree
          connectionId={props.connectionId}
          databaseType={props.databaseType}
          onSelectTable={props.onSelectTable}
          selectedTable={props.selectedTable}
          searchQuery={props.searchQuery}
        />
      );
    }
  }

  if (shouldUseMultiDatabaseTree(meta, props.initialDatabase)) {
    // Domain is not a logical DB — don't pass it as preferredDatabase.
    const treeProps =
      meta?.databaseFieldType === 'domain'
        ? { ...props, initialDatabase: undefined }
        : props;
    return <MultiDatabaseSchemaTree {...treeProps} />;
  }
  return <StandardSchemaTree {...props} isKeyValue={meta?.isKeyValue ?? false} />;
}
