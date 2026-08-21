import { describe, expect, it } from 'vitest';
import { buildCompareReportText } from '../compareReport';
import type { DataSyncTableResult } from '../../commands/sync';

describe('buildCompareReportText', () => {
  it('includes summary counts and incompatible reasons', () => {
    const rows: DataSyncTableResult[] = [
      {
        sourceTable: 'users',
        targetTable: 'users',
        status: 'MATCHED',
        rows: [
          { operation: 'INSERT', selected: true, key: ['1'], sourceValues: [], targetValues: [] },
          { operation: 'UPDATE', selected: true, key: ['2'], sourceValues: [], targetValues: [] },
        ],
      },
      {
        sourceTable: 'logs',
        targetTable: 'logs',
        status: 'INCOMPATIBLE',
        incompatibleReason: 'Missing primary key',
      },
    ];

    const text = buildCompareReportText(rows);
    expect(text).toContain('Inserts: 1');
    expect(text).toContain('Updates: 1');
    expect(text).toContain('Incompatible tables: 1');
    expect(text).toContain('logs: INCOMPATIBLE — Missing primary key');
  });
});
