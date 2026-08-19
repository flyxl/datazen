import { useEffect, type ComponentType } from 'react';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import type { DatabaseTypeMeta } from '../../../lib/databaseMeta';
import type { SchemaTreeNodeKind } from '../../../lib/schemaTreeContextMenu';
import { getPluginSchemaTree } from '../../../plugins/generated';
import { useSchemaStore } from '../../../stores/schemaStore';
import type { DatabaseType } from '../../../types';
import { UnifiedSchemaTree } from './UnifiedSchemaTree';

export type SchemaTreeNodeContextMenuPayload = {
  kind: SchemaTreeNodeKind;
  name: string;
  x: number;
  y: number;
  schema?: string;
};

export interface SchemaTreeProps {
  connectionId: string;
  databaseType: DatabaseType;
  initialDatabase?: string;
  selectedTable: string | null;
  searchQuery: string;
  onSelectTable: (table: string, schema?: string) => void;
  onNodeContextMenu?: (payload: SchemaTreeNodeContextMenuPayload) => void;
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
      return <CustomSchemaTreeHost PluginTree={PluginTree} {...props} />;
    }
  }

  // Domain is not a logical DB — don't pass it as preferredDatabase.
  const treeProps =
    meta?.databaseFieldType === 'domain' ? { ...props, initialDatabase: undefined } : props;
  return <UnifiedSchemaTree {...treeProps} isKeyValue={meta?.isKeyValue ?? false} />;
}

/** Ensure schemaStore.connectionId is set for custom trees (column autocomplete). */
function CustomSchemaTreeHost({
  PluginTree,
  ...props
}: SchemaTreeProps & { PluginTree: ComponentType<Record<string, unknown>> }) {
  useEffect(() => {
    useSchemaStore.setState({
      connectionId: props.connectionId,
      databaseType: props.databaseType,
    });
  }, [props.connectionId, props.databaseType]);

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
