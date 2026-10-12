import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invalidateSchemaCache } from '../schemaCache';
import {
  fetchTableSchemaForSqlGeneration,
  generateTableSqlWithFallbacks,
  buildPseudoTableSchema,
} from '../tableSchemaForSql';
const { readSchema, readColumns, capabilities } = vi.hoisted(() => ({
  readSchema: vi.fn(),
  readColumns: vi.fn(),
  capabilities: vi.fn(),
}));
vi.mock('../session/targetCommand', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../session/targetCommand')>()),
  metadataScopeKey: async (dbSessionId: string, connectionId?: string) =>
    `${connectionId ?? 'missing-owner'}:${dbSessionId}`,
  metadataTarget: (
    dbSessionId: string,
    database: string | null,
    schema: string | null,
    table?: string,
    connectionId?: string,
  ) => ({
    connectionId: connectionId ?? dbSessionId,
    namespace: { database, schema, catalog: null, path: [] },
    object: table ? { kind: 'relation', name: table, signature: null } : null,
  }),
  readTargetSchema: (...args: unknown[]) => readSchema(...args),
  executeTargetCommand: async (
    target: import('@datazen/backend-client').ExecutionTarget,
    call: import('@datazen/backend-client').CommandCall,
  ) => {
    if (call.command !== 'read_relation_columns')
      throw new Error(`Unexpected command: ${call.command}`);
    const input = call.input as { relations: import('@datazen/driver-sdk').RelationRef[] };
    return readColumns(target.connectionId, input.relations);
  },
}));
vi.mock('../driverCapabilities', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../driverCapabilities')>()),
  capabilitiesForDbSession: capabilities,
}));
describe('tableSchemaForSql', () => {
  beforeEach(() => {
    readSchema.mockReset();
    readColumns.mockReset();
    capabilities.mockReturnValue({ hasSchemaLevel: true });
    invalidateSchemaCache('sess-1');
  });
  it('reads a bare exact name once, preserving schema and dotted identifiers', async () => {
    readSchema.mockResolvedValueOnce(buildPseudoTableSchema('users.v2', ['id']));
    const result = await fetchTableSchemaForSqlGeneration({
      dbSessionId: 'sess-1',
      connectionId: 'cfg-1',
      tableName: 'users.v2',
      schema: 'archive',
      database: 'app',
      databaseType: 'postgresql',
    });
    expect(readSchema).toHaveBeenCalledExactlyOnceWith(
      'sess-1',
      'users.v2',
      'app',
      'archive',
      'cfg-1',
    );
    expect(readColumns).not.toHaveBeenCalled();
    expect(result?.tableName).toBe('users.v2');
  });
  it('reads typed columns in the same identity when full structure is unavailable', async () => {
    capabilities.mockReturnValue({ hasSchemaLevel: false });
    readSchema.mockRejectedValueOnce(new Error('unsupported'));
    const ref = { database: 'mydb', schema: null, name: 'users' };
    const columns = [{ name: 'id', dataType: 'int', nullable: false }];
    readColumns.mockResolvedValueOnce({
      results: [{ status: 'ok', value: { ref, columns, primaryKeys: ['id'] } }],
    });
    const result = await fetchTableSchemaForSqlGeneration({
      dbSessionId: 'sess-1',
      connectionId: 'cfg-1',
      tableName: 'users',
      schema: 'mydb',
      database: 'mydb',
      databaseType: 'mysql',
    });
    expect(readSchema).toHaveBeenCalledExactlyOnceWith('sess-1', 'users', 'mydb', null, 'cfg-1');
    expect(readColumns).toHaveBeenCalledExactlyOnceWith('cfg-1', [ref]);
    expect(result?.columns).toEqual(columns);
    expect(result?.primaryKeys).toEqual(['id']);
  });
  it('uses the caller snapshot after metadata failure without retrying name variants', async () => {
    readSchema.mockRejectedValueOnce(new Error('fail'));
    readColumns.mockResolvedValueOnce({
      results: [
        {
          status: 'error',
          ref: { database: 'app', schema: 'public', name: 'users' },
          error: { code: 'read-failed', message: 'fail' },
        },
      ],
    });
    const result = await fetchTableSchemaForSqlGeneration({
      dbSessionId: 'sess-1',
      connectionId: 'cfg-1',
      tableName: 'users',
      schema: 'public',
      database: 'app',
      databaseType: 'postgresql',
      columnMap: { users: ['id', 'email'] },
    });
    expect(result).toEqual(buildPseudoTableSchema('users', ['id', 'email']));
    expect(readSchema).toHaveBeenCalledExactlyOnceWith('sess-1', 'users', 'app', 'public', 'cfg-1');
    expect(readColumns).toHaveBeenCalledExactlyOnceWith('cfg-1', [
      { database: 'app', schema: 'public', name: 'users' },
    ]);
  });
  describe('generateTableSqlWithFallbacks', () => {
    const schema = buildPseudoTableSchema('users', ['id', 'name']);

    it('generates full INSERT when columns are present', () => {
      const sql = generateTableSqlWithFallbacks(schema, 'insert', 'postgresql', {
        tableName: 'users',
        schemaPrefix: 'public',
      });
      expect(sql).toContain('"id"');
      expect(sql).toContain('"name"');
      expect(sql).toContain('INSERT INTO "public"."users"');
    });

    it('falls back to SELECT * when schema is missing', () => {
      const sql = generateTableSqlWithFallbacks(null, 'select', 'postgresql', {
        tableName: 'users',
        schemaPrefix: 'public',
      });
      expect(sql).toBe('SELECT *\nFROM "public"."users";');
    });

    it('falls back to comment for INSERT when schema is missing', () => {
      const sql = generateTableSqlWithFallbacks(null, 'insert', 'mysql', {
        tableName: 'users',
        tableRefLabel: 'mydb.users',
      });
      expect(sql).toBe('/* No column metadata available for mydb.users */');
    });
  });
});
