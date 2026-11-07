import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { UpdateCheckResult } from '../../platform/contracts';

const mocks = vi.hoisted(() => ({
  channel: vi.fn(),
  check: vi.fn<() => Promise<UpdateCheckResult>>(),
  install: vi.fn(),
}));

vi.mock('../updater', () => ({
  getUpdateChannel: mocks.channel,
  checkForUpdates: mocks.check,
  downloadAndInstallUpdate: mocks.install,
}));

import { startUpdateReminderSchedule } from '../updateReminderSchedule';
import { useUpdateReminderStore } from '../../stores/updateReminderStore';

const DAY = 24 * 60 * 60 * 1000;
const START = Date.parse('2026-11-06T00:00:00Z');
let stop: (() => void) | undefined;

async function start() {
  stop = startUpdateReminderSchedule();
  await vi.advanceTimersByTimeAsync(0);
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.useFakeTimers();
  vi.setSystemTime(START);
  mocks.channel.mockReturnValue('auto');
  mocks.check.mockResolvedValue({ status: 'upToDate' });
  useUpdateReminderStore.setState(useUpdateReminderStore.getInitialState(), true);
  vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
});

afterEach(() => {
  stop?.();
  stop = undefined;
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('main-window update reminder schedule', () => {
  it.each(['manual', 'none'])('does not schedule checks for the %s channel', async (channel) => {
    mocks.channel.mockReturnValue(channel);
    await start();
    window.dispatchEvent(new Event('focus'));
    await vi.advanceTimersByTimeAsync(DAY);
    expect(mocks.check).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('checks on launch, shows the available version, and checks again at 24 hours', async () => {
    mocks.check.mockResolvedValueOnce({ status: 'available', version: '0.3.1' });
    await start();
    expect(mocks.check).toHaveBeenCalledTimes(1);
    expect(useUpdateReminderStore.getState().availableVersion).toBe('0.3.1');
    expect(mocks.install).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(DAY - 1);
    expect(mocks.check).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(mocks.check).toHaveBeenCalledTimes(2);
    expect(useUpdateReminderStore.getState().availableVersion).toBeNull();
  });

  it.each(['focus', 'online', 'visibilitychange'])(
    'catches up after a two-day sleep on %s and coalesces simultaneous events',
    async (event) => {
      await start();
      // A suspended WebView advances wall time without running JS timers.
      vi.setSystemTime(START + 2 * DAY);
      const target = event === 'visibilitychange' ? document : window;
      target.dispatchEvent(new Event(event));
      window.dispatchEvent(new Event('focus'));
      window.dispatchEvent(new Event('online'));
      document.dispatchEvent(new Event('visibilitychange'));
      await vi.advanceTimersByTimeAsync(60_000);
      expect(mocks.check).toHaveBeenCalledTimes(2);
      expect(useUpdateReminderStore.getState().lastCheckAttemptAt).toBe(START + 2 * DAY);
    },
  );

  it('catches up within one active minute even when wake produces no browser event', async () => {
    await start();
    vi.setSystemTime(START + 2 * DAY);
    await vi.advanceTimersByTimeAsync(59_999);
    expect(mocks.check).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(mocks.check).toHaveBeenCalledTimes(2);
  });

  it('ignores hidden visibility events and catches up when visible', async () => {
    await start();
    vi.setSystemTime(START + 2 * DAY);
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
    document.dispatchEvent(new Event('visibilitychange'));
    expect(mocks.check).toHaveBeenCalledTimes(1);
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
    document.dispatchEvent(new Event('visibilitychange'));
    await vi.advanceTimersByTimeAsync(0);
    expect(mocks.check).toHaveBeenCalledTimes(2);
  });

  it('does not consume an overdue check while installing', async () => {
    await start();
    vi.setSystemTime(START + 2 * DAY);
    useUpdateReminderStore.setState({ isInstalling: true });
    window.dispatchEvent(new Event('focus'));
    await vi.advanceTimersByTimeAsync(60_000);
    expect(mocks.check).toHaveBeenCalledTimes(1);
    expect(useUpdateReminderStore.getState().lastCheckAttemptAt).toBe(START);
    useUpdateReminderStore.setState({ isInstalling: false });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(mocks.check).toHaveBeenCalledTimes(2);
  });

  it('does not start concurrent requests when a pre-sleep check is still pending', async () => {
    let finish: ((result: UpdateCheckResult) => void) | undefined;
    mocks.check.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    await start();
    vi.setSystemTime(START + 2 * DAY);
    window.dispatchEvent(new Event('focus'));
    await vi.advanceTimersByTimeAsync(60_000);
    expect(mocks.check).toHaveBeenCalledTimes(1);
    expect(useUpdateReminderStore.getState().lastCheckAttemptAt).toBe(START);
    finish?.({ status: 'upToDate' });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(mocks.check).toHaveBeenCalledTimes(2);
  });

  it('preserves a reminder after failure, avoids an event retry loop, and recovers next day', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    mocks.check.mockResolvedValueOnce({ status: 'available', version: '0.3.1' });
    await start();
    mocks.check.mockResolvedValueOnce({ status: 'error', message: 'offline' });
    vi.setSystemTime(START + 2 * DAY);
    window.dispatchEvent(new Event('focus'));
    await vi.advanceTimersByTimeAsync(0);
    window.dispatchEvent(new Event('online'));
    await vi.advanceTimersByTimeAsync(60_000);
    expect(mocks.check).toHaveBeenCalledTimes(2);
    expect(useUpdateReminderStore.getState().availableVersion).toBe('0.3.1');
    vi.setSystemTime(START + 3 * DAY);
    window.dispatchEvent(new Event('focus'));
    await vi.advanceTimersByTimeAsync(0);
    expect(mocks.check).toHaveBeenCalledTimes(3);
    expect(useUpdateReminderStore.getState().availableVersion).toBeNull();
  });

  it('cleans up timers and listeners and deduplicates a StrictMode-style remount', async () => {
    await start();
    stop?.();
    stop = startUpdateReminderSchedule();
    await vi.advanceTimersByTimeAsync(0);
    expect(mocks.check).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(1);
    stop();
    vi.setSystemTime(START + 2 * DAY);
    window.dispatchEvent(new Event('focus'));
    window.dispatchEvent(new Event('online'));
    document.dispatchEvent(new Event('visibilitychange'));
    await vi.advanceTimersByTimeAsync(DAY);
    expect(mocks.check).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('re-establishes the deadline after a backward system-clock adjustment', async () => {
    await start();
    vi.setSystemTime(START - DAY);
    window.dispatchEvent(new Event('focus'));
    await vi.advanceTimersByTimeAsync(0);
    window.dispatchEvent(new Event('focus'));
    expect(mocks.check).toHaveBeenCalledTimes(2);
    expect(useUpdateReminderStore.getState().lastCheckAttemptAt).toBe(START - DAY);
  });
});
