import { beforeEach, describe, expect, it, vi } from 'vitest';
import { loadRelationColumns, resolveColumnRelations } from '../schemaColumnLoader';
import { createEmptyConnectionSchema } from '../schemaStoreState';
import type { RelationRef } from '@datazen/driver-sdk';

const readColumns = vi.hoisted(() => vi.fn());
vi.mock('@datazen/driver-sdk', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/driver-sdk')>()),
  schemaClient: { readColumns },
}));
vi.mock('../../lib/driverCapabilities', () => ({
  capabilitiesForDbSession: () => ({ hasSchemaLevel: true }),
  relationSchemaFor: (_capabilities: unknown, schema: string | null | undefined) => schema ?? null,
}));

describe('relation column loading', () => {
  beforeEach(() => {
    readColumns.mockReset();
  });

  it('uses the requested catalog without importing another database path cache', () => {
    const state = createEmptyConnectionSchema();
    state.currentDatabase = 'active';
    state.tableCatalogs.target = [
      { name: 'users', schema: 'public', tableType: 'table', rowCount: null },
    ];
    state.pathItems.foreign = [
      { name: 'users', schema: 'archive', tableType: 'table', rowCount: null },
    ];
    expect(resolveColumnRelations(state, ['users'], 'session', 'target')).toEqual([
      { database: 'target', schema: 'public', name: 'users' },
    ]);
    state.tableCatalogs.target.push({
      name: 'users',
      schema: 'archive',
      tableType: 'table',
      rowCount: null,
    });
    expect(resolveColumnRelations(state, ['users'], 'session', 'target')).toEqual([]);
    expect(resolveColumnRelations(state, ['users'], 'session', 'target', 'archive')).toEqual([
      { database: 'target', schema: 'archive', name: 'users' },
    ]);
  });

  it('bounds transport batches and accepts only successful requested identities', async () => {
    const refs: RelationRef[] = Array.from({ length: 257 }, (_, index) => ({
      database: 'app',
      schema: 'public',
      name: `table_${index}`,
    }));
    readColumns.mockImplementation(async (_session: string, relations: RelationRef[]) => ({
      results: [
        { status: 'ok', value: { ref: relations[0], columns: [], primaryKeys: [] } },
        { status: 'error', ref: relations[1], error: { code: 'read-failed', message: 'denied' } },
        {
          status: 'ok',
          value: { ref: { ...relations[0], schema: 'foreign' }, columns: [], primaryKeys: [] },
        },
      ],
    }));
    const values = await loadRelationColumns('session', refs);
    expect(readColumns.mock.calls.map((call) => (call[1] as RelationRef[]).length)).toEqual([
      256, 1,
    ]);
    expect(values.map((value) => value.ref)).toEqual([refs[0], refs[256]]);
  });
});
