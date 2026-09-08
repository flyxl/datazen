import type { TableSchema } from './databaseTypes';

export type TableSchemaProvider = (
  dbSessionId: string,
  tableName: string,
  database?: string,
) => Promise<TableSchema | undefined | null>;

let activeTableSchemaProvider: TableSchemaProvider | null = null;

export function setTableSchemaProvider(provider: TableSchemaProvider | null): void {
  activeTableSchemaProvider = provider;
}

export function getCachedTableSchema(
  dbSessionId: string,
  tableName: string,
  database?: string,
): Promise<TableSchema | undefined | null> {
  return activeTableSchemaProvider?.(dbSessionId, tableName, database) ?? Promise.resolve(null);
}
