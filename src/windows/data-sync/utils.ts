export interface SyncProgress {
  taskId: string;
  phase: string;
  tableIndex: number;
  totalTables: number;
  currentTable: string;
  sourceRowCount: number;
  syncedRows: number;
  completedTables: string[];
  error: string | null;
}

export interface ConflictInfo {
  table: string;
  originalRows: number;
  currentRows: number;
}

export type SyncState = 'idle' | 'comparing' | 'compared' | 'syncing' | 'done';

export function formatDuration(ms: number): string {
  const secs = Math.floor(ms / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  const rem = secs % 60;
  return `${mins}m ${rem}s`;
}

export function tableProgressPercent(progress: SyncProgress | null): number {
  if (!progress) return 0;
  if (progress.sourceRowCount <= 0) return 0;
  return Math.round((progress.syncedRows / progress.sourceRowCount) * 100);
}

export function overallProgressPercent(progress: SyncProgress | null): number {
  if (!progress || progress.totalTables <= 0) return 0;
  const tableFraction =
    progress.sourceRowCount > 0 ? progress.syncedRows / progress.sourceRowCount : 0;
  return Math.round(
    ((progress.completedTables.length + tableFraction) / progress.totalTables) * 100,
  );
}
