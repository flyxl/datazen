import { relationKey, type RelationRef, type RelationColumns } from '@datazen/driver-sdk';
import { schemaClient } from '@datazen/driver-sdk';
import { capabilitiesForDbSession, relationSchemaFor } from '../lib/driverCapabilities';
import { knownTableNames } from './schemaStoreHelpers';
import type { ConnectionSchemaState } from './schemaStoreState';

/** Legacy name requests resolve only when their catalog identity is unique. */
export function resolveColumnRelations(
  state: ConnectionSchemaState,
  names: readonly string[],
  dbSessionId: string,
  database: string,
  schemaFilter?: string | null,
): RelationRef[] {
  const capabilities = capabilitiesForDbSession(dbSessionId);
  const catalog =
    state.tableCatalogs[database] ??
    (state.currentDatabase === database || state.currentDatabase === null
      ? [...state.tables, ...state.views]
      : []);
  const pathItems =
    state.currentDatabase === database || state.currentDatabase === null ? state.pathItems : {};
  const items = [...catalog, ...Object.values(pathItems).flat()];
  const known = knownTableNames(state.namespaceTree, catalog, [], pathItems);
  const refs: RelationRef[] = [];
  for (const name of new Set(names.map((n) => n.trim()).filter(Boolean))) {
    const candidates = new Map<string, RelationRef>();
    for (const item of items) {
      if (item.name !== name) continue;
      const schema = relationSchemaFor(capabilities, item.schema);
      if (schemaFilter !== undefined && schema !== schemaFilter) continue;
      const ref = { database, schema, name: item.name };
      candidates.set(relationKey({ ...ref, dbSessionId }), ref);
    }
    if (candidates.size === 1) refs.push([...candidates.values()][0]);
    // Path trees may expose names before publishing their metadata rows.
    else if (
      candidates.size === 0 &&
      known.has(name) &&
      capabilities?.hasSchemaLevel !== true &&
      schemaFilter === undefined &&
      (state.namespaceOwnedByPlugin ||
        Object.keys(state.pathAliases).length > 0 ||
        (catalog.length === 0 && state.currentDatabase === null))
    ) {
      refs.push({ database, schema: null, name });
    }
  }
  return refs;
}

/** One typed read path; transport batching is bounded independently of drivers. */
export async function loadRelationColumns(
  dbSessionId: string,
  relations: readonly RelationRef[],
): Promise<RelationColumns[]> {
  const values: RelationColumns[] = [];
  for (let offset = 0; offset < relations.length; offset += 256) {
    const batch = relations.slice(offset, offset + 256);
    const expected = new Set(batch.map((ref) => relationKey({ ...ref, dbSessionId })));
    const response = await schemaClient.readColumns(dbSessionId, batch);
    for (const row of response.results) {
      if (row.status === 'ok' && expected.has(relationKey({ ...row.value.ref, dbSessionId }))) {
        values.push(row.value);
      }
    }
  }
  return values;
}

/** Bare-name consumers see only uniquely resolved relations in their context. */
export function projectRelationColumns(
  state: ConnectionSchemaState,
  dbSessionId: string,
  database: string,
  schemaFilter?: string | null,
): { columnMap: Record<string, string[]>; typedColumnMap: Record<string, Record<string, string>> } {
  const names = Object.values(state.relationColumns).map((value) => value.ref.name);
  const refs = resolveColumnRelations(state, names, dbSessionId, database, schemaFilter);
  const columnMap: Record<string, string[]> = {};
  const typedColumnMap: Record<string, Record<string, string>> = {};
  for (const ref of refs) {
    const value = state.relationColumns[relationKey({ ...ref, dbSessionId })];
    if (!value) continue;
    columnMap[ref.name] = value.columns.map((column) => column.name);
    typedColumnMap[ref.name] = Object.fromEntries(
      value.columns.map((column) => [column.name, column.dataType]),
    );
  }
  return { columnMap, typedColumnMap };
}
