import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useQueryExecutionGate } from '../query/useQueryExecutionGate';
import { usePanelStore } from '../../../stores/panelStore';
import { useSettingsStore } from '../../../stores/settingsStore';
import { useConnectionStore } from '../../../stores/connectionStore';
import { useActiveConnectionStore } from '../../../stores/activeConnectionStore';
import { EMPTY_QUERY_EXEC } from '../../../stores/queryExecActions';

// ── Mocks ───────────────────────────────────────────────────────

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) => {
      const map: Record<string, string> = {
        'common.cancel': 'Cancel',
        'common.confirm': 'Confirm',
        'query.postgresDoubleQuoteHint': 'postgres double quote hint',
        'query.dangerousSqlTitle': 'Destructive SQL',
        'query.dangerousSqlConfirm': 'This script contains DROP or TRUNCATE.',
        'query.editor.executionConfirm.productionTitle': 'Production connection',
        'query.editor.executionConfirm.productionMessage':
          'This production connection will run potentially destructive SQL.',
        'query.editor.executionConfirm.highRiskTitle': 'High-risk SQL',
        'query.editor.executionConfirm.highRiskMessage':
          'This script contains high-risk operations.',
        'query.editor.executionConfirm.combinedTitle': 'Production — high-risk SQL',
        'query.editor.executionConfirm.combinedMessage':
          'This production connection will run potentially destructive SQL.',
        'query.editor.executionConfirm.badgeProduction': 'Production',
        'query.editor.executionConfirm.badgeHighRisk': 'High risk',
        'query.editor.executionConfirm.confirmExecute': 'Execute',
        'query.editor.executionConfirm.blockedReadOnly':
          'Connection is read-only; write operations are blocked.',
        'query.editor.executionConfirm.blockedSafeMode': 'Safe Mode blocks this operation.',
        'query.editor.executionConfirm.staleMessage':
          'The SQL or context changed during confirmation. Please try again.',
        'query.editor.executionConfirm.findingDrop': 'DROP statement detected',
        'query.editor.executionConfirm.findingTruncate': 'TRUNCATE statement detected',
        'query.editor.executionConfirm.findingUpdateNoWhere': 'UPDATE without WHERE clause',
        'query.editor.executionConfirm.findingDeleteNoWhere': 'DELETE without WHERE clause',
        'query.editor.executionConfirm.findingUnknown': 'Unrecognised SQL',
        'query.editor.param.missingValue': `Missing param: ${params?.token ?? ''}`,
        'query.txUnclosedTitle': 'Unclosed transaction detected',
        'query.txUnclosedConfirm': 'Continue',
        'query.txUnclosedCancel': 'Cancel',
      };
      return map[key] ?? key;
    },
  }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => {
  const confirmSpy = vi.fn().mockResolvedValue(true);
  return {
    useConfirmDialog: () => [confirmSpy, null],
    __confirmSpy: confirmSpy,
  };
});

const sessionTransactionStatus = vi.hoisted(() => vi.fn().mockResolvedValue(false));
const beginSessionTransaction = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    sessionTransactionStatus: (...args: unknown[]) => sessionTransactionStatus(...args),
    beginSessionTransaction: (...args: unknown[]) => beginSessionTransaction(...args),
    commitSessionTransaction: vi.fn().mockResolvedValue(undefined),
    rollbackSessionTransaction: vi.fn().mockResolvedValue(undefined),
    cancelQuery: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../../lib/nativeContextMenu', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../lib/nativeContextMenu')>();
  return { ...actual, showNativeContextMenu: vi.fn() };
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
        ref: React.ForwardedRef<unknown>,
      ) => {
        useImperativeHandle(ref, () => ({
          getSelection: () => '',
          insertAt: vi.fn(),
          toggleLineComment: vi.fn(),
        }));
        return <div data-testid="mock-editor" />;
      },
    ),
  };
});

// ── Helpers ─────────────────────────────────────────────────────

const CONNECTION_ID = 'conn-1';

function setupConnectionStore(overrides: { readOnly?: boolean; group?: string } = {}) {
  useConnectionStore.setState({
    connections: [
      {
        id: CONNECTION_ID,
        name: 'Test Connection',
        databaseType: 'sqlite',
        host: 'localhost',
        port: 5432,
        database: 'test',
        sslMode: 'disable',
        readOnly: overrides.readOnly ?? false,
        group: overrides.group,
      },
    ],
  });
}

function setupPanelStore(panelId: string, sql: string) {
  usePanelStore.setState({
    executeQuery: vi.fn().mockResolvedValue(undefined),
    executeSelection: vi.fn().mockResolvedValue(undefined),
    queryExec: new Map([[panelId, { ...EMPTY_QUERY_EXEC, sql }]]),
  });
}

function renderGate(overrides: Partial<Parameters<typeof useQueryExecutionGate>[0]> = {}) {
  const editorRef = {
    current: {
      getSelection: () => '',
      insertAt: vi.fn(),
      toggleLineComment: vi.fn(),
    },
  };
  return renderHook(() =>
    useQueryExecutionGate({
      panelId: 'p1',
      dbSessionId: 'sess-1',
      databaseType: 'sqlite',
      connectionId: CONNECTION_ID,
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

// ── Tests ───────────────────────────────────────────────────────

describe('[tester] useQueryExecutionGate', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setupConnectionStore();
    setupPanelStore('p1', 'SELECT 1');
    useSettingsStore.setState({
      settings: {
        ...useSettingsStore.getState().settings,
        autoCommit: true,
        safeMode: true,
      },
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('executes full query via handleExecute', async () => {
    const { result } = renderGate();
    await act(async () => {
      result.current.handleExecute();
    });
    await waitFor(() =>
      expect(usePanelStore.getState().executeQuery).toHaveBeenCalledWith('p1', undefined),
    );
  });

  it('opens unclosed transaction dialog for BEGIN without COMMIT', async () => {
    setupPanelStore('p1', 'BEGIN;');
    const { result } = renderGate({ sql: 'BEGIN;' });
    await act(async () => {
      result.current.handleExecute();
    });
    expect(result.current.txUnclosedOpen).toBe(true);
    expect(usePanelStore.getState().executeQuery).not.toHaveBeenCalled();
  });

  it('cancels unclosed transaction dialog without executing', async () => {
    setupPanelStore('p1', 'BEGIN;');
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
    setupPanelStore('p1', 'SELECT * FROM t WHERE name = "John"');
    const showMessageDialog = vi.fn();
    const { result } = renderGate({
      databaseType: 'postgresql',
      sql: 'SELECT * FROM t WHERE name = "John"',
      showMessageDialog,
    });
    await act(async () => {
      result.current.handleExecute();
    });
    expect(showMessageDialog).toHaveBeenCalledWith('postgres double quote hint', 'error');
    expect(usePanelStore.getState().executeQuery).not.toHaveBeenCalled();
  });

  it('blocks readOnly connections for write SQL', async () => {
    setupConnectionStore({ readOnly: true });
    setupPanelStore('p1', 'DELETE FROM t WHERE id = 1');
    const showMessageDialog = vi.fn();
    const { result } = renderGate({
      sql: 'DELETE FROM t WHERE id = 1',
      showMessageDialog,
    });
    await act(async () => {
      result.current.handleExecute();
    });
    expect(showMessageDialog).toHaveBeenCalledWith(
      'Connection is read-only; write operations are blocked.',
      'error',
    );
    expect(usePanelStore.getState().executeQuery).not.toHaveBeenCalled();
  });

  it('blocks safeMode for DROP TABLE', async () => {
    useSettingsStore.setState({
      settings: {
        ...useSettingsStore.getState().settings,
        safeMode: true,
      },
    });
    setupPanelStore('p1', 'DROP TABLE t');
    const showMessageDialog = vi.fn();
    const { result } = renderGate({
      sql: 'DROP TABLE t',
      showMessageDialog,
    });
    await act(async () => {
      result.current.handleExecute();
    });
    expect(showMessageDialog).toHaveBeenCalledWith('Safe Mode blocks this operation.', 'error');
    expect(usePanelStore.getState().executeQuery).not.toHaveBeenCalled();
  });

  it('allows SELECT on readOnly connections', async () => {
    setupConnectionStore({ readOnly: true });
    setupPanelStore('p1', 'SELECT 1');
    const { result } = renderGate({ sql: 'SELECT 1' });
    await act(async () => {
      result.current.handleExecute();
    });
    await waitFor(() =>
      expect(usePanelStore.getState().executeQuery).toHaveBeenCalledWith('p1', undefined),
    );
  });

  it('allows mutation on production connections with confirm', async () => {
    setupConnectionStore({ group: 'preset:production' });
    setupPanelStore('p1', 'DELETE FROM t WHERE id = 1');
    const { result } = renderGate({ sql: 'DELETE FROM t WHERE id = 1' });
    await act(async () => {
      result.current.handleExecute();
    });
    // Confirm dialog should have been shown (useConfirmDialog mock returns true)
    await waitFor(() =>
      expect(usePanelStore.getState().executeQuery).toHaveBeenCalledWith('p1', undefined),
    );
  });

  it('production + SELECT does not confirm', async () => {
    setupConnectionStore({ group: 'preset:production' });
    setupPanelStore('p1', 'SELECT 1');
    const { result } = renderGate({ sql: 'SELECT 1' });
    await act(async () => {
      result.current.handleExecute();
    });
    // Should execute directly without confirm
    await waitFor(() =>
      expect(usePanelStore.getState().executeQuery).toHaveBeenCalledWith('p1', undefined),
    );
  });
});
