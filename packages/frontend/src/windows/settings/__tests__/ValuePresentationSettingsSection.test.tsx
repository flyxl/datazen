/**
 * ValuePresentationSettingsSection（issue-32 T07）：设置 → 表格数据 → 时间显示。
 *
 * 覆盖：开关开/关写回 store、格式三档切换、时区三档（本地/UTC/IANA，
 * 非法 IANA 拒存）、预览双锚点逐字、Unix 秒/毫秒显式选择、非法配置禁存
 * 与就地报错、贡献缺失提示、未启用不改变既有行为（只改草稿不写 store、
 * 保存失败旧配置继续生效）、规则列表空态与未来 rules 字段防崩，
 * 以及纯预览/草稿辅助的分支。
 *
 * store 按 TableDataSettingsSection.test 的惯例 mock（只 mock 读到的
 * settings.valuePresentation 与写回的 updateValuePresentationSettings，
 * 后端行为是 store 自身测试的范围）；注册中心由 props 注入全新实例，
 * 不动全局单例；i18n mock 为 key 直通，language 固定 'en-US'。
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { ValuePresentationRegistryImpl } from '@datazen/extension-points';
import {
  timestampContribution,
  validateValuePresentationRule,
  type StoredRendererSettings,
  type StoredValuePresentationSettings,
  type TimestampOptions,
  type ValuePresentationRule,
} from '../../../lib/valuePresentation/index';
import {
  ValuePresentationSettingsSection,
  buildPreviewOptions,
  computeTimestampPreview,
  draftFromStored,
} from '../ValuePresentationSettingsSection';

const mocks = vi.hoisted(() => {
  const state: { valuePresentation: StoredValuePresentationSettings } = {
    valuePresentation: { version: 1, renderers: {} },
  };
  const updateValuePresentationSettings = vi.fn(async (next: StoredValuePresentationSettings) => {
    state.valuePresentation = next;
  });
  return { state, updateValuePresentationSettings };
});

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (
    sel: (s: {
      settings: { valuePresentation: StoredValuePresentationSettings };
      updateValuePresentationSettings: typeof mocks.updateValuePresentationSettings;
    }) => unknown,
  ) =>
    sel({
      settings: { valuePresentation: mocks.state.valuePresentation },
      updateValuePresentationSettings: mocks.updateValuePresentationSettings,
    }),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key, language: 'en-US' }),
}));

const TIMESTAMP_ID = 'datazen.timestamp';

function registryWithTimestamp(): ValuePresentationRegistryImpl {
  const registry = new ValuePresentationRegistryImpl();
  registry.register(timestampContribution);
  return registry;
}

function renderSection(registry: ValuePresentationRegistryImpl = registryWithTimestamp()) {
  return render(<ValuePresentationSettingsSection registry={registry} />);
}

/** 打开一个 listbox Select（非原生 select）并点选指定 label 的选项。 */
function pickOption(testId: string, optionLabel: string) {
  fireEvent.click(screen.getByTestId(testId));
  fireEvent.mouseDown(screen.getByText(optionLabel));
}

function previewResult(): string {
  return screen.getByTestId('value-presentation-preview-result').textContent ?? '';
}

async function clickSave() {
  fireEvent.click(screen.getByTestId('value-presentation-save'));
}

function setStored(value: StoredValuePresentationSettings) {
  mocks.state.valuePresentation = value;
}

function storedWith(
  renderers: StoredValuePresentationSettings['renderers'],
  version = 1,
): StoredValuePresentationSettings {
  return { version, renderers };
}

const NATIVE_ISO: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso',
  displayTimeZone: { kind: 'local' },
};

/** PRD §3.1 例规则（T08 UI 用例共用）。 */
const VALID_RULE_INPUT = {
  name: 'unix-ms',
  enabled: true,
  match: {
    column: { kind: 'regex', source: '^(created_at|updated_at)$' },
    databaseTypeName: { kind: 'exact', value: 'BIGINT' },
  },
  options: {
    interpretation: { kind: 'unix', unit: 'milliseconds' },
    format: 'iso-tz',
    displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
  },
};

function validRule(name = 'unix-ms'): ValuePresentationRule {
  const validated = validateValuePresentationRule({ ...VALID_RULE_INPUT, name });
  if (!validated.ok) throw new Error('test fixture invalid');
  return validated.rule;
}

describe('ValuePresentationSettingsSection', () => {
  beforeEach(() => {
    mocks.state.valuePresentation = { version: 1, renderers: {} };
    mocks.updateValuePresentationSettings.mockClear();
  });

  afterEach(() => {
    cleanup();
  });

  it('renders disabled by default and previews the native anchor in UTC ISO', () => {
    renderSection();
    expect(screen.getByTestId('value-presentation-enable').getAttribute('aria-checked')).toBe(
      'false',
    );
    // 默认：iso + 本地时区（测试环境为 UTC），原值与结果均为 UTC ISO。
    expect(screen.getByTestId('value-presentation-preview')).toHaveTextContent(
      '2026-01-15T14:30:00Z',
    );
    expect(previewResult()).toBe('2026-01-15T14:30:00Z');
    // 未保存前不写 store。
    expect(mocks.updateValuePresentationSettings).not.toHaveBeenCalled();
  });

  it('enabling and saving writes the renderer entry to the store', async () => {
    renderSection();
    fireEvent.click(screen.getByTestId('value-presentation-enable'));
    await clickSave();
    await waitFor(() =>
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledWith({
        version: 1,
        renderers: { [TIMESTAMP_ID]: { enabled: true, options: NATIVE_ISO } },
        rules: [],
      }),
    );
    expect(screen.getByTestId('value-presentation-saved')).toBeInTheDocument();
  });

  it('disabling and saving writes enabled:false and keeps the options', async () => {
    setStored(
      storedWith({
        [TIMESTAMP_ID]: {
          enabled: true,
          options: {
            interpretation: { kind: 'native' },
            format: 'iso-tz',
            displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
          },
        },
      }),
    );
    renderSection();
    expect(screen.getByTestId('value-presentation-enable').getAttribute('aria-checked')).toBe(
      'true',
    );
    fireEvent.click(screen.getByTestId('value-presentation-enable'));
    await clickSave();
    await waitFor(() =>
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledWith({
        version: 1,
        renderers: {
          [TIMESTAMP_ID]: {
            enabled: false,
            options: {
              interpretation: { kind: 'native' },
              format: 'iso-tz',
              displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
            },
          },
        },
        rules: [],
      }),
    );
  });

  it('switching formats updates the live preview (iso-tz, iso, locale)', () => {
    renderSection();
    pickOption('value-presentation-format', 'settings.valuePresentation.format.iso-tz');
    pickOption('value-presentation-timezone', 'settings.valuePresentation.timeZone.iana');
    fireEvent.change(screen.getByTestId('value-presentation-iana-name'), {
      target: { value: 'Asia/Shanghai' },
    });
    // PRD 双锚点之一：2026-01-15T14:30:00Z 在 Asia/Shanghai 下为 22:30+08:00。
    expect(previewResult()).toBe('2026-01-15T22:30:00+08:00');
    pickOption('value-presentation-format', 'settings.valuePresentation.format.iso');
    expect(previewResult()).toBe('2026-01-15T14:30:00Z');
    pickOption('value-presentation-format', 'settings.valuePresentation.format.locale');
    const localeResult = previewResult();
    expect(localeResult).toContain('2026');
    expect(localeResult).not.toBe('2026-01-15T14:30:00Z');
  });

  it('UTC time zone renders the native anchor with a +00:00 offset under iso-tz', () => {
    renderSection();
    pickOption('value-presentation-format', 'settings.valuePresentation.format.iso-tz');
    pickOption('value-presentation-timezone', 'settings.valuePresentation.timeZone.utc');
    expect(previewResult()).toBe('2026-01-15T14:30:00+00:00');
  });

  it('local time zone renders a preview (no crash, no hard-coded zone)', () => {
    renderSection();
    pickOption('value-presentation-format', 'settings.valuePresentation.format.iso-tz');
    // 默认即 local：结果是合法的 iso-tz 文本，精确值由运行环境时区决定。
    expect(previewResult()).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[+-]\d{2}:\d{2}$/);
  });

  it('Unix milliseconds preview shows the second anchor with a milliseconds badge', () => {
    renderSection();
    pickOption('value-presentation-format', 'settings.valuePresentation.format.iso-tz');
    pickOption('value-presentation-timezone', 'settings.valuePresentation.timeZone.iana');
    fireEvent.change(screen.getByTestId('value-presentation-iana-name'), {
      target: { value: 'Asia/Shanghai' },
    });
    pickOption(
      'value-presentation-preview-source',
      'settings.valuePresentation.preview.source.unix-milliseconds',
    );
    expect(screen.getByTestId('value-presentation-preview')).toHaveTextContent('1705316400000');
    expect(screen.getByTestId('value-presentation-preview-unix-badge')).toHaveTextContent(
      'settings.valuePresentation.preview.unixMilliseconds',
    );
    // PRD 双锚点之二：1705316400000 在 Asia/Shanghai 下为 2024-01-15T19:00:00+08:00。
    expect(previewResult()).toBe('2024-01-15T19:00:00+08:00');
  });

  it('Unix seconds preview shows the same instant with a seconds badge', () => {
    renderSection();
    pickOption('value-presentation-format', 'settings.valuePresentation.format.iso-tz');
    pickOption('value-presentation-timezone', 'settings.valuePresentation.timeZone.iana');
    fireEvent.change(screen.getByTestId('value-presentation-iana-name'), {
      target: { value: 'Asia/Shanghai' },
    });
    pickOption(
      'value-presentation-preview-source',
      'settings.valuePresentation.preview.source.unix-seconds',
    );
    expect(screen.getByTestId('value-presentation-preview')).toHaveTextContent('1705316400');
    expect(screen.getByTestId('value-presentation-preview-unix-badge')).toHaveTextContent(
      'settings.valuePresentation.preview.unixSeconds',
    );
    expect(previewResult()).toBe('2024-01-15T19:00:00+08:00');
  });

  it('text preview interprets the wall-clock sample as UTC', () => {
    renderSection();
    pickOption('value-presentation-format', 'settings.valuePresentation.format.iso-tz');
    pickOption('value-presentation-timezone', 'settings.valuePresentation.timeZone.iana');
    fireEvent.change(screen.getByTestId('value-presentation-iana-name'), {
      target: { value: 'Asia/Shanghai' },
    });
    pickOption(
      'value-presentation-preview-source',
      'settings.valuePresentation.preview.source.text',
    );
    // 墙上文本 2026-01-15 14:30:00 按 UTC 解释，转 Asia/Shanghai 为 22:30+08:00。
    expect(previewResult()).toBe('2026-01-15T22:30:00+08:00');
  });

  it('an invalid IANA name blocks saving with an inline error', async () => {
    renderSection();
    pickOption('value-presentation-timezone', 'settings.valuePresentation.timeZone.iana');
    fireEvent.change(screen.getByTestId('value-presentation-iana-name'), {
      target: { value: 'Not A Zone' },
    });
    // 行内即时提示非法 IANA 名；保存同样被拦截并就地报错。
    expect(screen.getByTestId('value-presentation-iana-error')).toHaveTextContent(
      'settings.valuePresentation.ianaInvalid',
    );
    await clickSave();
    expect(mocks.updateValuePresentationSettings).not.toHaveBeenCalled();
    expect(screen.getByTestId('value-presentation-error')).toHaveTextContent(
      'settings.valuePresentation.ianaInvalid',
    );
    // 预览同样报无效配置，而不是静默回退。
    expect(previewResult()).toContain('settings.valuePresentation.preview.invalid');
  });

  it('a well-formed but unknown IANA name blocks saving', async () => {
    renderSection();
    pickOption('value-presentation-timezone', 'settings.valuePresentation.timeZone.iana');
    fireEvent.change(screen.getByTestId('value-presentation-iana-name'), {
      target: { value: 'Mars/Olympus' },
    });
    expect(screen.getByTestId('value-presentation-iana-error')).toHaveTextContent(
      'settings.valuePresentation.ianaUnknown',
    );
    await clickSave();
    expect(mocks.updateValuePresentationSettings).not.toHaveBeenCalled();
    expect(screen.getByTestId('value-presentation-error')).toHaveTextContent(
      'settings.valuePresentation.ianaUnknown',
    );
  });

  it('a valid IANA name saves', async () => {
    renderSection();
    pickOption('value-presentation-timezone', 'settings.valuePresentation.timeZone.iana');
    fireEvent.change(screen.getByTestId('value-presentation-iana-name'), {
      target: { value: 'Asia/Shanghai' },
    });
    await clickSave();
    await waitFor(() =>
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledWith({
        version: 1,
        renderers: {
          [TIMESTAMP_ID]: {
            enabled: false,
            options: {
              interpretation: { kind: 'native' },
              format: 'iso',
              displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
            },
          },
        },
        rules: [],
      }),
    );
  });

  it('a save failure reports the error and keeps the old config in effect', async () => {
    renderSection();
    mocks.updateValuePresentationSettings.mockRejectedValueOnce(new Error('boom'));
    fireEvent.click(screen.getByTestId('value-presentation-enable'));
    await clickSave();
    await waitFor(() =>
      expect(screen.getByTestId('value-presentation-error')).toHaveTextContent(
        'settings.valuePresentation.saveFailed',
      ),
    );
    expect(mocks.updateValuePresentationSettings).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId('value-presentation-saved')).toBeNull();
    // 草稿保留（开关仍开着），旧配置未被改写。
    expect(screen.getByTestId('value-presentation-enable').getAttribute('aria-checked')).toBe(
      'true',
    );
    expect(mocks.state.valuePresentation).toEqual({ version: 1, renderers: {} });
  });

  it('editing the draft without saving does not write to the store', () => {
    renderSection();
    fireEvent.click(screen.getByTestId('value-presentation-enable'));
    pickOption('value-presentation-format', 'settings.valuePresentation.format.locale');
    expect(mocks.updateValuePresentationSettings).not.toHaveBeenCalled();
  });

  it('reset restores the saved draft', () => {
    setStored(
      storedWith({
        [TIMESTAMP_ID]: { enabled: true, options: NATIVE_ISO },
      }),
    );
    renderSection();
    fireEvent.click(screen.getByTestId('value-presentation-enable'));
    expect(screen.getByTestId('value-presentation-enable').getAttribute('aria-checked')).toBe(
      'false',
    );
    fireEvent.click(screen.getByTestId('value-presentation-reset'));
    expect(screen.getByTestId('value-presentation-enable').getAttribute('aria-checked')).toBe(
      'true',
    );
    expect(screen.getByTestId('value-presentation-save')).toBeDisabled();
  });

  it('missing contribution keeps the config, shows a notice, and still allows saving', async () => {
    setStored(
      storedWith({
        [TIMESTAMP_ID]: { enabled: true, options: NATIVE_ISO },
      }),
    );
    renderSection(new ValuePresentationRegistryImpl());
    expect(screen.getByTestId('value-presentation-contribution-missing')).toBeInTheDocument();
    expect(screen.getByTestId('value-presentation-enable').getAttribute('aria-checked')).toBe(
      'true',
    );
    expect(previewResult()).toBe('settings.valuePresentation.preview.unavailable');
    fireEvent.click(screen.getByTestId('value-presentation-enable'));
    await clickSave();
    await waitFor(() =>
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledWith({
        version: 1,
        renderers: { [TIMESTAMP_ID]: { enabled: false, options: NATIVE_ISO } },
        rules: [],
      }),
    );
  });

  it('rules list shows an empty state when the stored shape has no rules field', () => {
    renderSection();
    expect(screen.queryByTestId('value-presentation-rules-empty')).toBeNull();
    fireEvent.click(screen.getByTestId('value-presentation-rules-toggle'));
    expect(screen.getByTestId('value-presentation-rules-empty')).toHaveTextContent(
      'settings.valuePresentation.rules.empty',
    );
  });

  it('rules list renders valid stored rules and drops invalid entries', () => {
    setStored({
      version: 1,
      renderers: {},
      rules: [VALID_RULE_INPUT, { name: 'broken' }, {}],
    } as unknown as StoredValuePresentationSettings);
    renderSection();
    fireEvent.click(screen.getByTestId('value-presentation-rules-toggle'));
    expect(screen.getByTestId('value-presentation-rules-list')).toHaveTextContent('unix-ms');
    expect(screen.getByTestId('value-presentation-rules-list')).toHaveTextContent(
      'column=/^(created_at|updated_at)$/, type=BIGINT',
    );
    expect(screen.getByTestId('value-presentation-rules-list')).not.toHaveTextContent('broken');
  });

  it('a non-array future rules field degrades to the empty state', () => {
    setStored({
      version: 1,
      renderers: {},
      rules: 'not-an-array',
    } as unknown as StoredValuePresentationSettings);
    renderSection();
    fireEvent.click(screen.getByTestId('value-presentation-rules-toggle'));
    expect(screen.getByTestId('value-presentation-rules-empty')).toBeInTheDocument();
  });

  it('stored invalid options fall back to defaults in the draft', async () => {
    setStored(
      storedWith({
        [TIMESTAMP_ID]: {
          enabled: true,
          options: { format: 'bogus' },
        } as unknown as StoredRendererSettings,
      }),
    );
    renderSection();
    expect(screen.getByTestId('value-presentation-enable').getAttribute('aria-checked')).toBe(
      'true',
    );
    // 草稿回落到默认（iso + 本地时区），预览走默认值。
    expect(previewResult()).toBe('2026-01-15T14:30:00Z');
    // 改动后保存写回合法值，非法原文被替换。
    pickOption('value-presentation-format', 'settings.valuePresentation.format.iso-tz');
    await clickSave();
    await waitFor(() =>
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledWith({
        version: 1,
        renderers: {
          [TIMESTAMP_ID]: {
            enabled: true,
            options: {
              interpretation: { kind: 'native' },
              format: 'iso-tz',
              displayTimeZone: { kind: 'local' },
            },
          },
        },
        rules: [],
      }),
    );
  });
});

describe('draftFromStored / buildPreviewOptions (pure)', () => {
  it('draftFromStored returns the disabled default without an entry', () => {
    expect(draftFromStored({ version: 1, renderers: {} })).toEqual({
      enabled: false,
      options: {
        interpretation: { kind: 'native' },
        format: 'iso',
        displayTimeZone: { kind: 'local' },
      },
    });
  });

  it('buildPreviewOptions drops sourceTimeZone for unix and defaults it to UTC for text', () => {
    const withSource: TimestampOptions = {
      interpretation: { kind: 'native' },
      format: 'iso-tz',
      displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
      sourceTimeZone: { kind: 'iana', name: 'UTC' },
    };
    expect(buildPreviewOptions(withSource, 'unix-milliseconds')).toEqual({
      interpretation: { kind: 'unix', unit: 'milliseconds' },
      format: 'iso-tz',
      displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
    });
    expect(buildPreviewOptions(withSource, 'text')).toEqual({
      interpretation: { kind: 'text' },
      format: 'iso-tz',
      displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
      sourceTimeZone: { kind: 'iana', name: 'UTC' },
    });
    const withoutSource: TimestampOptions = {
      interpretation: { kind: 'native' },
      format: 'iso',
      displayTimeZone: { kind: 'local' },
    };
    expect(buildPreviewOptions(withoutSource, 'text')).toEqual({
      interpretation: { kind: 'text' },
      format: 'iso',
      displayTimeZone: { kind: 'local' },
      sourceTimeZone: { kind: 'utc' },
    });
    expect(buildPreviewOptions(withSource, 'native')).toEqual(withSource);
  });
});

describe('computeTimestampPreview (pure)', () => {
  const environment = { locale: 'en-US', localTimeZone: 'UTC' } as const;

  it('returns text for a valid native preview', () => {
    const result = computeTimestampPreview({
      contribution: timestampContribution,
      options: {
        interpretation: { kind: 'native' },
        format: 'iso-tz',
        displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
      },
      previewInterpretation: 'native',
      environment,
    });
    expect(result).toEqual({
      kind: 'text',
      text: '2026-01-15T22:30:00+08:00',
      interpretation: 'native',
      sample: '2026-01-15T14:30:00Z',
    });
  });

  it('returns unavailable when the contribution is missing', () => {
    const result = computeTimestampPreview({
      contribution: undefined,
      options: NATIVE_ISO,
      previewInterpretation: 'native',
      environment,
    });
    expect(result).toEqual({ kind: 'unavailable', sample: '2026-01-15T14:30:00Z' });
  });

  it('returns invalid for options the contribution rejects', () => {
    const result = computeTimestampPreview({
      contribution: timestampContribution,
      options: {
        interpretation: { kind: 'native' },
        format: 'iso',
        displayTimeZone: { kind: 'iana', name: 'Bad Name' },
      },
      previewInterpretation: 'native',
      environment,
    });
    expect(result.kind).toBe('invalid');
  });

  it('returns unchanged when formatting fails (empty locale)', () => {
    const result = computeTimestampPreview({
      contribution: timestampContribution,
      options: {
        interpretation: { kind: 'native' },
        format: 'locale',
        displayTimeZone: { kind: 'utc' },
      },
      previewInterpretation: 'native',
      environment: { locale: '', localTimeZone: 'UTC' },
    });
    expect(result).toEqual({
      kind: 'unchanged',
      reason: 'invalid-locale',
      sample: '2026-01-15T14:30:00Z',
    });
  });
});

describe('ValuePresentationSettingsSection rules editor (issue-32 T08)', () => {
  beforeEach(() => {
    mocks.state.valuePresentation = { version: 1, renderers: {} };
    mocks.updateValuePresentationSettings.mockClear();
  });

  afterEach(() => {
    cleanup();
  });

  function setField(testId: string, value: string) {
    fireEvent.change(screen.getByTestId(testId), { target: { value } });
  }

  function openRules() {
    fireEvent.click(screen.getByTestId('value-presentation-rules-toggle'));
  }

  function openNewRuleEditor() {
    openRules();
    fireEvent.click(screen.getByTestId('value-presentation-rule-add'));
  }

  /** 按 PRD 例填表：列名正则 + BIGINT，显示时区 Asia/Shanghai。 */
  function fillPrdRule() {
    setField('value-presentation-rule-name', 'unix-ms');
    setField('value-presentation-rule-column-value', '^(created_at|updated_at)$');
    setField('value-presentation-rule-type-value', 'BIGINT');
    setField('value-presentation-rule-display-timezone', 'iana');
    setField('value-presentation-rule-display-iana-name', 'Asia/Shanghai');
  }

  function fillSample(value: string, column: string, type = 'BIGINT') {
    setField('value-presentation-rule-sample-value', value);
    setField('value-presentation-rule-sample-column', column);
    setField('value-presentation-rule-sample-type', type);
  }

  it('adds a rule with a live sample preview and saves it to the store', async () => {
    renderSection();
    openNewRuleEditor();
    // 未填样本时仍允许配置，只提示无样本，不编造数据。
    expect(screen.getByTestId('value-presentation-rule-sample-preview')).toHaveTextContent(
      'settings.valuePresentation.rules.editor.sampleHint',
    );
    fillPrdRule();
    fillSample('1705316400000', 'created_at');
    expect(screen.getByTestId('value-presentation-rule-sample-match')).toHaveTextContent(
      'settings.valuePresentation.rules.editor.matchHit',
    );
    expect(screen.getByTestId('value-presentation-rule-sample-preview')).toHaveTextContent(
      '2024-01-15T19:00:00+08:00',
    );
    fireEvent.click(screen.getByTestId('value-presentation-rule-save'));
    expect(screen.queryByTestId('value-presentation-rule-editor')).toBeNull();
    expect(screen.getByTestId('value-presentation-rule-0')).toHaveTextContent('unix-ms');
    await clickSave();
    await waitFor(() =>
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          rules: [
            {
              name: 'unix-ms',
              enabled: true,
              match: {
                column: { kind: 'regex', source: '^(created_at|updated_at)$' },
                databaseTypeName: { kind: 'exact', value: 'BIGINT' },
              },
              options: {
                interpretation: { kind: 'unix', unit: 'milliseconds' },
                format: 'iso-tz',
                displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
              },
            },
          ],
        }),
      ),
    );
  });

  it('shows the sample miss state when the sample does not match', () => {
    renderSection();
    openNewRuleEditor();
    fillPrdRule();
    fillSample('1705316400000', 'other_col');
    expect(screen.getByTestId('value-presentation-rule-sample-match')).toHaveTextContent(
      'settings.valuePresentation.rules.editor.matchMiss',
    );
  });

  it('rejects an invalid regex in the editor and keeps the editor open', () => {
    renderSection();
    openNewRuleEditor();
    setField('value-presentation-rule-name', 'bad-regex');
    setField('value-presentation-rule-column-value', '(a+)+');
    fireEvent.click(screen.getByTestId('value-presentation-rule-save'));
    expect(screen.getByTestId('value-presentation-rule-error')).toHaveTextContent(
      'settings.valuePresentation.rules.editor.invalidRule',
    );
    expect(screen.getByTestId('value-presentation-rule-editor')).toBeInTheDocument();
    expect(screen.queryByTestId('value-presentation-rules-list')).toBeNull();
  });

  it('rejects a rule with no match condition', () => {
    renderSection();
    openNewRuleEditor();
    setField('value-presentation-rule-name', 'no-match');
    fireEvent.click(screen.getByTestId('value-presentation-rule-save'));
    expect(screen.getByTestId('value-presentation-rule-error')).toHaveTextContent(
      'settings.valuePresentation.rules.editor.invalidRule',
    );
    expect(screen.getByTestId('value-presentation-rule-error')).toHaveTextContent('match');
  });

  it('rejects a duplicate rule name', () => {
    setStored({ version: 1, renderers: {}, rules: [validRule()] });
    renderSection();
    openNewRuleEditor();
    fillPrdRule();
    fireEvent.click(screen.getByTestId('value-presentation-rule-save'));
    expect(screen.getByTestId('value-presentation-rule-error')).toHaveTextContent(
      'settings.valuePresentation.rules.editor.duplicateName',
    );
    expect(screen.getByTestId('value-presentation-rule-editor')).toBeInTheDocument();
  });

  it('edits an existing rule', async () => {
    setStored({ version: 1, renderers: {}, rules: [validRule()] });
    renderSection();
    openRules();
    fireEvent.click(screen.getByTestId('value-presentation-rule-edit-0'));
    setField('value-presentation-rule-name', 'unix-ms-v2');
    fireEvent.click(screen.getByTestId('value-presentation-rule-save'));
    expect(screen.getByTestId('value-presentation-rule-0')).toHaveTextContent('unix-ms-v2');
    await clickSave();
    await waitFor(() =>
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledWith(
        expect.objectContaining({ rules: [expect.objectContaining({ name: 'unix-ms-v2' })] }),
      ),
    );
  });

  it('deletes a rule and saves the empty list', async () => {
    setStored({ version: 1, renderers: {}, rules: [validRule()] });
    renderSection();
    openRules();
    fireEvent.click(screen.getByTestId('value-presentation-rule-delete-0'));
    expect(screen.getByTestId('value-presentation-rules-empty')).toBeInTheDocument();
    await clickSave();
    await waitFor(() =>
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledWith(
        expect.objectContaining({ rules: [] }),
      ),
    );
  });

  it('disabling a rule keeps it listed and saves enabled:false', async () => {
    setStored({ version: 1, renderers: {}, rules: [validRule()] });
    renderSection();
    openRules();
    fireEvent.click(screen.getByTestId('value-presentation-rule-toggle-0'));
    expect(screen.getByTestId('value-presentation-rule-0')).toHaveTextContent(
      'settings.valuePresentation.rules.disabled',
    );
    await clickSave();
    await waitFor(() =>
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledWith(
        expect.objectContaining({ rules: [expect.objectContaining({ enabled: false })] }),
      ),
    );
  });

  it('reorders rules with up/down', async () => {
    setStored({
      version: 1,
      renderers: {},
      rules: [validRule('first'), validRule('second')],
    });
    renderSection();
    openRules();
    expect(screen.getByTestId('value-presentation-rule-0')).toHaveTextContent('first');
    fireEvent.click(screen.getByTestId('value-presentation-rule-down-0'));
    expect(screen.getByTestId('value-presentation-rule-0')).toHaveTextContent('second');
    expect(screen.getByTestId('value-presentation-rule-1')).toHaveTextContent('first');
    await clickSave();
    await waitFor(() =>
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          rules: [
            expect.objectContaining({ name: 'second' }),
            expect.objectContaining({ name: 'first' }),
          ],
        }),
      ),
    );
  });

  it('a save failure reports the error and keeps the rule draft', async () => {
    renderSection();
    openNewRuleEditor();
    fillPrdRule();
    fireEvent.click(screen.getByTestId('value-presentation-rule-save'));
    mocks.updateValuePresentationSettings.mockRejectedValueOnce(new Error('boom'));
    await clickSave();
    await waitFor(() =>
      expect(screen.getByTestId('value-presentation-error')).toHaveTextContent(
        'settings.valuePresentation.saveFailed',
      ),
    );
    expect(screen.queryByTestId('value-presentation-saved')).toBeNull();
    // 草稿保留：规则仍在列表里。
    expect(screen.getByTestId('value-presentation-rule-0')).toHaveTextContent('unix-ms');
  });

  it('fills every editor field and saves the full rule', async () => {
    renderSection();
    openNewRuleEditor();
    setField('value-presentation-rule-name', 'full-rule');
    fireEvent.click(screen.getByTestId('value-presentation-rule-enabled'));
    setField('value-presentation-rule-connection', 'conn-1');
    setField('value-presentation-rule-database', 'shop');
    setField('value-presentation-rule-namespace', 'public');
    setField('value-presentation-rule-table-mode', 'regex');
    setField('value-presentation-rule-table-value', '^orders$');
    fireEvent.click(screen.getByTestId('value-presentation-rule-table-ignore-case'));
    setField('value-presentation-rule-column-mode', 'regex');
    setField('value-presentation-rule-column-value', '^(created_at|updated_at)$');
    fireEvent.click(screen.getByTestId('value-presentation-rule-column-ignore-case'));
    setField('value-presentation-rule-type-mode', 'regex');
    setField('value-presentation-rule-type-value', '^bigint$');
    fireEvent.click(screen.getByTestId('value-presentation-rule-type-ignore-case'));
    setField('value-presentation-rule-interpretation', 'text');
    setField('value-presentation-rule-format', 'locale');
    setField('value-presentation-rule-display-timezone', 'utc');
    setField('value-presentation-rule-source-timezone', 'iana');
    setField('value-presentation-rule-source-iana-name', 'Asia/Shanghai');
    setField('value-presentation-rule-sample-value', '2026-01-15 14:30:00');
    setField('value-presentation-rule-sample-column', 'created_at');
    setField('value-presentation-rule-sample-type', 'BIGINT');
    setField('value-presentation-rule-sample-table', 'orders');
    setField('value-presentation-rule-sample-connection', 'conn-1');
    setField('value-presentation-rule-sample-database', 'shop');
    setField('value-presentation-rule-sample-namespace', 'public');
    // 文本解释不适用于 number 列：命中但 present 回退 unchanged。
    expect(screen.getByTestId('value-presentation-rule-sample-match')).toHaveTextContent(
      'settings.valuePresentation.rules.editor.matchHit',
    );
    expect(screen.getByTestId('value-presentation-rule-sample-preview')).toHaveTextContent(
      'settings.valuePresentation.preview.unchanged',
    );
    fireEvent.click(screen.getByTestId('value-presentation-rule-save'));
    expect(screen.queryByTestId('value-presentation-rule-editor')).toBeNull();
    await clickSave();
    await waitFor(() =>
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          rules: [
            {
              name: 'full-rule',
              enabled: false,
              match: {
                connectionId: 'conn-1',
                database: 'shop',
                namespace: 'public',
                table: { kind: 'regex', source: '^orders$', flags: 'i' },
                column: { kind: 'regex', source: '^(created_at|updated_at)$', flags: 'i' },
                databaseTypeName: { kind: 'regex', source: '^bigint$', flags: 'i' },
              },
              options: {
                interpretation: { kind: 'text' },
                format: 'locale',
                displayTimeZone: { kind: 'utc' },
                sourceTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
              },
            },
          ],
        }),
      ),
    );
  });

  it('previews a matched text rule through the wall-clock path', () => {
    renderSection();
    openNewRuleEditor();
    setField('value-presentation-rule-name', 'text-rule');
    setField('value-presentation-rule-column-value', 'start_at');
    setField('value-presentation-rule-type-value', 'TIMESTAMP');
    setField('value-presentation-rule-interpretation', 'text');
    setField('value-presentation-rule-format', 'iso-tz');
    setField('value-presentation-rule-display-timezone', 'utc');
    setField('value-presentation-rule-source-timezone', 'iana');
    setField('value-presentation-rule-source-iana-name', 'Asia/Shanghai');
    setField('value-presentation-rule-sample-value', '2026-01-15 14:30:00');
    setField('value-presentation-rule-sample-column', 'start_at');
    setField('value-presentation-rule-sample-type', 'TIMESTAMP');
    expect(screen.getByTestId('value-presentation-rule-sample-match')).toHaveTextContent(
      'settings.valuePresentation.rules.editor.matchHit',
    );
    expect(screen.getByTestId('value-presentation-rule-sample-preview')).toHaveTextContent(
      '2026-01-15T06:30:00+00:00',
    );
  });

  it('shows the sample preview as unavailable when the contribution is missing', () => {
    renderSection(new ValuePresentationRegistryImpl());
    openNewRuleEditor();
    fillPrdRule();
    fillSample('1705316400000', 'created_at');
    expect(screen.getByTestId('value-presentation-rule-sample-preview')).toHaveTextContent(
      'settings.valuePresentation.preview.unavailable',
    );
  });

  it('disables adding at the rule limit and shows the limits note', () => {
    const rules = Array.from({ length: 20 }, (_, i) => validRule(`rule-${i}`));
    setStored({ version: 1, renderers: {}, rules });
    renderSection();
    openRules();
    expect(screen.getByTestId('value-presentation-rule-add')).toBeDisabled();
    expect(screen.getByText('settings.valuePresentation.rules.limits')).toBeInTheDocument();
  });
});

describe('ValuePresentationRuleEditor pure preview (issue-32 T08)', () => {
  it('computeRuleSamplePreview covers no-sample / miss / unavailable / unchanged / matched', async () => {
    const { computeRuleSamplePreview } = await import('../ValuePresentationRuleEditor');
    const rule = validRule();
    const environment = { locale: 'en-US', localTimeZone: 'UTC' };
    const base = {
      rule,
      sampleColumn: 'created_at',
      sampleTypeName: 'BIGINT',
      contribution: timestampContribution,
      environment,
    };
    expect(computeRuleSamplePreview({ ...base, sampleValue: '' })).toEqual({ kind: 'no-sample' });
    expect(computeRuleSamplePreview({ ...base, sampleValue: '1705316400000' })).toEqual({
      kind: 'matched',
      text: '2024-01-15T19:00:00+08:00',
      interpretation: 'unix',
    });
    expect(
      computeRuleSamplePreview({ ...base, sampleValue: '1705316400000', sampleColumn: 'other' }),
    ).toEqual({ kind: 'not-matched' });
    expect(
      computeRuleSamplePreview({ ...base, sampleValue: '1705316400000', contribution: undefined }),
    ).toEqual({ kind: 'unavailable' });
    const unchanged = computeRuleSamplePreview({ ...base, sampleValue: 'not-a-number' });
    expect(unchanged.kind).toBe('matched-unchanged');
  });
});
