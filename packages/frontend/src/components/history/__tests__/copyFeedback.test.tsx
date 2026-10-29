/**
 * Copy-feedback convergence for `GlobalQueryHistoryDialog`
 * (track: copy-feedback-converge).
 *
 * The dialog used to keep a per-row `copiedId` plus its own `setTimeout`.
 * Converged onto `useCopyFeedback`, the row id is still tracked locally but the
 * flag is the hook's — so the marker is now also request-bound, and the two
 * are combined into `copiedRowId` for rendering.
 */
import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  advanceBy,
  installControlledClipboard,
  installResolvedClipboard,
  spyOnWindowTimers,
} from '../../../test/copyFeedbackHarness';
import { GlobalQueryHistoryDialog } from '../GlobalQueryHistoryDialog';
import { queryCommands } from '../../../commands/query';
import { useConnectionStore } from '../../../stores/connectionStore';
import type { QueryHistoryEntry } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    getQueryHistoryPage: vi.fn(),
    clearQueryHistory: vi.fn(),
    deleteQueryHistoryEntry: vi.fn(),
    addFavoriteQuery: vi.fn(),
    saveSqlFile: vi.fn(),
  },
}));

/** This dialog's `useCopyFeedback` window. */
const FEEDBACK_MS = 2000;

const entries: QueryHistoryEntry[] = [
  {
    id: 'entry-1',
    connectionId: 'conn-1',
    database: 'db',
    sql: 'SELECT 1',
    executedAt: '2026-09-07T10:00:00Z',
    executionTimeMs: 12,
    rowsAffected: 5,
    success: true,
  },
  {
    id: 'entry-2',
    connectionId: 'conn-1',
    database: 'db',
    sql: 'SELECT 2',
    executedAt: '2026-09-07T11:00:00Z',
    executionTimeMs: 8,
    rowsAffected: 2,
    success: true,
  },
];

/** The row's copy button, bound through its data attribute rather than a label. */
function rowCopyButtons(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>('[data-history-action="copy"]'));
}

function confirmed(button: HTMLElement): boolean {
  return button.querySelector('.lucide-check') !== null;
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.mocked(queryCommands.getQueryHistoryPage).mockResolvedValue({
    entries,
    total: entries.length,
  });
  useConnectionStore.setState({ connections: [] } as never);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

async function renderDialog() {
  const utils = render(
    <GlobalQueryHistoryDialog open onClose={vi.fn()} initialConnectionId="conn-1" />,
  );
  // The history arrives through an async command; flush it with microtasks
  // rather than `waitFor`, which cannot make progress under fake timers.
  await act(async () => {
    await Promise.resolve();
  });
  expect(rowCopyButtons()).toHaveLength(2);
  return utils;
}

describe('GlobalQueryHistoryDialog copy feedback', () => {
  it('marks only the row whose SQL was copied', async () => {
    const writeText = installResolvedClipboard();
    await renderDialog();
    const [first, second] = rowCopyButtons();

    fireEvent.click(first);
    expect(writeText).toHaveBeenCalledWith('SELECT 1');
    expect(confirmed(first)).toBe(true);
    expect(confirmed(second)).toBe(false);

    fireEvent.click(second);
    expect(confirmed(first)).toBe(false);
    expect(confirmed(second)).toBe(true);
  });

  it('holds the row marker for the full 2000ms window', async () => {
    installResolvedClipboard();
    await renderDialog();
    const [first] = rowCopyButtons();

    fireEvent.click(first);

    advanceBy(FEEDBACK_MS - 1);
    expect(confirmed(first)).toBe(true);

    advanceBy(1);
    expect(confirmed(first)).toBe(false);
  });

  it('rolls the row marker back when the clipboard write rejects', async () => {
    const clipboard = installControlledClipboard();
    await renderDialog();
    const [first] = rowCopyButtons();

    fireEvent.click(first);
    expect(confirmed(first)).toBe(true);

    await clipboard.settle(0, 'reject');
    expect(confirmed(first)).toBe(false);
  });

  it('clears the feedback timer when unmounted inside the window', async () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount } = await renderDialog();
    const [first] = rowCopyButtons();

    fireEvent.click(first);
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('a late rejection from an earlier row does not move the marker', async () => {
    const clipboard = installControlledClipboard();
    await renderDialog();
    const [first, second] = rowCopyButtons();

    fireEvent.click(first);
    fireEvent.click(second);

    await clipboard.settle(0, 'reject');
    expect(confirmed(first)).toBe(false);
    expect(confirmed(second)).toBe(true);

    advanceBy(FEEDBACK_MS);
    expect(confirmed(second)).toBe(false);
  });
});
