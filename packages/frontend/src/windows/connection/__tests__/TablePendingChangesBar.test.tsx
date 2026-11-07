/**
 * Behaviour pins for the components extracted out of `TableView` in DB-01 Step 10.
 *
 * `TableView.test.tsx` only exercises `table-tx-begin`; the rest of the extracted lifecycle
 * (commit/rollback, terminal execution failure, session loss, open-new-session recovery) and
 * the pending-changes bar are covered here. Every `data-testid` consumed by
 * `e2e/specs/table-edit.ts`, `detail-panel.ts` and `table-session-loss-recovery.ts` is
 * asserted verbatim, so a rename during a later refactor fails here instead of silently
 * breaking those specs.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import {
  TablePendingChangesBar,
  TableTransactionControls,
} from '../TablePendingChangesBar';
import type { TableChangeContext } from '../../../lib/tableChanges';

const i18n = vi.hoisted(() => ({
  t: (key: string, params?: Record<string, unknown>) =>
    params && params.count !== undefined ? `${key}#${String(params.count)}` : key,
}));

vi.mock('../../../hooks/useI18n', () => ({ useI18n: () => i18n }));

const controller = vi.hoisted(() => ({
  snapshot: { state: 'ready' as string },
  refresh: vi.fn(),
  execute: vi.fn(),
  getExecution: vi.fn(),
  openNewSession: vi.fn(),
  acceptSessionError: vi.fn((_error: unknown) => false),
}));

vi.mock('../../../lib/session/tableSession', () => ({
  tableSessionController: vi.fn(async () => controller),
}));

const CONTEXT: TableChangeContext = {
  connectionId: 'conn-1',
  dbSessionId: 'session-1',
  driverType: 'postgres',
  database: 'app',
  schema: null,
  table: 'users',
};

function renderPending(
  overrides: Partial<Parameters<typeof TablePendingChangesBar>[0]> = {},
) {
  const props = {
    pendingCount: 2,
    updateCount: 1,
    deleteCount: 1,
    busy: false,
    onPreview: vi.fn(),
    onCommit: vi.fn(),
    onRollback: vi.fn(),
    ...overrides,
  };
  render(<TablePendingChangesBar {...props} />);
  return props;
}

function renderTx(overrides: Partial<Parameters<typeof TableTransactionControls>[0]> = {}) {
  const props = {
    panelId: 'panel-1',
    context: CONTEXT,
    isEditable: true,
    loading: false,
    onReload: vi.fn(),
    ...overrides,
  };
  render(<TableTransactionControls {...props} />);
  return props;
}

beforeEach(() => {
  vi.clearAllMocks();
  controller.snapshot = { state: 'ready' };
  controller.refresh.mockResolvedValue({
    state: 'ready',
    observedContext: { transactionState: 'none' },
  });
  controller.execute.mockResolvedValue({ executionId: 'exec-1' });
  controller.getExecution.mockResolvedValue({ state: 'succeeded' });
  controller.openNewSession.mockResolvedValue(undefined);
  controller.acceptSessionError.mockReturnValue(false);
});

afterEach(cleanup);

describe('TablePendingChangesBar', () => {
  it('renders nothing while no changes are staged', () => {
    renderPending({ pendingCount: 0 });
    expect(screen.queryByTestId('pending-changes-bar')).toBeNull();
  });

  it('keeps the four e2e testids and shows the staged counts', () => {
    renderPending();

    expect(screen.getByTestId('pending-changes-bar')).toHaveAttribute('aria-busy', 'false');
    expect(screen.getByTestId('pending-changes-bar')).toHaveTextContent('tableData.pendingChanges#2');
    expect(screen.getByTestId('pending-changes-bar')).toHaveTextContent('tableData.pendingUpdates#1');
    expect(screen.getByTestId('pending-changes-bar')).toHaveTextContent('tableData.pendingDeletes#1');
    expect(screen.getByTestId('pending-preview')).toBeInTheDocument();
    expect(screen.getByTestId('pending-commit')).toBeInTheDocument();
    expect(screen.getByTestId('pending-rollback')).toBeInTheDocument();
  });

  it('omits empty update/delete counts', () => {
    renderPending({ updateCount: 0, deleteCount: 0 });
    const bar = screen.getByTestId('pending-changes-bar');
    expect(bar).not.toHaveTextContent('tableData.pendingUpdates');
    expect(bar).not.toHaveTextContent('tableData.pendingDeletes');
  });

  it('routes preview/commit/rollback to their callbacks', () => {
    const props = renderPending();

    fireEvent.click(screen.getByTestId('pending-preview'));
    fireEvent.click(screen.getByTestId('pending-commit'));
    fireEvent.click(screen.getByTestId('pending-rollback'));

    expect(props.onPreview).toHaveBeenCalledTimes(1);
    expect(props.onCommit).toHaveBeenCalledTimes(1);
    expect(props.onRollback).toHaveBeenCalledTimes(1);
  });

  it('disables every action and marks the bar busy while committing', () => {
    renderPending({ busy: true });
    expect(screen.getByTestId('pending-changes-bar')).toHaveAttribute('aria-busy', 'true');
    expect(screen.getByTestId('pending-preview')).toBeDisabled();
    expect(screen.getByTestId('pending-commit')).toBeDisabled();
    expect(screen.getByTestId('pending-rollback')).toBeDisabled();
  });
});

describe('TableTransactionControls', () => {
  it('refreshes the session on mount even for a read-only panel, without rendering controls', async () => {
    renderTx({ isEditable: false });

    await vi.waitFor(() => expect(controller.refresh).toHaveBeenCalledTimes(1));
    expect(screen.queryByTestId('table-tx-controls')).toBeNull();
    expect(screen.queryByTestId('table-tx-begin')).toBeNull();
  });

  it('begins, then offers commit/rollback once the transaction is active', async () => {
    controller.refresh
      .mockResolvedValueOnce({ state: 'ready', observedContext: { transactionState: 'none' } })
      .mockResolvedValue({ state: 'ready', observedContext: { transactionState: 'active' } });
    renderTx();

    await vi.waitFor(() => expect(screen.getByTestId('table-tx-begin')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('table-tx-begin'));

    await vi.waitFor(() =>
      expect(controller.execute).toHaveBeenCalledWith({
        command: 'platform.beginTransaction',
        input: { defersCommit: true },
      }),
    );
    expect(controller.getExecution).toHaveBeenCalledWith('exec-1');
    await vi.waitFor(() => expect(screen.getByTestId('table-tx-commit')).toBeInTheDocument());
    expect(screen.getByTestId('table-tx-rollback')).toBeInTheDocument();
    expect(screen.queryByTestId('table-tx-begin')).toBeNull();
  });

  it('commits through the platform command', async () => {
    controller.refresh.mockResolvedValue({
      state: 'ready',
      observedContext: { transactionState: 'active' },
    });
    renderTx();

    await vi.waitFor(() => expect(screen.getByTestId('table-tx-commit')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('table-tx-commit'));

    await vi.waitFor(() =>
      expect(controller.execute).toHaveBeenCalledWith({
        command: 'platform.commitTransaction',
        input: { defersCommit: true },
      }),
    );
  });

  it('rolls back through the platform command and reloads the page', async () => {
    controller.refresh.mockResolvedValue({
      state: 'ready',
      observedContext: { transactionState: 'active' },
    });
    const props = renderTx();

    await vi.waitFor(() => expect(screen.getByTestId('table-tx-rollback')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('table-tx-rollback'));

    await vi.waitFor(() =>
      expect(controller.execute).toHaveBeenCalledWith({
        command: 'platform.rollbackTransaction',
        input: { defersCommit: true },
      }),
    );
    await vi.waitFor(() => expect(props.onReload).toHaveBeenCalledTimes(1));
  });

  it('surfaces a terminal execution failure with the preserved effect outcome and no open-new button', async () => {
    controller.getExecution.mockResolvedValue({
      state: 'failed',
      errorCode: 'tx_deadlock',
      effectOutcome: 'rolled_back',
    });
    renderTx();

    fireEvent.click(await screen.findByTestId('table-tx-begin'));

    const banner = await screen.findByTestId('table-transaction-error');
    expect(banner).toHaveTextContent('tx_deadlock; effect: rolled_back');
    expect(screen.queryByTestId('table-session-open-new')).toBeNull();
  });

  it('offers open-new-session recovery when the session is lost, and reloads after recovery', async () => {
    controller.getExecution.mockImplementation(async () => {
      controller.snapshot = { state: 'lost' };
      return { state: 'failed', errorCode: 'session_gone', effectOutcome: 'unknown' };
    });
    const props = renderTx();

    fireEvent.click(await screen.findByTestId('table-tx-begin'));

    const openNew = await screen.findByTestId('table-session-open-new');
    expect(screen.getByTestId('table-transaction-error')).toHaveTextContent('session_gone');
    expect(props.onReload).not.toHaveBeenCalled();

    fireEvent.click(openNew);

    await vi.waitFor(() => expect(controller.openNewSession).toHaveBeenCalledTimes(1));
    await vi.waitFor(() => expect(props.onReload).toHaveBeenCalledTimes(1));
    await vi.waitFor(() =>
      expect(screen.queryByTestId('table-transaction-error')).toBeNull(),
    );
  });

  it('polls a still-running transaction execution before refreshing', async () => {
    controller.getExecution
      .mockResolvedValueOnce({ state: 'running' })
      .mockResolvedValue({ state: 'succeeded' });
    renderTx();

    fireEvent.click(await screen.findByTestId('table-tx-begin'));

    await vi.waitFor(() => expect(controller.getExecution).toHaveBeenCalledTimes(2));
    // mount + executeTableTransaction's own refresh + refreshTxStatus
    await vi.waitFor(() => expect(controller.refresh).toHaveBeenCalledTimes(3));
    expect(screen.queryByTestId('table-transaction-error')).toBeNull();
  });

  it('keeps the error visible when open-new-session recovery itself fails', async () => {
    controller.getExecution.mockImplementation(async () => {
      controller.snapshot = { state: 'lost' };
      return { state: 'failed', errorCode: 'session_gone', effectOutcome: 'unknown' };
    });
    controller.openNewSession.mockRejectedValueOnce(new Error('still offline'));
    const props = renderTx();

    fireEvent.click(await screen.findByTestId('table-tx-begin'));
    fireEvent.click(await screen.findByTestId('table-session-open-new'));

    await vi.waitFor(() =>
      expect(screen.getByTestId('table-transaction-error')).toHaveTextContent('still offline'),
    );
    expect(props.onReload).not.toHaveBeenCalled();
  });

  it('keeps the begin button disabled while the page is loading', async () => {
    renderTx({ loading: true });
    expect(await screen.findByTestId('table-tx-begin')).toBeDisabled();
  });

  it('reports a session refresh failure with the disconnected fallback', async () => {
    controller.refresh.mockRejectedValueOnce(new Error(''));
    renderTx({ isEditable: false });

    const banner = await screen.findByTestId('table-transaction-error');
    expect(banner).toHaveTextContent('query.session.disconnected');
    // No transaction error means the reconnect entry stays hidden.
    expect(screen.queryByTestId('table-session-open-new')).toBeNull();
  });
});
