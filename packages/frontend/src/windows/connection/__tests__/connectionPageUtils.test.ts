import { describe, expect, it, vi, beforeEach } from 'vitest';
import {
  consumePendingConnection,
  enrichOpenConnectionPayload,
  makeTabFromPayload,
  removeConnectionFromStores,
  syncStoresActiveConnection,
} from '../connectionPageUtils';
import { PENDING_CONNECTION_KEY } from '../../../lib/windowManager';

const schemaStore = vi.hoisted(() => ({
  setActiveConnection: vi.fn(),
  removeConnection: vi.fn(),
}));

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: { getState: () => schemaStore },
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

  it('enrichOpenConnectionPayload fills name/type/database from the store record', () => {
    expect(
      enrichOpenConnectionPayload(
        { connectionId: 'cfg-1', requestId: 'cli:cfg-1:0' },
        [
          {
            id: 'cfg-1',
            name: 'Local PG',
            databaseType: 'postgresql',
            database: 'app',
          },
        ],
      ),
    ).toMatchObject({
      connectionId: 'cfg-1',
      connectionName: 'Local PG',
      databaseType: 'postgresql',
      database: 'app',
    });
    expect(enrichOpenConnectionPayload({ connectionId: 'missing' }, [])).toEqual({
      connectionId: 'missing',
    });
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

  it('syncStoresActiveConnection and removeConnectionFromStores delegate to the schema store', () => {
    syncStoresActiveConnection('sess-1');
    expect(schemaStore.setActiveConnection).toHaveBeenCalledWith('sess-1');

    removeConnectionFromStores('sess-1');
    expect(schemaStore.removeConnection).toHaveBeenCalledWith('sess-1');
  });
});
