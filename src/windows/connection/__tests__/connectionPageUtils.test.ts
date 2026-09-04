import { describe, expect, it, vi, beforeEach } from 'vitest';
import {
  consumePendingConnection,
  makeTabFromPayload,
  removeConnectionFromStores,
  syncStoresActiveConnection,
} from '../connectionPageUtils';
import { PENDING_CONNECTION_KEY } from '../../../lib/windowManager';

const schemaStore = vi.hoisted(() => ({
  setActiveConnection: vi.fn(),
  removeConnection: vi.fn(),
}));
const tableDataStore = vi.hoisted(() => ({
  setActiveConnection: vi.fn(),
  removeConnection: vi.fn(),
}));

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: { getState: () => schemaStore },
}));
vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: { getState: () => tableDataStore },
}));

describe('[tester] connectionPageUtils', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it('makeTabFromPayload returns null without connectionId', () => {
    expect(makeTabFromPayload({})).toBeNull();
    expect(
      makeTabFromPayload({
        connectionId: 'cfg-1',
        connectionName: 'PG',
        databaseType: 'postgresql',
      }),
    ).toMatchObject({
      connectionId: 'cfg-1',
      status: 'connecting',
      databaseType: 'postgresql',
    });
    expect(
      makeTabFromPayload({
        connectionId: 'cfg-1',
        dbSessionId: 'sess-1',
        connectionName: 'PG',
        databaseType: 'postgresql',
      })?.status,
    ).toBe('connected');
  });

  it('consumePendingConnection reads and clears localStorage payload', () => {
    localStorage.setItem(
      PENDING_CONNECTION_KEY,
      JSON.stringify({
        connectionId: 'cfg-1',
        connectionName: 'PG',
        databaseType: 'postgresql',
        action: 'open-query',
      }),
    );
    const pending = consumePendingConnection();
    expect(pending?.tab.connectionId).toBe('cfg-1');
    expect(pending?.action).toBe('open-query');
    expect(localStorage.getItem(PENDING_CONNECTION_KEY)).toBeNull();
    expect(consumePendingConnection()).toBeNull();
  });

  it('consumePendingConnection returns null on invalid JSON', () => {
    localStorage.setItem(PENDING_CONNECTION_KEY, '{bad json');
    expect(consumePendingConnection()).toBeNull();
  });

  it('syncStoresActiveConnection and removeConnectionFromStores delegate to stores', () => {
    syncStoresActiveConnection('sess-1');
    expect(schemaStore.setActiveConnection).toHaveBeenCalledWith('sess-1');
    expect(tableDataStore.setActiveConnection).toHaveBeenCalledWith('sess-1');

    removeConnectionFromStores('sess-1');
    expect(schemaStore.removeConnection).toHaveBeenCalledWith('sess-1');
    expect(tableDataStore.removeConnection).toHaveBeenCalledWith('sess-1');
  });
});
