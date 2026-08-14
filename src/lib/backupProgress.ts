export type BackupDumpPhase = 'object' | 'writing' | 'done';

export interface BackupProgressPayload {
  current: number;
  total: number;
  objectName: string;
  phase: BackupDumpPhase;
}

export function formatBackupProgress(
  payload: BackupProgressPayload,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  if (payload.phase === 'writing') return t('backup.progressWriting');
  if (payload.phase === 'done') return t('backup.success');
  if (payload.total > 0 && payload.objectName) {
    return t('backup.progressObject', {
      name: payload.objectName,
      current: payload.current,
      total: payload.total,
    });
  }
  return t('backup.progressPreparing');
}

export function backupProgressRatio(payload: BackupProgressPayload): number {
  if (payload.phase === 'done') return 1;
  if (payload.phase === 'writing') return 0.95;
  if (payload.total <= 0) {
    if (payload.current <= 0) return 0;
    return Math.min(0.9, 0.12 + Math.log10(payload.current + 1) / 5);
  }
  return Math.min(0.94, payload.current / payload.total);
}

export function formatRestoreProgress(
  payload: BackupProgressPayload,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  if (payload.phase === 'done') return t('backup.restoreSuccess');
  if (payload.phase === 'writing') return t('backup.restorePreparing');
  if (payload.objectName) {
    if (payload.total > 0) {
      return t('backup.restoreProgress', {
        name: payload.objectName,
        current: payload.current,
        total: payload.total,
      });
    }
    return t('backup.restoreProgress', {
      name: payload.objectName,
      current: payload.current,
      total: '…',
    });
  }
  return t('backup.restoring');
}

const MAX_PROGRESS_LOG_LINES = 5000;

/** Append a progress line, skipping consecutive duplicates and bounding length. */
export function appendProgressLog(lines: string[], next: string): string[] {
  const trimmed = next.trim();
  if (!trimmed) return lines;
  if (lines[lines.length - 1] === trimmed) return lines;
  const out = [...lines, trimmed];
  return out.length > MAX_PROGRESS_LOG_LINES ? out.slice(-MAX_PROGRESS_LOG_LINES) : out;
}

/** Buffer progress lines and flush to React on an interval (avoids per-statement re-renders). */
export function createProgressLogPump(
  setLines: (lines: string[]) => void,
  intervalMs = 80,
): {
  reset: (initial: string[]) => void;
  push: (line: string) => void;
  flush: () => void;
} {
  let lines: string[] = [];
  let timer: ReturnType<typeof setTimeout> | null = null;
  const flush = () => {
    timer = null;
    setLines(lines);
  };
  return {
    reset(initial) {
      if (timer != null) {
        clearTimeout(timer);
        timer = null;
      }
      lines = [...initial];
      setLines(lines);
    },
    push(line) {
      lines = appendProgressLog(lines, line);
      if (timer == null) timer = setTimeout(flush, intervalMs);
    },
    flush() {
      if (timer != null) {
        clearTimeout(timer);
        timer = null;
      }
      setLines(lines);
    },
  };
}
