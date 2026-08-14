import type { I18nKey } from '../locales';

export type BackupDumpPhase = 'object' | 'writing' | 'done';

export interface BackupProgressPayload {
  current: number;
  total: number;
  objectName: string;
  phase: BackupDumpPhase;
}

/** Translation function as provided by `useI18n` / `getTranslation`. */
type TFunc = (key: I18nKey, params?: Record<string, string | number>) => string;

export function formatBackupProgress(payload: BackupProgressPayload, t: TFunc): string {
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

export function formatRestoreProgress(payload: BackupProgressPayload, t: TFunc): string {
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

export const MAX_PROGRESS_LOG_LINES = 1500;
export const MAX_PROGRESS_LOG_CHARS = 256 * 1024; // 256 KiB hard cap on retained log text
const MAX_LINE_CHARS = 4096;
const LOG_HEAD_KEEP = 100;
const LOG_TAIL_KEEP = MAX_PROGRESS_LOG_LINES - LOG_HEAD_KEEP - 1;

/**
 * Every "N lines omitted" marker (localized or fallback) starts with `… ` so
 * it can be recognized again on the next trim and its count carried forward.
 */
const OMITTED_MARKER_RE = /^… (\d+)/;

function totalChars(lines: string[]): number {
  let total = 0;
  for (const line of lines) total += line.length;
  return total;
}

function omittedLabel(omitted: number, t?: TFunc): string {
  if (t) return t('backup.logOmitted', { count: omitted });
  return `… ${omitted} lines omitted …`;
}

/**
 * Keep the newest progress line. The log is bounded so that a large restore
 * (which streams one statement at a time) cannot retain unbounded text in
 * memory: when the log grows past the budget it is trimmed to the head
 * (restore start), a single "N lines omitted" marker, and the tail (which
 * includes the newest entries and any error message).
 */
export function appendProgressLog(lines: string[], next: string, t?: TFunc): string[] {
  const trimmed = next.trim();
  if (!trimmed) return lines;
  if (lines[lines.length - 1] === trimmed) return lines;
  const line = trimLine(trimmed);
  return trimProgressLog([...lines, line], t);
}

/** Truncate a single oversized line (e.g. a long error dump) to bound memory. */
function trimLine(line: string): string {
  if (line.length <= MAX_LINE_CHARS) return line;
  return `${line.slice(0, MAX_LINE_CHARS)}… (truncated)`;
}

function trimProgressLog(lines: string[], t?: TFunc): string[] {
  if (lines.length <= MAX_PROGRESS_LOG_LINES && totalChars(lines) <= MAX_PROGRESS_LOG_CHARS) {
    return lines;
  }
  const headCount = Math.min(LOG_HEAD_KEEP, lines.length);
  let tailStart = Math.max(headCount, lines.length - LOG_TAIL_KEEP);

  // Hard character budget: drop the oldest tail lines, then oldest head lines,
  // so the newest entries (and errors) always survive.
  let total = totalChars(lines.slice(0, headCount)) + totalChars(lines.slice(tailStart));
  while (total > MAX_PROGRESS_LOG_CHARS && tailStart < lines.length - 1) {
    total -= lines[tailStart].length;
    tailStart += 1;
  }
  let headEnd = headCount;
  while (total > MAX_PROGRESS_LOG_CHARS && headEnd > 1) {
    headEnd -= 1;
    total -= lines[headEnd].length;
  }
  const keptHead = lines.slice(0, headEnd);
  const keptTail = lines.slice(tailStart);

  // Count every dropped line; absorb the count of any earlier marker that is
  // being dropped so the marker reports the cumulative number of omitted lines.
  let omitted = tailStart - headEnd;
  if (omitted > 0) {
    for (const line of lines.slice(headEnd, tailStart)) {
      const m = OMITTED_MARKER_RE.exec(line);
      if (m) omitted += Number(m[1]) - 1;
    }
  }
  const marker = omitted > 0 ? [omittedLabel(omitted, t)] : [];
  return [...keptHead, ...marker, ...keptTail];
}

/** Buffer progress lines and flush to React on an interval (avoids per-statement re-renders). */
export function createProgressLogPump(
  setLines: (lines: string[]) => void,
  intervalMs = 80,
  t?: TFunc,
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
      lines = appendProgressLog(lines, line, t);
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
