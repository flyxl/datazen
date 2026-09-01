import { describe, it, expect, vi } from 'vitest';
import { hideSplash, waitForStartupTask } from '../splash';

describe('hideSplash', () => {
  it('adds hide class and schedules remove', () => {
    vi.useFakeTimers();
    const el = document.createElement('div');
    document.body.appendChild(el);
    hideSplash(el);
    expect(el.classList.contains('hide')).toBe(true);
    vi.advanceTimersByTime(350);
    expect(document.body.contains(el)).toBe(false);
    vi.useRealTimers();
  });

  it('stops waiting when a startup task does not settle', async () => {
    vi.useFakeTimers();
    const result = waitForStartupTask(new Promise(() => {}), 3000);

    await vi.advanceTimersByTimeAsync(3000);

    await expect(result).resolves.toBe('timed-out');
    vi.useRealTimers();
  });

  it('clears the deadline when a startup task completes', async () => {
    vi.useFakeTimers();

    await expect(waitForStartupTask(Promise.resolve(), 3000)).resolves.toBe('completed');
    expect(vi.getTimerCount()).toBe(0);
    vi.useRealTimers();
  });
});
