/**
 * Copy-feedback convergence for the connection-home copy affordances
 * (track: copy-feedback-converge).
 *
 * `RecentQueriesList` and `McpPromoBar` each owned a `copied` flag and a bare
 * `setTimeout`. Converged onto `useCopyFeedback`, the per-row id is still local
 * to the list but the flag is the hook's, so a rollback drops the stale marker.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import {
  advanceBy,
  installControlledClipboard,
  installResolvedClipboard,
  spyOnWindowTimers,
} from '../../../../test/copyFeedbackHarness';
import { McpPromoBar } from '../McpPromoBar';
import { RecentQueriesList } from '../RecentQueriesList';
import { clearCachedAppExecutablePathForTest } from '../../../../lib/mcpAgentConfig';
import { settingsCommands } from '../../../../commands/settings';
import type { QueryHistoryEntry } from '../../../../types';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

/** Both components share the same `useCopyFeedback` window. */
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

function renderList() {
  return render(
    <RecentQueriesList entries={entries} savedConnections={[]} onOpenHistory={vi.fn()} />,
  );
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

describe('RecentQueriesList copy feedback', () => {
  it('marks only the row whose SQL was copied', () => {
    const writeText = installResolvedClipboard();
    renderList();

    fireEvent.click(rowButton('q1'));
    expect(writeText).toHaveBeenCalledWith('SELECT 1');
    expect(rowConfirmed('q1')).toBe(true);
    expect(rowConfirmed('q2')).toBe(false);

    fireEvent.click(rowButton('q2'));
    expect(rowConfirmed('q1')).toBe(false);
    expect(rowConfirmed('q2')).toBe(true);
  });

  it('does not treat the row click as a history selection', () => {
    installResolvedClipboard();
    const onOpenHistory = vi.fn();
    render(
      <RecentQueriesList entries={entries} savedConnections={[]} onOpenHistory={onOpenHistory} />,
    );

    fireEvent.click(rowButton('q1'));
    expect(onOpenHistory).not.toHaveBeenCalled();
  });

  it('holds the row marker for the full 2000ms window', () => {
    installResolvedClipboard();
    renderList();

    fireEvent.click(rowButton('q1'));

    advanceBy(FEEDBACK_MS - 1);
    expect(rowConfirmed('q1')).toBe(true);

    advanceBy(1);
    expect(rowConfirmed('q1')).toBe(false);
  });

  it('rolls the row marker back when the clipboard write rejects', async () => {
    const clipboard = installControlledClipboard();
    renderList();

    fireEvent.click(rowButton('q1'));
    expect(rowConfirmed('q1')).toBe(true);

    await clipboard.settle(0, 'reject');
    expect(rowConfirmed('q1')).toBe(false);
  });

  it('clears the feedback timer when unmounted inside the window', () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount } = renderList();

    fireEvent.click(rowButton('q1'));
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('a late rejection from an earlier row does not move the marker', async () => {
    const clipboard = installControlledClipboard();
    renderList();

    fireEvent.click(rowButton('q1'));
    fireEvent.click(rowButton('q2'));

    await clipboard.settle(0, 'reject');
    expect(rowConfirmed('q1')).toBe(false);
    expect(rowConfirmed('q2')).toBe(true);

    advanceBy(FEEDBACK_MS);
    expect(rowConfirmed('q2')).toBe(false);
  });
});

describe('McpPromoBar copy feedback', () => {
  const copyButton = () => screen.getByTestId('home-mcp-copy-button');
  const confirmed = () => copyButton().textContent?.includes('copied') ?? false;

  it('confirms on click, holds for 2000ms, then reverts', () => {
    const writeText = installResolvedClipboard();
    render(<McpPromoBar />);

    fireEvent.click(copyButton());
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining('datazen'));
    expect(confirmed()).toBe(true);

    advanceBy(FEEDBACK_MS - 1);
    expect(confirmed()).toBe(true);

    advanceBy(1);
    expect(confirmed()).toBe(false);
  });

  it('rolls back when the clipboard write rejects', async () => {
    const clipboard = installControlledClipboard();
    render(<McpPromoBar />);

    fireEvent.click(copyButton());
    expect(confirmed()).toBe(true);

    await clipboard.settle(0, 'reject');
    expect(confirmed()).toBe(false);
  });

  it('clears the feedback timer when unmounted inside the window', () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount } = render(<McpPromoBar />);

    fireEvent.click(copyButton());
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('a late rejection from an earlier click does not erase a newer confirmation', async () => {
    const clipboard = installControlledClipboard();
    render(<McpPromoBar />);

    fireEvent.click(copyButton());
    fireEvent.click(copyButton());

    await clipboard.settle(0, 'reject');
    expect(confirmed()).toBe(true);

    advanceBy(FEEDBACK_MS);
    expect(confirmed()).toBe(false);
  });
});
