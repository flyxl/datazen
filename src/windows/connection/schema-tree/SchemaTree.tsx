import { DB_REGISTRY } from '../../../lib/databaseTypes';
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
        />
      );
    }
  }

  if (meta?.hasMultiDatabase) {
    return <MultiDatabaseSchemaTree {...props} />;
  }
  return <StandardSchemaTree {...props} isKeyValue={meta?.isKeyValue ?? false} />;
}
