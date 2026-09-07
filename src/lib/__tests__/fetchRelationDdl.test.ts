import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fetchRelationDdl, copyToClipboard } from '../fetchRelationDdl';
import * as schemaCache from '../schemaCache';
import { databaseCommands } from '../../commands/database';

vi.mock('../schemaCache', () => ({
  getCachedDDL: vi.fn(),
}));

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getObjectDdl: vi.fn(),
  },
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('fetchRelationDdl', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns empty string if dbSessionId or tableName is missing', async () => {
    expect(await fetchRelationDdl('', 'users', 'mysql')).toBe('');
    expect(await fetchRelationDdl('sess-1', '', 'mysql')).toBe('');
  });

  it('calls getCachedDDL with mysql SHOW CREATE TABLE query', async () => {
    vi.mocked(schemaCache.getCachedDDL).mockImplementation(async (_sid, _name, _sql, extractor) => {
      return extractor([['users', 'CREATE TABLE users (id INT)']]);
    });

    const res = await fetchRelationDdl('sess-1', 'users', 'mysql');
    expect(res).toBe('CREATE TABLE users (id INT)');
    expect(schemaCache.getCachedDDL).toHaveBeenCalledWith(
      'sess-1',
      'users',
      expect.stringContaining('SHOW CREATE TABLE `users`'),
      expect.any(Function),
      undefined,
    );
  });

  it('calls getCachedDDL with sqlite schema query', async () => {
    vi.mocked(schemaCache.getCachedDDL).mockImplementation(async (_sid, _name, _sql, extractor) => {
      return extractor([['CREATE TABLE sqlite_users (id INT)']]);
    });

    const res = await fetchRelationDdl('sess-1', 'sqlite_users', 'sqlite');
    expect(res).toBe('CREATE TABLE sqlite_users (id INT)');
    expect(schemaCache.getCachedDDL).toHaveBeenCalledWith(
      'sess-1',
      'sqlite_users',
      expect.stringContaining("WHERE type='table' AND name='sqlite_users'"),
      expect.any(Function),
      undefined,
    );
  });

  it('falls back to databaseCommands.getObjectDdl if dialect is not found or fails', async () => {
    vi.mocked(schemaCache.getCachedDDL).mockRejectedValue(new Error('Dialect query failed'));
    vi.mocked(databaseCommands.getObjectDdl).mockResolvedValue('CREATE TABLE fallback (id INT)');

    const res = await fetchRelationDdl('sess-1', 'fallback', 'custom-db');
    expect(res).toBe('CREATE TABLE fallback (id INT)');
    expect(databaseCommands.getObjectDdl).toHaveBeenCalledWith('sess-1', 'table', 'fallback', null);
  });
  it('passes database to getCachedDDL namespacePath and getObjectDdl schema fallback', async () => {
    vi.mocked(schemaCache.getCachedDDL).mockImplementation(async (_sid, _name, _sql, extractor) => {
      return extractor([['users', 'CREATE TABLE users (id INT)']]);
    });

    await fetchRelationDdl('sess-1', 'users', 'mysql', false, undefined, 'tenant_db');
    expect(schemaCache.getCachedDDL).toHaveBeenCalledWith(
      'sess-1',
      'users',
      expect.any(String),
      expect.any(Function),
      { namespacePath: ['tenant_db'] },
    );

    vi.mocked(schemaCache.getCachedDDL).mockRejectedValueOnce(new Error('fail'));
    vi.mocked(databaseCommands.getObjectDdl).mockResolvedValueOnce('CREATE TABLE users (id INT)');
    await fetchRelationDdl('sess-1', 'users', 'custom', false, undefined, 'tenant_db');
    expect(databaseCommands.getObjectDdl).toHaveBeenCalledWith(
      'sess-1',
      'table',
      'users',
      'tenant_db',
    );
  });
});

describe('copyToClipboard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns false for empty text', async () => {
    expect(await copyToClipboard('')).toBe(false);
  });

  it('uses navigator.clipboard.writeText when available', async () => {
    const writeTextMock = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, {
      clipboard: {
        writeText: writeTextMock,
      },
    });

    const ok = await copyToClipboard('test ddl');
    expect(ok).toBe(true);
    expect(writeTextMock).toHaveBeenCalledWith('test ddl');
  });

  it('invokes Tauri write_clipboard command when navigator.clipboard fails', async () => {
    const writeTextMock = vi.fn().mockRejectedValue(new Error('Permission denied'));
    Object.assign(navigator, {
      clipboard: {
        writeText: writeTextMock,
      },
    });

    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    const ok = await copyToClipboard('tauri ddl');
    expect(ok).toBe(true);
    expect(invoke).toHaveBeenCalledWith('write_clipboard', { text: 'tauri ddl' });
  });

  it('falls back to execCommand if navigator.clipboard and Tauri invoke throw', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockRejectedValueOnce(new Error('Tauri not running'));

    const writeTextMock = vi.fn().mockRejectedValue(new Error('Permission denied'));
    Object.assign(navigator, {
      clipboard: {
        writeText: writeTextMock,
      },
    });
    const execCommandMock = vi.fn().mockReturnValue(true);
    document.execCommand = execCommandMock;

    const ok = await copyToClipboard('fallback ddl');
    expect(ok).toBe(true);
    expect(execCommandMock).toHaveBeenCalledWith('copy');
  });
});
