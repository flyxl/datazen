import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { cleanup, render } from '@testing-library/react';
import { useThemeSync } from '../useThemeSync';
import { useSettingsStore } from '../../stores/settingsStore';

const listeners = new Map<string, (payload?: unknown) => void>();
const saveSettings = vi.fn().mockResolvedValue(undefined);

vi.mock('../../lib/crossWindowBus', () => ({
  listenCrossWindow: vi.fn(async (event: string, handler: (payload?: unknown) => void) => {
    listeners.set(event, handler);
    return () => {
      listeners.delete(event);
    };
  }),
}));

vi.mock('../../commands/settings', () => ({
  settingsCommands: {
    saveSettings: (...args: unknown[]) => saveSettings(...args),
  },
}));

vi.mock('../../lib/themePackApply', () => ({
  applyThemePack: vi.fn().mockResolvedValue({ ok: true }),
  syncWebviewBackgroundFromTokens: vi.fn(),
}));

function Probe() {
  useThemeSync();
  return null;
}

describe('useThemeSync', () => {
  beforeEach(() => {
    listeners.clear();
    saveSettings.mockClear();
    useSettingsStore.setState({
      settings: {
        ...useSettingsStore.getState().settings,
        theme: { mode: 'light', packId: null },
      },
    });
  });

  afterEach(() => {
    cleanup();
    listeners.clear();
  });

  it('applies theme from another window without saving', async () => {
    render(<Probe />);
    await vi.waitFor(() => {
      expect(listeners.get('datazen:theme-changed')).toBeTypeOf('function');
    });

    listeners.get('datazen:theme-changed')?.('dark');

    await vi.waitFor(() => {
      expect(useSettingsStore.getState().settings.theme.mode).toBe('dark');
    });
    expect(saveSettings).not.toHaveBeenCalled();
  });
});
