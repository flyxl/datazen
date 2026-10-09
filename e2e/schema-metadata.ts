import { invokeBackend } from './helpers.js';

export interface CatalogMetadata {
  database: string;
  schemas: string[];
  relations: Array<{
    ref: { database: string; schema: string | null; name: string };
    kind: string;
    rowCount: number | null;
  }>;
}

export async function listDatabases(target: { dbSessionId: string }): Promise<string[]> {
  const result = await invokeBackend<{ data: { databases: string[] } }>('execute_driver_command', {
    request: { dbSessionId: target.dbSessionId, command: 'list_databases', input: {} },
  });
  return result.data.databases;
}

export async function readCatalog(target: {
  dbSessionId: string;
  database: string;
  schema?: string | null;
}): Promise<CatalogMetadata> {
  const result = await invokeBackend<{ data: CatalogMetadata }>('execute_driver_command', {
    request: {
      dbSessionId: target.dbSessionId,
      command: 'list_catalog',
      input: { database: target.database, schema: target.schema ?? null },
    },
  });
  return result.data;
}
