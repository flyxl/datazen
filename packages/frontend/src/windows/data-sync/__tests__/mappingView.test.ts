import { describe, expect, it } from 'vitest';
import {
  applyOptionsToRows,
  defaultRowSelected,
  displayTableName,
  mappingLabelKey,
  operationAllowed,
  rowDiffCounts,
  rowKeyString,
  selectedRowCount,
  summarizeMappings,
  tableHasRowDiffs,
  tableKey,
  tableMatchesFilter,
  type DataSyncTableResult,
} from '../mappingView';
import type { DataSyncOperation, DataSyncRowChange, SyncOptions } from '../../../commands/sync';
import type { Value } from '../../../types';

describe('mappingView', () => {
  it('labels every mapping status', () => {
    expect(mappingLabelKey('MATCHED')).toBe('sync.mappingMatched');
    expect(mappingLabelKey('UNMAPPED_SOURCE')).toBe('sync.mappingUnmappedSource');
    expect(mappingLabelKey('UNMAPPED_TARGET')).toBe('sync.mappingUnmappedTarget');
    expect(mappingLabelKey('DISABLED')).toBe('sync.mappingDisabled');
    expect(mappingLabelKey('INCOMPATIBLE')).toBe('sync.mappingIncompatible');
  });

  it('displays renamed mappings', () => {
    expect(
      displayTableName({
        sourceTable: 'customers',
        targetTable: 'clients',
        status: 'MATCHED',
      }),
    ).toBe('customers → clients');
    expect(
      displayTableName({
        sourceTable: 'users',
        targetTable: 'users',
        status: 'MATCHED',
      }),
    ).toBe('users');
    expect(
      displayTableName({
        sourceTable: '',
        targetTable: 'only_tgt',
        status: 'UNMAPPED_TARGET',
      }),
    ).toBe('only_tgt');
  });

  it('summarizes mapping buckets', () => {
    const rows: DataSyncTableResult[] = [
      { sourceTable: 'a', targetTable: 'a', status: 'MATCHED' },
      { sourceTable: 'b', targetTable: 'b', status: 'INCOMPATIBLE' },
      { sourceTable: 'c', targetTable: '', status: 'UNMAPPED_SOURCE' },
      { sourceTable: '', targetTable: 'd', status: 'UNMAPPED_TARGET' },
    ];
    expect(summarizeMappings(rows)).toEqual({
      matched: 1,
      incompatible: 1,
      unmapped: 2,
      disabled: 0,
    });
  });

  it('counts INSERT/UPDATE/DELETE row diffs', () => {
    const row: DataSyncTableResult = {
      sourceTable: 'users',
      targetTable: 'users',
      status: 'MATCHED',
      rows: [
        {
          operation: 'INSERT',
          key: [1],
          sourceRow: [[1]],
          targetRow: null,
          changedColumns: [],
          selected: true,
        },
        {
          operation: 'UPDATE',
          key: [2],
          sourceRow: [[2, 'a']],
          targetRow: [[2, 'b']],
          changedColumns: ['name'],
          selected: true,
        },
        {
          operation: 'UPDATE',
          key: [3],
          sourceRow: [[3]],
          targetRow: [[3]],
          changedColumns: [],
          selected: false,
        },
        {
          operation: 'DELETE',
          key: [4],
          sourceRow: null,
          targetRow: [[4]],
          changedColumns: [],
          selected: false,
        },
        {
          operation: 'UNCHANGED',
          key: [5],
          sourceRow: [[5]],
          targetRow: [[5]],
          changedColumns: [],
          selected: false,
        },
      ],
    };
    expect(rowDiffCounts(row)).toEqual({ inserts: 1, updates: 2, deletes: 1, unchanged: 1 });
    expect(tableHasRowDiffs(row)).toBe(true);
    expect(
      tableHasRowDiffs({
        sourceTable: 'users',
        targetTable: 'users',
        status: 'MATCHED',
        rows: [
          {
            operation: 'UNCHANGED',
            key: [1],
            sourceRow: [[1]],
            targetRow: [[1]],
            changedColumns: [],
            selected: false,
          },
        ],
      }),
    ).toBe(false);
  });

  it('keeps operation selection and filters fail-closed for every row state', () => {
    const options: SyncOptions = { insert: true, update: false, delete: true };
    expect(defaultRowSelected('INSERT', options)).toBe(true);
    expect(defaultRowSelected('UPDATE', options)).toBe(false);
    expect(defaultRowSelected('DELETE', options)).toBe(false);
    expect(defaultRowSelected('UNCHANGED', options)).toBe(false);
    expect(operationAllowed('INSERT', options)).toBe(true);
    expect(operationAllowed('UPDATE', options)).toBe(false);
    expect(operationAllowed('DELETE', options)).toBe(true);
    expect(operationAllowed('UNCHANGED', options)).toBe(false);

    const rows: DataSyncRowChange[] = [
      { operation: 'INSERT', key: [1], sourceRow: [1], targetRow: null, changedColumns: [], selected: true },
      { operation: 'UPDATE', key: [2], sourceRow: [2], targetRow: [1], changedColumns: ['id'], selected: true },
      { operation: 'DELETE', key: [3], sourceRow: null, targetRow: [3], changedColumns: [], selected: true },
      { operation: 'UNCHANGED', key: [4], sourceRow: [4], targetRow: [4], changedColumns: [], selected: true },
    ];
    const table: DataSyncTableResult = {
      sourceTable: 'source_rows',
      targetTable: 'target_rows',
      status: 'MATCHED',
      rows,
    };
    expect(selectedRowCount(table, options)).toBe(2);
    expect(applyOptionsToRows(rows, options).map((row) => row.selected)).toEqual([
      true,
      false,
      true,
      false,
    ]);
    expect(tableMatchesFilter(table, 'insert', '')).toBe(true);
    expect(tableMatchesFilter(table, 'update', '')).toBe(true);
    expect(tableMatchesFilter(table, 'delete', '')).toBe(true);
    expect(tableMatchesFilter(table, 'all', 'SOURCE')).toBe(true);
    expect(tableMatchesFilter(table, 'incompatible', '')).toBe(false);
    expect(tableKey({ ...table, sourceTable: '' })).toBe('target_rows');

    expect(mappingLabelKey('BROKEN' as DataSyncTableResult['status'])).toBe(
      'sync.mappingIncompatible',
    );
    expect(defaultRowSelected('BROKEN' as DataSyncOperation, options)).toBe(false);
    expect(operationAllowed('BROKEN' as DataSyncOperation, options)).toBe(false);
    expect(tableMatchesFilter(table, 'BROKEN' as never, '')).toBe(true);
    expect(rowKeyString([BigInt(7)] as unknown as Value[])).toBe('7');
  });
});
