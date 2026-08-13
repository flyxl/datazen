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

describe('exportTableStructureToFile', () => {
  it('returns unsupported when dialect missing', async () => {
    const result = await exportTableStructureToFile({
      connectionId: 'c1',
      tableName: 'users',
      databaseType: 'mongodb' as never,
      getDdl: vi.fn(),
      saveText: vi.fn(),
    });
    expect(result).toBe('unsupported');
  });

  it('fetches DDL and saves via dialog', async () => {
    const getDdl = vi.fn().mockResolvedValue('CREATE TABLE users (id INT)');
    const saveText = vi.fn().mockResolvedValue(true);
    const result = await exportTableStructureToFile({
      connectionId: 'c1',
      tableName: 'users',
      databaseType: 'mysql',
      getDdl,
      saveText,
    });
    expect(result).toBe('saved');
    expect(getDdl).toHaveBeenCalledWith(
      'c1',
      'users',
      'SHOW CREATE TABLE users',
      expect.any(Function),
    );
    expect(saveText).toHaveBeenCalledWith('CREATE TABLE users (id INT)', 'users.sql', 'SQL', [
      'sql',
    ]);
  });

  it('returns cancelled when save dialog dismissed', async () => {
    const result = await exportTableStructureToFile({
      connectionId: 'c1',
      tableName: 'users',
      databaseType: 'mysql',
      getDdl: vi.fn().mockResolvedValue('CREATE TABLE t (id INT)'),
      saveText: vi.fn().mockResolvedValue(false),
    });
    expect(result).toBe('cancelled');
  });

  it('uses fallback comment when DDL blank', async () => {
    const saveText = vi.fn().mockResolvedValue(true);
    await exportTableStructureToFile({
      connectionId: 'c1',
      tableName: 'users',
      databaseType: 'mysql',
      getDdl: vi.fn().mockResolvedValue('  '),
      saveText,
    });
    expect(saveText.mock.calls[0][0]).toBe('-- DDL unavailable for users');
  });
});
