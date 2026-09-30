import { invoke } from '@tauri-apps/api/core';
import type {
  CatalogScope,
  ListCatalogOutput,
  ReadColumnsOutput,
  RelationRef,
  RelationSchema,
  MetadataRefreshScope,
} from '../types/schemaMetadata';

/** Metadata targeting lives exclusively inside the command input. */
async function execute<T>(
  dbSessionId: string,
  command: string,
  input: Record<string, unknown>,
): Promise<T> {
  const result = await invoke<{ data: T }>('execute_driver_command', {
    request: { dbSessionId, command, input },
  });
  return result.data;
}

export const schemaClient = {
  listDatabases: (dbSessionId: string) =>
    execute<{ databases: string[] }>(dbSessionId, 'list_databases', {}),
  listCatalog: (dbSessionId: string, scope: CatalogScope) =>
    execute<ListCatalogOutput>(dbSessionId, 'list_catalog', { ...scope }),
  readColumns: (dbSessionId: string, relations: readonly RelationRef[]) =>
    execute<ReadColumnsOutput>(dbSessionId, 'read_relation_columns', { relations }),
  readSchema: (dbSessionId: string, relation: RelationRef) =>
    execute<{ value: RelationSchema }>(dbSessionId, 'read_relation_schema', { relation }),
  refresh: (dbSessionId: string, scope: MetadataRefreshScope) =>
    execute<{ revision: number }>(dbSessionId, 'refresh_schema_metadata', { scope }),
};
