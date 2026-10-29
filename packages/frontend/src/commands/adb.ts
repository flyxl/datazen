import { driverCommands } from './driver';

// IPC refactor decision 2: the ADB helpers are SQLite-driver commands
// (`requiresConnection = false`) executed through the unified
// `execute_driver_command(driverType: "sqlite", …)` entry point instead of
// dedicated host IPCs. The JSON shapes match the former host commands so
// component types stay unchanged.

/** ADB driver command ids (declared by packages/drivers/sqlite). */
export const ADB_DRIVER_TYPE = 'sqlite';
export type AdbCommand = 'adb_list_packages' | 'adb_list_databases' | 'adb_pull_database';

export interface AdbPackage {
  package_name: string;
}

export interface AdbDatabaseFile {
  path: string;
  name: string;
}

async function executeAdbCommand<T>(command: AdbCommand, input: Record<string, unknown>): Promise<T> {
  const result = await driverCommands.execute({
    // Unbound driver command: no connection session required.
    driverType: ADB_DRIVER_TYPE,
    command,
    input,
  });
  return result.data as T;
}

export async function adbListPackages(): Promise<AdbPackage[]> {
  return executeAdbCommand<AdbPackage[]>('adb_list_packages', {});
}

export async function adbListDatabases(packageName: string): Promise<AdbDatabaseFile[]> {
  return executeAdbCommand<AdbDatabaseFile[]>('adb_list_databases', {
    package: packageName,
  });
}

/**
 * Pull a device database and save it through the host's native save dialog.
 *
 * The sqlite driver command declares a saveDialog in its metadata: it returns
 * `{ fileName, dataBase64 }`, the host thin shell pops the native dialog,
 * writes the bytes and replaces the payload with `{ savedPath }` (null when
 * cancelled). No path ever travels from JS to the filesystem.
 */
export async function adbPullDatabaseWithDialog(
  packageName: string,
  dbPath: string,
): Promise<string | null> {
  const { savedPath } = await executeAdbCommand<{ savedPath?: string | null }>(
    'adb_pull_database',
    { package: packageName, dbPath },
  );
  return savedPath ?? null;
}
