import { describe, expect, it } from 'vitest';
import {
  buildTableContext,
  emptyTableState,
  extractErrorMessage,
  rowsToRecords,
  toCellValue,
} from '../connectionState';

describe('[tester] tableData/connectionState', () => {
  it('rowsToRecords maps column names to row values', () => {
    const records = rowsToRecords(
      [
        { name: 'id', dataType: 'int', isPrimaryKey: true, nullable: false },
        { name: 'name', dataType: 'text', isPrimaryKey: false, nullable: true },
      ],
      [
        [1, 'Alice'],
        [2, null],
      ],
    );
    expect(records).toEqual([
      { id: 1, name: 'Alice' },
      { id: 2, name: null },
    ]);
  });

  it('extractErrorMessage prefers string and Error.message', () => {
    expect(extractErrorMessage('boom', 'fallback')).toBe('boom');
    expect(extractErrorMessage(new Error('err'), 'fallback')).toBe('err');
    expect(extractErrorMessage({ message: 'obj' }, 'fallback')).toBe('obj');
    expect(extractErrorMessage({}, 'fallback')).toBe('fallback');
  });

  it('buildTableContext defaults every optional target field to null', () => {
    expect(buildTableContext({ dbSessionId: 'sess-1', table: 'users' })).toEqual({
      connectionId: null,
      dbSessionId: 'sess-1',
      driverType: null,
      database: null,
      schema: null,
      table: 'users',
    });

    expect(
      buildTableContext({
        dbSessionId: 'sess-1',
        table: 'users',
        connectionId: 'cfg-1',
        driverType: 'postgres',
        database: 'app',
        schema: 'sales',
      }),
    ).toEqual({
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      driverType: 'postgres',
      database: 'app',
      schema: 'sales',
      table: 'users',
    });
  });

  it('emptyTableState starts an unloaded panel with no staged changes', () => {
    const ctx = {
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
      table: 'users',
    };
    const ts = emptyTableState(ctx);
    expect(ts.context).toBe(ctx);
    expect(ts.columns).toEqual([]);
    expect(ts.rows).toEqual([]);
    expect(ts.page).toBe(0);
    expect(ts.detailRowIndex).toBeNull();
    expect(ts.pendingChanges.size).toBe(0);
    expect(ts.requestRevision).toBe(0);
    expect(ts.loadingRevision).toBeNull();
    expect(emptyTableState().context).toBeNull();
  });

  it('utility helpers behave consistently', () => {
    expect(toCellValue(null)).toBeNull();
    expect(toCellValue('x')).toBe('x');
  });
});
