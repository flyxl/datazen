import { describe, expect, it } from 'vitest';
import {
  appendProgressLog,
  backupProgressRatio,
  formatBackupProgress,
  formatRestoreProgress,
} from '../backupProgress';

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
