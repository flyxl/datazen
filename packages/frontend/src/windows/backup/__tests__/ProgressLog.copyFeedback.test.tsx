/**
 * Copy-feedback convergence for `ProgressLog` (track: copy-feedback-converge).
 *
 * This log already had a `clearTimeout` on re-arm and an unmount cleanup, so
 * the visible behaviour it had to keep is: optimistic confirmation, 1500ms
 * window, and no revert if the write never resolved. What changed is that the
 * write is no longer success-only, and the manual `console.warn` branch is gone.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import {
  advanceBy,
  installControlledClipboard,
  installResolvedClipboard,
  spyOnWindowTimers,
} from '../../../test/copyFeedbackHarness';
import { ProgressLog } from '../ProgressLog';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

/** This log's `useCopyFeedback` window. */
const FEEDBACK_MS = 1500;

const copyButton = () => screen.getByTestId('backup-progress-log-copy');

beforeEach(() => {
  vi.useFakeTimers();
  Element.prototype.scrollTo = vi.fn();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe('ProgressLog copy feedback', () => {
  it('confirms immediately, holds for 1500ms, then reverts', () => {
    const writeText = installResolvedClipboard();
    render(<ProgressLog lines={['a', 'b']} />);

    fireEvent.click(copyButton());
    expect(writeText).toHaveBeenCalledWith('a\nb');
    expect(copyButton().textContent).toContain('backup.logCopied');

    advanceBy(FEEDBACK_MS - 1);
    expect(copyButton().textContent).toContain('backup.logCopied');

    advanceBy(1);
    expect(copyButton().textContent).toContain('backup.copyLog');
  });

  it('does nothing when the log is empty', () => {
    const writeText = installResolvedClipboard();
    render(<ProgressLog lines={[]} />);

    fireEvent.click(copyButton());
    expect(writeText).not.toHaveBeenCalled();
  });

  it('rolls back when the clipboard write rejects', async () => {
    const clipboard = installControlledClipboard();
    render(<ProgressLog lines={['a']} />);

    fireEvent.click(copyButton());
    expect(copyButton().textContent).toContain('backup.logCopied');

    await clipboard.settle(0, 'reject');
    expect(copyButton().textContent).toContain('backup.copyLog');
  });

  it('clears the feedback timer when unmounted inside the window', () => {
    installResolvedClipboard();
    const timers = spyOnWindowTimers();
    const { unmount } = render(<ProgressLog lines={['a']} />);

    fireEvent.click(copyButton());
    const windowHandle = timers.lastArmedHandle();
    expect(timers.clearedHandles()).not.toContain(windowHandle);

    unmount();

    expect(timers.clearedHandles()).toContain(windowHandle);
  });

  it('a late rejection from an earlier click does not erase a newer confirmation', async () => {
    const clipboard = installControlledClipboard();
    render(<ProgressLog lines={['a']} />);

    fireEvent.click(copyButton());
    fireEvent.click(copyButton());

    await clipboard.settle(0, 'reject');
    expect(copyButton().textContent).toContain('backup.logCopied');

    advanceBy(FEEDBACK_MS);
    expect(copyButton().textContent).toContain('backup.copyLog');
  });
});
