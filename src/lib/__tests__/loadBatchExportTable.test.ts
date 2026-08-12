import { describe, expect, it, vi } from 'vitest';
import type { ColumnSchema, TableDataResult, TableSchema } from '../../types';
import type { SqlDialectStrategy } from '../sqlDialects';
import {
  loadBatchExportTableData,
  rowsToRecords,
  type LoadBatchExportTableDeps,
} from '../loadBatchExportTable';

const columns: ColumnSchema[] = [
  {
    name: 'id',
    dataType: 'int',
    nullable: false,
    isPrimaryKey: true,
  },
  {
    name: 'name',
    dataType: 'text',
    nullable: true,
  },
];

const schema: TableSchema = {
  tableName: 'users',
  columns,
  primaryKeys: ['id'],
  indexes: [],
  foreignKeys: [],
};

function fakeDialect(extractColumnIndex: number): SqlDialectStrategy {
  return {
    family: 'sqlite',
    ddl: {
      getTableDdlQuery(tableName: string) {
        return {
          sql: `SELECT ddl FROM meta WHERE name='${tableName}'`,
          extractColumnIndex,
        };
      },
    },
    index: {
      supportedIndexMethods: ['btree'],
      getDropIndexSql: () => '',
      getCreateIndexSql: () => '',
    },
    backupOptions: [],
  };
}

function pageResult(
  page: number,
  pageSize: number,
  rows: (string | number | null)[][],
  totalRows?: number,
): TableDataResult {
  return {
    columns,
    rows,
    page,
    pageSize,
    totalRows,
  };
}

function makeDeps(overrides: Partial<LoadBatchExportTableDeps> = {}): LoadBatchExportTableDeps {
  return {
    getSchema: vi.fn().mockResolvedValue(schema),
    getDdl: vi.fn().mockResolvedValue('CREATE TABLE users (id INT);'),
    getTableData: vi.fn().mockResolvedValue(pageResult(0, 500, [[1, 'Alice']], 1)),
    getDialect: vi.fn().mockReturnValue(fakeDialect(0)),
    ...overrides,
  };
}

describe('rowsToRecords', () => {
  it('maps 2D rows to named records and null-fills missing cells', () => {
    expect(rowsToRecords(columns, [[1, 'Alice'], [2, null], [3]])).toEqual([
      { id: 1, name: 'Alice' },
      { id: 2, name: null },
      { id: 3, name: null },
    ]);
  });
});

describe('loadBatchExportTableData', () => {
  it('merges multiple pages until a short page', async () => {
    const getTableData = vi
      .fn()
      .mockResolvedValueOnce(
        pageResult(
          0,
          2,
          [
            [1, 'a'],
            [2, 'b'],
          ],
          5,
        ),
      )
      .mockResolvedValueOnce(
        pageResult(1, 2, [
          [3, 'c'],
          [4, 'd'],
        ]),
      )
      .mockResolvedValueOnce(pageResult(2, 2, [[5, 'e']]));

    const result = await loadBatchExportTableData({
      connectionId: 'c1',
      tableName: 'users',
      databaseType: 'sqlite',
      pageSize: 2,
      deps: makeDeps({ getTableData }),
    });

    expect(getTableData).toHaveBeenCalledTimes(3);
    expect(getTableData).toHaveBeenNthCalledWith(1, {
      connectionId: 'c1',
      table: 'users',
      page: 0,
      pageSize: 2,
      filters: [],
      sorts: [],
      skipCount: false,
    });
    expect(getTableData).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({
        page: 1,
        skipCount: true,
      }),
    );
    expect(result.rows).toEqual([
      { id: 1, name: 'a' },
      { id: 2, name: 'b' },
      { id: 3, name: 'c' },
      { id: 4, name: 'd' },
      { id: 5, name: 'e' },
    ]);
    expect(result.columns).toEqual([{ name: 'id' }, { name: 'name' }]);
    expect(result.ddl).toBe('CREATE TABLE users (id INT);');
  });

  it('stops when accumulated rows reach known totalRows', async () => {
    const getTableData = vi
      .fn()
      .mockResolvedValueOnce(
        pageResult(
          0,
          2,
          [
            [1, 'a'],
            [2, 'b'],
          ],
          4,
        ),
      )
      .mockResolvedValueOnce(
        pageResult(1, 2, [
          [3, 'c'],
          [4, 'd'],
        ]),
      );

    const result = await loadBatchExportTableData({
      connectionId: 'c1',
      tableName: 'users',
      pageSize: 2,
      deps: makeDeps({ getTableData }),
    });

    expect(getTableData).toHaveBeenCalledTimes(2);
    expect(result.rows).toHaveLength(4);
  });

  it('truncates at maxRows mid-page', async () => {
    const getTableData = vi
      .fn()
      .mockResolvedValueOnce(
        pageResult(
          0,
          3,
          [
            [1, 'a'],
            [2, 'b'],
            [3, 'c'],
          ],
          10,
        ),
      )
      .mockResolvedValueOnce(
        pageResult(1, 3, [
          [4, 'd'],
          [5, 'e'],
          [6, 'f'],
        ]),
      );

    const result = await loadBatchExportTableData({
      connectionId: 'c1',
      tableName: 'users',
      pageSize: 3,
      maxRows: 4,
      deps: makeDeps({ getTableData }),
    });

    expect(getTableData).toHaveBeenCalledTimes(2);
    expect(result.rows).toEqual([
      { id: 1, name: 'a' },
      { id: 2, name: 'b' },
      { id: 3, name: 'c' },
      { id: 4, name: 'd' },
    ]);
  });

  it('does not fetch further pages once maxRows is hit on a full first page', async () => {
    const getTableData = vi.fn().mockResolvedValueOnce(
      pageResult(
        0,
        3,
        [
          [1, 'a'],
          [2, 'b'],
          [3, 'c'],
        ],
        100,
      ),
    );

    const result = await loadBatchExportTableData({
      connectionId: 'c1',
      tableName: 'users',
      pageSize: 3,
      maxRows: 3,
      deps: makeDeps({ getTableData }),
    });

    expect(getTableData).toHaveBeenCalledTimes(1);
    expect(result.rows).toHaveLength(3);
  });

  it('sets ddl to null when databaseType is missing (no dialect)', async () => {
    const getDdl = vi.fn();
    const getDialect = vi.fn();
    const result = await loadBatchExportTableData({
      connectionId: 'c1',
      tableName: 'users',
      deps: makeDeps({ getDdl, getDialect }),
    });

    expect(getDialect).not.toHaveBeenCalled();
    expect(getDdl).not.toHaveBeenCalled();
    expect(result.ddl).toBeNull();
    expect(result.rows).toEqual([{ id: 1, name: 'Alice' }]);
  });

  it('sets ddl to null when dialect is unavailable for the database type', async () => {
    const getDdl = vi.fn();
    const getDialect = vi.fn().mockReturnValue(null);
    const result = await loadBatchExportTableData({
      connectionId: 'c1',
      tableName: 'users',
      databaseType: 'redis',
      deps: makeDeps({ getDdl, getDialect }),
    });

    expect(getDialect).toHaveBeenCalledWith('redis');
    expect(getDdl).not.toHaveBeenCalled();
    expect(result.ddl).toBeNull();
  });

  it('sets ddl to null when DDL fetch fails but still returns schema and rows', async () => {
    const getDdl = vi.fn().mockRejectedValue(new Error('ddl boom'));
    const result = await loadBatchExportTableData({
      connectionId: 'c1',
      tableName: 'users',
      databaseType: 'sqlite',
      deps: makeDeps({ getDdl }),
    });

    expect(getDdl).toHaveBeenCalled();
    expect(result.ddl).toBeNull();
    expect(result.tableName).toBe('users');
    expect(result.columns).toEqual([{ name: 'id' }, { name: 'name' }]);
    expect(result.rows).toEqual([{ id: 1, name: 'Alice' }]);
  });

  it('treats blank DDL string as null', async () => {
    const getDdl = vi.fn().mockResolvedValue('   ');
    const result = await loadBatchExportTableData({
      connectionId: 'c1',
      tableName: 'users',
      databaseType: 'mysql',
      deps: makeDeps({ getDdl }),
    });

    expect(result.ddl).toBeNull();
  });

  it('passes extractor that reads the dialect DDL column', async () => {
    const getDdl = vi.fn(
      async (
        _connectionId: string,
        _tableName: string,
        _sql: string,
        extractor: (rows: unknown[][]) => string,
      ) => extractor([['ignored', 'CREATE TABLE users ()']]),
    );

    const result = await loadBatchExportTableData({
      connectionId: 'c1',
      tableName: 'users',
      databaseType: 'mysql',
      deps: makeDeps({
        getDdl,
        getDialect: () => fakeDialect(1),
      }),
    });

    expect(result.ddl).toBe('CREATE TABLE users ()');
    expect(getDdl).toHaveBeenCalledWith(
      'c1',
      'users',
      expect.stringContaining('users'),
      expect.any(Function),
    );
  });

  it('extractor stringifies non-string DDL cells and empty when missing', async () => {
    const getDdl = vi.fn(
      async (_c: string, _t: string, _sql: string, extractor: (rows: unknown[][]) => string) => {
        expect(extractor([[42]])).toBe('42');
        expect(extractor([])).toBe('');
        expect(extractor([[null]])).toBe('');
        return 'ok';
      },
    );

    const result = await loadBatchExportTableData({
      connectionId: 'c1',
      tableName: 'users',
      databaseType: 'sqlite',
      deps: makeDeps({ getDdl, getDialect: () => fakeDialect(0) }),
    });

    expect(result.ddl).toBe('ok');
  });
});
