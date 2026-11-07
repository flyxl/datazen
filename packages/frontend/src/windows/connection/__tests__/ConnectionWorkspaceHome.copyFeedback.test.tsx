/**
 * Copy-feedback convergence for `ConnectionWorkspaceHome`
 * (track: copy-feedback-converge).
 *
 * The home page's recent-query rows kept a local `copiedSqlId` and a bare
 * `setTimeout`. Converged onto `useCopyFeedback`, the id is still local but the
 * flag is the hook's, so the marker is request-bound and a failure drops it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import {
  advanceBy,
  installControlledClipboard,
  installResolvedClipboard,
  spyOnWindowTimers,
} from '../../../test/copyFeedbackHarness';
import { ConnectionWorkspaceHome } from '../ConnectionWorkspaceHome';
import { queryCommands } from '../../../commands/query';
import { settingsCommands } from '../../../commands/settings';
import { clearCachedAppExecutablePathForTest } from '../../../lib/mcpAgentConfig';
import { useConnectionStore } from '../../../stores/connectionStore';
import type { ConnectionConfig, QueryHistoryEntry } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: { getQueryHistoryPage: vi.fn() },
}));

vi.mock('../../../lib/databaseTypes', () => ({
  getDbIcon: () => null,
  getDbLabel: (type: string) => type,
  getDriverIconParents: () => ({}),
}));

/** This page's `useCopyFeedback` window. */
const FEEDBACK_MS = 2000;

const entries: QueryHistoryEntry[] = [
  {
    id: 'q1',
    connectionId: 'conn-1',
    database: 'db',
    sql: 'SELECT 1',
    executedAt: '2026-09-07T10:00:00Z',
    executionTimeMs: 12,
    rowsAffected: 5,
    success: true,
  },
  {
    id: 'q2',
    connectionId: 'conn-1',
    database: 'db',
    sql: 'SELECT 2',
    executedAt: '2026-09-07T11:00:00Z',
    executionTimeMs: 8,
    rowsAffected: 2,
    success: true,
  },
];

const rowButton = (id: string) => screen.getByTestId(`home-query-copy-${id}`);
const rowConfirmed = (id: string) => rowButton(id).textContent?.includes('copied') ?? false;

const connection: ConnectionConfig = {
  id: 'conn-1',
  name: 'PostgreSQL-Local',
  databaseType: 'postgresql',
  host: 'localhost',
  port: 5432,
  database: 'postgres',
  sslMode: 'prefer',
};

async function renderHome() {
  vi.mocked(queryCommands.getQueryHistoryPage).mockResolvedValue({ entries, total: entries.length });
  useConnectionStore.setState({ connections: [connection] } as never);
  const utils = render(
    <ConnectionWorkspaceHome
      hasConnections
      connectionContext={null}
      recentPanels={[]}
      showNewQuery={false}
      showNewTable={false}
      showErDiagram={false}
      showObjects={false}
      onNewConnection={vi.fn()}
      onNewQuery={vi.fn()}
      onCreateTable={vi.fn()}
      onOpenErDiagram={vi.fn()}
      onOpenObjects={vi.fn()}
      onOpenPanel={vi.fn()}
    />,
  );
  // History arrives through an async command; flush with microtasks because
  // `waitFor` cannot make progress under fake timers.
  await act(async () => {
    await Promise.resolve();
  });
  expect(screen.getByTestId('home-query-copy-q1')).toBeInTheDocument();
  return utils;
}

beforeEach(() => {
  vi.useFakeTimers();
  clearCachedAppExecutablePathForTest();
  vi.spyOn(settingsCommands, 'getAppExecutablePath').mockResolvedValue('/usr/bin/datazen');
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe('ConnectionWorkspaceHome copy feedback', () => {
  it('marks only the row whose SQL was copied', async () => {
    const writeText = installResolvedClipboard();
    await renderHome();

    fireEvent.click(rowButton('q1'));
    expect(writeText).toHaveBeenCalledWith('SELECT 1');
    expect(rowConfirmed('q1')).toBe(true);
    expect(rowConfirmed('q2')).toBe(false);

    fireEvent.click(rowButton('q2'));
    expect(rowConfirmed('q1')).toBe(false);
    expect(rowConfirmed('q2')).toBe(true);
  });

  it('holds the row marker for the full 2000ms window', async () => {
    installResolvedClipboard();
    await renderHome();

    fireEvent.click(rowButton('q1'));

    advanceBy(FEEDBACK_MS - 1);
    expect(rowConfirmed('q1')).toBe(true);

    advanceBy(1);
    expect(rowConfirmed('q1')).toBe(false);
  });

  it('rolls the row marker back when the clipboard write rejects', async () => {
    const clipboard = installControlledClipboard();
    await renderHome();

    fireEvent.click(rowButton('q1'));
    expect(rowConfirmed('q1')).toBe(true);

    await clipboard.settle(0, 'reject');
    expect(rowConfirmed('q1')).toBe(false);
  });

  it('clears the feedback timer when unmounted inside the window', async () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount } = await renderHome();

    fireEvent.click(rowButton('q1'));
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('a late rejection from an earlier row does not move the marker', async () => {
    const clipboard = installControlledClipboard();
    await renderHome();

    fireEvent.click(rowButton('q1'));
    fireEvent.click(rowButton('q2'));

    await clipboard.settle(0, 'reject');
    expect(rowConfirmed('q1')).toBe(false);
    expect(rowConfirmed('q2')).toBe(true);

    advanceBy(FEEDBACK_MS);
    expect(rowConfirmed('q2')).toBe(false);
  });
});
