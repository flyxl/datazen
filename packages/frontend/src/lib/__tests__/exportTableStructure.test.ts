import { describe, expect, it, vi } from 'vitest';
import { exportTableStructureToFile } from '../exportTableStructure';

vi.mock('../sqlDialects', () => ({
  getSqlDialect: (type: string) => {
    if (type === 'mysql') {
      return {
        ddl: {
          getTableDdlQuery: (tableName: string) => ({
            sql: `SHOW CREATE TABLE ${tableName}`,
            extractColumnIndex: 1,
          }),
        },
      };
    }
    return null;
  },
}));

describe('exportTableStructureToFile (item 6: thin shell over C)', () => {
  it('returns unsupported when dialect missing', async () => {
    const exportStructure = vi.fn();
    const result = await exportTableStructureToFile({
      dbSessionId: 'c1',
      tableName: 'users',
      database: 'app',
      databaseType: 'mongodb' as never,
      getDdl: vi.fn(),
      exportStructure,
    });
    expect(result).toBe('unsupported');
    expect(exportStructure).not.toHaveBeenCalled();
  });

  it('fetches DDL and delegates the save to the Rust structure_only export', async () => {
    const getDdl = vi.fn().mockResolvedValue('CREATE TABLE users (id INT)');
    const exportStructure = vi.fn().mockResolvedValue('saved');
    const result = await exportTableStructureToFile({
      dbSessionId: 'c1',
      tableName: 'users',
      database: 'app',
      databaseType: 'mysql',
      schema: 'public',
      getDdl,
      exportStructure,
    });
    expect(result).toBe('saved');
    expect(getDdl).toHaveBeenCalledWith(
      'c1',
      'users',
      'SHOW CREATE TABLE users',
      expect.any(Function),
      'app',
    );
    expect(exportStructure).toHaveBeenCalledWith({
      dbSessionId: 'c1',
      databaseType: 'mysql',
      database: 'app',
      schema: 'public',
      tableName: 'users',
      ddl: 'CREATE TABLE users (id INT)',
    });
    expect(exportStructure).toHaveBeenCalledTimes(1);
  });

  it('returns cancelled when the Rust export is cancelled', async () => {
    const result = await exportTableStructureToFile({
      dbSessionId: 'c1',
      tableName: 'users',
      database: 'app',
      databaseType: 'mysql',
      getDdl: vi.fn().mockResolvedValue('CREATE TABLE t (id INT)'),
      exportStructure: vi.fn().mockResolvedValue('cancelled'),
    });
    expect(result).toBe('cancelled');
  });

  it('passes a blank DDL through for the backend placeholder (old behaviour)', async () => {
    // Old G saved `-- DDL unavailable for users` itself; now the blank ddl
    // reaches C as a missing ddl and the Rust planner emits the identical
    // placeholder (`resolve_ddl` in crates/datazen/src/export/mod.rs).
    const exportStructure = vi.fn().mockResolvedValue('saved');
    const result = await exportTableStructureToFile({
      dbSessionId: 'c1',
      tableName: 'users',
      database: 'app',
      databaseType: 'mysql',
      getDdl: vi.fn().mockResolvedValue('  '),
      exportStructure,
    });
    expect(result).toBe('saved');
    expect(exportStructure).toHaveBeenCalledWith(
      expect.objectContaining({ tableName: 'users', ddl: '  ' }),
    );
  });

  it('extracts non-string DDL values through the dialect column', async () => {
    const exportStructure = vi.fn().mockResolvedValue('saved');
    const getDdl = vi
      .fn()
      .mockImplementationOnce(
        async (
          _dbSessionId: string,
          _tableName: string,
          _sql: string,
          extract: (rows: unknown[][]) => string,
        ) => extract([[null, 42]]),
      )
      .mockImplementationOnce(
        async (
          _dbSessionId: string,
          _tableName: string,
          _sql: string,
          extract: (rows: unknown[][]) => string,
        ) => extract([[null, null]]),
      );
    const baseOptions = {
      dbSessionId: 'c1',
      tableName: 'users',
      database: 'app',
      databaseType: 'mysql' as const,
      getDdl,
      exportStructure,
    };
    await exportTableStructureToFile(baseOptions);
    expect(exportStructure).toHaveBeenLastCalledWith(
      expect.objectContaining({ ddl: '42' }),
    );
    await exportTableStructureToFile(baseOptions);
    expect(exportStructure).toHaveBeenLastCalledWith(expect.objectContaining({ ddl: '' }));
  });

  it('propagates DDL fetch errors', async () => {
    const exportStructure = vi.fn();
    await expect(
      exportTableStructureToFile({
        dbSessionId: 'c1',
        tableName: 'users',
        database: 'app',
        databaseType: 'mysql',
        getDdl: vi.fn().mockRejectedValue(new Error('ddl failed')),
        exportStructure,
      }),
    ).rejects.toThrow('ddl failed');
    expect(exportStructure).not.toHaveBeenCalled();
  });
});
