/**
 * Copy-feedback convergence for `QueryErrorPanel` (track: copy-feedback-converge).
 *
 * The panel kept its own `errorCopied` flag, a bare `setTimeout` and an extra
 * `onCopy?.()` side effect. Converged onto `useCopyFeedback`, the side effect
 * stays — only the flag and the timer moved into the hook.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import {
  advanceBy,
  installControlledClipboard,
  installResolvedClipboard,
  spyOnWindowTimers,
} from '../../../test/copyFeedbackHarness';
import { QueryErrorPanel } from '../QueryErrorPanel';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

/** This panel's `useCopyFeedback` window. */
const FEEDBACK_MS = 1500;

const copyButton = () => screen.getByTestId('query-copy-error');
const confirmed = () => copyButton().querySelector('.lucide-check') !== null;

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe('QueryErrorPanel copy feedback', () => {
  it('confirms immediately, holds for 1500ms, then reverts', () => {
    const writeText = installResolvedClipboard();
    render(<QueryErrorPanel message="boom: constraint violated" />);

    fireEvent.click(copyButton());
    expect(writeText).toHaveBeenCalledWith('boom: constraint violated');
    expect(confirmed()).toBe(true);

    advanceBy(FEEDBACK_MS - 1);
    expect(confirmed()).toBe(true);

    advanceBy(1);
    expect(confirmed()).toBe(false);
  });

  it('still fires the onCopy side effect exactly once per click', () => {
    installResolvedClipboard();
    const onCopy = vi.fn();
    render(<QueryErrorPanel message="boom" onCopy={onCopy} />);

    fireEvent.click(copyButton());
    expect(onCopy).toHaveBeenCalledTimes(1);
  });

  it('rolls back when the clipboard write rejects', async () => {
    const clipboard = installControlledClipboard();
    render(<QueryErrorPanel message="boom" />);

    fireEvent.click(copyButton());
    expect(confirmed()).toBe(true);

    await clipboard.settle(0, 'reject');
    expect(confirmed()).toBe(false);
  });

  it('clears the feedback timer when unmounted inside the window', () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount } = render(<QueryErrorPanel message="boom" />);

    fireEvent.click(copyButton());
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('a late rejection from an earlier click does not erase a newer confirmation', async () => {
    const clipboard = installControlledClipboard();
    render(<QueryErrorPanel message="boom" />);

    fireEvent.click(copyButton());
    fireEvent.click(copyButton());

    await clipboard.settle(0, 'reject');
    expect(confirmed()).toBe(true);

    advanceBy(FEEDBACK_MS);
    expect(confirmed()).toBe(false);
  });
});
