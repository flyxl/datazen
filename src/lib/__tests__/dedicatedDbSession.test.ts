import { beforeEach, describe, expect, it, vi } from 'vitest';
import { connectionCommands } from '../../commands/connection';
import { ensureDedicatedSession, listDatabasesDedicated } from '../dedicatedDbSession';

const mockGetDatabases = vi.fn();

vi.mock('../../commands/connection', () => ({
  connectionCommands: {
    connectDedicated: vi.fn(),
    releaseConnection: vi.fn(),
    pingConnection: vi.fn(),
  },
}));

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getDatabases: (...args: unknown[]) => mockGetDatabases(...args),
  },
}));

describe('ensureDedicatedSession', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(connectionCommands.connectDedicated).mockResolvedValue('sqlite-session');
    mockGetDatabases.mockResolvedValue(['app', 'other']);
  });

  it('uses the connection configuration for a SQLite catalog alias', async () => {
    const session = await ensureDedicatedSession(null, 'sqlite-file', 'main', null);
    expect(connectionCommands.connectDedicated).toHaveBeenCalledWith('sqlite-file', undefined);
    expect(session).toEqual({
      connectionId: 'sqlite-file',
      database: 'main',
      dbSessionId: 'sqlite-session',
    });
  });

  it('pins a SQL database selection to its dedicated connection', async () => {
    await ensureDedicatedSession(null, 'pg', 'app_db');
    expect(connectionCommands.connectDedicated).toHaveBeenCalledWith('pg', 'app_db');
  });

  it('lists databases for the requested owner and releases the temporary session', async () => {
    await expect(listDatabasesDedicated('pg', 'app_db')).resolves.toEqual({
      databases: ['app', 'other'],
      dbSessionId: null,
    });

    expect(connectionCommands.connectDedicated).toHaveBeenCalledWith('pg', 'app_db');
    expect(mockGetDatabases).toHaveBeenCalledWith('sqlite-session', 'pg');
    expect(connectionCommands.releaseConnection).toHaveBeenCalledWith('sqlite-session');
  });
});
