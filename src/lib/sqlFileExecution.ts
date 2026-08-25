import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  createProgressLogPump,
  formatRestoreProgress,
  type BackupProgressPayload,
} from './backupProgress';
import type { I18nKey } from '../locales';
import type { TableInfo } from '../types';

type TFunc = (key: I18nKey, params?: Record<string, string | number>) => string;

export interface RunSqlFileExecutionOptions {
  connectionId: string;
  database: string;
  t: TFunc;
  /** Restore flow: confirm dropping existing objects when the database is non-empty. */
  confirmOverwrite?: (tableCount: number) => Promise<boolean>;
  /** Execute SQL file flow: generic warning before opening the file picker (no auto-drop). */
  confirmBeforeExecute?: () => Promise<boolean>;
  onProgress?: (payload: BackupProgressPayload | null, statusLine: string) => void;
  onError?: (message: string) => void;
  logPump?: ReturnType<typeof createProgressLogPump>;
  /** Defaults to `execute_sql_file_with_dialog`; restore window uses `restore_database_with_dialog`. */
  command?: 'execute_sql_file_with_dialog' | 'restore_database_with_dialog';
  /** Defaults to `backup.restoreSuccess`. */
  successMessageKey?: I18nKey;
}

/**
 * Pick a `.sql` file via native dialog and stream-execute it on the backend
 * (same pipeline as Restore Database). Never loads file contents into JS.
 */
export async function runSqlFileExecution({
  connectionId,
  database,
  t,
  confirmOverwrite,
  confirmBeforeExecute,
  onProgress,
  onError,
  logPump: externalLogPump,
  command = 'execute_sql_file_with_dialog',
  successMessageKey = 'backup.restoreSuccess',
}: RunSqlFileExecutionOptions): Promise<boolean> {
  await invoke('use_database', { dbSessionId: connectionId, database });

  const options: string[] = [];

  if (confirmBeforeExecute) {
    const ok = await confirmBeforeExecute();
    if (!ok) return false;
  } else if (confirmOverwrite) {
    const tables = await invoke<TableInfo[]>('get_tables', {
      dbSessionId: connectionId,
      database,
    });
    if (tables.length > 0) {
      const ok = await confirmOverwrite(tables.length);
      if (!ok) return false;
      options.push('overwrite');
    }
  }

  const logPump =
    externalLogPump ??
    createProgressLogPump(
      () => {
        /* optional external log sink */
      },
      80,
      t,
    );
  logPump.reset([t('backup.restoring')]);
  onProgress?.(null, t('backup.restoring'));

  const unlisten = await listen<BackupProgressPayload>('restore-progress', (event) => {
    const line = formatRestoreProgress(event.payload, t);
    onProgress?.(event.payload, line);
    logPump.push(line);
  });

  try {
    const executed = await invoke<boolean>(command, {
      connectionId,
      database,
      options,
    });
    if (!executed) {
      logPump.reset([]);
      onProgress?.(null, '');
      return false;
    }
    const doneLine = t(successMessageKey);
    logPump.push(doneLine);
    logPump.flush();
    onProgress?.({ current: 0, total: 0, objectName: '', phase: 'done' }, doneLine);
    return true;
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    logPump.push(message);
    logPump.flush();
    onError?.(message);
    onProgress?.(null, message);
    throw e;
  } finally {
    unlisten();
  }
}
