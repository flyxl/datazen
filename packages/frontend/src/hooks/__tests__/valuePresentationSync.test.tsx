/**
 * valuePresentation 跨窗口同步与扩展停用回退（issue-32 T11，设计 §6/§7，
 * PRD §5/§6 验收 10/12）。
 *
 * 覆盖：同一快照驱动的两处消费者（grid 计划与 aside 计划）随 store
 * 变化同步更新；停用 datazen.timestamp 后两处同时回退宿主默认且
 * 配置保留原文；停用期间改规则、重启用后生效的是新规则（计划缓存
 * 因贡献修订失效重建，无旧缓存残留）；初次加载前两处统一用默认快照。
 *
 * 全部用全新 ValuePresentationRegistryImpl，不动全局单例；settingsStore
 * 用与 settingsStore.test.ts 相同的 mock（commands/settings、
 * crossWindowBus、themePackApply），状态直接经 setState 驱动——
 * 「另一窗口保存」在本文件内模拟为 store 值 + revision 变化（跨窗
 * 口传输本身由 useSettingsSync 测试覆盖）。
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { ValuePresentationRegistryImpl } from '@datazen/extension-points';
import { useValuePresentationSnapshot } from '../useValuePresentationSnapshot';
import { useValuePresentation } from '../useValuePresentation';
import { useSettingsStore } from '../../stores/settingsStore';
import {
  TIMESTAMP_CONTRIBUTION_ID,
  timestampContribution,
  type StoredValuePresentationSettings,
  type TimestampOptions,
  type ValuePresentationRule,
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

const SOURCE = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
} as const;

const COLUMNS = [
  { name: 'created_at', dataType: 'timestamp with time zone' },
  { name: 'epoch_ms', dataType: 'BIGINT' },
] as const;

const ISO_TZ_SHANGHAI: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso-tz',
  displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
};

const ISO_UTC: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso',
  displayTimeZone: { kind: 'utc' },
};

const EPOCH_MS_RULE: ValuePresentationRule = {
  name: 'unix-ms',
  enabled: true,
  match: { column: { kind: 'exact', value: 'epoch_ms' } },
  options: {
    interpretation: { kind: 'unix', unit: 'milliseconds' },
    format: 'iso-tz',
    displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
  },
};

function setStoredValuePresentation(
  valuePresentation: StoredValuePresentationSettings,
  revision: number,
): void {
  useSettingsStore.setState({
    settings: { ...useSettingsStore.getState().settings, valuePresentation },
    valuePresentationRevision: revision,
  });
}

function storedWithGlobal(options: TimestampOptions): StoredValuePresentationSettings {
  return {
    version: 1,
    renderers: { [TIMESTAMP_CONTRIBUTION_ID]: { enabled: true, options } },
  };
}

/** 同一快照驱动的两处消费者（grid 与 aside），与 PanelContentRenderer 的接线一致。 */
function useTwoConsumers(registry: ValuePresentationRegistryImpl) {
  const snapshot = useValuePresentationSnapshot(registry);
  const grid = useValuePresentation({
    columns: [...COLUMNS],
    source: SOURCE,
    snapshot,
    registry,
  });
  const aside = useValuePresentation({
    columns: [...COLUMNS],
    source: SOURCE,
    snapshot,
    registry,
  });
  return { snapshot, grid, aside };
}

describe('valuePresentation cross-window sync and contribution fallback (T11)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setStoredValuePresentation({ version: 1, renderers: {} }, 0);
    useSettingsStore.setState((state) => ({
      settings: { ...state.settings, language: 'en' },
    }));
  });

  it('uses the same default snapshot for both consumers before the first load', () => {
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    const { result } = renderHook(() => useTwoConsumers(registry));
    expect(result.current.snapshot.enabledRendererIds).toEqual([]);
    expect(result.current.grid.plans[0]?.status).toBe('disabled');
    expect(result.current.aside.plans[0]?.status).toBe('disabled');
    expect(result.current.grid.plans[0]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'disabled',
    });
    expect(result.current.aside.plans[0]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'disabled',
    });
  });

  it('updates both consumers together when another window saves', () => {
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    const { result } = renderHook(() => useTwoConsumers(registry));
    act(() => {
      setStoredValuePresentation(storedWithGlobal(ISO_TZ_SHANGHAI), 1);
    });
    expect(result.current.grid.plans[0]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });
    expect(result.current.aside.plans[0]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
    });

    // 另一窗口把全局格式改为 iso / UTC：两处同步变，无需重启。
    act(() => {
      setStoredValuePresentation(storedWithGlobal(ISO_UTC), 2);
    });
    expect(result.current.grid.plans[0]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T14:30:00Z',
      interpretation: 'native',
    });
    expect(result.current.aside.plans[0]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T14:30:00Z',
      interpretation: 'native',
    });
  });

  it('falls back in both places when the contribution is unregistered, keeping the stored config', () => {
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    const { result } = renderHook(() => useTwoConsumers(registry));
    act(() => {
      setStoredValuePresentation(storedWithGlobal(ISO_TZ_SHANGHAI), 1);
    });
    expect(result.current.grid.plans[0]?.status).toBe('ready');
    expect(result.current.aside.plans[0]?.status).toBe('ready');

    act(() => {
      registry.unregister(TIMESTAMP_CONTRIBUTION_ID);
    });

    // 时间列回退宿主默认（快照里无该贡献，计划报 disabled），数字列
    // 回退默认；两处一致，present 一律 unchanged。
    expect(result.current.snapshot.configs).toEqual({});
    expect(result.current.snapshot.enabledRendererIds).toEqual([]);
    expect(result.current.grid.plans[0]?.status).toBe('disabled');
    expect(result.current.aside.plans[0]?.status).toBe('disabled');
    expect(result.current.grid.plans[0]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'disabled',
    });
    expect(result.current.aside.plans[0]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'unchanged',
      reason: 'disabled',
    });
    expect(result.current.grid.plans[1]?.status).toBe('default');
    expect(result.current.aside.plans[1]?.status).toBe('default');
    // 配置保留原文，不被默认值写回覆盖。
    expect(useSettingsStore.getState().settings.valuePresentation).toEqual(
      storedWithGlobal(ISO_TZ_SHANGHAI),
    );
  });

  it('restores the saved config on re-register, and a rule saved while unregistered wins over the old plan', () => {
    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    const { result } = renderHook(() => useTwoConsumers(registry));
    act(() => {
      setStoredValuePresentation(storedWithGlobal(ISO_TZ_SHANGHAI), 1);
    });
    expect(result.current.grid.plans[1]?.present(1705316400000)).toEqual({
      kind: 'unchanged',
      reason: 'no-renderer',
    });

    // 停用后改规则（全局改回 iso / UTC，新增 unix-ms 规则）：两处仍回退。
    act(() => {
      registry.unregister(TIMESTAMP_CONTRIBUTION_ID);
    });
    act(() => {
      setStoredValuePresentation({ ...storedWithGlobal(ISO_UTC), rules: [EPOCH_MS_RULE] }, 2);
    });
    expect(result.current.grid.plans[0]?.status).toBe('disabled');
    expect(result.current.grid.plans[1]?.status).toBe('default');

    // 重新注册：已保存配置恢复生效；epoch_ms 列命中停用期间保存的新
    // 规则（不是旧计划缓存的无规则状态），created_at 列走新全局默认。
    act(() => {
      registry.register(timestampContribution);
    });
    expect(result.current.grid.plans[0]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T14:30:00Z',
      interpretation: 'native',
    });
    expect(result.current.aside.plans[0]?.present('2026-01-15T14:30:00Z')).toEqual({
      kind: 'text',
      text: '2026-01-15T14:30:00Z',
      interpretation: 'native',
    });
    expect(result.current.grid.plans[1]?.present(1705316400000)).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
    });
    expect(result.current.aside.plans[1]?.present(1705316400000)).toEqual({
      kind: 'text',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
    });
    expect(result.current.grid.plans[1]?.matchedRuleName).toBe('unix-ms');
    expect(result.current.aside.plans[1]?.matchedRuleName).toBe('unix-ms');
  });
});
