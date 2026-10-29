import { afterEach, describe, expect, it, vi, beforeEach } from 'vitest';
import { clearPlatformServices, setPlatformServices } from '@datazen/backend-client';
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

describe('fetchRelationDdl', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns empty string if dbSessionId or tableName is missing', async () => {
    expect(await fetchRelationDdl('', 'users', '', 'mysql')).toBe('');
    expect(await fetchRelationDdl('sess-1', '', 'app', 'mysql')).toBe('');
  });

  it('calls getCachedDDL with mysql SHOW CREATE TABLE query', async () => {
    vi.mocked(schemaCache.getCachedDDL).mockImplementation(async (_sid, _name, _sql, extractor) => {
      return extractor([['users', 'CREATE TABLE users (id INT)']]);
    });

    const res = await fetchRelationDdl('sess-1', 'users', 'app', 'mysql');
    expect(res).toBe('CREATE TABLE users (id INT)');
    expect(schemaCache.getCachedDDL).toHaveBeenCalledWith(
      'sess-1',
      'users',
      expect.stringContaining('SHOW CREATE TABLE `users`'),
      expect.any(Function),
      'app',
      { namespacePath: ['app'] },
    );
  });

  it('calls getCachedDDL with sqlite schema query', async () => {
    vi.mocked(schemaCache.getCachedDDL).mockImplementation(async (_sid, _name, _sql, extractor) => {
      return extractor([['CREATE TABLE sqlite_users (id INT)']]);
    });

    const res = await fetchRelationDdl('sess-1', 'sqlite_users', 'main', 'sqlite');
    expect(res).toBe('CREATE TABLE sqlite_users (id INT)');
    expect(schemaCache.getCachedDDL).toHaveBeenCalledWith(
      'sess-1',
      'sqlite_users',
      expect.stringContaining("WHERE type='table' AND name='sqlite_users'"),
      expect.any(Function),
      'main',
      { namespacePath: ['main'] },
    );
  });

  it('falls back to databaseCommands.getObjectDdl if dialect is not found or fails', async () => {
    vi.mocked(schemaCache.getCachedDDL).mockRejectedValue(new Error('Dialect query failed'));
    vi.mocked(databaseCommands.getObjectDdl).mockResolvedValue('CREATE TABLE fallback (id INT)');

    const res = await fetchRelationDdl('sess-1', 'fallback', 'app', 'custom-db');
    expect(res).toBe('CREATE TABLE fallback (id INT)');
    expect(databaseCommands.getObjectDdl).toHaveBeenCalledWith(
      'sess-1',
      'table',
      'fallback',
      'app',
      null,
      null,
      null,
      'app',
    );
  });
  it('passes database to getCachedDDL namespacePath and getObjectDdl schema fallback', async () => {
    vi.mocked(schemaCache.getCachedDDL).mockImplementation(async (_sid, _name, _sql, extractor) => {
      return extractor([['users', 'CREATE TABLE users (id INT)']]);
    });

    await fetchRelationDdl('sess-1', 'users', 'tenant_db', 'mysql');
    expect(schemaCache.getCachedDDL).toHaveBeenCalledWith(
      'sess-1',
      'users',
      expect.any(String),
      expect.any(Function),
      'tenant_db',
      { namespacePath: ['tenant_db'] },
    );

    vi.mocked(schemaCache.getCachedDDL).mockRejectedValueOnce(new Error('fail'));
    vi.mocked(databaseCommands.getObjectDdl).mockResolvedValueOnce('CREATE TABLE users (id INT)');
    await fetchRelationDdl('sess-1', 'users', 'tenant_db', 'custom');
    expect(databaseCommands.getObjectDdl).toHaveBeenCalledWith(
      'sess-1',
      'table',
      'users',
      'tenant_db',
      null,
      null,
      null,
      'tenant_db',
    );
  });
});

describe('copyToClipboard', () => {
  // Form-split R6: the native fallback leg rides the PlatformServices port —
  // the tests bind the port with the invoke mock behind it, which keeps the
  // old wire assertions (command name + payload) intact.
  const invoke = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    clearPlatformServices();
  });

  afterEach(() => {
    clearPlatformServices();
    delete (navigator as { clipboard?: unknown }).clipboard;
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

  it('reaches the host clipboard command when navigator.clipboard fails', async () => {
    const writeTextMock = vi.fn().mockRejectedValue(new Error('Permission denied'));
    Object.assign(navigator, {
      clipboard: {
        writeText: writeTextMock,
      },
    });

    invoke.mockResolvedValueOnce(undefined);
    setPlatformServices({
      writeClipboard: (text: string) => invoke('write_clipboard', { text }),
    } as never);

    const ok = await copyToClipboard('tauri ddl');
    expect(ok).toBe(true);
    expect(invoke).toHaveBeenCalledWith('write_clipboard', { text: 'tauri ddl' });
  });

  it('falls back to execCommand if navigator.clipboard and the host command throw', async () => {
    invoke.mockRejectedValueOnce(new Error('Tauri not running'));

    const writeTextMock = vi.fn().mockRejectedValue(new Error('Permission denied'));
    Object.assign(navigator, {
      clipboard: {
        writeText: writeTextMock,
      },
    });
    const execCommandMock = vi.fn().mockReturnValue(true);
    document.execCommand = execCommandMock;

    setPlatformServices({
      writeClipboard: (text: string) => invoke('write_clipboard', { text }),
    } as never);

    const ok = await copyToClipboard('fallback ddl');
    expect(ok).toBe(true);
    expect(execCommandMock).toHaveBeenCalledWith('copy');
  });
});
