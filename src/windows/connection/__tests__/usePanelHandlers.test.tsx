import { describe, expect, it, beforeEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { usePanelHandlers } from '../usePanelHandlers';
import { usePanelStore } from '../../../stores/panelStore';
import { useActiveConnectionStore } from '../../../stores/activeConnectionStore';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import type { DatabaseType } from '../../../types';
import type { ConnectionContext } from '../../../stores/panelStore';

/**
 * `DatabaseType` is codegen'd from the drivers resolved into this SKU, so a
 * driver a case pins (and registers a meta for itself) may be missing from the
 * current union.
 */
function driverType(id: string): DatabaseType {
  return id as DatabaseType;
}

function renderHandler(databaseType: DatabaseType, currentDatabase: string | null) {
  const connCtx: ConnectionContext = {
    connectionId: 'conn-1',
    dbSessionId: 'sess-1',
    connectionName: 'MyConn',
    databaseType,
  };
  return renderHook(() =>
    usePanelHandlers({
      connCtx,
      showStructureEditor: false,
      currentDatabase,
      initialDatabase: undefined,
      lastTableSchema: null,
      schemaViews: [],
    }),
  );
}

describe('usePanelHandlers.handleNewQuery binds a database to the query tab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePanelStore.getState().reset();
    useActiveConnectionStore.getState().reset();
  });

  it('binds the store currentDatabase onto a freshly created query tab', async () => {
    const { result } = renderHandler('mysql', 'db_b');

    let created = false;
    await act(async () => {
      created = result.current.handleNewQuery();
    });

    expect(created).toBe(true);

    const panel = usePanelStore.getState().panels.find((p) => p.type === 'query');
    expect(panel).toBeDefined();
    // The tab carries its own database (db_b), not undefined — so re-execution
    // uses the bound database instead of the session-wide store value.
    expect(panel?.database).toBe('db_b');
  });

  it('prefers the explicit target database over the store currentDatabase', async () => {
    const { result } = renderHandler('mysql', 'db_b');

    await act(async () => {
      result.current.handleNewQuery(undefined, { database: 'db_a', schema: 'public' });
    });

    const panel = usePanelStore.getState().panels.find((p) => p.type === 'query');
    expect(panel?.database).toBe('db_a');
    expect(panel?.schema).toBe('public');
  });

  it('keeps the panel database stable even when the store currentDatabase changes', async () => {
    const { result, rerender } = renderHook(
      ({ currentDatabase }) =>
        usePanelHandlers({
          connCtx: {
            connectionId: 'conn-1',
            dbSessionId: 'sess-1',
            connectionName: 'MyConn',
            databaseType: 'mysql',
          },
          showStructureEditor: false,
          currentDatabase,
          initialDatabase: undefined,
          lastTableSchema: null,
          schemaViews: [],
        }),
      { initialProps: { currentDatabase: 'db_b' } },
    );

    await act(async () => {
      result.current.handleNewQuery();
    });
    expect(usePanelStore.getState().panels.find((p) => p.type === 'query')?.database).toBe('db_b');

    // Another tab / a Settings round-trip changes the shared store value to db_a.
    rerender({ currentDatabase: 'db_a' });

    const panel = usePanelStore.getState().panels.find((p) => p.type === 'query');
    // The existing tab is unchanged — still bound to its own db_b.
    expect(panel?.database).toBe('db_b');
  });

  it('binds the whole path hierarchy (root + catalog/schema) for path-hierarchy drivers', async () => {
    // Superset is a path-hierarchy driver, but it is a git driver that is not
    // resolved in the basic `test:unit` set, so its meta is absent from
    // DB_REGISTRY. Register a minimal meta here to keep this assertion
    // self-contained and independent of the resolved driver set.
    (
      DB_REGISTRY as Record<
        string,
        {
          namespaceEnsure?: 'default-sql' | 'postgresql' | 'path-hierarchy';
        }
      >
    ).superset = { namespaceEnsure: 'path-hierarchy' };

    // The store currentDatabase is a pin that already encodes the full path:
    // <root>/<catalog>/<schema>.
    const { result } = renderHandler(driverType('superset'), '558:hive/snap');

    await act(async () => {
      result.current.handleNewQuery();
    });

    const panel = usePanelStore.getState().panels.find((p) => p.type === 'query');
    expect(panel).toBeDefined();
    // Root is bound as the panel database…
    expect(panel?.database).toBe('558:hive');
    // …and the *entire* path (root-first, matching the selector path) is
    // bound to the tab, not just the first level.
    expect(panel?.namespacePath).toEqual(['558:hive', 'snap']);
  });

  it('[tester] keeps routine overload and trigger target panel identities independent', async () => {
    const { result } = renderHandler('postgresql', 'db_a');

    await act(async () => {
      result.current.handleOpenDbObject('function', 'lookup', 'public', 'integer');
      result.current.handleOpenDbObject('function', 'lookup', 'public', 'text');
      result.current.handleOpenDbObject(
        'trigger',
        'audit_trigger',
        'public',
        undefined,
        'public',
        'orders',
      );
      result.current.handleOpenDbObject(
        'trigger',
        'audit_trigger',
        'public',
        undefined,
        'public',
        'users',
      );
      result.current.handleOpenDbObject(
        'function',
        'lookup',
        'public',
        'integer',
        undefined,
        undefined,
        'db_b',
      );
    });

    const panels = usePanelStore.getState().panels.filter((panel) => panel.type === 'db-object');
    expect(panels).toHaveLength(5);
    expect(panels.map((panel) => panel.type === 'db-object' && panel.objectDatabase)).toEqual([
      'db_a',
      'db_a',
      'db_a',
      'db_a',
      'db_b',
    ]);
    expect(panels.map((panel) => panel.type === 'db-object' && panel.objectSignature)).toEqual([
      'integer',
      'text',
      null,
      null,
      'integer',
    ]);
    expect(panels.map((panel) => panel.type === 'db-object' && panel.objectTargetName)).toEqual([
      null,
      null,
      'orders',
      'users',
      null,
    ]);
  });
});

describe('usePanelHandlers.handleOpenErDiagram binds the database and inherits the panel schema', () => {
  const connCtx: ConnectionContext = {
    connectionId: 'conn-1',
    dbSessionId: 'sess-1',
    connectionName: 'MyConn',
    databaseType: 'postgresql',
  };

  function renderWith(
    resolveTableSchema?: (table: string) => string | null,
    currentDatabase: string | null = 'db_a',
  ) {
    return renderHook(() =>
      usePanelHandlers({
        connCtx,
        showStructureEditor: false,
        currentDatabase,
        initialDatabase: undefined,
        lastTableSchema: null,
        schemaViews: [{ name: 'v_report', schema: 'reporting' }],
        resolveTableSchema,
      }),
    );
  }

  function seedActiveTablePanel(tableSchema: string | null) {
    usePanelStore.getState().addPanel({
      id: 'tbl-1',
      type: 'table',
      tableName: 'orders',
      database: 'db_a',
      tableSchema,
      subTab: 'data',
      connectionId: 'conn-1',
      dbSessionId: 'sess-1',
      connectionName: 'MyConn',
      databaseType: 'postgresql',
    });
    expect(usePanelStore.getState().activePanelId).toBe('tbl-1');
  }

  function erPanel() {
    return usePanelStore.getState().panels.find((p) => p.type === 'er-diagram');
  }

  beforeEach(() => {
    vi.clearAllMocks();
    usePanelStore.getState().reset();
    useActiveConnectionStore.getState().reset();
  });

  it('takes the schema from the panel that was active when it opened', () => {
    seedActiveTablePanel('reporting');
    const { result } = renderWith();

    act(() => {
      result.current.handleOpenErDiagram();
    });

    // Not the connection default, and not the database name.
    expect(erPanel()?.schema).toBe('reporting');
  });

  it('keeps null when the active panel carries no schema', () => {
    seedActiveTablePanel(null);
    const { result } = renderWith();

    act(() => {
      result.current.handleOpenErDiagram();
    });

    expect(erPanel()?.schema).toBeNull();
  });

  it('prefers the focused relation schema over the active panel', () => {
    seedActiveTablePanel('reporting');
    const { result } = renderWith((table) => (table === 'orders' ? 'sales' : null));

    act(() => {
      result.current.handleOpenErDiagram('orders');
    });

    const panel = erPanel();
    expect(panel?.schema).toBe('sales');
    expect(panel && 'focusTable' in panel ? panel.focusTable : undefined).toBe('orders');
  });

  it('falls back to the loaded view schema when the resolver has no answer', () => {
    seedActiveTablePanel('reporting');
    const { result } = renderWith(() => null);

    act(() => {
      result.current.handleOpenErDiagram('v_report');
    });

    expect(erPanel()?.schema).toBe('reporting');
  });

  it('re-follows the current panel when an existing diagram is reopened', () => {
    seedActiveTablePanel('reporting');
    const { result } = renderWith();

    act(() => {
      result.current.handleOpenErDiagram();
    });
    expect(usePanelStore.getState().panels.filter((p) => p.type === 'er-diagram')).toHaveLength(1);

    // The user switches to a table in another schema and reopens the diagram:
    // it must follow, not keep the stale namespace.
    act(() => {
      usePanelStore.getState().updatePanel('tbl-1', { tableSchema: 'audit' });
      usePanelStore.getState().setActivePanel('tbl-1');
    });
    act(() => {
      result.current.handleOpenErDiagram();
    });

    expect(usePanelStore.getState().panels.filter((p) => p.type === 'er-diagram')).toHaveLength(1);
    expect(erPanel()?.schema).toBe('audit');
  });

  function renderErHandlers(props: { currentDatabase: string | null; initialDatabase?: string }) {
    return renderHook(
      ({ currentDatabase, initialDatabase }) =>
        usePanelHandlers({
          connCtx: {
            connectionId: 'conn-1',
            dbSessionId: 'sess-1',
            connectionName: 'MyConn',
            databaseType: 'mysql',
          },
          showStructureEditor: false,
          currentDatabase,
          initialDatabase,
          lastTableSchema: null,
          schemaViews: [],
        }),
      { initialProps: props },
    );
  }

  it('carries the database selected at open time', async () => {
    const { result } = renderErHandlers({ currentDatabase: 'app' });
    await act(async () => {
      result.current.handleOpenErDiagram();
    });
    expect(usePanelStore.getState().panels.find((p) => p.type === 'er-diagram')?.database).toBe(
      'app',
    );
  });

  it('falls back to the configured database when the session has none', async () => {
    const { result } = renderErHandlers({ currentDatabase: null, initialDatabase: 'app' });
    await act(async () => {
      result.current.handleOpenErDiagram();
    });
    expect(usePanelStore.getState().panels.find((p) => p.type === 'er-diagram')?.database).toBe(
      'app',
    );
  });

  it('keeps its own database when another tab moves the session default', async () => {
    const { result, rerender } = renderErHandlers({ currentDatabase: 'app' });
    await act(async () => {
      result.current.handleOpenErDiagram();
    });
    rerender({ currentDatabase: 'analytics' });
    expect(usePanelStore.getState().panels.find((p) => p.type === 'er-diagram')?.database).toBe(
      'app',
    );
  });

  it('re-binds an existing ER tab to an explicit target database', async () => {
    const { result } = renderErHandlers({ currentDatabase: 'app' });
    await act(async () => {
      result.current.handleOpenErDiagram();
    });
    expect(usePanelStore.getState().panels.find((p) => p.type === 'er-diagram')?.database).toBe(
      'app',
    );
    // Tree context menu on another database passes the target explicitly.
    await act(async () => {
      result.current.handleOpenErDiagram(undefined, 'analytics');
    });
    expect(usePanelStore.getState().panels.find((p) => p.type === 'er-diagram')?.database).toBe(
      'analytics',
    );
  });

  it('re-binds a bare toolbar invocation to the current session database', async () => {
    const { result, rerender } = renderErHandlers({ currentDatabase: 'app' });
    await act(async () => {
      result.current.handleOpenErDiagram();
    });
    // The user switches database, then hits the toolbar "查看 ER" again: the
    // reused tab must follow the current selection, not stay stale.
    rerender({ currentDatabase: 'analytics' });
    await act(async () => {
      result.current.handleOpenErDiagram();
    });
    expect(usePanelStore.getState().panels.find((p) => p.type === 'er-diagram')?.database).toBe(
      'analytics',
    );
  });

  it('keeps the tab database for a focus-only invocation', async () => {
    const { result } = renderErHandlers({ currentDatabase: 'app' });
    await act(async () => {
      result.current.handleOpenErDiagram();
    });
    // Focusing a table from inside the diagram must not rebind the database.
    await act(async () => {
      result.current.handleOpenErDiagram('orders');
    });
    const panel = usePanelStore.getState().panels.find((p) => p.type === 'er-diagram');
    expect(panel?.database).toBe('app');
    expect(panel?.focusTable).toBe('orders');
  });
});
