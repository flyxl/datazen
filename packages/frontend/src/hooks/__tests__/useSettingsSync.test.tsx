import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { cleanup, render } from '@testing-library/react';
import { useSettingsSync } from '../useSettingsSync';
import { useSettingsStore } from '../../stores/settingsStore';

const listeners = new Map<string, (payload?: unknown) => void>();

vi.mock('../../lib/crossWindowBus', () => ({
  listenCrossWindow: vi.fn(async (event: string, handler: (payload?: unknown) => void) => {
    listeners.set(event, handler);
    return () => {
      listeners.delete(event);
    };
  }),
}));

vi.mock('../../lib/themePackApply', () => ({
  applyThemePack: vi.fn().mockResolvedValue({ ok: true }),
  syncWebviewBackgroundFromTokens: vi.fn(),
}));

function Probe() {
  useSettingsSync();
  return null;
}

describe('useSettingsSync', () => {
  beforeEach(() => {
    listeners.clear();
    useSettingsStore.setState({
      settings: {
        ...useSettingsStore.getState().settings,
        autoChartOnQuery: true,
      },
    });
  });

  afterEach(() => {
    cleanup();
    listeners.clear();
  });

  it('updates local settings cache when another window saves settings', async () => {
    render(<Probe />);
    await vi.waitFor(() => {
      expect(listeners.get('datazen:settings-changed')).toBeTypeOf('function');
    });

    listeners.get('datazen:settings-changed')?.({
      ...useSettingsStore.getState().settings,
      autoChartOnQuery: false,
    });

    await vi.waitFor(() => {
      expect(useSettingsStore.getState().settings.autoChartOnQuery).toBe(false);
    });
  });
});
