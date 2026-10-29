import { afterEach, describe, expect, it, vi } from 'vitest';
import { installTauriEventUnlistenRaceWorkaround } from '../tauriEventCompat';

interface TestEventInternals {
  unregisterListener: (event: string, eventId: number) => void;
  __datazenSafeUnlistenInstalled?: boolean;
}

const tauriGlobal = globalThis as typeof globalThis & {
  __TAURI_EVENT_PLUGIN_INTERNALS__?: TestEventInternals;
};

afterEach(() => {
  delete tauriGlobal.__TAURI_EVENT_PLUGIN_INTERNALS__;
});

describe('installTauriEventUnlistenRaceWorkaround', () => {
  it('allows Tauri public unlisten to continue after the known missing-entry TypeError', () => {
    const unregisterListener = vi.fn(() => {
      throw new TypeError("undefined is not an object (evaluating 'listeners[eventId].handlerId')");
    });
    tauriGlobal.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener };

    installTauriEventUnlistenRaceWorkaround();

    expect(() =>
      tauriGlobal.__TAURI_EVENT_PLUGIN_INTERNALS__?.unregisterListener('datazen:test', 7),
    ).not.toThrow();
    expect(unregisterListener).toHaveBeenCalledWith('datazen:test', 7);
  });

  it('does not hide unrelated unregister failures', () => {
    tauriGlobal.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener: () => {
        throw new Error('unexpected failure');
      },
    };

    installTauriEventUnlistenRaceWorkaround();

    expect(() =>
      tauriGlobal.__TAURI_EVENT_PLUGIN_INTERNALS__?.unregisterListener('datazen:test', 9),
    ).toThrow('unexpected failure');
  });

  it('does not hide an unrelated TypeError', () => {
    tauriGlobal.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener: () => {
        throw new TypeError('unrelated type failure');
      },
    };

    installTauriEventUnlistenRaceWorkaround();

    expect(() =>
      tauriGlobal.__TAURI_EVENT_PLUGIN_INTERNALS__?.unregisterListener('datazen:test', 10),
    ).toThrow('unrelated type failure');
  });

  it('installs only once', () => {
    const unregisterListener = vi.fn();
    tauriGlobal.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener };

    installTauriEventUnlistenRaceWorkaround();
    const installed = tauriGlobal.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener;
    installTauriEventUnlistenRaceWorkaround();

    expect(tauriGlobal.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener).toBe(installed);
    installed('datazen:test', 11);
    expect(unregisterListener).toHaveBeenCalledOnce();
  });
});
