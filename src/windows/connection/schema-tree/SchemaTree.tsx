import { DB_REGISTRY } from '../../../lib/databaseTypes';
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
  if (meta?.hasMultiDatabase) {
    return <MultiDatabaseSchemaTree {...props} />;
  }
  return <StandardSchemaTree {...props} isKeyValue={meta?.isKeyValue ?? false} />;
}
