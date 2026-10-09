import { transitionalTransport } from './desktopBinding';
import type {
  CatalogScope,
  ListCatalogOutput,
  ReadColumnsOutput,
  RelationRef,
  RelationSchema,
  MetadataRefreshScope,
} from '../types/schemaMetadata';

/**
 * Metadata targeting lives exclusively inside the command input.
 *
 * Thin re-export over the bound transport (§7.2 薄再导出过渡): same command
 * name, same `{ request: { dbSessionId, command, input } }` payload, same
 * `{ data }` unwrap. The unwrap stays here rather than moving into the
 * transport because it is a property of the legacy gateway's envelope, and the
 * kernel surface will not have one.
 */
async function execute<T>(
  dbSessionId: string,
  command: string,
  input: Record<string, unknown>,
): Promise<T> {
  const result = await transitionalTransport().call('execute_driver_command', {
    request: { dbSessionId, command, input },
  });
  return (result as { data: T }).data;
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
