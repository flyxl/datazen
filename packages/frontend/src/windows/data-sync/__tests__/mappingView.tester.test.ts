import { describe, expect, it } from 'vitest';
import { mergeCompareIntoMappings, rowDiffCounts, tableMatchesFilter, summarizeCompare, markDisabledTables, tablesForCompare } from '../mappingView';
import type { DataSyncTableResult } from '../../../commands/sync';

describe('[tester] canonical projection and summary merge', () => {
  it('keeps metadata-only unchanged counts and distinguishes unchanged from incompatible tables', () => {
    const table: DataSyncTableResult = { sourceTable: 'orders', targetTable: 'archive', status: 'MATCHED', unchangedCount: 1000000, rows: [] };
    expect(rowDiffCounts(table)).toEqual({ inserts: 0, updates: 0, deletes: 0, unchanged: 1000000 });
    expect(tableMatchesFilter(table, 'unchanged', 'ARCHIVE')).toBe(true);
    expect(tableMatchesFilter(table, 'unchanged', 'missing')).toBe(false);
    expect(tableMatchesFilter(table, 'insert', '')).toBe(false);
    expect(tableMatchesFilter(table, 'update', '')).toBe(false);
    expect(tableMatchesFilter(table, 'delete', '')).toBe(false);
    const incompatible: DataSyncTableResult = { ...table, status: 'INCOMPATIBLE' };
    expect(tableMatchesFilter(incompatible, 'unchanged', '')).toBe(false);
    expect(tableMatchesFilter(incompatible, 'incompatible', '')).toBe(true);
    expect(summarizeCompare([table, incompatible])).toEqual({ inserts: 0, updates: 0, deletes: 0, unchangedTables: 1, incompatible: 1 });
  });
  it('retains disabled selection and merges projection only into participating mappings', () => {
    const table: DataSyncTableResult = { sourceTable: 'orders', targetTable: 'archive', status: 'MATCHED', warnings: ['existing'] };
    const disabled = markDisabledTables([table], new Set(['orders']));
    const compared: DataSyncTableResult = { ...table, columns: ['id', 'name'], columnTypes: ['int', 'text'], primaryKeys: ['id'], unchangedCount: 55, warnings: undefined };
    expect(mergeCompareIntoMappings(disabled, [compared])).toEqual(disabled);
    expect(tablesForCompare(disabled)).toEqual([]);
    const merged = mergeCompareIntoMappings([table], [compared]);
    expect(merged[0]).toMatchObject({ columns: ['id', 'name'], unchangedCount: 55, warnings: ['existing'] });
    expect(tablesForCompare(merged)).toEqual(['orders']);
    expect(mergeCompareIntoMappings([table], [])).toEqual([table]);

    const targetOnly: DataSyncTableResult = {
      sourceTable: '',
      targetTable: 'orphan_archive',
      status: 'UNMAPPED_TARGET',
    };
    expect(
      mergeCompareIntoMappings([targetOnly], [{ ...targetOnly, warnings: ['target only'] }]),
    ).toEqual([{ ...targetOnly, warnings: ['target only'] }]);
  });
});
