/**
 * Cross-boundary consistency: the KV context bar's db selector must reach the
 * **one** host implementation of "activate or open this db's panel".
 *
 * Why this file exists rather than a case inside `useKvSlotActions.test.tsx`:
 * the dispatcher test proves the dispatcher forwards `database` to whatever sink
 * it was given. It cannot prove *which* sink `ContentView` handed it. The
 * coordinator's ruling on the `selectDatabase` action allowed the contract to
 * grow **only** on the condition that `ContentView` reuses `ConnectionPage`'s
 * `handleSelectKvDb` instead of re-implementing "find the panel for this db, or
 * create one" — a second implementation would compile, pass every dispatcher
 * test, and silently fork the behaviour (two panels for one db, or a panel the
 * navigation tree cannot see).
 *
 * So the tests below assert the *shape of the wiring*, not just its effect:
 *
 * 1. `ContentView` calls `useKvSlotActions` with an `onSelectDatabase` that
 *    forwards `(connectionId, database)` to the prop it was handed — i.e. the
 *    callback is threaded, never invented.
 * 2. With no callback handed down, `onSelectDatabase` is `undefined`, so the
 *    dispatcher's documented no-op + warning path stays reachable instead of a
 *    hollow wrapper that pretends to work.
 * 3. `ConnectionPage` passes the **same** function object it gives the
 *    navigation tree to `ContentView`, which is the mechanical statement of
 *    "one implementation".
 *
 * `useKvSlotActions` is mocked, so these are wiring assertions with no Redis or
 * workspace runtime involved.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render } from '@testing-library/react';
import type { ConnectionSchemaState } from '../../../stores/schemaStoreState';

const captured = vi.hoisted(() => ({
  args: [] as Array<{ onRefresh: () => void; onSelectDatabase?: (db: string) => void }>,
  handleOpenErDiagram: vi.fn(),
}));

vi.mock('../useKvSlotActions', () => ({
  useKvSlotActions: (args: { onRefresh: () => void; onSelectDatabase?: (db: string) => void }) => {
    captured.args.push(args);
    return { request: () => {}, dialog: null };
  },
}));

// The heavyweight workspace machinery `ContentView` mounts is irrelevant here and
// would drag in the whole store graph; each is replaced by the thinnest stub that
// keeps the module graph resolvable. The prop under test never touches them.
vi.mock('../useConnectionWorkspaceMeta', () => ({
  useConnectionWorkspaceMeta: () => ({
    sidebarConnCtx: null,
    initialDatabase: undefined,
    databaseType: 'redis',
    dbSessionId: 'sess-1',
    connectionName: 'local',
    hasSavedConnections: false,
    showStructureEditor: false,
    exportScope: undefined,
    batchExportSupported: false,
    showNewQuery: false,
    showNewTable: false,
    showErDiagramToolbar: false,
    showObjectsToolbar: false,
    connectingEntry: null,
    connectingName: '',
    connectingDbType: undefined,
    recentPanels: [],
    statusDatabase: 'db5',
    isKvPanel: true,
    connectionId: 'cfg-1',
  }),
}));

vi.mock('../useKvWorkspaceSlots', () => ({
  useKvWorkspaceSlots: () => ({}),
}));

vi.mock('../usePanelHandlers', () => ({
  usePanelHandlers: () => ({
    handleRefresh: () => {},
    handleNewQuery: () => {},
    handleCreateTable: () => {},
    handleOpenErDiagram: captured.handleOpenErDiagram,
    handleOpenObjects: () => {},
    handleOpenPrivileges: () => {},
    handleSelectTable: () => {},
    handleClosePanel: () => {},
    handlePanelTabContextMenu: () => {},
    handleSetSubTab: () => {},
    handleExitStructureEditing: () => {},
    handleEditTableStructure: () => {},
    handleOpenQueryHistory: () => {},
  }),
}));

// `vi.mock` replaces the whole module, so the pane-identity helpers and the
// frozen exec default `panelStore` re-exports must exist here too; both come
// from pure leaf modules, so take the real implementations.
vi.mock('../../../stores/panelStore', async () => {
  const pane = await vi.importActual<typeof import('../../../stores/paneKeys')>(
    '../../../stores/paneKeys',
  );
  const { EMPTY_QUERY_EXEC } = await vi.importActual<
    typeof import('../../../stores/queryExecActions')
  >('../../../stores/queryExecActions');
  const state = {
    panels: [],
    activePanelId: null,
    setActivePanel: () => {},
    updatePanel: () => {},
  };
  const usePanelStore = (selector: (s: typeof state) => unknown) => selector(state);
  usePanelStore.getState = () => state;
  return { ...pane, EMPTY_QUERY_EXEC, usePanelStore, nextPanelId: (prefix: string) => `panel-${prefix}` };
});

vi.mock('../../../stores/settingsStore', () => {
  const state = { settings: { safeMode: false } };
  const useSettingsStore = (selector: (s: typeof state) => unknown) => selector(state);
  useSettingsStore.getState = () => state;
  return { useSettingsStore };
});

vi.mock('../../../stores/connectionStore', () => {
  const state = { connections: [] };
  const useConnectionStore = (selector: (s: typeof state) => unknown) => selector(state);
  useConnectionStore.getState = () => state;
  return { useConnectionStore };
});

// Wholesale replacement of the real module, so it must expose every value export
// it has — `useConnectionSchemaField` was missing, and `ContentView` calls it
// three times, so every render in this file threw before asserting anything.
vi.mock('../../../stores/schemaStore', async () => {
  const { schemaStoreMockModule } = await import('../../../test/mocks/schemaStore');
  const state = {
    schemas: new Map<string, ConnectionSchemaState>(),
    activeDbSessionId: null as string | null,
    loadForConnection: async () => {},
    loadTables: async () => {},
    setCurrentDatabase: () => {},
  };
  return schemaStoreMockModule(state);
});

vi.mock('../../../stores/tableDataStore', () => {
  const state = { byPanel: new Map(), removePanel: () => {} };
  const useTableDataStore = (selector: (s: typeof state) => unknown) => selector(state);
  useTableDataStore.getState = () => state;
  return { useTableDataStore };
});

vi.mock('../../../../src/lib/kvSlotState', () => ({ pruneKvSlotStates: () => {} }));
vi.mock('../../../lib/kvSlotState', () => ({ pruneKvSlotStates: () => {} }));

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { ContentView } from '../ContentView';
import type { ConnectionViewActions } from '../../../lib/connectionViews/types';

beforeEach(() => {
  captured.args.length = 0;
  captured.handleOpenErDiagram.mockReset();
});

afterEach(() => {
  cleanup();
});

/** The dispatcher args `ContentView` produced on its last render. */
function lastArgs() {
  const args = captured.args.at(-1);
  if (!args) throw new Error('ContentView never called useKvSlotActions');
  return args;
}

describe('ContentView ⇄ useKvSlotActions — the selectDatabase sink is threaded, not invented', () => {
  it('binds the host callback to the active panel\u2019s connection', () => {
    const onSelectKvDb = vi.fn();
    render(<ContentView onSelectKvDb={onSelectKvDb} />);

    const sink = lastArgs().onSelectDatabase;
    expect(typeof sink).toBe('function');
    sink?.('db3');

    // `KvSlotAction.selectDatabase` carries only the db label; the connection
    // comes from the panel this layer resolved.
    expect(onSelectKvDb).toHaveBeenCalledTimes(1);
    // pendingAction is optional; the context-bar sink passes no action.
    expect(onSelectKvDb).toHaveBeenCalledWith('cfg-1', 'db3', undefined);
  });

  it('leaves the sink undefined when the host threaded no callback down', () => {
    render(<ContentView />);
    // Not a hollow wrapper: the dispatcher must still be able to detect the
    // missing capability and degrade to its documented no-op + warning.
    expect(lastArgs().onSelectDatabase).toBeUndefined();
  });

  it('keeps the sink stable across re-renders so memoised slots do not churn', () => {
    const onSelectKvDb = vi.fn();
    const { rerender } = render(<ContentView onSelectKvDb={onSelectKvDb} />);
    const first = lastArgs().onSelectDatabase;

    rerender(<ContentView onSelectKvDb={onSelectKvDb} />);

    expect(lastArgs().onSelectDatabase).toBe(first);
  });

  it('rebinds when the host hands it a different callback', () => {
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = render(<ContentView onSelectKvDb={first} />);
    rerender(<ContentView onSelectKvDb={second} />);

    lastArgs().onSelectDatabase?.('db7');

    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledWith('cfg-1', 'db7', undefined);
  });
});

describe('ContentView connection-view action ref', () => {
  it('passes an explicit ER database target to the panel handler', () => {
    const actionsRef: { current: ConnectionViewActions | undefined } = { current: undefined };
    render(<ContentView actionsRef={actionsRef} />);

    act(() => {
      actionsRef.current?.openErDiagram(undefined, 'analytics');
    });

    expect(captured.handleOpenErDiagram).toHaveBeenCalledWith(undefined, 'analytics');
  });
});

/**
 * The mechanical half of the ruling: "reuse the same `handleSelectKvDb`, never
 * add a second implementation".
 *
 * The rendering tests above cannot catch a *duplicate* implementation, because a
 * duplicate would still forward to whatever prop it was given — it would simply
 * also live in `ContentView` and create panels on its own. The only reliable
 * check is structural: the panel-creating call (`addPanel`) must appear in
 * exactly one module on this path.
 *
 * This is a source-shape assertion, so it is written to fail loudly rather than
 * silently pass if the files move: a missing file is an error, not a skip.
 */
describe('one implementation of "open / activate this db\u2019s panel"', () => {
  const CONNECTION_DIR = resolve(process.cwd(), 'src/windows/connection');

  function read(name: string): string {
    return readFileSync(resolve(CONNECTION_DIR, name), 'utf8');
  }

  it('creates KV panels only in ConnectionPage, never in ContentView', () => {
    // `ContentView` reaches the panel store for activation/updates, which is
    // legitimate; what it must not do is *create* a database panel.
    const contentView = read('ContentView.tsx');
    expect(contentView).not.toContain('addPanel');
    expect(contentView).not.toContain('nextPanelId');

    // And it must not carry its own copy of the "find the existing panel" probe.
    expect(contentView).not.toContain('dbName ===');

    // The single implementation really is where we say it is.
    const connectionPage = read('ConnectionPage.tsx');
    expect(connectionPage).toContain('const handleSelectKvDb');
    expect(connectionPage).toContain('addPanel(panel)');
  });

  it('hands that one implementation to both consumers', () => {
    const connectionPage = read('ConnectionPage.tsx');
    // The navigation tree…
    expect(connectionPage).toContain('onSelectKvDb={handleSelectKvDb}');
    // …and ContentView (hence the KV slots). Counted, so deleting one fails.
    const bindings = connectionPage.match(/onSelectKvDb=\{handleSelectKvDb\}/g) ?? [];
    expect(bindings.length).toBe(2);
  });

  it('does not let ContentView name a driver or a panel type to switch dbs', () => {
    // PRD §7-4: no `databaseType === 'redis'` branch on the host side. The db
    // switch must stay metadata/panel-store driven.
    const contentView = read('ContentView.tsx');
    expect(contentView).not.toMatch(/databaseType\s*===\s*'redis'/);
    expect(contentView).not.toMatch(/type:\s*'redis-db'/);
  });
});
