import { describe, expect, it } from 'vitest';
import { buildCompareReportText } from '../compareReport';
import type { DataSyncRowChange, DataSyncTableResult } from '../../../commands/sync';
import type { Value } from '../../../types';

/** Only `operation` drives the report counts; the rest keeps the row a real change. */
function change(operation: DataSyncRowChange['operation'], key: Value[]): DataSyncRowChange {
  return {
    operation,
    key,
    sourceRow: operation === 'INSERT' ? null : [],
    targetRow: operation === 'INSERT' ? [] : null,
    changedColumns: [],
    selected: true,
  };
}

describe('buildCompareReportText', () => {
  it('includes summary counts and incompatible reasons', () => {
    const rows: DataSyncTableResult[] = [
      {
        sourceTable: 'users',
        targetTable: 'users',
        status: 'MATCHED',
        rows: [change('INSERT', ['1']), change('UPDATE', ['2'])],
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
