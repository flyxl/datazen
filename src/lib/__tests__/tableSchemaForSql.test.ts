import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invalidateSchemaCache } from '../schemaCache';
import {
  driverTableRefsToTry,
  fetchTableSchemaForSqlGeneration,
  generateTableSqlWithFallbacks,
  buildPseudoTableSchema,
} from '../tableSchemaForSql';

const getTableSchema = vi.fn();
const getColumns = vi.fn();

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getTableSchema: (...args: unknown[]) => getTableSchema(...args),
    getColumns: (...args: unknown[]) => getColumns(...args),
  },
}));

describe('tableSchemaForSql', () => {
  beforeEach(() => {
    getTableSchema.mockReset();
    getColumns.mockReset();
    invalidateSchemaCache('sess-1');
  });

  describe('driverTableRefsToTry', () => {
    it('prefers schema.table for postgresql', () => {
      expect(driverTableRefsToTry('users', 'public', 'postgresql')).toEqual([
        'public.users',
        'users',
      ]);
    });

    it('uses bare name only for mysql even when schema is set', () => {
      expect(driverTableRefsToTry('users', 'mydb', 'mysql')).toEqual(['users']);
    });

    it('returns bare name when schema is absent', () => {
      expect(driverTableRefsToTry('users', undefined, 'postgresql')).toEqual(['users']);
    });
  });

  describe('fetchTableSchemaForSqlGeneration', () => {
    it('returns schema from getTableSchema with bare tableName override', async () => {
      getTableSchema.mockResolvedValueOnce({
        tableName: 'public.users',
        columns: [{ name: 'id', dataType: 'int', nullable: false }],
        primaryKeys: ['id'],
        indexes: [],
        foreignKeys: [],
      });

      const result = await fetchTableSchemaForSqlGeneration({
        dbSessionId: 'sess-1',
        tableName: 'users',
        schema: 'public',
        database: 'app',
        databaseType: 'postgresql',
      });

      expect(getTableSchema).toHaveBeenCalledWith('sess-1', 'public.users', 'app');
      expect(result?.tableName).toBe('users');
      expect(result?.columns).toHaveLength(1);
    });

    it('falls back to bare name when qualified ref returns empty columns', async () => {
      getTableSchema
        .mockResolvedValueOnce({
          tableName: 'public.users',
          columns: [],
          primaryKeys: [],
          indexes: [],
          foreignKeys: [],
        })
        .mockResolvedValueOnce({
          tableName: 'users',
          columns: [{ name: 'id', dataType: 'int', nullable: false }],
          primaryKeys: ['id'],
          indexes: [],
          foreignKeys: [],
        });

      const result = await fetchTableSchemaForSqlGeneration({
        dbSessionId: 'sess-1',
        tableName: 'users',
        schema: 'public',
        databaseType: 'postgresql',
      });

      expect(getTableSchema).toHaveBeenCalledTimes(2);
      expect(result?.columns.map((c) => c.name)).toEqual(['id']);
    });

    it('falls back to getColumns for mysql with bare name', async () => {
      getTableSchema.mockResolvedValueOnce({
        tableName: 'users',
        columns: [],
        primaryKeys: [],
        indexes: [],
        foreignKeys: [],
      });
      getColumns.mockResolvedValueOnce(['id', 'name']);

      const result = await fetchTableSchemaForSqlGeneration({
        dbSessionId: 'sess-1',
        tableName: 'users',
        schema: 'mydb',
        database: 'mydb',
        databaseType: 'mysql',
      });

      expect(getTableSchema).toHaveBeenCalledWith('sess-1', 'users', 'mydb');
      expect(getColumns).toHaveBeenCalledWith('sess-1', 'users', 'mydb');
      expect(result?.columns.map((c) => c.name)).toEqual(['id', 'name']);
    });

    it('uses columnMap when IPC calls fail or return empty', async () => {
      getTableSchema.mockRejectedValueOnce(new Error('fail'));
      getColumns.mockRejectedValueOnce(new Error('fail'));

      const result = await fetchTableSchemaForSqlGeneration({
        dbSessionId: 'sess-1',
        tableName: 'users',
        databaseType: 'mysql',
        columnMap: { users: ['id', 'email'] },
      });

      expect(result).toEqual(buildPseudoTableSchema('users', ['id', 'email']));
    });
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
