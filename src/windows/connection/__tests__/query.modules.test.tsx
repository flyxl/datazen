import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import {
  render,
  fireEvent,
  cleanup,
  screen,
  renderHook,
  act,
  waitFor,
} from '@testing-library/react';
import type { ComponentProps } from 'react';
import {
  buildQueryPanelDiagnosisContext,
  hasSuspiciousPostgresDoubleQuotedLiteral,
  readCurrentQueryPanelRetryValidationInput,
} from '../query/contracts';
import { createQueryDropHandler, detectDropContext } from '../query/queryDropHandler';
import { useQueryExecutionGate } from '../query/useQueryExecutionGate';
import {
  FavoriteNameDialog,
  QueryResultsPane,
  QueryTransactionModals,
  useQueryTransaction,
} from '../query/QueryTransactionModals';
import { QueryEditorSection } from '../query/QueryEditorSection';
import { QuerySidebarSection, useQueryContextPath } from '../query/QuerySidebarSection';
import { useQueryPanelWorkflows } from '../query/queryDropHandler';
import type { QueryHistoryEntry } from '../../../types';
import { usePanelStore } from '../../../stores/panelStore';
import { EMPTY_QUERY_EXEC } from '../../../stores/queryExecActions';
import { extensionRegistry, sqlEditorEnhancedEP } from '@datazen/extension-points';
import { toQueryExecutionViewModel } from '../../../lib/queryExecutionViewModel';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/usePlatform', () => ({
  usePlatform: () => 'macos',
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(true), null],
}));

const sessionTransactionStatus = vi.hoisted(() => vi.fn().mockResolvedValue(false));
const beginSessionTransaction = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
const commitSessionTransaction = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
const rollbackSessionTransaction = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    sessionTransactionStatus: (...args: unknown[]) => sessionTransactionStatus(...args),
    beginSessionTransaction: (...args: unknown[]) => beginSessionTransaction(...args),
    commitSessionTransaction: (...args: unknown[]) => commitSessionTransaction(...args),
    rollbackSessionTransaction: (...args: unknown[]) => rollbackSessionTransaction(...args),
    getExplain: (...args: unknown[]) => getExplain(...args),
    clearQueryHistory: vi.fn().mockResolvedValue(undefined),
  },
}));

const getTableSchema = vi.hoisted(() => vi.fn());
const getColumns = vi.hoisted(() => vi.fn());

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getTableSchema: (...args: unknown[]) => getTableSchema(...args),
    getColumns: (...args: unknown[]) => getColumns(...args),
  },
}));

vi.mock('../../../lib/nativeContextMenu', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../lib/nativeContextMenu')>();
  return {
    ...actual,
    showNativeContextMenu: vi.fn(),
  };
});

vi.mock('../../../components/SqlEditor', async () => {
  const { forwardRef, useImperativeHandle } = await import('react');
  return {
    SqlEditor: forwardRef(
      (
        props: {
          onContextMenu?: (e: MouseEvent, sql: string) => void;
          onExecute?: () => void;
        },
        ref,
      ) => {
        useImperativeHandle(ref, () => ({
          getSelection: () => 'SELECT 2',
          insertAt: vi.fn(),
          toggleLineComment: vi.fn(),
        }));
        return (
          <div
            data-testid="mock-sql-editor"
            onContextMenu={(e) => props.onContextMenu?.(e.nativeEvent, 'SELECT 1')}
          />
        );
      },
    ),
  };
});

vi.mock('../../../components/query/QueryContextSelectors', () => ({
  QueryContextSelectors: () => <div data-testid="context-selectors" />,
}));

vi.mock('../../../components/query/QueryExecutionStatus', () => ({
  QueryExecutionStatus: ({ onCancel }: { onCancel: () => void }) => (
    <button type="button" data-testid="cancel-running" onClick={onCancel}>
      cancel
    </button>
  ),
}));

extensionRegistry.register(sqlEditorEnhancedEP, {
  renderBindParamPanel: ({
    onChange,
  }: {
    params: Array<{ name: string }>;
    onChange: (name: string, value: string) => void;
  }) => (
    <button type="button" data-testid="bind-param-change" onClick={() => onChange('id', '42')}>
      bind
    </button>
  ),
});

vi.mock('../../../components/ai/Nl2SqlPanel', () => ({
  Nl2SqlPanel: () => <div data-testid="nl2sql-panel" />,
}));

vi.mock('../../../components/ai/DiagnosisPanel', () => ({
  DiagnosisPanel: ({ onClose }: { onClose: () => void }) => (
    <div data-testid="diagnosis-panel">
      <button type="button" onClick={onClose}>
        close-diagnosis
      </button>
    </div>
  ),
}));

vi.mock('../../../components/ai/ExplainPanel', () => ({
  ExplainPanel: () => <div data-testid="explain-panel" />,
}));

vi.mock('../../../components/query/QueryErrorPanel', () => ({
  QueryErrorPanel: ({
    onRetry,
    onFixSql,
    onExplain,
  }: {
    message: string;
    onRetry?: () => void;
    onFixSql?: () => void;
    onExplain?: () => void;
  }) => (
    <div data-testid="query-error-panel">
      <button type="button" data-testid="error-retry" onClick={onRetry}>
        retry
      </button>
      <button type="button" data-testid="error-fix" onClick={onFixSql}>
        fix
      </button>
      <button type="button" data-testid="error-explain" onClick={onExplain}>
        explain
      </button>
    </div>
  ),
}));

vi.mock('../../dashboard/AddToDashboardDialog', () => ({
  AddToDashboardDialog: ({
    open,
    onConfirm,
    onClose,
  }: {
    open: boolean;
    onConfirm: (id: string) => void;
    onClose: () => void;
  }) =>
    open ? (
      <div data-testid="add-dashboard-dialog">
        <button type="button" onClick={() => onConfirm('dash-1')}>
          confirm-dashboard
        </button>
        <button type="button" onClick={onClose}>
          close-dashboard
        </button>
      </div>
    ) : null,
}));

vi.mock('../result-workspace/ResultWorkspace', () => ({
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- mock; forward headerActions so the toolbar button is rendered
  ResultWorkspace: ({ headerActions }: any) => (
    <div data-testid="result-workspace">{headerActions}</div>
  ),
}));

const getExplain = vi.hoisted(() => vi.fn());
const createWidgetFromSql = vi.hoisted(() => vi.fn());
const saveDashboard = vi.hoisted(() => vi.fn());

vi.mock('../../../commands/dashboard', () => ({
  dashboardCommands: {
    createWidgetFromSql: (...args: unknown[]) => createWidgetFromSql(...args),
    saveDashboard: (...args: unknown[]) => saveDashboard(...args),
  },
}));

vi.mock('../../../lib/windowManager', () => ({
  openDashboardWindow: vi.fn(),
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  emitCrossWindow: vi.fn(),
}));

const activeConnectionStoreState = vi.hoisted(() => ({
  connections: {
    'cfg-1': {
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      status: 'connected' as const,
      serverInfo: { serverVersion: '16' },
      currentDatabase: 'app',
    },
  },
}));

const schemaStoreState = vi.hoisted(() => ({
  schemas: new Map(),
  tables: [{ name: 'users', tableType: 'table' as const }],
  views: [],
  columnMap: { users: ['id', 'name'] },
  currentDatabase: 'app',
  currentSchema: null as string | null,
  ensureNamespacePath: vi.fn(),
  switchDatabase: vi.fn(),
}));

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: Object.assign(
    (selector: (state: typeof activeConnectionStoreState) => unknown) =>
      selector(activeConnectionStoreState),
    { getState: () => activeConnectionStoreState },
  ),
}));

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: Object.assign(
    (sel: (s: typeof schemaStoreState) => unknown) => sel(schemaStoreState),
    { getState: () => schemaStoreState },
  ),
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (
    sel: (s: { settings: { safeMode: boolean; autoCommit: boolean } }) => unknown,
  ) => sel({ settings: { safeMode: false, autoCommit: true } }),
}));

afterEach(cleanup);

describe('[tester] query/contracts', () => {
  it('detects suspicious Postgres double-quoted literals', () => {
    expect(hasSuspiciousPostgresDoubleQuotedLiteral('WHERE name = "John"')).toBe(true);
    expect(hasSuspiciousPostgresDoubleQuotedLiteral("WHERE name = 'John'")).toBe(false);
  });

  it('builds diagnosis context from panel state', () => {
    const result = buildQueryPanelDiagnosisContext({
      execution: { sql: 'SELECT 1', error: 'syntax error at line 1' },
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      databaseType: 'postgresql',
      schemaState: {
        currentDatabase: 'app',
        currentSchema: null,
        tables: [{ name: 'users', tableType: 'table' }],
        views: [],
        columnMap: { users: ['id'] },
      },
    });
    expect(result.ok).toBe(true);
  });

  it('returns null retry validation when active connection mismatches panel session', () => {
    usePanelStore.setState({
      panels: [
        {
          id: 'p1',
          type: 'query',
          connectionId: 'cfg-1',
          dbSessionId: 'sess-other',
          title: 'Q',
        },
      ],
      queryExec: new Map([['p1', { ...EMPTY_QUERY_EXEC, sql: 'SELECT 1' }]]),
    } as Partial<ReturnType<typeof usePanelStore.getState>>);

    expect(readCurrentQueryPanelRetryValidationInput('p1', {})).toBeNull();
  });

  it('returns retry validation input when panel session matches active connection', () => {
    usePanelStore.setState({
      panels: [
        {
          id: 'p1',
          type: 'query',
          connectionId: 'cfg-1',
          dbSessionId: 'sess-1',
          databaseType: 'postgresql',
          title: 'Q',
        },
      ],
      queryExec: new Map([
        ['p1', { ...EMPTY_QUERY_EXEC, sql: 'SELECT 1', error: 'timeout expired' }],
      ]),
    } as Partial<ReturnType<typeof usePanelStore.getState>>);

    const input = readCurrentQueryPanelRetryValidationInput('p1', {});
    expect(input?.sql).toBe('SELECT 1');
    expect(input?.contextFingerprint).toBeTruthy();
  });
});

describe('[tester] query/queryDropHandler', () => {
  it('inserts generated SQL from table schema on drop', async () => {
    getTableSchema.mockResolvedValueOnce({
      tableName: 'users',
      columns: [{ name: 'id', dataType: 'int', nullable: false }],
      primaryKeys: ['id'],
      indexes: [],
      foreignKeys: [],
    });
    const insertAt = vi.fn();
    const editorRef = { current: { insertAt, getSelection: () => '', toggleLineComment: vi.fn() } };

    const handler = createQueryDropHandler({
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      databaseType: 'postgresql',
      editorRef,
    });

    await handler({ tables: [{ tableName: 'users', schema: 'public' }] }, 10);
    expect(getTableSchema).toHaveBeenCalledWith('sess-1', 'public.users', undefined);
    expect(insertAt).toHaveBeenCalled();
  });

  it('falls back to column list when schema fetch fails', async () => {
    getTableSchema.mockRejectedValueOnce(new Error('no schema'));
    getColumns.mockResolvedValueOnce(['id', 'name']);
    const insertAt = vi.fn();
    const editorRef = { current: { insertAt, getSelection: () => '', toggleLineComment: vi.fn() } };

    const handler = createQueryDropHandler({
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      databaseType: 'sqlite',
      editorRef,
    });

    await handler({ tables: [{ tableName: 'users' }] }, null);
    expect(insertAt).toHaveBeenCalled();
  });

  it('falls back to SELECT * when both schema and columns fail', async () => {
    getTableSchema.mockRejectedValueOnce(new Error('no schema'));
    getColumns.mockRejectedValueOnce(new Error('no columns'));
    const insertAt = vi.fn();
    const editorRef = { current: { insertAt, getSelection: () => '', toggleLineComment: vi.fn() } };

    const handler = createQueryDropHandler({
      connectionId: 'cfg-1',
      dbSessionId: '',
      databaseType: 'sqlite',
      editorRef,
    });

    await handler({ tables: [{ tableName: 'users', schema: 'main' }] }, 5);
    expect(insertAt).toHaveBeenCalledWith(expect.stringContaining('SELECT *'), 5);
  });

  it('smart insert: inserts table name after FROM when editor has content', async () => {
    const rawInsert = vi.fn();
    const insertAt = vi.fn();
    const editorRef = {
      current: {
        insertAt,
        rawInsert,
        getDocument: () => 'SELECT * FROM ',
        getSelection: () => '',
        toggleLineComment: vi.fn(),
      },
    };

    const handler = createQueryDropHandler({
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      databaseType: 'postgresql',
      editorRef,
    });

    await handler({ tables: [{ tableName: 'orders', schema: 'public' }] }, 14);
    // formatTableIdentifier quotes identifiers: "public"."orders"
    expect(rawInsert).toHaveBeenCalled();
    const insertedText = rawInsert.mock.calls[0][0];
    expect(insertedText).toContain('orders');
    expect(rawInsert.mock.calls[0][1]).toBe(14);
  });

  it('smart insert: inserts raw table name in arbitrary context without prepending FROM', async () => {
    const rawInsert = vi.fn();
    const insertAt = vi.fn();
    const editorRef = {
      current: {
        insertAt,
        rawInsert,
        getDocument: () => 'SELECT * FROM users WHERE ',
        getSelection: () => '',
        toggleLineComment: vi.fn(),
      },
    };

    const handler = createQueryDropHandler({
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      databaseType: 'postgresql',
      editorRef,
    });

    await handler({ tables: [{ tableName: 'orders', schema: 'public' }] }, 26);
    expect(rawInsert).toHaveBeenCalled();
    const insertedText = rawInsert.mock.calls[0][0];
    expect(insertedText).not.toMatch(/^FROM /);
    expect(insertedText).toContain('orders');
    expect(insertAt).not.toHaveBeenCalled();
  });

  it('smart insert: inserts SELECT * FROM for empty editor via getDocument', async () => {
    // Clear any leftover mock values from prior tests
    getTableSchema.mockReset();
    getColumns.mockReset();
    getTableSchema.mockRejectedValue(new Error('no schema'));
    getColumns.mockRejectedValue(new Error('no columns'));

    const rawInsert = vi.fn();
    const insertAt = vi.fn();
    const editorRef = {
      current: {
        insertAt,
        rawInsert,
        getDocument: () => '',
        getSelection: () => '',
        toggleLineComment: vi.fn(),
      },
    };

    const handler = createQueryDropHandler({
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      databaseType: 'postgresql',
      editorRef,
    });

    // Empty document → falls through to async schema-fetch path
    await handler({ tables: [{ tableName: 'users', schema: 'public' }] }, 0);
    expect(insertAt).toHaveBeenCalled();
    const insertedText = insertAt.mock.calls[0][0];
    expect(insertedText).toMatch(/SELECT (\*|"[^"]+")[\s\S]*FROM/);
    expect(insertedText).toContain('users');
  });

  it('smart insert: inserts table name after JOIN', async () => {
    const rawInsert = vi.fn();
    const insertAt = vi.fn();
    const editorRef = {
      current: {
        insertAt,
        rawInsert,
        getDocument: () => 'SELECT * FROM a JOIN ',
        getSelection: () => '',
        toggleLineComment: vi.fn(),
      },
    };

    const handler = createQueryDropHandler({
      connectionId: 'cfg-1',
      dbSessionId: 'sess-1',
      databaseType: 'sqlite',
      editorRef,
    });

    await handler({ tables: [{ tableName: 'b' }] }, 20);
    expect(rawInsert).toHaveBeenCalled();
    const insertedText = rawInsert.mock.calls[0][0];
    expect(insertedText).toContain('b');
    expect(rawInsert.mock.calls[0][1]).toBe(20);
  });
});

describe('[tester] query/detectDropContext', () => {
  it('generates SELECT * FROM for empty document', () => {
    const result = detectDropContext('', 0, 'public.users');
    expect(result.text).toBe('SELECT * FROM public.users ');
    expect(result.raw).toBe(true);
  });

  it('inserts table name after FROM keyword', () => {
    const doc = 'SELECT * FROM ';
    const result = detectDropContext(doc, doc.length, 'public.orders');
    expect(result.text).toBe(' public.orders ');
    expect(result.raw).toBe(true);
  });

  it('inserts table name after JOIN keyword', () => {
    const doc = 'SELECT * FROM users JOIN ';
    const result = detectDropContext(doc, doc.length, 'public.orders');
    expect(result.text).toBe(' public.orders ');
    expect(result.raw).toBe(true);
  });

  it('inserts table name after LEFT JOIN', () => {
    const doc = 'SELECT * FROM users LEFT JOIN ';
    const result = detectDropContext(doc, doc.length, 'public.orders');
    expect(result.text).toBe(' public.orders ');
    expect(result.raw).toBe(true);
  });

  it('inserts table name after comma in FROM clause', () => {
    const doc = 'SELECT * FROM users, ';
    const result = detectDropContext(doc, doc.length, 'public.orders');
    expect(result.text).toBe(' public.orders ');
    expect(result.raw).toBe(true);
  });

  it('inserts raw table name with spacing for arbitrary context without prepending FROM', () => {
    const doc = 'SELECT * FROM users WHERE ';
    const result = detectDropContext(doc, doc.length, 'public.orders');
    expect(result.text).toBe('public.orders');
    expect(result.raw).toBe(true);
  });

  it('handles case-insensitive FROM keyword', () => {
    const doc = 'select * from ';
    const result = detectDropContext(doc, doc.length, 'users');
    expect(result.text).toBe(' users ');
    expect(result.raw).toBe(true);
  });

  it('handles INNER JOIN', () => {
    const doc = 'SELECT * FROM a INNER JOIN ';
    const result = detectDropContext(doc, doc.length, 'b');
    expect(result.text).toBe(' b ');
    expect(result.raw).toBe(true);
  });
});

describe('[tester] query/useQueryExecutionGate', () => {
  const executeQuery = vi.fn().mockResolvedValue(undefined);
  const executeSelection = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    vi.clearAllMocks();
    executeQuery.mockResolvedValue(undefined);
    executeSelection.mockResolvedValue(undefined);
    usePanelStore.setState({
      executeQuery,
      executeSelection,
      queryExec: new Map([['p1', { ...EMPTY_QUERY_EXEC, sql: 'SELECT 1' }]]),
    } as Partial<ReturnType<typeof usePanelStore.getState>>);
  });

  function renderGate(overrides: Partial<Parameters<typeof useQueryExecutionGate>[0]> = {}) {
    const editorRef = {
      current: { getSelection: () => '', insertAt: vi.fn(), toggleLineComment: vi.fn() },
    };
    return renderHook(() =>
      useQueryExecutionGate({
        panelId: 'p1',
        dbSessionId: 'sess-1',
        databaseType: 'sqlite',
        connectionId: 'conn-test',
        editorRef,
        sql: 'SELECT 1',
        boundPayload: undefined,
        inTransaction: false,
        setInTransaction: vi.fn(),
        refreshTxStatus: vi.fn().mockResolvedValue(undefined),
        maybeOfferAbortedDialog: vi.fn().mockResolvedValue(undefined),
        syncContextFromSql: vi.fn().mockResolvedValue(undefined),
        showMessageDialog: vi.fn(),
        onExecutionComplete: vi.fn(),
        ...overrides,
      }),
    );
  }

  it('executes full query via handleExecute', async () => {
    const { result } = renderGate();
    await act(async () => {
      result.current.handleExecute();
    });
    await waitFor(() => expect(executeQuery).toHaveBeenCalledWith('p1', undefined));
  });

  it('opens unclosed transaction dialog for BEGIN without COMMIT', async () => {
    usePanelStore.setState({
      executeQuery,
      executeSelection,
      queryExec: new Map([['p1', { ...EMPTY_QUERY_EXEC, sql: 'BEGIN;' }]]),
    } as Partial<ReturnType<typeof usePanelStore.getState>>);

    const { result } = renderGate({ sql: 'BEGIN;' });
    await act(async () => {
      result.current.handleExecute();
    });
    expect(result.current.txUnclosedOpen).toBe(true);
    expect(executeQuery).not.toHaveBeenCalled();
  });

  it('cancels unclosed transaction dialog without executing', async () => {
    usePanelStore.setState({
      executeQuery,
      executeSelection,
      queryExec: new Map([['p1', { ...EMPTY_QUERY_EXEC, sql: 'BEGIN;' }]]),
    } as Partial<ReturnType<typeof usePanelStore.getState>>);

    const { result } = renderGate({ sql: 'BEGIN;' });
    await act(async () => {
      result.current.handleExecute();
    });
    act(() => {
      result.current.handleCancelUnclosedTx();
    });
    expect(result.current.txUnclosedOpen).toBe(false);
  });

  it('blocks postgres execution when double-quoted literals look suspicious', async () => {
    usePanelStore.setState({
      executeQuery,
      executeSelection,
      queryExec: new Map([
        ['p1', { ...EMPTY_QUERY_EXEC, sql: 'SELECT * FROM t WHERE name = "John"' }],
      ]),
    } as Partial<ReturnType<typeof usePanelStore.getState>>);

    const showMessageDialog = vi.fn();
    const { result } = renderGate({
      databaseType: 'postgresql',
      sql: 'SELECT * FROM t WHERE name = "John"',
      showMessageDialog,
    });
    await act(async () => {
      result.current.handleExecute();
    });
    expect(showMessageDialog).toHaveBeenCalledWith('query.postgresDoubleQuoteHint', 'error');
    expect(executeQuery).not.toHaveBeenCalled();
  });
});

describe('[tester] query/QueryTransactionModals', () => {
  it('renders unclosed and aborted transaction dialogs', () => {
    render(
      <QueryTransactionModals
        txUnclosedOpen
        txAbortedOpen
        txAbortedDetail="transaction aborted"
        txBusy={false}
        onConfirmUnclosedTx={vi.fn()}
        onCancelUnclosedTx={vi.fn()}
        onAbortedRollback={vi.fn()}
        onAbortedSkip={vi.fn()}
      />,
    );

    expect(screen.getByText('query.txUnclosedTitle')).toBeInTheDocument();
    expect(screen.getByText('query.txAbortedTitle')).toBeInTheDocument();
    expect(screen.getByText('transaction aborted')).toBeInTheDocument();
  });

  it('invokes transaction modal action handlers', () => {
    const onConfirmUnclosedTx = vi.fn();
    const onCancelUnclosedTx = vi.fn();
    const onAbortedRollback = vi.fn();
    const onAbortedSkip = vi.fn();

    render(
      <QueryTransactionModals
        txUnclosedOpen
        txAbortedOpen
        txAbortedDetail="aborted"
        txBusy={false}
        onConfirmUnclosedTx={onConfirmUnclosedTx}
        onCancelUnclosedTx={onCancelUnclosedTx}
        onAbortedRollback={onAbortedRollback}
        onAbortedSkip={onAbortedSkip}
      />,
    );

    fireEvent.click(screen.getByText('query.txUnclosedConfirm'));
    fireEvent.click(screen.getByText('query.txUnclosedCancel'));
    fireEvent.click(screen.getByText('query.txAbortedRollback'));
    fireEvent.click(screen.getByText('query.txAbortedSkip'));

    expect(onConfirmUnclosedTx).toHaveBeenCalled();
    expect(onCancelUnclosedTx).toHaveBeenCalled();
    expect(onAbortedRollback).toHaveBeenCalled();
    expect(onAbortedSkip).toHaveBeenCalled();
  });

  it('renders favorite name dialog and saves on Enter', () => {
    const onSave = vi.fn();
    render(
      <FavoriteNameDialog
        open
        favoriteName="My query"
        favoriteDialogSql="SELECT 1"
        onFavoriteNameChange={vi.fn()}
        onClose={vi.fn()}
        onSave={onSave}
      />,
    );

    fireEvent.keyDown(screen.getByDisplayValue('My query'), { key: 'Enter' });
    expect(onSave).toHaveBeenCalled();
  });
});

describe('[tester] query/useQueryTransaction', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    sessionTransactionStatus.mockResolvedValue(false);
  });

  it('refreshes transaction status on mount and handles begin/commit/rollback', async () => {
    const { result } = renderHook(() => useQueryTransaction({ dbSessionId: 'sess-1' }));

    await waitFor(() => expect(sessionTransactionStatus).toHaveBeenCalled());

    await act(async () => {
      await result.current.handleBeginTx();
    });
    expect(beginSessionTransaction).toHaveBeenCalledWith('sess-1');

    sessionTransactionStatus.mockResolvedValueOnce(true);
    await act(async () => {
      await result.current.handleCommitTx();
    });
    expect(commitSessionTransaction).toHaveBeenCalledWith('sess-1');

    await act(async () => {
      await result.current.handleRollbackTx();
    });
    expect(rollbackSessionTransaction).toHaveBeenCalledWith('sess-1');
  });

  it('opens aborted dialog when error indicates aborted transaction', async () => {
    sessionTransactionStatus.mockResolvedValue(false);
    const { result } = renderHook(() => useQueryTransaction({ dbSessionId: 'sess-1' }));

    await act(async () => {
      await result.current.maybeOfferAbortedDialog('current transaction is aborted');
    });

    expect(result.current.txAbortedOpen).toBe(true);
    expect(result.current.txAbortedDetail).toContain('aborted');
  });
});

describe('[tester] query/QueryEditorSection', () => {
  function openMoreMenu() {
    fireEvent.click(screen.getByTestId('query-toolbar-more-menu-trigger'));
  }

  function renderSection(overrides: Partial<ComponentProps<typeof QueryEditorSection>> = {}) {
    const editorRef = {
      current: { getSelection: () => 'SELECT 2', insertAt: vi.fn(), toggleLineComment: vi.fn() },
    };
    const execVm = toQueryExecutionViewModel(
      { ...EMPTY_QUERY_EXEC, sql: 'SELECT 1', running: false },
      { supportsCancelQuery: true, supportsQueryExecutionCancel: true },
    );
    const defaults: ComponentProps<typeof QueryEditorSection> = {
      dbSessionId: 'sess-1',
      databaseType: 'postgresql',
      editorRef,
      toolbarRef: { current: null },
      compactToolbar: false,
      sql: 'SELECT 1',
      running: false,
      executionTimeMs: 42,
      executionViewModel: execVm,
      sqlParams: [{ name: 'id', syntax: 'colon' as const, ordinal: 1 }],
      paramValues: {},
      onParamChange: vi.fn(),
      editorHeight: 200,
      editorResizeRef: { current: null },
      editorSchema: [],
      editorDefaultSchema: undefined,
      editorDefaultTable: undefined,
      namespaceLoading: false,
      supportsExplain: true,
      safeMode: true,
      inTransaction: false,
      txBusy: false,
      isMultiDb: true,
      isPathHierarchy: false,
      hasContextSelectors: true,
      databases: ['app'],
      selectedDatabase: 'app',
      selectedSchema: null,
      namespaceTree: [],
      pathAliases: {},
      contextPath: ['app'],
      nl2sqlVisible: true,
      onToggleNl2sql: vi.fn(),
      historyVisible: false,
      favoritesVisible: false,
      onToggleHistory: vi.fn(),
      onToggleFavorites: vi.fn(),
      onUpdateSql: vi.fn(),
      onExecute: vi.fn(),
      onExecuteSelection: vi.fn(),
      onCancel: vi.fn(),
      onFormat: vi.fn(),
      onExplain: vi.fn(),
      onBeginTx: vi.fn(),
      onCommitTx: vi.fn(),
      onRollbackTx: vi.fn(),
      onApplyAiSql: vi.fn(),
      onOpenAddFavoriteDialog: vi.fn(),
      onQualifiedPath: vi.fn(),
      onSelectContextLevel: vi.fn(),
      onDropTable: vi.fn(),
    };

    return render(<QueryEditorSection {...defaults} {...overrides} />);
  }

  it('wires toolbar actions and editor context menu', () => {
    const onExecute = vi.fn();
    const onExplain = vi.fn();
    const onFormat = vi.fn();
    const onBeginTx = vi.fn();
    const onCommitTx = vi.fn();
    const onRollbackTx = vi.fn();
    const onToggleNl2sql = vi.fn();
    const onToggleHistory = vi.fn();
    const onToggleFavorites = vi.fn();
    const onParamChange = vi.fn();

    renderSection({
      onExecute,
      onExplain,
      onFormat,
      onBeginTx,
      onCommitTx,
      onRollbackTx,
      onToggleNl2sql,
      onToggleHistory,
      onToggleFavorites,
      onParamChange,
      inTransaction: true,
    });

    fireEvent.click(screen.getByRole('button', { name: 'query.execute' }));
    openMoreMenu();
    fireEvent.click(screen.getByTestId('more-menu-explain'));
    openMoreMenu();
    fireEvent.click(screen.getByTestId('more-menu-format'));
    fireEvent.click(screen.getByRole('button', { name: 'query.commitTx' }));
    fireEvent.click(screen.getByRole('button', { name: 'query.rollbackTx' }));
    fireEvent.click(screen.getByRole('button', { name: 'query.history' }));
    fireEvent.click(screen.getByRole('button', { name: 'query.favorites' }));
    fireEvent.click(screen.getByRole('button', { name: 'nl2sql.title' }));
    fireEvent.click(screen.getByTestId('bind-param-change'));
    fireEvent.contextMenu(screen.getByTestId('mock-sql-editor'));

    expect(onExecute).toHaveBeenCalled();
    expect(onExplain).toHaveBeenCalled();
    expect(onFormat).toHaveBeenCalled();
    expect(onCommitTx).toHaveBeenCalled();
    expect(onRollbackTx).toHaveBeenCalled();
    expect(onToggleHistory).toHaveBeenCalled();
    expect(onToggleFavorites).toHaveBeenCalled();
    expect(onToggleNl2sql).toHaveBeenCalled();
    expect(onParamChange).toHaveBeenCalledWith('id', '42');
    expect(screen.getByTestId('nl2sql-panel')).toBeInTheDocument();
    expect(screen.getByTestId('context-selectors')).toBeInTheDocument();
    expect(screen.getByText('settings.safeMode')).toBeInTheDocument();
    expect(screen.getByText('TX')).toBeInTheDocument();
  });

  it('shows in-transaction badge and begin transaction when idle', () => {
    const onBeginTx = vi.fn();
    renderSection({ inTransaction: false, onBeginTx });
    openMoreMenu();
    fireEvent.click(screen.getByTestId('more-menu-begin-tx'));
    expect(onBeginTx).toHaveBeenCalled();
    expect(screen.queryByText('TX')).toBeNull();
  });

  it('shows cancel control while running', () => {
    const onCancel = vi.fn();
    const execVm = toQueryExecutionViewModel(
      { ...EMPTY_QUERY_EXEC, sql: 'SELECT 1', running: true },
      { supportsCancelQuery: true, supportsQueryExecutionCancel: true },
    );
    renderSection({ running: true, executionViewModel: execVm, onCancel });
    fireEvent.click(screen.getByTestId('cancel-running'));
    expect(onCancel).toHaveBeenCalled();
  });
});

describe('[tester] query/QuerySidebarSection favorites', () => {
  beforeEach(() => {
    usePanelStore.setState({
      queryFavorites: [{ id: 'f1', title: 'Daily', sql: 'SELECT 1', connectionId: 'cfg-1' }],
      queryHistory: [],
      favoritesVisible: true,
      historyVisible: false,
      updateSql: vi.fn(),
      deleteFavorite: vi.fn().mockResolvedValue(undefined),
      loadHistory: vi.fn(),
    } as Partial<ReturnType<typeof usePanelStore.getState>>);
  });

  it('renders favorites and opens a new tab with SQL on click', () => {
    const updateSql = vi.fn();
    const addPanel = vi.fn();
    usePanelStore.setState({
      updateSql,
      addPanel,
    } as Partial<ReturnType<typeof usePanelStore.getState>>);

    render(
      <QuerySidebarSection
        panelId="p1"
        connectionId="cfg-1"
        selectedDatabase="app"
        favoritesVisible
        historyVisible={false}
      />,
    );

    expect(screen.getByText('Daily')).toBeInTheDocument();
    fireEvent.click(screen.getByText('Daily'));
    expect(addPanel).toHaveBeenCalledWith(
      expect.objectContaining({ title: 'Daily', type: 'query' }),
      true,
    );
    expect(updateSql).toHaveBeenCalledWith(expect.stringMatching(/^panel-qry-/), 'SELECT 1');
  });

  it('deletes favorite via trash button', () => {
    const deleteFavorite = vi.fn().mockResolvedValue(undefined);
    usePanelStore.setState({ deleteFavorite } as Partial<
      ReturnType<typeof usePanelStore.getState>
    >);

    render(
      <QuerySidebarSection
        panelId="p1"
        connectionId="cfg-1"
        selectedDatabase="app"
        favoritesVisible
        historyVisible={false}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '' }));
    expect(deleteFavorite).toHaveBeenCalledWith('f1');
  });
});

function historyEntry(
  overrides: Partial<QueryHistoryEntry> &
    Pick<QueryHistoryEntry, 'id' | 'sql' | 'database' | 'executedAt'>,
): QueryHistoryEntry {
  return {
    connectionId: 'cfg-1',
    executionTimeMs: 12,
    success: true,
    ...overrides,
  };
}

describe('[tester] query/QuerySidebarSection history', () => {
  const HISTORY: QueryHistoryEntry[] = [
    historyEntry({
      id: 'h1',
      database: 'app',
      sql: 'SELECT * FROM users',
      executedAt: '2024-01-02T00:00:00Z',
    }),
    historyEntry({
      id: 'h2',
      database: 'analytics',
      sql: 'SELECT * FROM events',
      executedAt: '2024-01-01T00:00:00Z',
      success: false,
      rowsAffected: 3,
    }),
  ];

  beforeEach(() => {
    usePanelStore.setState({
      queryHistory: HISTORY,
      queryFavorites: [],
      historyVisible: true,
      favoritesVisible: false,
      updateSql: vi.fn(),
      loadHistory: vi.fn(),
    } as Partial<ReturnType<typeof usePanelStore.getState>>);
  });

  it('renders history entries and applies SQL on click', () => {
    const updateSql = vi.fn();
    usePanelStore.setState({ updateSql } as Partial<ReturnType<typeof usePanelStore.getState>>);

    render(
      <QuerySidebarSection
        panelId="p1"
        connectionId="cfg-1"
        selectedDatabase="app"
        favoritesVisible={false}
        historyVisible
      />,
    );

    expect(screen.getByText('SELECT * FROM users')).toBeInTheDocument();
    fireEvent.click(screen.getByText('SELECT * FROM users'));
    expect(updateSql).toHaveBeenCalledWith('p1', 'SELECT * FROM users');
  });

  it('filters history by search and toggles scope', () => {
    render(
      <QuerySidebarSection
        panelId="p1"
        connectionId="cfg-1"
        selectedDatabase="app"
        favoritesVisible={false}
        historyVisible
      />,
    );

    fireEvent.click(screen.getByTestId('history-scope-all'));
    expect(screen.getByTestId('history-scope-all')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText('SELECT * FROM events')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('query.searchHistory'), {
      target: { value: 'users' },
    });
    expect(screen.getByText('SELECT * FROM users')).toBeInTheDocument();
    expect(screen.queryByText('SELECT * FROM events')).toBeNull();
  });

  it('shows empty history message when no entries', () => {
    usePanelStore.setState({ queryHistory: [] } as Partial<
      ReturnType<typeof usePanelStore.getState>
    >);

    render(
      <QuerySidebarSection
        panelId="p1"
        connectionId="cfg-1"
        selectedDatabase="app"
        favoritesVisible={false}
        historyVisible
      />,
    );

    expect(screen.getByText('query.noHistory')).toBeInTheDocument();
  });

  it('clears history via header trash button', async () => {
    const loadHistory = vi.fn().mockResolvedValue(undefined);
    usePanelStore.setState({ loadHistory } as Partial<ReturnType<typeof usePanelStore.getState>>);
    const { queryCommands } = await import('../../../commands/query');

    render(
      <QuerySidebarSection
        panelId="p1"
        connectionId="cfg-1"
        selectedDatabase="app"
        favoritesVisible={false}
        historyVisible
      />,
    );

    fireEvent.click(screen.getByLabelText('query.clearHistory'));
    await waitFor(() => expect(queryCommands.clearQueryHistory).toHaveBeenCalled());
    expect(loadHistory).toHaveBeenCalledWith('cfg-1');
  });
});

describe('[tester] query/QueryResultsPane', () => {
  const baseProps: ComponentProps<typeof QueryResultsPane> = {
    dbSessionId: 'sess-1',
    sql: 'SELECT 1',
    running: false,
    error: null,
    results: [],
    activeResultIdx: 0,
    activeResult: undefined,
    resultViewMode: 'table',
    chartConfig: undefined,
    resultDetailRowIndex: null,
    queryResultExportCapability: 'loaded_only',
    selectedDatabase: 'app',
    showExplain: false,
    explainLoading: false,
    explainError: null,
    explainResult: null,
    diagnosisVisible: false,
    diagnosisContext: { ok: false, error: { code: 'empty', message: 'empty' } },
    retryActionEnabled: false,
    addToDashboardOpen: false,
    onApplyAiSql: vi.fn(),
    onApplyFixSql: vi.fn(),
    onRetry: vi.fn(),
    onSetActiveResult: vi.fn(),
    onSetResultViewMode: vi.fn(),
    onChartConfigChange: vi.fn(),
    onRowDetail: vi.fn(),
    onShowExplain: vi.fn(),
    onDiagnosisVisible: vi.fn(),
    onAddToDashboardOpen: vi.fn(),
    onAddToDashboardConfirm: vi.fn(),
  };

  it('shows shortcut hint when idle with no results', () => {
    render(<QueryResultsPane {...baseProps} />);
    expect(screen.getByText('query.shortcutHint')).toBeInTheDocument();
  });

  it('renders error panel with retry and diagnosis actions', () => {
    const onRetry = vi.fn();
    const onDiagnosisVisible = vi.fn();
    const onExplainError = vi.fn();

    render(
      <QueryResultsPane
        {...baseProps}
        error="syntax error"
        retryActionEnabled
        diagnosisContext={{
          ok: true,
          context: {
            sql: 'SELECT bad',
            error: 'syntax error',
            contextFingerprint: 'fp-1',
            connectionContext: {},
            schemaContext: { tables: [], views: [], columns: {} },
          },
        }}
        onRetry={onRetry}
        onDiagnosisVisible={onDiagnosisVisible}
        onExplainError={onExplainError}
        diagnosisVisible
      />,
    );

    fireEvent.click(screen.getByTestId('error-retry'));
    fireEvent.click(screen.getByTestId('error-fix'));
    fireEvent.click(screen.getByTestId('error-explain'));
    expect(onRetry).toHaveBeenCalled();
    expect(onDiagnosisVisible).toHaveBeenCalledWith(true);
    expect(onExplainError).toHaveBeenCalled();
    expect(screen.getByTestId('diagnosis-panel')).toBeInTheDocument();
  });

  it('renders multi-result tabs and result workspace', () => {
    const onSetActiveResult = vi.fn();
    const onAddToDashboardOpen = vi.fn();
    const results = [
      { rows: [{ id: 1 }], columns: ['id'], executionTimeMs: 5 },
      { rows: [{ id: 2 }], columns: ['id'], executionTimeMs: 8 },
    ];

    render(
      <QueryResultsPane
        {...baseProps}
        results={results}
        activeResult={results[0]}
        onSetActiveResult={onSetActiveResult}
        onAddToDashboardOpen={onAddToDashboardOpen}
      />,
    );

    fireEvent.click(screen.getByText(/query\.result 2/));
    expect(onSetActiveResult).toHaveBeenCalledWith(1);
    fireEvent.click(screen.getByTestId('query-add-to-dashboard'));
    expect(onAddToDashboardOpen).toHaveBeenCalledWith(true);
    expect(screen.getByTestId('result-workspace')).toBeInTheDocument();
  });

  it('shows explain panel when showExplain is true', () => {
    render(
      <QueryResultsPane
        {...baseProps}
        showExplain
        explainResult={{ plan: 'Seq Scan', raw: 'plan text' }}
      />,
    );
    expect(screen.getByTestId('explain-panel')).toBeInTheDocument();
  });
});

describe('[tester] query/useQueryPanelWorkflows', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getExplain.mockResolvedValue({ plan: 'Index Scan', raw: 'plan' });
    createWidgetFromSql.mockResolvedValue({
      dashboard: { id: 'dash-1', name: 'Board' },
    });
    usePanelStore.setState({
      panels: [{ id: 'p1', type: 'query', title: 'My Query', connectionId: 'cfg-1' }],
    } as Partial<ReturnType<typeof usePanelStore.getState>>);
  });

  function renderWorkflows(overrides: Partial<Parameters<typeof useQueryPanelWorkflows>[0]> = {}) {
    return renderHook(() =>
      useQueryPanelWorkflows({
        panelId: 'p1',
        connectionId: 'cfg-1',
        dbSessionId: 'sess-1',
        databaseType: 'postgresql',
        sql: 'SELECT 1',
        error: 'timeout',
        chartConfig: undefined,
        resultViewMode: 'table',
        activeResultRowsLength: 2,
        boundPayload: {},
        paramValuesRef: { current: {} },
        schemaState: {
          currentDatabase: 'app',
          currentSchema: null,
          tables: [],
          views: [],
          columnMap: {},
        },
        selectedDatabase: 'app',
        runExecute: vi.fn().mockResolvedValue(undefined),
        confirmRetry: vi.fn().mockResolvedValue(true),
        updateSql: vi.fn(),
        showMessageDialog: vi.fn(),
        t: (key) => key,
        ...overrides,
      }),
    );
  }

  it('loads explain result via handleExplain', async () => {
    const { result } = renderWorkflows();
    await act(async () => {
      await result.current.handleExplain();
    });
    expect(getExplain).toHaveBeenCalledWith('sess-1', 'SELECT 1', 'app');
    expect(result.current.showExplain).toBe(true);
    expect(result.current.explainResult).toEqual({ plan: 'Index Scan', raw: 'plan' });
  });

  it('opens favorite dialog for non-empty SQL', () => {
    const { result } = renderWorkflows();
    act(() => {
      result.current.openAddFavoriteDialog('SELECT 2');
    });
    expect(result.current.showFavoriteDialog).toBe(true);
    expect(result.current.favoriteDialogSql).toBe('SELECT 2');
  });

  it('creates dashboard widget on confirm', async () => {
    const { result } = renderWorkflows();
    act(() => {
      result.current.setAddToDashboardOpen(true);
    });
    await act(async () => {
      result.current.handleAddToDashboardConfirm('dash-1');
    });
    await waitFor(() =>
      expect(createWidgetFromSql).toHaveBeenCalledWith(
        expect.objectContaining({ dashboardId: 'dash-1', sql: 'SELECT 1' }),
      ),
    );
  });
});

describe('[tester] query/useQueryContextPath', () => {
  it('syncs context path from SQL references', async () => {
    const updatePanel = vi.fn();
    const switchDatabase = vi.fn().mockResolvedValue(undefined);
    const ensureNamespacePath = vi.fn().mockResolvedValue(undefined);

    usePanelStore.setState({ updatePanel } as Partial<ReturnType<typeof usePanelStore.getState>>);
    schemaStoreState.switchDatabase = switchDatabase;
    schemaStoreState.ensureNamespacePath = ensureNamespacePath;
    schemaStoreState.databases = ['app', 'other'];
    schemaStoreState.currentDatabase = 'app';

    const { result } = renderHook(() =>
      useQueryContextPath({
        panelId: 'p1',
        dbSessionId: 'sess-1',
        isPathHierarchy: false,
        selectedDatabase: 'app',
        namespaceTree: [],
        pathAliases: {},
        databases: ['app', 'other'],
        currentDatabase: 'app',
      }),
    );

    await act(async () => {
      await result.current.syncContextFromSql('SELECT * FROM other.users');
    });
    expect(switchDatabase).toHaveBeenCalled();
  });
});
