/**
 * Copy-feedback convergence for `ExecutionSummaryCard`
 * (track: copy-feedback-converge).
 *
 * The card previously copied with a fire-and-forget write and never reverted on
 * failure, so a denied clipboard write left "已复制" on screen forever. The
 * converged hook makes the confirmation optimistic *and* bounded.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import {
  advanceBy,
  installControlledClipboard,
  installResolvedClipboard,
  spyOnWindowTimers,
} from '../../../../test/copyFeedbackHarness';
import { ExecutionSummaryCard } from '../ExecutionSummaryCard';
import type { StatementResult } from '../../../../types';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

/** This card's `useCopyFeedback` window. */
const FEEDBACK_MS = 2000;

/** A fully-populated `StatementResult`, so the fixture cannot drift from the type. */
const result: StatementResult = {
  sql: 'SELECT 1',
  columns: [{ name: 'n', dataType: 'int4', nullable: false }],
  rows: [[1]],
  rowsAffected: 1,
  executionTimeMs: 5,
};

const copyButton = () => screen.getByTitle('复制 SQL');
const confirmed = () => copyButton().textContent?.includes('已复制') ?? false;

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe('ExecutionSummaryCard copy feedback', () => {
  it('confirms immediately, holds for 2000ms, then reverts', () => {
    const writeText = installResolvedClipboard();
    render(<ExecutionSummaryCard result={result} />);

    fireEvent.click(copyButton());
    expect(writeText).toHaveBeenCalledWith('SELECT 1');
    expect(confirmed()).toBe(true);

    advanceBy(FEEDBACK_MS - 1);
    expect(confirmed()).toBe(true);

    advanceBy(1);
    expect(confirmed()).toBe(false);
  });

  it('renders no copy action when the statement produced no SQL', () => {
    installResolvedClipboard();
    render(<ExecutionSummaryCard result={{ ...result, sql: '' }} />);
    expect(screen.queryByTitle('复制 SQL')).toBeNull();
  });

  it('rolls back when the clipboard write rejects', async () => {
    const clipboard = installControlledClipboard();
    render(<ExecutionSummaryCard result={result} />);

    fireEvent.click(copyButton());
    expect(confirmed()).toBe(true);

    await clipboard.settle(0, 'reject');
    expect(confirmed()).toBe(false);
  });

  it('clears the feedback timer when unmounted inside the window', () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount } = render(<ExecutionSummaryCard result={result} />);

    fireEvent.click(copyButton());
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('a late rejection from an earlier click does not erase a newer confirmation', async () => {
    const clipboard = installControlledClipboard();
    render(<ExecutionSummaryCard result={result} />);

    fireEvent.click(copyButton());
    fireEvent.click(copyButton());

    await clipboard.settle(0, 'reject');
    expect(confirmed()).toBe(true);

    advanceBy(FEEDBACK_MS);
    expect(confirmed()).toBe(false);
  });
});
