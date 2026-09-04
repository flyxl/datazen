import { describe, expect, it } from 'vitest';
import {
  activeTableContext,
  buildTableChangeContext,
  editKey,
  emptyConnectionTableState,
  emptyTableState,
  extractErrorMessage,
  flattenActive,
  getState,
  patchConnection,
  rowsToRecords,
  syncFlat,
  toCellValue,
} from '../connectionState';

describe('[tester] tableData/connectionState', () => {
  it('rowsToRecords maps column names to row values', () => {
    const records = rowsToRecords(
      [
        { name: 'id', dataType: 'int', isPrimaryKey: true, isNullable: false },
        { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
      ],
      [[1, 'Alice'], [2, null]],
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

  it('flattenActive and patchConnection preserve per-connection slices', () => {
    const perConnection = new Map<string, ReturnType<typeof emptyConnectionTableState>>();
    const conn = emptyConnectionTableState();
    conn.activeTable = 'users';
    conn.activeTableKey = 'users-key';
    const tableState = emptyTableState();
    tableState.rows = [{ id: 1 }];
    conn.tableStates.set('users-key', tableState);
    perConnection.set('sess-1', conn);

    const flat = flattenActive(perConnection, 'sess-1');
    expect(flat.activeTable).toBe('users');
    expect(flat.rows).toEqual([{ id: 1 }]);

    const patched = patchConnection(perConnection, 'sess-1', { connectionId: 'cfg-1' });
    expect(patched.get('sess-1')?.connectionId).toBe('cfg-1');
  });

  it('buildTableChangeContext merges params with connection defaults', () => {
    const conn = emptyConnectionTableState();
    conn.connectionId = 'cfg-1';
    conn.databaseType = 'postgres';
    conn.activeDatabase = 'app';
    conn.activeSchema = 'public';

    expect(
      buildTableChangeContext(conn, { dbSessionId: 'sess-1', table: 'users' }),
    ).toEqual({
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      driverType: 'postgres',
      database: 'app',
      schema: 'public',
      table: 'users',
    });

    expect(
      buildTableChangeContext(conn, {
        dbSessionId: 'sess-1',
        table: 'users',
        database: 'other',
        schema: 'sales',
      }).database,
    ).toBe('other');
  });

  it('activeTableContext returns context from active table key', () => {
    const conn = emptyConnectionTableState();
    expect(activeTableContext(conn)).toBeNull();

    const ctx = {
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
      table: 'users',
    };
    conn.activeTableKey = 'key-1';
    conn.tableStates.set('key-1', { ...emptyTableState(ctx), context: ctx });
    expect(activeTableContext(conn)).toEqual(ctx);
  });

  it('utility helpers behave consistently', () => {
    expect(editKey(2, 'name')).toBe('2:name');
    expect(toCellValue(null)).toBeNull();
    expect(toCellValue('x')).toBe('x');
    expect(getState(new Map(), null).page).toBe(0);
    expect(syncFlat(null, null, new Map()).tableName).toBeNull();
  });
});
