import { describe, it, expect, vi, beforeEach } from 'vitest';
import { openHistoryQuery } from '../openHistoryQuery';
import { useConnectionStore } from '../../../stores/connectionStore';
import { useActiveConnectionStore } from '../../../stores/activeConnectionStore';
import { usePanelStore } from '../../../stores/panelStore';

describe('openHistoryQuery', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePanelStore.getState().reset();
    useConnectionStore.setState({
      connections: [
        {
          id: 'conn-pg',
          name: 'PostgreSQL DB',
          databaseType: 'postgresql',
          database: 'app_db',
        } as any,
        {
          id: 'conn-mysql',
          name: 'MySQL DB',
          databaseType: 'mysql',
          database: 'sales_db',
        } as any,
      ],
    });
    useActiveConnectionStore.setState({
      connections: {},
    });
  });

  it('creates and activates QueryPanel immediately if target connection is connected', () => {
    useActiveConnectionStore.setState({
      connections: {
        'conn-pg': {
          connectionId: 'conn-pg',
          dbSessionId: 'sess-pg-123',
          status: 'connected',
        } as any,
      },
    });

    const onSelectConnection = vi.fn();
    openHistoryQuery(
      {
        connectionId: 'conn-pg',
        sql: 'SELECT 1;',
        database: 'app_db',
        schema: 'public',
      },
      { onSelectConnection },
    );

    expect(onSelectConnection).toHaveBeenCalledWith('conn-pg');
    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0]).toMatchObject({
      type: 'query',
      connectionId: 'conn-pg',
      dbSessionId: 'sess-pg-123',
      database: 'app_db',
      schema: 'public',
    });
    expect(state.activePanelId).toBe(state.panels[0].id);
    expect(state.queryExec.get(state.panels[0].id)?.sql).toBe('SELECT 1;');
  });

  it('sets pendingHistoryQuery and triggers onSelectConnection if target connection is not connected', () => {
    const onSelectConnection = vi.fn();
    openHistoryQuery(
      {
        connectionId: 'conn-mysql',
        sql: 'SELECT * FROM orders;',
        database: 'sales_db',
      },
      { onSelectConnection },
    );

    expect(onSelectConnection).toHaveBeenCalledWith('conn-mysql');
    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(0);
    expect(state.pendingHistoryQuery).toEqual({
      connectionId: 'conn-mysql',
      sql: 'SELECT * FROM orders;',
      database: 'sales_db',
      schema: undefined,
    });
  });

  it('falls back to currentConnectionId if entry.connectionId is absent', () => {
    useActiveConnectionStore.setState({
      connections: {
        'conn-pg': {
          connectionId: 'conn-pg',
          dbSessionId: 'sess-pg-123',
          status: 'connected',
        } as any,
      },
    });

    const onSelectConnection = vi.fn();
    openHistoryQuery(
      {
        sql: 'SELECT version();',
      },
      { onSelectConnection, currentConnectionId: 'conn-pg' },
    );

    expect(onSelectConnection).toHaveBeenCalledWith('conn-pg');
    const state = usePanelStore.getState();
    expect(state.panels).toHaveLength(1);
    expect(state.panels[0].connectionId).toBe('conn-pg');
    expect(state.queryExec.get(state.panels[0].id)?.sql).toBe('SELECT version();');
  });
});
