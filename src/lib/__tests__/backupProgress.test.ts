import { describe, expect, it, vi } from 'vitest';
import {
  appendProgressLog,
  backupProgressRatio,
  createProgressLogPump,
  formatBackupProgress,
  formatRestoreProgress,
  MAX_PROGRESS_LOG_CHARS,
  MAX_PROGRESS_LOG_LINES,
} from '../backupProgress';
import { getTranslation } from '../../locales';

const t = (key: string, params?: Record<string, string | number>) => {
  if (!params) return key;
  return `${key}:${JSON.stringify(params)}`;
};

describe('formatBackupProgress', () => {
  it('formats object / writing / done phases', () => {
    expect(
      formatBackupProgress({ current: 2, total: 5, objectName: 'users', phase: 'object' }, t),
    ).toContain('backup.progressObject');
    expect(
      formatBackupProgress({ current: 0, total: 0, objectName: '', phase: 'writing' }, t),
    ).toBe('backup.progressWriting');
    expect(formatBackupProgress({ current: 0, total: 0, objectName: '', phase: 'done' }, t)).toBe(
      'backup.success',
    );
  });
});

describe('formatRestoreProgress', () => {
  it('formats restore object / preparing / done phases', () => {
    expect(
      formatRestoreProgress(
        { current: 3, total: 10, objectName: 'INSERT INTO t', phase: 'object' },
        t,
      ),
    ).toContain('backup.restoreProgress');
    expect(
      formatRestoreProgress({ current: 0, total: 0, objectName: '', phase: 'writing' }, t),
    ).toBe('backup.restorePreparing');
    expect(formatRestoreProgress({ current: 0, total: 0, objectName: '', phase: 'done' }, t)).toBe(
      'backup.restoreSuccess',
    );
    expect(
      formatRestoreProgress({ current: 0, total: 4, objectName: '', phase: 'object' }, t),
    ).toBe('backup.restoring');
    expect(
      formatRestoreProgress(
        { current: 3, total: 0, objectName: 'INSERT INTO t', phase: 'object' },
        t,
      ),
    ).toContain('INSERT INTO t');
  });
});

describe('appendProgressLog', () => {
  it('skips blanks and consecutive duplicates, then appends', () => {
    expect(appendProgressLog([], '')).toEqual([]);
    expect(appendProgressLog(['a'], 'a')).toEqual(['a']);
    expect(appendProgressLog(['a'], 'b')).toEqual(['a', 'b']);
  });

  it('truncates an oversized single line to bound memory', () => {
    const huge = 'E'.repeat(10_000);
    const [only] = appendProgressLog([], huge);
    expect(only.length).toBeLessThan(5000);
    expect(only).toContain('(truncated)');
  });

  it('bounds total lines, keeping the head, a marker, and the tail', () => {
    const appended = MAX_PROGRESS_LOG_LINES + 1000;
    let lines: string[] = [];
    for (let i = 0; i < appended; i += 1) {
      lines = appendProgressLog(lines, `line-${i}`);
    }
    expect(lines.length).toBeLessThanOrEqual(MAX_PROGRESS_LOG_LINES);
    // Head (start of the restore) is preserved.
    expect(lines[0]).toBe('line-0');
    // Tail (newest entries / errors) is preserved.
    expect(lines[lines.length - 1]).toBe(`line-${appended - 1}`);
    // A single marker reports the cumulative number of omitted lines.
    const markers = lines.filter((l) => l.includes('lines omitted'));
    expect(markers).toHaveLength(1);
    // 100 head + 1399 tail lines are kept, so 1000 over the budget are dropped.
    expect(markers[0]).toContain(String(appended - 100 - 1399));
    // No duplicates introduced by trimming.
    expect(new Set(lines).size).toBe(lines.length);
  });

  it('localizes the omission marker when t is provided', () => {
    const appended = MAX_PROGRESS_LOG_LINES + 7;
    let lines: string[] = [];
    for (let i = 0; i < appended; i += 1) {
      lines = appendProgressLog(lines, `l-${i}`, (key, params) =>
        getTranslation('zh-CN', key as Parameters<typeof getTranslation>[1], params),
      );
    }
    const markers = lines.filter((l) => l.startsWith('… '));
    expect(markers).toHaveLength(1);
    expect(markers[0]).toContain(String(appended - 100 - 1399));
    expect(markers[0]).not.toContain('{');
  });

  it('bounds total retained characters even with long lines', () => {
    let lines: string[] = [];
    for (let i = 0; i < 200; i += 1) {
      lines = appendProgressLog(lines, `[${i}] ${'x'.repeat(3000)}`);
    }
    const total = lines.reduce((sum, l) => sum + l.length, 0);
    expect(total).toBeLessThanOrEqual(MAX_PROGRESS_LOG_CHARS);
    // Newest entries always survive the character budget trim.
    expect(lines[lines.length - 1]).toContain('[199]');
    // Head survives too.
    expect(lines[0]).toContain('[0]');
  });
});

describe('createProgressLogPump', () => {
  it('batches pushes and flushes on demand', () => {
    vi.useFakeTimers();
    const seen: string[][] = [];
    const pump = createProgressLogPump((lines) => {
      seen.push(lines);
    }, 50);
    pump.reset(['start']);
    pump.push('a');
    pump.push('b');
    expect(seen.at(-1)).toEqual(['start']);
    vi.advanceTimersByTime(50);
    expect(seen.at(-1)).toEqual(['start', 'a', 'b']);
    pump.push('c');
    pump.flush();
    expect(seen.at(-1)).toEqual(['start', 'a', 'b', 'c']);
    vi.useRealTimers();
  });
});

describe('backupProgressRatio', () => {
  it('maps phases to a 0–1 bar', () => {
    expect(backupProgressRatio({ current: 2, total: 4, objectName: 't', phase: 'object' })).toBe(
      0.5,
    );
    expect(backupProgressRatio({ current: 0, total: 0, objectName: '', phase: 'writing' })).toBe(
      0.95,
    );
    expect(backupProgressRatio({ current: 0, total: 0, objectName: '', phase: 'done' })).toBe(1);
    const streamed = backupProgressRatio({
      current: 12,
      total: 0,
      objectName: 'INSERT',
      phase: 'object',
    });
    expect(streamed).toBeGreaterThan(0);
    expect(streamed).toBeLessThan(0.95);
  });
});
