/**
 * ColumnPresentationDialog（issue-32 T09）：列配置面板。
 *
 * 覆盖：原值 + 预览逐字双锚点（Unix 毫秒 1705316400000 →
 * 2024-01-15T19:00:00+08:00）、Unix 秒与毫秒显式区分、原始值阻断
 * 命中规则的列、跟随规则清除覆盖、总开关关闭提示且不静默开启
 * （显式开启走回调 / 默认写 store 链路）、无来源查询列禁用保存
 * 并说明原因、非法 IANA 禁存就地报错、贡献缺失提示、草稿辅助
 * 纯函数分支。
 *
 * i18n mock 为 key 直通（T07 惯例）；注册中心由 props 注入全新实例；
 * settingsStore 按 T07 惯例 mock（只 mock 读到的 valuePresentation 与
 * 写回的 updateValuePresentationSettings），并补 getState 供默认开启
 * 路径使用。
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { ValuePresentationRegistryImpl, type ColumnSource } from '@datazen/extension-points';
import {
  TIMESTAMP_CONTRIBUTION_ID,
  columnOverrideKey,
  resetColumnPresentationSession,
  getColumnPresentationSessionState,
  timestampContribution,
  timestampOptionsToConfigValue,
  type StoredValuePresentationSettings,
  type TimestampOptions,
  type ValuePresentationRule,
  type ValuePresentationSnapshot,
} from '../../../lib/valuePresentation/index';
import {
  ColumnPresentationDialog,
  draftFromColumnOverride,
  draftToTimestampOptions,
  validateColumnPresentationDraft,
} from '../ColumnPresentationDialog';

const mocks = vi.hoisted(() => {
  const state: { valuePresentation: StoredValuePresentationSettings } = {
    valuePresentation: { version: 1, renderers: {} },
  };
  const updateValuePresentationSettings = vi.fn(async (next: StoredValuePresentationSettings) => {
    state.valuePresentation = next;
  });
  return { state, updateValuePresentationSettings };
});

vi.mock('../../../stores/settingsStore', () => {
  const storeState = () => ({
    settings: { valuePresentation: mocks.state.valuePresentation },
    updateValuePresentationSettings: mocks.updateValuePresentationSettings,
  });
  const useSettingsStore = Object.assign(
    (sel: (s: ReturnType<typeof storeState>) => unknown) => sel(storeState()),
    { getState: storeState },
  );
  return { useSettingsStore };
});

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key, language: 'en-US' }),
}));

const SOURCE: Readonly<Omit<ColumnSource, 'column'>> = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
};

const ENVIRONMENT = { locale: 'en-US', localTimeZone: 'Asia/Shanghai' } as const;

const GLOBAL_OPTIONS: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso-tz',
  displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
};

function makeSnapshot(
  overrides: ValuePresentationSnapshot['overrides'] = {},
  rules: ValuePresentationSnapshot['rules'] = [],
  enabled = true,
): ValuePresentationSnapshot {
  return {
    revision: 1,
    environment: ENVIRONMENT,
    configs: {
      [TIMESTAMP_CONTRIBUTION_ID]: {
        configVersion: 1,
        config: timestampOptionsToConfigValue(GLOBAL_OPTIONS),
      },
    },
    enabledRendererIds: enabled ? [TIMESTAMP_CONTRIBUTION_ID] : [],
    overrides,
    rules,
  };
}

const RULE: ValuePresentationRule = {
  name: 'unix-ms',
  enabled: true,
  match: { column: { kind: 'exact', value: 'epoch_ms' } },
  options: {
    interpretation: { kind: 'unix', unit: 'milliseconds' },
    format: 'iso-tz',
    displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
  },
};

function registryWithTimestamp(): ValuePresentationRegistryImpl {
  const registry = new ValuePresentationRegistryImpl();
  registry.register(timestampContribution);
  return registry;
}

interface RenderDialogOptions {
  snapshot?: ValuePresentationSnapshot;
  source?: Readonly<Omit<ColumnSource, 'column'>>;
  /** True ⇒ render with no source (query result, no reliable table source). */
  sourceless?: boolean;
  columnName?: string;
  columnType?: string;
  sampleValue?: number | string;
  onSave?: (override: unknown) => void;
  onEnableTimeDisplay?: () => void | Promise<void>;
  registry?: ValuePresentationRegistryImpl;
}

function renderDialog(options: RenderDialogOptions = {}) {
  const onSave = options.onSave ?? vi.fn();
  const onClose = vi.fn();
  const onEnableTimeDisplay = options.onEnableTimeDisplay;
  render(
    <ColumnPresentationDialog
      open
      columnName={options.columnName ?? 'epoch_ms'}
      columnType={options.columnType ?? 'bigint'}
      source={options.sourceless ? undefined : (options.source ?? SOURCE)}
      snapshot={options.snapshot ?? makeSnapshot()}
      sampleValue={options.sampleValue ?? 1705316400000}
      registry={options.registry ?? registryWithTimestamp()}
      onSave={onSave}
      onClose={onClose}
      onEnableTimeDisplay={onEnableTimeDisplay}
    />,
  );
  return { onSave, onClose, onEnableTimeDisplay };
}

function previewResult(): string {
  return screen.getByTestId('column-presentation-preview-result').textContent ?? '';
}

function originalValue(): string {
  return screen.getByTestId('column-presentation-original').textContent ?? '';
}

function chooseMode(mode: 'inherit' | 'raw' | 'custom'): void {
  fireEvent.click(screen.getByTestId(`column-presentation-mode-${mode}`));
}

function chooseInterpretation(value: string): void {
  fireEvent.change(screen.getByTestId('column-presentation-interpretation'), {
    target: { value },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  resetColumnPresentationSession();
  mocks.state.valuePresentation = { version: 1, renderers: {} };
});

afterEach(cleanup);

describe('ColumnPresentationDialog preview', () => {
  it('shows the original value and the exact preview (PRD §3.2 anchor)', () => {
    renderDialog();
    chooseMode('custom');
    expect(originalValue()).toBe('1705316400000');
    expect(previewResult()).toBe('2024-01-15T19:00:00+08:00');
  });

  it('keeps Unix seconds and milliseconds explicitly distinct for the same digits', () => {
    renderDialog();
    chooseMode('custom');
    expect(previewResult()).toBe('2024-01-15T19:00:00+08:00');
    chooseInterpretation('unix-seconds');
    const secondsPreview = previewResult();
    expect(secondsPreview).not.toBe('2024-01-15T19:00:00+08:00');
    expect(secondsPreview).toBe('+56009-04-29T16:00:00+08:00');
    expect(screen.getByTestId('column-presentation-preview-unix-badge').textContent).toBe(
      'settings.valuePresentation.preview.unixSeconds',
    );
    chooseInterpretation('unix-milliseconds');
    expect(previewResult()).toBe('2024-01-15T19:00:00+08:00');
  });

  it('raw blocks a matching rule: the preview still shows the raw value', () => {
    const snapshot = makeSnapshot({}, [RULE]);
    renderDialog({ snapshot });
    // Inherit follows the rule first (sanity for the blocking claim).
    expect(previewResult()).toBe('2024-01-15T19:00:00+08:00');
    chooseMode('raw');
    expect(previewResult()).toContain('columnPresentation.preview.unchanged');
    expect(previewResult()).toContain('raw-override');
    expect(originalValue()).toBe('1705316400000');
  });

  it('shows the matched rule name as the effective source while inheriting', () => {
    renderDialog({ snapshot: makeSnapshot({}, [RULE]) });
    expect(screen.getByTestId('column-presentation-effective-source').textContent).toContain(
      'columnPresentation.source.rule',
    );
  });

  it('inherit previews the rule value and source when a stored override is pending removal (T09-BUG-001)', () => {
    const isoRule: ValuePresentationRule = {
      name: 'iso-rule',
      enabled: true,
      match: { column: { kind: 'exact', value: 'epoch_ms' } },
      options: {
        interpretation: { kind: 'unix', unit: 'milliseconds' },
        format: 'iso',
        displayTimeZone: { kind: 'utc' },
      },
    };
    const snapshot = makeSnapshot(
      {
        [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: {
          mode: 'custom',
          rendererId: TIMESTAMP_CONTRIBUTION_ID,
          options: {
            interpretation: { kind: 'unix', unit: 'milliseconds' },
            format: 'iso-tz',
            displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
          },
        },
      },
      [isoRule],
    );
    renderDialog({ snapshot });
    // The draft starts from the stored custom override.
    expect(
      (screen.getByTestId('column-presentation-mode-custom') as HTMLInputElement).checked,
    ).toBe(true);
    expect(previewResult()).toBe('2024-01-15T19:00:00+08:00');
    expect(screen.getByTestId('column-presentation-effective-source').textContent).toContain(
      'columnPresentation.source.override',
    );
    // Switching to inherit previews the rule value and source immediately,
    // before saving.
    chooseMode('inherit');
    expect(previewResult()).toBe('2024-01-15T11:00:00Z');
    expect(screen.getByTestId('column-presentation-effective-source').textContent).toContain(
      'columnPresentation.source.rule',
    );
  });

  it('previews wall-clock text with an explicit source time zone', () => {
    renderDialog({
      columnName: 'ts',
      columnType: 'timestamp without time zone',
      sampleValue: '2026-01-15 14:30:00',
    });
    chooseMode('custom');
    chooseInterpretation('text');
    // No source time zone yet: the runtime refuses to guess (unchanged).
    expect(previewResult()).toContain('missing-source-time-zone');
    fireEvent.change(screen.getByTestId('column-presentation-source-timezone'), {
      target: { value: 'utc' },
    });
    expect(previewResult()).toBe('2026-01-15T22:30:00+08:00');
  });

  it('shows a no-sample state for a text column with no value to preview', () => {
    render(
      <ColumnPresentationDialog
        open
        columnName="name"
        columnType="varchar"
        source={SOURCE}
        snapshot={makeSnapshot()}
        registry={registryWithTimestamp()}
        onSave={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByTestId('column-presentation-preview-result').textContent).toContain(
      'columnPresentation.preview.noSample',
    );
  });

  it('restores the mode from an explicit currentOverride prop', () => {
    render(
      <ColumnPresentationDialog
        open
        columnName="epoch_ms"
        columnType="bigint"
        source={SOURCE}
        snapshot={makeSnapshot()}
        sampleValue={1705316400000}
        currentOverride={{ mode: 'raw' }}
        registry={registryWithTimestamp()}
        onSave={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect((screen.getByTestId('column-presentation-mode-raw') as HTMLInputElement).checked).toBe(
      true,
    );
  });

  it('reports the preview as unavailable when the contribution is missing', () => {
    renderDialog({ registry: new ValuePresentationRegistryImpl() });
    chooseMode('custom');
    expect(screen.getByTestId('column-presentation-contribution-missing')).toBeTruthy();
    expect(previewResult()).toContain('settings.valuePresentation.preview.unavailable');
  });
});

describe('ColumnPresentationDialog save', () => {
  it('saves a custom Unix-milliseconds override through onSave', () => {
    const { onSave } = renderDialog();
    chooseMode('custom');
    fireEvent.click(screen.getByTestId('column-presentation-save'));
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith({
      mode: 'custom',
      rendererId: 'datazen.timestamp',
      options: {
        interpretation: { kind: 'unix', unit: 'milliseconds' },
        format: 'iso-tz',
        displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
      },
    });
  });

  it('inherit clears the explicit override (and the session entry by default)', () => {
    const snapshot = makeSnapshot({
      [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: {
        mode: 'custom',
        rendererId: TIMESTAMP_CONTRIBUTION_ID,
        options: {
          interpretation: { kind: 'unix', unit: 'milliseconds' },
          format: 'iso-tz',
          displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
        },
      },
    });
    const { onSave } = renderDialog({ snapshot });
    expect(
      (screen.getByTestId('column-presentation-mode-custom') as HTMLInputElement).checked,
    ).toBe(true);
    chooseMode('inherit');
    fireEvent.click(screen.getByTestId('column-presentation-save'));
    expect(onSave).toHaveBeenCalledWith({ mode: 'inherit' });
  });

  it('writes the override to the session store when no onSave is given', () => {
    render(
      <ColumnPresentationDialog
        open
        columnName="epoch_ms"
        columnType="bigint"
        source={SOURCE}
        snapshot={makeSnapshot()}
        sampleValue={1705316400000}
        registry={registryWithTimestamp()}
        onClose={vi.fn()}
      />,
    );
    chooseMode('raw');
    fireEvent.click(screen.getByTestId('column-presentation-save'));
    expect(
      getColumnPresentationSessionState().overrides[
        columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })
      ],
    ).toEqual({ mode: 'raw' });
  });

  it('refuses to save an invalid IANA name and reports the error in place', () => {
    const { onSave } = renderDialog();
    chooseMode('custom');
    fireEvent.change(screen.getByTestId('column-presentation-display-timezone'), {
      target: { value: 'iana' },
    });
    fireEvent.change(screen.getByTestId('column-presentation-display-iana-name'), {
      target: { value: 'not a zone' },
    });
    fireEvent.click(screen.getByTestId('column-presentation-save'));
    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByTestId('column-presentation-error').textContent).toContain(
      'settings.valuePresentation.invalidConfig',
    );
    // A format-valid but unknown zone is refused too.
    fireEvent.change(screen.getByTestId('column-presentation-display-iana-name'), {
      target: { value: 'Mars/Olympus' },
    });
    fireEvent.click(screen.getByTestId('column-presentation-save'));
    expect(onSave).not.toHaveBeenCalled();
  });
});

describe('ColumnPresentationDialog persistent save (issue-32 T10)', () => {
  function renderDialogWithoutOnSave() {
    const onClose = vi.fn();
    render(
      <ColumnPresentationDialog
        open
        columnName="epoch_ms"
        columnType="bigint"
        source={SOURCE}
        snapshot={makeSnapshot()}
        sampleValue={1705316400000}
        registry={registryWithTimestamp()}
        onClose={onClose}
      />,
    );
    return { onClose };
  }

  it('persists the override and clears the session entry on success', async () => {
    const { onClose } = renderDialogWithoutOnSave();
    chooseMode('raw');
    fireEvent.click(screen.getByTestId('column-presentation-save'));
    await waitFor(() => {
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledTimes(1);
    });
    const next = mocks.updateValuePresentationSettings.mock.calls[0]![0];
    expect(next.columnOverrides).toEqual({
      [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: { mode: 'raw' },
    });
    await waitFor(() => {
      expect(getColumnPresentationSessionState().overrides).toEqual({});
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('keeps the session entry, reports the failure in place and stays open when saving fails', async () => {
    const { onClose } = renderDialogWithoutOnSave();
    mocks.updateValuePresentationSettings.mockRejectedValueOnce(new Error('disk full'));
    chooseMode('raw');
    fireEvent.click(screen.getByTestId('column-presentation-save'));
    await waitFor(() => {
      expect(screen.getByTestId('column-presentation-error').textContent).toContain(
        'settings.valuePresentation.saveFailed',
      );
    });
    expect(onClose).not.toHaveBeenCalled();
    // 会话条目保留：用户刚改的配置不丢，旧配置继续生效。
    expect(
      getColumnPresentationSessionState().overrides[
        columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })
      ],
    ).toEqual({ mode: 'raw' });
    // 重试成功后关闭，会话条目随之清除。
    fireEvent.click(screen.getByTestId('column-presentation-save'));
    await waitFor(() => {
      expect(onClose).toHaveBeenCalledTimes(1);
    });
    await waitFor(() => {
      expect(getColumnPresentationSessionState().overrides).toEqual({});
    });
  });

  it('deletes the stored entry when inherit is saved by default', async () => {
    mocks.state.valuePresentation = {
      version: 1,
      renderers: {},
      columnOverrides: {
        [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: {
          mode: 'raw',
        },
      },
    };
    const onClose = vi.fn();
    render(
      <ColumnPresentationDialog
        open
        columnName="epoch_ms"
        columnType="bigint"
        source={SOURCE}
        snapshot={makeSnapshot({
          [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: { mode: 'raw' },
        })}
        sampleValue={1705316400000}
        registry={registryWithTimestamp()}
        onClose={onClose}
      />,
    );
    chooseMode('inherit');
    fireEvent.click(screen.getByTestId('column-presentation-save'));
    await waitFor(() => {
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledTimes(1);
    });
    const next = mocks.updateValuePresentationSettings.mock.calls[0]![0];
    expect(next.columnOverrides).toEqual({});
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('clears the session entry when saving an unchanged stored override (T10-BUG-001)', async () => {
    mocks.state.valuePresentation = {
      version: 1,
      renderers: {},
      columnOverrides: {
        [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: { mode: 'raw' },
      },
    };
    const onClose = vi.fn();
    render(
      <ColumnPresentationDialog
        open
        columnName="epoch_ms"
        columnType="bigint"
        source={SOURCE}
        snapshot={makeSnapshot({
          [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: { mode: 'raw' },
        })}
        sampleValue={1705316400000}
        registry={registryWithTimestamp()}
        onClose={onClose}
      />,
    );
    // 不改动直接保存：草稿即已存 raw 覆盖。
    fireEvent.click(screen.getByTestId('column-presentation-save'));
    await waitFor(() => {
      expect(onClose).toHaveBeenCalledTimes(1);
    });
    // 无变化不写持久化，会话条目仍须清除。
    expect(mocks.updateValuePresentationSettings).not.toHaveBeenCalled();
    expect(getColumnPresentationSessionState().overrides).toEqual({});
  });
});

describe('ColumnPresentationDialog total switch off', () => {
  it('warns, offers an explicit enable, and save does not silently enable', () => {
    const onEnableTimeDisplay = vi.fn();
    const { onSave } = renderDialog({
      snapshot: makeSnapshot({}, [], false),
      onEnableTimeDisplay,
    });
    expect(screen.getByTestId('column-presentation-not-enabled')).toBeTruthy();
    fireEvent.click(screen.getByTestId('column-presentation-enable'));
    expect(onEnableTimeDisplay).toHaveBeenCalledTimes(1);
    // Saving the column config still works — and still does not enable.
    chooseMode('raw');
    fireEvent.click(screen.getByTestId('column-presentation-save'));
    expect(onSave).toHaveBeenCalledWith({ mode: 'raw' });
    expect(onEnableTimeDisplay).toHaveBeenCalledTimes(1);
  });

  it('the default enable path writes only the timestamp renderer as enabled', async () => {
    render(
      <ColumnPresentationDialog
        open
        columnName="epoch_ms"
        columnType="bigint"
        source={SOURCE}
        snapshot={makeSnapshot({}, [], false)}
        sampleValue={1705316400000}
        registry={registryWithTimestamp()}
        onClose={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByTestId('column-presentation-enable'));
    await vi.waitFor(() => {
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledTimes(1);
    });
    const next = mocks.updateValuePresentationSettings.mock.calls[0]![0];
    expect(next.renderers[TIMESTAMP_CONTRIBUTION_ID]?.enabled).toBe(true);
    expect(next.renderers[TIMESTAMP_CONTRIBUTION_ID]?.options).toEqual(GLOBAL_OPTIONS);
  });

  it('reports an enable failure in place instead of pretending it worked', async () => {
    const onEnableTimeDisplay = vi.fn(async () => {
      throw new Error('boom');
    });
    renderDialog({ snapshot: makeSnapshot({}, [], false), onEnableTimeDisplay });
    fireEvent.click(screen.getByTestId('column-presentation-enable'));
    await vi.waitFor(() => {
      expect(screen.queryByText('settings.valuePresentation.saveFailed')).toBeTruthy();
    });
    expect(onEnableTimeDisplay).toHaveBeenCalledTimes(1);
  });
});

describe('ColumnPresentationDialog without a reliable source', () => {
  it('disables saving as a table column override and explains why', () => {
    renderDialog({
      sourceless: true,
      columnName: 'created_at',
      columnType: 'timestamp with time zone',
      sampleValue: '2026-01-15T14:30:00Z',
    });
    expect(screen.getByTestId('column-presentation-no-source')).toBeTruthy();
    expect(screen.getByTestId('column-presentation-save')).toBeDisabled();
    // The global default still previews: native iso-tz / Shanghai.
    expect(previewResult()).toBe('2026-01-15T22:30:00+08:00');
  });
});

describe('column presentation draft helpers', () => {
  it('draftToTimestampOptions maps the interpretation choices explicitly', () => {
    const base = draftFromColumnOverride({ mode: 'inherit' }, GLOBAL_OPTIONS, 'number');
    expect(base.interpretation).toBe('unix-milliseconds');
    expect(draftToTimestampOptions({ ...base, mode: 'custom' })).toEqual({
      interpretation: { kind: 'unix', unit: 'milliseconds' },
      format: 'iso-tz',
      displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
    });
    expect(
      draftToTimestampOptions({ ...base, mode: 'custom', interpretation: 'unix-seconds' }),
    ).toEqual({
      interpretation: { kind: 'unix', unit: 'seconds' },
      format: 'iso-tz',
      displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
    });
  });

  it('validateColumnPresentationDraft rejects an IANA name with inner whitespace', () => {
    const base = draftFromColumnOverride({ mode: 'inherit' }, GLOBAL_OPTIONS, 'instant');
    const problems = validateColumnPresentationDraft({
      ...base,
      mode: 'custom',
      displayTimeZoneKind: 'iana',
      displayIanaName: 'Asia/Shang hai',
    });
    expect(problems.length).toBeGreaterThan(0);
    // A trimmed valid name passes.
    expect(
      validateColumnPresentationDraft({
        ...base,
        mode: 'custom',
        displayTimeZoneKind: 'iana',
        displayIanaName: '  Asia/Shanghai  ',
      }),
    ).toEqual([]);
  });

  it('draftFromColumnOverride restores a saved custom override', () => {
    const draft = draftFromColumnOverride(
      {
        mode: 'custom',
        rendererId: TIMESTAMP_CONTRIBUTION_ID,
        options: {
          interpretation: { kind: 'unix', unit: 'seconds' },
          format: 'iso',
          displayTimeZone: { kind: 'utc' },
        },
      },
      GLOBAL_OPTIONS,
      'number',
    );
    expect(draft).toMatchObject({
      mode: 'custom',
      interpretation: 'unix-seconds',
      format: 'iso',
      displayTimeZoneKind: 'utc',
    });
  });
});
