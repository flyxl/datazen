/**
 * useSettingsSync（issue-32 T11）：跨窗口设置通知只触发权威重读。
 *
 * 覆盖：收到 `datazen:settings-changed` 后重读权威配置（广播 payload
 * 不被信任：陈旧 payload 不得覆盖新值）、重读失败保留最后一份有效
 * 快照、未知版本同步不写回（store 保留 version 99 且不触发保存）。
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { cleanup, render } from '@testing-library/react';
import { useSettingsSync } from '../useSettingsSync';
import { useSettingsStore } from '../../stores/settingsStore';

const listeners = new Map<string, (payload?: unknown) => void>();

const { mockSettingsCommands } = vi.hoisted(() => ({
  mockSettingsCommands: {
    getSettings: vi.fn(),
    saveSettings: vi.fn(),
    syncLanguageMenu: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../commands/settings', () => ({
  settingsCommands: mockSettingsCommands,
}));

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
    vi.clearAllMocks();
    listeners.clear();
    document.documentElement.classList.remove('dark');
    Object.defineProperty(window, 'matchMedia', {
      writable: true,
      value: vi.fn().mockImplementation((query: string) => ({
        matches: query.includes('dark'),
        media: query,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    });
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
    document.documentElement.classList.remove('dark');
  });

  async function waitForListener(): Promise<void> {
    await vi.waitFor(() => {
      expect(listeners.get('datazen:settings-changed')).toBeTypeOf('function');
    });
  }

  function fireSettingsChanged(payload: unknown): void {
    listeners.get('datazen:settings-changed')?.(payload);
  }

  it('re-reads the authoritative settings instead of trusting the broadcast payload', async () => {
    render(<Probe />);
    await waitForListener();
    // 权威端是 false；广播 payload 里是发送窗口保存时刻的旧值 true。
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      autoChartOnQuery: false,
    });

    fireSettingsChanged({
      ...useSettingsStore.getState().settings,
      autoChartOnQuery: true,
    });

    await vi.waitFor(() => {
      expect(useSettingsStore.getState().settings.autoChartOnQuery).toBe(false);
    });
    expect(mockSettingsCommands.getSettings).toHaveBeenCalledTimes(1);
  });

  it('does not re-read after unmount, even if a notification arrives late', async () => {
    const { unmount } = render(<Probe />);
    // listen 的 promise 尚未落定就卸载：.then 的 cancelled 分支直接退订。
    const handler = listeners.get('datazen:settings-changed');
    expect(handler).toBeTypeOf('function');
    unmount();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(listeners.get('datazen:settings-changed')).toBeUndefined();
    mockSettingsCommands.getSettings.mockClear();
    handler?.({});
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(mockSettingsCommands.getSettings).not.toHaveBeenCalled();
  });

  it('keeps the last valid snapshot when the authoritative re-read fails', async () => {
    render(<Probe />);
    await waitForListener();
    // 先成功重读一次（建立有效快照），再失败一次。
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      autoChartOnQuery: false,
    });
    fireSettingsChanged({});
    await vi.waitFor(() => {
      expect(useSettingsStore.getState().settings.autoChartOnQuery).toBe(false);
    });

    mockSettingsCommands.getSettings.mockRejectedValueOnce(new Error('backend down'));
    fireSettingsChanged({});
    await vi.waitFor(() => {
      expect(mockSettingsCommands.getSettings).toHaveBeenCalledTimes(2);
    });
    // 给失败的重读留出完成时间：值保持上次有效快照，不被 wipe。
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(useSettingsStore.getState().settings.autoChartOnQuery).toBe(false);
    expect(mockSettingsCommands.saveSettings).not.toHaveBeenCalled();
  });

  it('keeps an unknown-version config verbatim on sync without writing back', async () => {
    render(<Probe />);
    await waitForListener();
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      valuePresentation: {
        version: 99,
        renderers: {
          'datazen.timestamp': {
            enabled: true,
            options: {
              interpretation: { kind: 'native' },
              format: 'iso',
              displayTimeZone: { kind: 'utc' },
            },
          },
        },
      },
    });

    fireSettingsChanged({});

    await vi.waitFor(() => {
      expect(useSettingsStore.getState().settings.valuePresentation).toEqual({
        version: 99,
        renderers: {
          'datazen.timestamp': {
            enabled: true,
            options: {
              interpretation: { kind: 'native' },
              format: 'iso',
              displayTimeZone: { kind: 'utc' },
            },
          },
        },
      });
    });
    // 同步只读不写：未知版本不被默认值写回覆盖。
    expect(mockSettingsCommands.saveSettings).not.toHaveBeenCalled();
  });
});
