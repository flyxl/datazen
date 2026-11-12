/**
 * ValuePresentationSettingsSection 对真实 settingsStore 的回归
 * （issue-32 T11-BUG-001，探针 P7-panel）。
 *
 * 本文件不用既有 Section 测试的 mock store：mock store 直接把
 * updater 作用于已含未知字段的 state，绕过了真实 store 在保存前
 * 用 parseStoredValuePresentationSettings 重读权威值的那一步，
 * 复现不了本 bug。真实链路是：v99 后端 → loadSettings（store 内
 * 为 parse 后的 { version: 99, renderers }）→ 面板改格式并保存 →
 * store 保存前重读权威原文作 fresh → 写回值必须保留 rules /
 * columnOverrides / 未知顶层字段。
 *
 * store 模块是单例：每测 vi.resetModules() 后动态 import，保证
 * loadSettings 代次与 hasLoadedSettings 不跨测泄漏。
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { ValuePresentationRegistryImpl } from '@datazen/extension-points';
import {
  columnOverrideKey,
  timestampContribution,
  type StoredValuePresentationSettings,
  type ValuePresentationRule,
} from '../../../lib/valuePresentation/index';

const mockSettingsCommands = {
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
  syncLanguageMenu: vi.fn().mockResolvedValue(undefined),
};

vi.mock('../../../commands/settings', () => ({
  settingsCommands: mockSettingsCommands,
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  emitCrossWindow: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../lib/themePackApply', () => ({
  applyThemePack: vi.fn().mockResolvedValue({ ok: true }),
  syncWebviewBackgroundFromTokens: vi.fn(),
}));

vi.mock('../../../lib/resolveUiLanguage', () => {
  const resolveUiLanguage = (locale: string) => {
    const normalized = locale.trim().toLowerCase().replace(/_/g, '-');
    if (normalized.startsWith('zh')) return 'zh-CN';
    if (normalized.startsWith('en')) return 'en';
    return 'en';
  };
  const normalizeSettingsLanguage = (language: string) => {
    const normalized = language.trim().replace(/_/g, '-');
    if (!normalized) return 'en';
    const lower = normalized.toLowerCase();
    if (lower === 'zh-cn' || lower.startsWith('zh')) return 'zh-CN';
    if (lower === 'en' || lower.startsWith('en')) return 'en';
    return normalized;
  };
  return { resolveUiLanguage, normalizeSettingsLanguage };
});

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key, language: 'en-US' }),
}));

type UnknownVersionValuePresentation = StoredValuePresentationSettings & {
  futureField: { kept: boolean };
};

const V99_RULE: ValuePresentationRule = {
  name: 'unix-ms',
  enabled: true,
  match: { column: { kind: 'exact', value: 'epoch_ms' } },
  options: {
    interpretation: { kind: 'unix', unit: 'milliseconds' },
    format: 'iso-tz',
    displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
  },
};

function v99Authoritative(): UnknownVersionValuePresentation {
  return {
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
    rules: [V99_RULE],
    columnOverrides: {
      [columnOverrideKey({
        connectionId: 'conn-1',
        database: 'shop',
        namespace: 'public',
        table: 'orders',
        column: 'epoch_ms',
      })]: { mode: 'raw' },
    },
    futureField: { kept: true },
  };
}

/** 打开一个 listbox Select（非原生 select）并点选指定 label 的选项。 */
function pickOption(testId: string, optionLabel: string) {
  fireEvent.click(screen.getByTestId(testId));
  fireEvent.mouseDown(screen.getByText(optionLabel));
}

describe('ValuePresentationSettingsSection with the real settingsStore', () => {
  let useSettingsStore: typeof import('../../../stores/settingsStore').useSettingsStore;
  let ValuePresentationSettingsSection: typeof import('../ValuePresentationSettingsSection').ValuePresentationSettingsSection;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    document.documentElement.classList.remove('dark');
    localStorage.clear();
    Object.defineProperty(window, 'matchMedia', {
      writable: true,
      value: vi.fn().mockImplementation((query: string) => ({
        matches: query.includes('dark'),
        media: query,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    });

    const storeMod = await import('../../../stores/settingsStore');
    useSettingsStore = storeMod.useSettingsStore;
    const sectionMod = await import('../ValuePresentationSettingsSection');
    ValuePresentationSettingsSection = sectionMod.ValuePresentationSettingsSection;
  });

  afterEach(() => {
    cleanup();
    document.documentElement.classList.remove('dark');
  });

  it('saving a format change on a v99 config keeps unknown-version fields (T11-BUG-001, P7-panel)', async () => {
    const authoritative = v99Authoritative();
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      valuePresentation: authoritative,
    });
    await useSettingsStore.getState().loadSettings();
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);

    const registry = new ValuePresentationRegistryImpl();
    registry.register(timestampContribution);
    render(<ValuePresentationSettingsSection registry={registry} />);
    // 草稿取自已加载的 v99 渲染器：已启用、iso / UTC。
    expect(screen.getByTestId('value-presentation-enable').getAttribute('aria-checked')).toBe(
      'true',
    );

    pickOption('value-presentation-format', 'settings.valuePresentation.format.locale');
    // 保存前重读读到的仍是同一份 v99 权威原文。
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      valuePresentation: authoritative,
    });
    fireEvent.click(screen.getByTestId('value-presentation-save'));

    await waitFor(() => expect(mockSettingsCommands.saveSettings).toHaveBeenCalled());
    const saved = mockSettingsCommands.saveSettings.mock.lastCall?.[0] as {
      valuePresentation: UnknownVersionValuePresentation;
    };
    expect(saved.valuePresentation.version).toBe(99);
    expect(saved.valuePresentation.renderers['datazen.timestamp']?.options.format).toBe('locale');
    // 面板只改了格式路径：未知版本字段原样保留。
    expect(saved.valuePresentation.rules).toEqual([V99_RULE]);
    expect(saved.valuePresentation.columnOverrides).toEqual(authoritative.columnOverrides);
    expect(saved.valuePresentation.futureField).toEqual({ kept: true });
  });
});
