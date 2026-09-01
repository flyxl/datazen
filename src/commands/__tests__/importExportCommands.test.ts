import { describe, expect, it, vi, beforeEach } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { backupCommands } from '../backup';
import { connectionCommands } from '../connection';

/**
 * Decision 3 (F4): connections/app-data import-export path/dialog pairs are
 * merged into single IPCs taking a webdriver-gated `override_path`. The
 * wrapper layer is production surface: it must pass dialog-era params
 * verbatim and never send `overridePath`.
 */
describe('merged import/export wrappers (decision 3, f4)', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  function expectSingleInvoke(cmd: string, args: Record<string, unknown>) {
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith(cmd, args);
    // Production callers never opt into the webdriver-only override channel.
    expect(args).not.toHaveProperty('overridePath');
  }

  it('exportConnections passes password + defaultFileName to export_connections', async () => {
    invokeMock.mockResolvedValue(7);
    const count = await connectionCommands.exportConnections('pw', 'share.datazenconnection');
    expectSingleInvoke('export_connections', {
      password: 'pw',
      defaultFileName: 'share.datazenconnection',
    });
    expect(count).toBe(7);
  });

  it('exportConnections forwards a null result when the dialog is dismissed', async () => {
    invokeMock.mockResolvedValue(null);
    await expect(connectionCommands.exportConnections('pw', 'x.datazenconnection')).resolves.toBe(
      null,
    );
  });

  it('importConnectionsPreview passes only the password to import_connections_preview', async () => {
    invokeMock.mockResolvedValue({ connections: [], groups: [] });
    await connectionCommands.importConnectionsPreview('pw');
    expectSingleInvoke('import_connections_preview', { password: 'pw' });
  });

  it('importConnections passes only the password to import_connections_with_dialog', async () => {
    const stats = { imported: 2, overwritten: 1, groupsAdded: 1 };
    invokeMock.mockResolvedValue(stats);
    await expect(connectionCommands.importConnections('pw')).resolves.toEqual(stats);
    expectSingleInvoke('import_connections_with_dialog', { password: 'pw' });
  });

  it('pickConnectionsImportFile opens the native file picker only', async () => {
    invokeMock.mockResolvedValue('/tmp/share.datazenconnection');
    await expect(connectionCommands.pickConnectionsImportFile()).resolves.toBe(
      '/tmp/share.datazenconnection',
    );
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith('pick_connections_import_file');
  });

  it('importConnectionsAtPath forwards password and path', async () => {
    const stats = { imported: 1, overwritten: 0, groupsAdded: 0 };
    invokeMock.mockResolvedValue(stats);
    await expect(
      connectionCommands.importConnectionsAtPath('pw', '/tmp/share.datazenconnection'),
    ).resolves.toEqual(stats);
    expectSingleInvoke('import_connections_at_path', {
      password: 'pw',
      path: '/tmp/share.datazenconnection',
    });
  });

  it('exportAppData passes defaultFileName to export_app_data', async () => {
    invokeMock.mockResolvedValue(true);
    await expect(backupCommands.exportAppData('datazen-backup.zip')).resolves.toBe(true);
    expectSingleInvoke('export_app_data', { defaultFileName: 'datazen-backup.zip' });
  });

  it('exportAppData returns false when the dialog is dismissed', async () => {
    invokeMock.mockResolvedValue(false);
    await expect(backupCommands.exportAppData('datazen-backup.zip')).resolves.toBe(false);
  });

  it('pickAppDataImportFile opens the native file picker only', async () => {
    invokeMock.mockResolvedValue('/tmp/datazen-backup.zip');
    await expect(backupCommands.pickAppDataImportFile()).resolves.toBe('/tmp/datazen-backup.zip');
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith('pick_app_data_import_file');
  });

  it('importAppData passes sourcePath to import_app_data', async () => {
    invokeMock.mockResolvedValue(true);
    await expect(backupCommands.importAppData('/tmp/datazen-backup.zip')).resolves.toBe(true);
    expectSingleInvoke('import_app_data', { sourcePath: '/tmp/datazen-backup.zip' });
  });
});

describe('remaining connection wrappers keep their wire contract', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('lifecycle + config wrappers forward camelCase args', async () => {
    invokeMock.mockResolvedValue([]);

    await connectionCommands.getConnections();
    expect(invokeMock).toHaveBeenLastCalledWith('get_connections');

    const config = { id: 'c1' } as never;
    const plain = { id: 'c1' };
    await connectionCommands.saveConnection(config);
    expect(invokeMock).toHaveBeenLastCalledWith('save_connection', { config: plain });

    await connectionCommands.deleteConnection('c1');
    expect(invokeMock).toHaveBeenLastCalledWith('delete_connection', { id: 'c1' });

    await connectionCommands.reorderConnections(['b', 'a']);
    expect(invokeMock).toHaveBeenLastCalledWith('reorder_connections', { orderedIds: ['b', 'a'] });

    await connectionCommands.testConnection(config);
    expect(invokeMock).toHaveBeenLastCalledWith('test_connection', { config: plain });

    await connectionCommands.connect('c1');
    expect(invokeMock).toHaveBeenLastCalledWith('connect', { connectionId: 'c1' });

    await connectionCommands.pingConnection('s1');
    expect(invokeMock).toHaveBeenLastCalledWith('ping_connection', { dbSessionId: 's1' });

    await connectionCommands.releaseConnection('s1');
    expect(invokeMock).toHaveBeenLastCalledWith('release_connection', { dbSessionId: 's1' });

    await connectionCommands.disconnect('s1');
    expect(invokeMock).toHaveBeenLastCalledWith('disconnect', { dbSessionId: 's1' });

    await connectionCommands.getConnectionInfo('s1');
    expect(invokeMock).toHaveBeenLastCalledWith('get_connection_info', { dbSessionId: 's1' });

    await connectionCommands.getAvailableDrivers();
    expect(invokeMock).toHaveBeenLastCalledWith('get_available_drivers');

    await connectionCommands.getGroups();
    expect(invokeMock).toHaveBeenLastCalledWith('get_groups');

    await connectionCommands.saveGroups(['g']);
    expect(invokeMock).toHaveBeenLastCalledWith('save_groups', { groups: ['g'] });
  });

  it('competitor-import wrappers forward source/password/dataPath', async () => {
    await connectionCommands.detectConnectionImportPath('dbx');
    expect(invokeMock).toHaveBeenLastCalledWith('detect_connection_import_path', {
      source: 'dbx',
    });

    await connectionCommands.pickConnectionImportPathWithDialog('folder', 'navicat');
    expect(invokeMock).toHaveBeenLastCalledWith('pick_connection_import_path_with_dialog', {
      mode: 'folder',
      source: 'navicat',
    });

    invokeMock.mockResolvedValue({ imported: 1, overwritten: 0, groupsAdded: 0 });
    await connectionCommands.importConnectionsFromApp('dbeaver', 'pw', '/opt/data');
    expect(invokeMock).toHaveBeenLastCalledWith('import_connections_from_app', {
      source: 'dbeaver',
      password: 'pw',
      dataPath: '/opt/data',
    });
  });
});

describe('remaining backup wrappers keep their wire contract', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('encryption key dialog + restart wrappers forward args', async () => {
    invokeMock.mockResolvedValue(true);

    await backupCommands.saveEncryptionKeyWithDialog('datazen.key');
    expect(invokeMock).toHaveBeenLastCalledWith('save_encryption_key_with_dialog', {
      defaultFileName: 'datazen.key',
    });

    await backupCommands.restartApp();
    expect(invokeMock).toHaveBeenLastCalledWith('restart_app');
  });
});
