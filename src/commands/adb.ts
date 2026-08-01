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
