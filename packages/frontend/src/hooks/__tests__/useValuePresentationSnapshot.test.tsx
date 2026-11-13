/**
 * useValuePresentationSnapshot（issue-32 T06）：从 settingsStore 提供展示快照。
 *
 * renderHook + act 范式（见 useValuePresentation.test.tsx）。全部用全新
 * ValuePresentationRegistryImpl，不动全局单例；settingsStore 用与
 * settingsStore.test.ts 相同的 mock（commands/settings、crossWindowBus、
 * themePackApply），状态直接经 setState 驱动。
 *
 * 覆盖：默认存储值 → 空快照；store 变化重建；registry 注册/注销重建；
 * revision 递增透传；未知版本 → 空快照。
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { ValuePresentationRegistryImpl } from '@datazen/extension-points';
import { useValuePresentationSnapshot } from '../useValuePresentationSnapshot';
import { useSettingsStore } from '../../stores/settingsStore';
import {
  TIMESTAMP_CONTRIBUTION_ID,
  timestampContribution,
  type TimestampOptions,
} from '../../lib/valuePresentation/index';

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
  emitCrossWindow: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../lib/themePackApply', () => ({
  applyThemePack: vi.fn().mockResolvedValue({ ok: true }),
  syncWebviewBackgroundFromTokens: vi.fn(),
}));

const NATIVE_UTC_ISO: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso',
  displayTimeZone: { kind: 'utc' },
};

function setStoredValuePresentation(
  valuePresentation: {
    version: number;
    renderers: Record<string, { enabled: boolean; options: TimestampOptions }>;
  },
  revision: number,
): void {
  useSettingsStore.setState({
    settings: { ...useSettingsStore.getState().settings, valuePresentation },
    valuePresentationRevision: revision,
  });
}

describe('useValuePresentationSnapshot', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setStoredValuePresentation({ version: 1, renderers: {} }, 0);
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, language: 'en' },
    }));
  });

  it('returns an empty snapshot for the default (all-off) stored value', () => {
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    const { result } = renderHook(() => useValuePresentationSnapshot(registry));
    expect(result.current.revision).toBe(0);
    expect(result.current.environment.locale).toBe('en');
    expect(result.current.configs).toEqual({});
    expect(result.current.enabledRendererIds).toEqual([]);
    expect(result.current.overrides).toEqual({});
  });

  it('rebuilds when the store value changes', () => {
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    const { result } = renderHook(() => useValuePresentationSnapshot(registry));
    expect(result.current.enabledRendererIds).toEqual([]);

    act(() => {
      setStoredValuePresentation(
        {
          version: 1,
          renderers: {
            [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO },
          },
        },
        1,
      );
    });

    expect(result.current.revision).toBe(1);
    expect(result.current.enabledRendererIds).toEqual([TIMESTAMP_CONTRIBUTION_ID]);
    expect(result.current.configs[TIMESTAMP_CONTRIBUTION_ID]?.configVersion).toBe(1);
  });

  it('rebuilds when a contribution is registered or unregistered', () => {
    const registry = new ValuePresentationRegistryImpl();
    const { result } = renderHook(() => useValuePresentationSnapshot(registry));
    act(() => {
      setStoredValuePresentation(
        {
          version: 1,
          renderers: {
            [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO },
          },
        },
        1,
      );
    });
    // 贡献缺失：enabled 也不进 configs（missing-renderer 由计划构建器报）。
    expect(result.current.enabledRendererIds).toEqual([]);

    act(() => {
      registry.register(timestampContribution);
    });
    expect(result.current.enabledRendererIds).toEqual([TIMESTAMP_CONTRIBUTION_ID]);
    expect(result.current.configs[TIMESTAMP_CONTRIBUTION_ID]).toBeDefined();

    act(() => {
      registry.unregister(TIMESTAMP_CONTRIBUTION_ID);
    });
    expect(result.current.enabledRendererIds).toEqual([]);
    expect(result.current.configs).toEqual({});
  });

  it('passes the store revision through as the snapshot revision', () => {
    const registry = new ValuePresentationRegistryImpl();
    const { result } = renderHook(() => useValuePresentationSnapshot(registry));
    expect(result.current.revision).toBe(0);
    act(() => {
      setStoredValuePresentation({ version: 1, renderers: {} }, 5);
    });
    expect(result.current.revision).toBe(5);
  });

  it('falls back to revision 0 when the store has no revision counter (pre-T06 / mocked state)', () => {
    // 防御分支：旧 store / 局部 mock 的 settings 可能没有
    // valuePresentationRevision，hook 不应因此抛错或返回 undefined。
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    act(() => {
      useSettingsStore.setState({
        settings: {
          ...useSettingsStore.getState().settings,
          valuePresentation: {
            version: 1,
            renderers: {
              [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO },
            },
          },
        },
        valuePresentationRevision: undefined as unknown as number,
      });
    });
    const { result } = renderHook(() => useValuePresentationSnapshot(registry));
    expect(result.current.revision).toBe(0);
    expect(result.current.enabledRendererIds).toEqual([TIMESTAMP_CONTRIBUTION_ID]);
  });

  it('returns an empty snapshot for an unknown stored version', () => {
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    act(() => {
      setStoredValuePresentation(
        {
          version: 99,
          renderers: {
            [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options: NATIVE_UTC_ISO },
          },
        },
        2,
      );
    });
    const { result } = renderHook(() => useValuePresentationSnapshot(registry));
    expect(result.current.revision).toBe(2);
    expect(result.current.configs).toEqual({});
    expect(result.current.enabledRendererIds).toEqual([]);
  });
});
