import { invoke } from '@tauri-apps/api/core';

export interface AdbPackage {
  package_name: string;
}

export interface AdbDatabaseFile {
  path: string;
  name: string;
}

export async function adbListPackages(): Promise<AdbPackage[]> {
  return invoke<AdbPackage[]>('adb_list_packages');
}

export async function adbListDatabases(
  packageName: string,
): Promise<AdbDatabaseFile[]> {
  return invoke<AdbDatabaseFile[]>('adb_list_databases', {
    package: packageName,
  });
}

/** Pull DB via native save dialog (XSS-safe). Returns saved path or null if cancelled. */
export async function adbPullDatabaseWithDialog(
  packageName: string,
  dbPath: string,
): Promise<string | null> {
  return invoke<string | null>('adb_pull_database_with_dialog', {
    package: packageName,
    dbPath,
  });
}

/** @deprecated E2E-only; prefer adbPullDatabaseWithDialog. */
export async function adbPullDatabase(
  packageName: string,
  dbPath: string,
  localPath: string,
): Promise<string> {
  return invoke<string>('adb_pull_database', {
    package: packageName,
    dbPath,
    localPath,
  });
}
