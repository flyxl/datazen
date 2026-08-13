import { describe, expect, it } from 'vitest';
import {
  displayTableName,
  mappingLabelKey,
  rowDiffCounts,
  summarizeMappings,
  tableHasRowDiffs,
  type DataSyncTableResult,
} from '../mappingView';

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
    });
  });

  it('counts INSERT/UPDATE/DELETE row diffs', () => {
    const row: DataSyncTableResult = {
      sourceTable: 'users',
      targetTable: 'users',
      status: 'MATCHED',
      rows: [
        { operation: 'INSERT' },
        { operation: 'UPDATE' },
        { operation: 'UPDATE' },
        { operation: 'DELETE' },
        { operation: 'UNCHANGED' },
      ],
    };
    expect(rowDiffCounts(row)).toEqual({ inserts: 1, updates: 2, deletes: 1 });
    expect(tableHasRowDiffs(row)).toBe(true);
    expect(
      tableHasRowDiffs({
        sourceTable: 'users',
        targetTable: 'users',
        status: 'MATCHED',
        rows: [{ operation: 'UNCHANGED' }],
      }),
    ).toBe(false);
  });
});
