import { beforeEach, describe, expect, it, vi } from 'vitest';
import { databaseCommands } from '../database';
const targetCommands = vi.hoisted(() => ({
  execute: vi.fn(),
  discovery: vi.fn(),
  metadata: vi.fn(),
}));
vi.mock('../../lib/session/targetCommand', () => ({
  executeTargetCommand: (...args: unknown[]) => targetCommands.execute(...args),
  databaseDiscoveryTarget: (...args: unknown[]) => targetCommands.discovery(...args),
  metadataTarget: (...args: unknown[]) => targetCommands.metadata(...args),
}));

const target = (connectionId: string, database: string | null, schema: string | null) => ({
  connectionId,
  namespace: { database, schema, catalog: null, path: [] },
  object: null,
});

describe('database metadata facade', () => {
  beforeEach(() => {
    targetCommands.execute.mockReset();
    targetCommands.discovery.mockReset();
    targetCommands.metadata.mockReset();
  });

  it('loads database names through the command gateway', async () => {
    const owner = target('cfg-1', null, null);
    targetCommands.discovery.mockReturnValue(owner);
    targetCommands.execute.mockResolvedValue({ databases: ['app'] });
    expect(await databaseCommands.getDatabases('session', 'cfg-1')).toEqual(['app']);
    expect(targetCommands.discovery).toHaveBeenCalledExactlyOnceWith('session', 'cfg-1');
    expect(targetCommands.execute).toHaveBeenCalledExactlyOnceWith(owner, {
      command: 'list_databases',
      input: {},
    });
  });

  it('preserves same-named relations and empty schemas at the tree adapter boundary', async () => {
    const owner = target('cfg-1', 'app', null);
    targetCommands.metadata.mockReturnValue(owner);
    targetCommands.execute.mockResolvedValue({
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
    });
    expect(await databaseCommands.listTables('session', 'app', null, 'cfg-1')).toEqual([
      { name: 'users', schema: 'public', tableType: 'table', rowCount: 2 },
      { name: 'users', schema: 'archive', tableType: 'view', rowCount: null },
      { name: '', schema: 'empty', tableType: 'systemTable', rowCount: null },
    ]);
    expect(targetCommands.metadata).toHaveBeenCalledExactlyOnceWith(
      'session',
      'app',
      null,
      undefined,
      'cfg-1',
    );
    expect(targetCommands.execute).toHaveBeenCalledExactlyOnceWith(
      owner,
      {
        command: 'list_catalog',
        input: { database: 'app', schema: null },
      },
      'commandResult',
      undefined,
    );
  });
});
