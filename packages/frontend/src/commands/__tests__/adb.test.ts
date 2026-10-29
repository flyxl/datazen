import { beforeEach, describe, expect, it, vi } from 'vitest';

const executeMock = vi.hoisted(() => vi.fn());

vi.mock('../driver', () => ({
  driverCommands: {
    execute: executeMock,
  },
}));

import type { ExecuteDriverCommandRequest } from '../driver';
import {
  ADB_DRIVER_TYPE,
  adbListDatabases,
  adbListPackages,
  adbPullDatabaseWithDialog,
} from '../adb';

/** All envelopes captured by the mocked driverCommands.execute. */
function requests(): ExecuteDriverCommandRequest[] {
  return executeMock.mock.calls.map((call) => call[0] as ExecuteDriverCommandRequest);
}

describe('adb command wrappers (sqlite driver commands)', () => {
  beforeEach(() => {
    executeMock.mockReset();
  });

  it('pins the unbound driver type to sqlite', () => {
    expect(ADB_DRIVER_TYPE).toBe('sqlite');
  });

  describe('adbListPackages', () => {
    it('sends an unbound sqlite envelope with empty input and unwraps result.data', async () => {
      const packages = [{ package_name: 'com.example.app' }, { package_name: 'org.demo.reader' }];
      executeMock.mockResolvedValueOnce({ data: packages });

      // Passthrough: the resolved CommandResult.data payload is returned as-is.
      await expect(adbListPackages()).resolves.toBe(packages);

      expect(executeMock).toHaveBeenCalledTimes(1);
      expect(requests()[0]).toEqual({
        driverType: 'sqlite',
        command: 'adb_list_packages',
        input: {},
      });
    });
  });

  describe('adbListDatabases', () => {
    it('carries the package under the same input key as the former IPC argument', async () => {
      const databases = [
        { path: '/data/data/com.example.app/databases/main.db', name: 'main.db' },
      ];
      executeMock.mockResolvedValueOnce({ data: databases });

      await expect(adbListDatabases('com.example.app')).resolves.toEqual(databases);

      expect(executeMock).toHaveBeenCalledTimes(1);
      expect(requests()[0]).toEqual({
        driverType: ADB_DRIVER_TYPE,
        command: 'adb_list_databases',
        input: { package: 'com.example.app' },
      });
    });
  });

  describe('adbPullDatabaseWithDialog', () => {
    it('sends package + dbPath and returns the saved path string on success', async () => {
      executeMock.mockResolvedValueOnce({
        data: { savedPath: '/home/user/Downloads/main.db' },
      });

      await expect(
        adbPullDatabaseWithDialog(
          'com.example.app',
          '/data/data/com.example.app/databases/main.db',
        ),
      ).resolves.toBe('/home/user/Downloads/main.db');

      expect(executeMock).toHaveBeenCalledTimes(1);
      expect(requests()[0]).toEqual({
        driverType: 'sqlite',
        command: 'adb_pull_database',
        input: {
          package: 'com.example.app',
          dbPath: '/data/data/com.example.app/databases/main.db',
        },
      });
    });

    it('maps an explicit cancel (savedPath null) to null like the former *_with_dialog', async () => {
      executeMock.mockResolvedValueOnce({ data: { savedPath: null } });

      await expect(
        adbPullDatabaseWithDialog('com.example.app', 'databases/main.db'),
      ).resolves.toBeNull();
    });

    it('maps a missing savedPath field (undefined) to null', async () => {
      executeMock.mockResolvedValueOnce({ data: {} });

      await expect(
        adbPullDatabaseWithDialog('com.example.app', 'databases/main.db'),
      ).resolves.toBeNull();
    });
  });

  describe('envelope discipline', () => {
    it('never carries session-bound fields — unbound driverType execution only', async () => {
      executeMock.mockResolvedValue({ data: {} });

      await adbListPackages();
      await adbListDatabases('com.example.app');
      await adbPullDatabaseWithDialog('com.example.app', 'databases/main.db');

      expect(executeMock).toHaveBeenCalledTimes(3);
      expect(requests().map((request) => request.command)).toEqual([
        'adb_list_packages',
        'adb_list_databases',
        'adb_pull_database',
      ]);
      for (const request of requests()) {
        expect(Object.keys(request).sort()).toEqual(['command', 'driverType', 'input']);
        expect(request.driverType).toBe('sqlite');
        expect(request.dbSessionId).toBeUndefined();
        expect(request.database).toBeUndefined();
      }
    });
  });
});
