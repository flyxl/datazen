import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor, cleanup } from '@testing-library/react';
import { ApiError, toId, type Counter, type Id, type SessionView } from '@datazen/backend-client';
import type { EditorSessionController } from '../../../lib/session/EditorSessionController';

/**
 * Journey tests for the two-phase database switch of a QueryPanel editor
 * session: the runtime session context is switched first, and the legacy
 * `switchDatabase` pointer only moves once the session switch succeeded.
 */

const panelMocks = vi.hoisted(() => ({ updatePanel: vi.fn() }));
const schemaMocks = vi.hoisted(() => ({
  switchDatabase: vi.fn().mockResolvedValue(undefined),
  ensureNamespacePath: vi.fn().mockResolvedValue(undefined),
  databases: ['app', 'other'] as string[],
  currentDatabase: 'app',
}));

vi.mock('../../../stores/panelStore', () => ({
  usePanelStore: Object.assign(
    (sel: (s: unknown) => unknown) => sel({ updatePanel: panelMocks.updatePanel }),
    { getState: () => ({ updatePanel: panelMocks.updatePanel }) },
  ),
}));

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: (sel: (s: unknown) => unknown) => sel(schemaMocks),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/usePlatform', () => ({
  usePlatform: () => 'macos',
}));

vi.mock('../../../lib/nativeContextMenu', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../lib/nativeContextMenu')>()),
  showNativeContextMenu: vi.fn(),
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    clearQueryHistory: vi.fn().mockResolvedValue(undefined),
    refreshFavorites: vi.fn().mockResolvedValue([]),
    getFavoritesRoot: vi.fn().mockResolvedValue('/tmp/favorites'),
    sessionTransactionStatus: vi.fn().mockResolvedValue(false),
  },
}));

// Imported after the mocks above are registered.
const { useQueryContextPath } = await import('../query/QuerySidebarSection');

const counter = (n: number) => n as Counter;

function sessionView(overrides: Partial<SessionView> = {}): SessionView {
  return {
    handle: { dbSessionId: toId('sess-conn-1') as Id, runtimeEpoch: toId('epoch-1') as Id },
    connectionId: toId('cfg-1') as Id,
    configRevision: counter(3),
    owner: {
      kind: 'editor',
      clientInstanceId: toId('client-1') as Id,
      editorSessionId: toId('panel-1') as Id,
    },
    initialTarget: {
      connectionId: toId('cfg-1') as Id,
      namespace: { database: 'app', catalog: null, schema: 'public', path: [] },
      object: null,
    },
    observedContext: {
      namespace: { database: 'app', catalog: null, schema: 'public', path: [] },
      searchPath: null,
      effectiveIdentity: 'app_user',
      transactionState: 'none',
      autocommit: true,
      confidence: 'confirmed',
    },
    contextRevision: counter(7),
    state: 'ready',
    attachmentState: 'attached',
    activeExecutionId: null,
    expiresAt: null,
    ...overrides,
  };
}

/** Only the members `switchDatabaseSession` uses; the rest stays inert. */
function controllerWith(behavior: {
  view?: SessionView | null;
  setContext?: (view: SessionView) => Promise<SessionView>;
}): { controller: EditorSessionController; setContext: ReturnType<typeof vi.fn> } {
  const setContext = vi.fn(
    behavior.setContext ?? ((view: SessionView) => Promise.resolve(view)),
  );
  const controller = {
    snapshot: behavior.view ?? sessionView(),
    refresh: vi.fn().mockResolvedValue(behavior.view === undefined ? sessionView() : behavior.view),
    setContext,
  } as unknown as EditorSessionController;
  return { controller, setContext };
}

function renderHookWith(options: {
  controller?: EditorSessionController | null;
  confirmTransactionSwitch?: (view: SessionView) => boolean;
  onSessionPrompt?: (key: string, message?: string) => void;
}) {
  return renderHook(() =>
    useQueryContextPath({
      panelId: 'panel-1',
      dbSessionId: 'sess-1',
      isPathHierarchy: false,
      selectedDatabase: 'app',
      namespaceTree: [],
      pathAliases: {},
      databases: schemaMocks.databases,
      currentDatabase: schemaMocks.currentDatabase,
      sessionController: options.controller ?? null,
      confirmTransactionSwitch: options.confirmTransactionSwitch,
      onSessionPrompt: options.onSessionPrompt,
    }),
  );
}

describe('[tester] useQueryContextPath two-phase session switch', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    schemaMocks.switchDatabase.mockResolvedValue(undefined);
    schemaMocks.ensureNamespacePath.mockResolvedValue(undefined);
    schemaMocks.databases = ['app', 'other'];
    schemaMocks.currentDatabase = 'app';
  });

  afterEach(cleanup);

  it('moves the legacy pointer only after the session switch succeeded', async () => {
    const { controller, setContext } = controllerWith({});
    const onSessionPrompt = vi.fn();
    const { result } = renderHookWith({ controller, onSessionPrompt });

    await act(async () => {
      result.current.handleSelectContextLevel(0, 'other');
    });

    await waitFor(() => expect(setContext).toHaveBeenCalledTimes(1));
    expect(setContext.mock.calls[0][0]).toMatchObject({
      database: 'other',
      catalog: null,
      schema: null,
      path: [],
    });
    await waitFor(() => expect(schemaMocks.switchDatabase).toHaveBeenCalledWith('other', 'sess-1'));
    expect(panelMocks.updatePanel).toHaveBeenCalledWith('panel-1', { database: 'other' });
    // A clean session needs no transaction confirmation and no user prompt.
    expect(onSessionPrompt).not.toHaveBeenCalled();
  });

  it('keeps the previous database when the transaction switch is declined', async () => {
    const activeTx = sessionView({
      observedContext: {
        ...sessionView().observedContext,
        transactionState: 'active',
      },
    });
    const { controller, setContext } = controllerWith({ view: activeTx });
    const confirmTransactionSwitch = vi.fn((_session: SessionView) => false);
    const onSessionPrompt = vi.fn();
    const { result } = renderHookWith({ controller, confirmTransactionSwitch, onSessionPrompt });

    await act(async () => {
      result.current.handleSelectContextLevel(0, 'other');
    });

    await waitFor(() => expect(confirmTransactionSwitch).toHaveBeenCalledTimes(1));
    expect(confirmTransactionSwitch.mock.calls[0][0].state).toBe('ready');
    expect(setContext).not.toHaveBeenCalled();
    expect(schemaMocks.switchDatabase).not.toHaveBeenCalled();
    expect(onSessionPrompt).not.toHaveBeenCalled();
  });

  it('switches when the user confirms leaving an active transaction', async () => {
    const activeTx = sessionView({
      observedContext: {
        ...sessionView().observedContext,
        transactionState: 'active',
      },
    });
    const { controller, setContext } = controllerWith({ view: activeTx });
    const confirmTransactionSwitch = vi.fn((_session: SessionView) => true);
    const { result } = renderHookWith({ controller, confirmTransactionSwitch });

    await act(async () => {
      result.current.handleSelectContextLevel(0, 'other');
    });

    await waitFor(() => expect(setContext).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(schemaMocks.switchDatabase).toHaveBeenCalledWith('other', 'sess-1'));
  });

  it('prompts a revision conflict and keeps the previous database', async () => {
    const { controller, setContext } = controllerWith({
      setContext: () => Promise.reject(new ApiError('ContextConflict', 'stale revision')),
    });
    const onSessionPrompt = vi.fn();
    const { result } = renderHookWith({ controller, onSessionPrompt });

    await act(async () => {
      result.current.handleSelectContextLevel(0, 'other');
    });

    await waitFor(() =>
      expect(onSessionPrompt).toHaveBeenCalledWith(
        'query.session.revisionConflict',
        expect.anything(),
      ),
    );
    expect(setContext).toHaveBeenCalledTimes(1);
    expect(schemaMocks.switchDatabase).not.toHaveBeenCalled();
  });

  it('prompts a switch failure with the runtime message', async () => {
    const { controller } = controllerWith({
      setContext: () => Promise.reject(new ApiError('ResourceBusy', 'engine is busy')),
    });
    const onSessionPrompt = vi.fn();
    const { result } = renderHookWith({ controller, onSessionPrompt });

    await act(async () => {
      result.current.handleSelectContextLevel(0, 'other');
    });

    await waitFor(() =>
      expect(onSessionPrompt).toHaveBeenCalledWith('query.session.switchFailed', 'engine is busy'),
    );
    expect(schemaMocks.switchDatabase).not.toHaveBeenCalled();
  });

  it('prompts a lost session without a retryable error message', async () => {
    const { controller } = controllerWith({
      setContext: () => Promise.reject(new ApiError('SessionLost', 'connection closed')),
    });
    const onSessionPrompt = vi.fn();
    const { result } = renderHookWith({ controller, onSessionPrompt });

    await act(async () => {
      result.current.handleSelectContextLevel(0, 'other');
    });

    await waitFor(() => expect(onSessionPrompt).toHaveBeenCalledTimes(1));
    expect(onSessionPrompt.mock.calls[0][0]).toBe('query.session.disconnected');
    expect(schemaMocks.switchDatabase).not.toHaveBeenCalled();
  });

  it('falls back to the legacy pointer when the pane has no session controller', async () => {
    const onSessionPrompt = vi.fn();
    const { result } = renderHookWith({ controller: null, onSessionPrompt });

    await act(async () => {
      result.current.handleSelectContextLevel(0, 'other');
    });

    await waitFor(() => expect(schemaMocks.switchDatabase).toHaveBeenCalledWith('other', 'sess-1'));
    expect(onSessionPrompt).not.toHaveBeenCalled();
  });
});
