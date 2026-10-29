/**
 * Copy-feedback convergence for `SqlPreview` (track: copy-feedback-converge).
 *
 * `SqlPreview` used to `await` the write before confirming, so a slow or denied
 * clipboard left the button unresponsive; the converged hook confirms on click
 * and rolls back if the write never lands.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import {
  advanceBy,
  installControlledClipboard,
  installResolvedClipboard,
  spyOnWindowTimers,
} from '../../../test/copyFeedbackHarness';
import { SqlPreview } from '../SqlPreview';
import { DEFAULT_SYNC_OPTIONS, syncCommands } from '../../../commands/sync';
import type { DataSyncTableResult } from '../mappingView';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/sync', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../commands/sync')>()),
  syncCommands: { generateDataSyncSql: vi.fn() },
}));

/** This preview's `useCopyFeedback` window. */
const FEEDBACK_MS = 2000;

const tables = [
  { sourceTable: 'public.users', targetTable: 'public.users', columns: [] },
] as unknown as DataSyncTableResult[];
const options = DEFAULT_SYNC_OPTIONS;

/** The confirmed label is what the test reads; the button is captured once. */
const isConfirmed = () => screen.queryByText('common.copied') !== null;

async function renderPreview() {
  vi.mocked(syncCommands.generateDataSyncSql).mockResolvedValue([
    { sql: 'CREATE TABLE users (id int);', op: 'create' },
  ] as never);
  const utils = render(
    <SqlPreview
      sourceConnId="conn-1"
      targetConnId="conn-2"
      sourceDatabase="a"
      targetDatabase="b"
      sourceSchema="public"
      targetSchema="public"
      tables={tables}
      options={options}
    />,
  );
  // The preview is generated through an async command; flush it with
  // microtasks rather than `waitFor`, which stalls under fake timers.
  await act(async () => {
    await Promise.resolve();
  });
  // React keeps this node across the label swap, so it is a stable locator.
  return { ...utils, copyButton: screen.getByText('common.copy').closest('button') as HTMLElement };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe('SqlPreview copy feedback', () => {
  it('confirms on click without waiting for the write to settle', async () => {
    const clipboard = installControlledClipboard();
    const { copyButton } = await renderPreview();

    fireEvent.click(copyButton);
    // Optimistic: the label has flipped while the write is still in flight.
    expect(isConfirmed()).toBe(true);
    expect(clipboard.outstanding()).toBe(1);

    await clipboard.settle(0);
    expect(isConfirmed()).toBe(true);
  });

  it('holds the confirmation for the full 2000ms window', async () => {
    installResolvedClipboard();
    const { copyButton } = await renderPreview();

    fireEvent.click(copyButton);

    advanceBy(FEEDBACK_MS - 1);
    expect(isConfirmed()).toBe(true);

    advanceBy(1);
    expect(isConfirmed()).toBe(false);
  });

  it('rolls back when the clipboard write rejects', async () => {
    const clipboard = installControlledClipboard();
    const { copyButton } = await renderPreview();

    fireEvent.click(copyButton);
    expect(isConfirmed()).toBe(true);

    await clipboard.settle(0, 'reject');
    expect(isConfirmed()).toBe(false);
  });

  it('clears the feedback timer when unmounted inside the window', async () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount, copyButton } = await renderPreview();

    fireEvent.click(copyButton);
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('a late rejection from an earlier click does not erase a newer confirmation', async () => {
    const clipboard = installControlledClipboard();
    const { copyButton } = await renderPreview();

    fireEvent.click(copyButton);
    fireEvent.click(copyButton);

    await clipboard.settle(0, 'reject');
    expect(isConfirmed()).toBe(true);

    advanceBy(FEEDBACK_MS);
    expect(isConfirmed()).toBe(false);
  });
});
