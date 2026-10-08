import { beforeEach, describe, expect, it, vi } from 'vitest';
import { databaseCommands } from '../database';
const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

describe('database metadata facade', () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it('loads database names through the command gateway', async () => {
    invoke.mockResolvedValue({ data: { databases: ['app'] } });
    expect(await databaseCommands.getDatabases('session')).toEqual(['app']);
    expect(invoke).toHaveBeenCalledExactlyOnceWith('execute_driver_command', {
      request: { dbSessionId: 'session', command: 'list_databases', input: {} },
    });
  });

  it('preserves same-named relations and empty schemas at the tree adapter boundary', async () => {
    invoke.mockResolvedValue({
      data: {
        database: 'app',
        schemas: ['public', 'archive', 'empty'],
        relations: [
          { ref: { database: 'app', schema: 'public', name: 'users' }, kind: 'table', rowCount: 2 },
          {
            ref: { database: 'app', schema: 'archive', name: 'users' },
            kind: 'view',
            rowCount: null,
          },
        ],
      },
    });
    expect(await databaseCommands.listTables('session', 'app')).toEqual([
      { name: 'users', schema: 'public', tableType: 'table', rowCount: 2 },
      { name: 'users', schema: 'archive', tableType: 'view', rowCount: null },
      { name: '', schema: 'empty', tableType: 'systemTable', rowCount: null },
    ]);
    expect(invoke).toHaveBeenCalledExactlyOnceWith('execute_driver_command', {
      request: {
        dbSessionId: 'session',
        command: 'list_catalog',
        input: { database: 'app', schema: null },
      },
    });
  });
});
