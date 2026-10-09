import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, screen, fireEvent } from '@testing-library/react';
import { ConnectionPage } from '../ConnectionPage';
import { tauriWindowTestState } from '../../../test/mocks/tauriWindow';

/**
 * Journey: the content column must never keep showing the previous connection.
 *
 * The reported sequence is three clicks, and it is the middle state that is the
 * trap — redis opens, db0 is picked, *then* mysql is opened for the first time.
 * A connection with no tab yet takes a different branch in
 * `handleSelectConnection` than one that already has one, and the panel retarget
 * used to exist on only one of them, so the redis workbench stayed on screen and
 * the tree kept highlighting redis.
 *
 * These cases assert the symptom (`data-connection` on the content column, which
 * the mock mirrors from the real ContentView's `activePanel ? … : overview`
 * branch) instead of the mechanism, so a future refactor cannot pass by moving
 * the `setActivePanel` call somewhere else while keeping the behaviour broken.
 */

const connectMock = vi.fn();
const releaseConnectionMock = vi.fn();
const pingMock = vi.fn().mockResolvedValue(undefined);
const fetchConnectionsMock = vi.fn().mockResolvedValue(undefined);
const fetchGroupsMock = vi.fn().mockResolvedValue(undefined);
const emitCrossWindowMock = vi.fn().mockResolvedValue(undefined);
const listenCrossWindowMock = vi.fn().mockResolvedValue(() => {});
const hasOpenChildWindowsMock = vi.fn().mockResolvedValue(false);
const openNewConnectionDialogMock = vi.fn();
const openBackupWindowMock = vi.fn();
const openDataSyncWindowMock = vi.fn();
const openSchemaDiffWindowMock = vi.fn();
const openWorkflowWindowMock = vi.fn();
const openDashboardWindowMock = vi.fn();
const upsertSchemaMock = vi.fn();

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (k: string) => k }),
}));

vi.mock('../../../hooks/useSettings', () => ({ useSettings: () => ({}) }));
vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(false), null],
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: Record<string, unknown>) => unknown) =>
    sel({
      loadSettings: vi.fn().mockResolvedValue(undefined),
      settings: { theme: { mode: 'dark' } },
    }),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: Record<string, unknown>) => unknown) =>
    sel({
      loadConfig: vi.fn().mockResolvedValue(undefined),
      // Resolves to the *cleanup function*, not undefined: `ConnectionPage` does
      // `setupAiListeners().then((fn) => fn())` in its unmount effect.
      setupEventListeners: vi.fn().mockResolvedValue(() => {}),
    }),
}));

const CONNECTIONS = [
  { id: 'conn-redis', name: 'Local Redis', databaseType: 'redis', database: '' },
  { id: 'conn-mysql', name: 'Local MySQL', databaseType: 'mysql', database: 'shop' },
];

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: (sel: (s: Record<string, unknown>) => unknown) =>
    sel({
      connections: CONNECTIONS,
      groups: [],
      fetchConnections: fetchConnectionsMock,
      fetchGroups: fetchGroupsMock,
      deleteConnection: vi.fn(),
    }),
  groupConnections: (c: unknown[]) => c,
}));

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: Object.assign(
    (sel: (s: Record<string, unknown>) => unknown) =>
      sel({ connections: activeSessions, connect: vi.fn() }),
    { getState: () => ({ connections: activeSessions, markConnecting: vi.fn() }) },
  ),
}));

vi.mock('../../../stores/schemaStore', async () => {
  // See ConnectionPage.test.tsx: the per-session selectors are part of the real
  // module's surface, so the mock reproduces them via the shared double.
  const { schemaStoreMockModule } = await import('../../../test/mocks/schemaStore');
  const perSession = schemaStoreMockModule({ schemas: new Map(), activeDbSessionId: null });
  return {
    useSchemaStore: {
      getState: () => ({
        reset: vi.fn(),
        removeConnection: vi.fn(),
        setActiveConnection: vi.fn(),
        databases: [],
        currentDatabase: null,
        tables: [],
        upsertSchema: upsertSchemaMock,
      }),
    },
    useConnectionSchemaField: perSession.useConnectionSchemaField,
    useConnectionColumnMaps: perSession.useConnectionColumnMaps,
  };
});

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: { getState: () => ({ reset: vi.fn(), removeConnection: vi.fn() }) },
}));

/*
 * A stateful panel store. The existing `ConnectionPage.test.tsx` mock freezes
 * state and stubs `setActivePanel` with a `vi.fn()`, which is enough for "does it
 * render" but cannot see *which panel ends up active* — precisely the fact under
 * test here.
 */
vi.mock('../../../stores/panelStore', async () => {
  const { useSyncExternalStore } = await import('react');
  // `vi.mock` replaces the whole module, so the pane-identity helpers and the
  // frozen exec default it re-exports must exist here too. Both come from pure
  // leaf modules, so take the real implementations rather than faking a shape.
  const pane = await vi.importActual<typeof import('../../../stores/paneKeys')>(
    '../../../stores/paneKeys',
  );
  const { EMPTY_QUERY_EXEC } = await vi.importActual<
    typeof import('../../../stores/queryExecActions')
  >('../../../stores/queryExecActions');
  type Panel = { id: string; connectionId: string; databaseType: string; dbName?: string };
  let state: { panels: Panel[]; activePanelId: string | null } = {
    panels: [],
    activePanelId: null,
  };
  const listeners = new Set<() => void>();
  const setState = (patch: Partial<typeof state>) => {
    state = { ...state, ...patch };
    listeners.forEach((l) => l());
  };
  const useStore = (sel: (s: typeof state) => unknown) =>
    useSyncExternalStore(
      (cb) => {
        listeners.add(cb);
        return () => listeners.delete(cb);
      },
      () => sel(state),
    );
  const noop = () => {};
  const store = Object.assign(useStore, {
    getState: () => ({
      ...state,
      setActivePanel: (id: string | null) => setState({ activePanelId: id ?? null }),
      // Faithful to the real store's `activate = true` default, so picking a db
      // for the first time really does create *and* activate its panel — which
      // is the state the bug report starts from.
      addPanel: (panel: Panel, activate = true) =>
        setState({
          panels: [...state.panels, panel],
          activePanelId: activate ? panel.id : state.activePanelId,
        }),
      removePanel: noop,
      updatePanel: noop,
      closeAllPanels: noop,
      closeOtherPanels: noop,
      closePanelsToTheLeft: noop,
      closePanelsToTheRight: noop,
      removeAllForConnection: noop,
      executeQuery: noop,
      executeSelection: noop,
      cancelQuery: noop,
      setActiveResult: noop,
      togglePinResult: noop,
      setResultDetailRow: noop,
      setSelectedRows: noop,
      openQueryHistory: noop,
      setPendingQueryHistory: noop,
      setPendingHistoryQuery: noop,
      loadHistory: noop,
    }),
  });
  return {
    ...pane,
    EMPTY_QUERY_EXEC,
    usePanelStore: store,
    nextPanelId: (prefix: string, n?: number) => `${prefix}-${n ?? 0}`,
    __panelTest: {
      read: () => state,
      reset() {
        setState({ panels: [], activePanelId: null });
      },
    },
  };
});

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    connect: (...args: unknown[]) => connectMock(...args),
    releaseConnection: (...args: unknown[]) => releaseConnectionMock(...args),
    pingConnection: (...args: unknown[]) => pingMock(...args),
  },
}));

vi.mock('../../../lib/windowManager', () => ({
  PENDING_CONNECTION_KEY: 'datazen:pending-connection',
  hasOpenChildWindows: (...args: unknown[]) => hasOpenChildWindowsMock(...args),
  openNewConnectionDialog: (...args: unknown[]) => openNewConnectionDialogMock(...args),
  openBackupWindow: (...args: unknown[]) => openBackupWindowMock(...args),
  openDataSyncWindow: (...args: unknown[]) => openDataSyncWindowMock(...args),
  openSchemaDiffWindow: (...args: unknown[]) => openSchemaDiffWindowMock(...args),
  openWorkflowWindow: (...args: unknown[]) => openWorkflowWindowMock(...args),
  openDashboardWindow: (...args: unknown[]) => openDashboardWindowMock(...args),
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  emitCrossWindow: (...args: unknown[]) => emitCrossWindowMock(...args),
  listenCrossWindow: (...args: unknown[]) => listenCrossWindowMock(...args),
}));

/*
 * The real ContentView resolves its content from `activePanel` and falls back to
 * `ConnectionWorkspaceHome` when there is none. Mirroring exactly that decision
 * here is what makes `data-connection="overview"` a real assertion instead of a
 * restatement of the store.
 */
vi.mock('../ContentView', async () => {
  const { usePanelStore } = await import('../../../stores/panelStore');
  return {
    ContentView: () => {
      const { panels, activePanelId } = usePanelStore(
        (s: { panels: { id: string; connectionId: string }[]; activePanelId: string | null }) => s,
      );
      const active = panels.find((p) => p.id === activePanelId);
      return (
        <div data-testid="content-view" data-connection={active?.connectionId ?? 'overview'} />
      );
    },
  };
});

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: () => <div data-testid="title-bar" />,
}));
vi.mock('../../../components/MenuBar', () => ({ MenuBar: () => <div data-testid="menu-bar" /> }));
vi.mock('../../../components/ThemeToggle', () => ({
  ThemeToggle: () => <div data-testid="theme-toggle" />,
}));
vi.mock('../../../components/ui/Dialog', () => ({
  Dialog: ({ children }: { children?: React.ReactNode }) => <div>{children}</div>,
}));

/* One clickable row per connection, so the journey is driven the way a user drives it. */
vi.mock('../ConnectionNavigatorTree', async () => {
  const { forwardRef } = await import('react');
  return {
    ConnectionNavigatorTree: forwardRef(
      (
        {
          activeConnectionId,
          onSelectConnection,
          onSelectKvDb,
        }: {
          activeConnectionId: string | null;
          onSelectConnection: (id: string) => void;
          onSelectKvDb: (id: string, db: string) => void;
        },
        _ref: unknown,
      ) => (
        <div data-testid="navigator-tree" data-active={activeConnectionId ?? ''}>
          {CONNECTIONS.map((c) => (
            <div key={c.id}>
              <button
                type="button"
                data-testid={`select-conn-${c.id}`}
                onClick={() => onSelectConnection(c.id)}
              >
                {c.name}
              </button>
              {c.databaseType === 'redis' ? (
                <button
                  type="button"
                  data-testid="select-redis-db0"
                  onClick={() => onSelectKvDb(c.id, 'db0')}
                >
                  db0
                </button>
              ) : null}
            </div>
          ))}
        </div>
      ),
    ),
  };
});

let activeSessions: Record<string, { status: string; dbSessionId: string }> = {};
let panelTest: {
  read(): { panels: { id: string; connectionId: string }[]; activePanelId: string | null };
  reset(): void;
};

function activePanelConnectionId(): string {
  return screen.getByTestId('content-view').getAttribute('data-connection') ?? '';
}

describe('ConnectionPage — the content column follows the selected connection', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    localStorage.clear();
    activeSessions = {
      'conn-redis': { status: 'connected', dbSessionId: 'sess-redis' },
      'conn-mysql': { status: 'connected', dbSessionId: 'sess-mysql' },
    };
    const store = await import('../../../stores/panelStore');
    panelTest = (store as unknown as { __panelTest: typeof panelTest }).__panelTest;
    panelTest.reset();
    tauriWindowTestState.closeRequestedHandler.current = null;
    tauriWindowTestState.closeHandlerRegistrationCount.current = 0;
  });

  afterEach(() => {
    cleanup();
  });

  /**
   * Drives the reported three clicks and returns the id of the redis db0 panel,
   * which is the thing that must not survive step 3.
   */
  async function openRedisDb0(): Promise<string> {
    // A connection with no db picked yet has no panel, so its overview is the
    // correct first state — not a bug, and asserted so a later "overview" cannot
    // pass simply because the column never left it.
    fireEvent.click(screen.getByTestId('select-conn-conn-redis'));
    await waitFor(() => expect(activePanelConnectionId()).toBe('overview'));

    fireEvent.click(screen.getByTestId('select-redis-db0'));
    await waitFor(() => expect(activePanelConnectionId()).toBe('conn-redis'));
    const db0Panel = panelTest.read().activePanelId;
    expect(db0Panel).toBeTruthy();
    return db0Panel as string;
  }

  it('shows mysql, not the redis db0 workbench, when mysql is opened for the first time', async () => {
    render(<ConnectionPage />);
    await screen.findByTestId('content-view');
    const db0Panel = await openRedisDb0();

    // Now open mysql, which has never been opened ⇒ no tab ⇒ the other branch.
    fireEvent.click(screen.getByTestId('select-conn-conn-mysql'));
    await waitFor(() => expect(activePanelConnectionId()).toBe('overview'));
    // The symptom, stated directly: the redis panel is no longer the active one.
    // Asserting only "overview" would also pass for a column stuck on overview.
    expect(panelTest.read().activePanelId).toBeNull();
    expect(panelTest.read().activePanelId).not.toBe(db0Panel);

    // The tree's own selection is derived from the active panel, so it has to
    // move too — a content column that switched while the tree still pointed at
    // redis would be the same bug wearing a different hat.
    await waitFor(() =>
      expect(screen.getByTestId('navigator-tree').getAttribute('data-active')).toBe('conn-mysql'),
    );
  });

  it('goes back to redis db0 when redis is re-selected, rather than to an empty column', async () => {
    render(<ConnectionPage />);
    await screen.findByTestId('content-view');
    const db0Panel = await openRedisDb0();

    fireEvent.click(screen.getByTestId('select-conn-conn-mysql'));
    await waitFor(() => expect(panelTest.read().activePanelId).toBeNull());

    // Round trip. The retarget re-adopts the new connection's own panel, so the
    // user lands back where they were instead of on another connection's view.
    fireEvent.click(screen.getByTestId('select-conn-conn-redis'));
    await waitFor(() => expect(activePanelConnectionId()).toBe('conn-redis'));
    expect(panelTest.read().activePanelId).toBe(db0Panel);
  });

  it('leaves the active panel alone when the same connection is re-selected', async () => {
    render(<ConnectionPage />);
    await screen.findByTestId('content-view');
    const db0Panel = await openRedisDb0();

    // Re-clicking the connection you are already on is not a context switch; if
    // the retarget ran here it would drop the user out of the db they opened.
    fireEvent.click(screen.getByTestId('select-conn-conn-redis'));
    await waitFor(() => expect(activePanelConnectionId()).toBe('conn-redis'));
    expect(panelTest.read().activePanelId).toBe(db0Panel);
  });
});
