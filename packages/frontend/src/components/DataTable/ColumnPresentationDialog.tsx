/**
 * ColumnPresentationDialog — 列配置面板（issue-32 T09，设计 §9，PRD §3.2/§3.3）。
 *
 * 两处入口（网格表头右键菜单「显示方式…」、aside 字段菜单同名入口）
 * 打开的是同一个面板、编辑同一份列配置：面板本身不持有任何持久化
 * 状态，只读写调用方传入的快照与会话覆盖。
 *
 * 三种模式（设计 §5）：
 * - 跟随规则（inherit）：清除本列显式覆盖，恢复规则 / 全局默认；
 * - 原始值（raw）：跳过自定义展示，不继续匹配全局规则；
 * - 时间（custom，rendererId 'datazen.timestamp'，options 为 T02
 *   TimestampOptions，走 validateTimestampOptions 校验）：选择时间来
 *   源、格式和时区。BIGINT 选时间后必须明确指定 Unix 秒或毫秒——
 *   面板不以位数自动决定单位，单位是显式下拉值。
 *
 * 预览与运行时同一条路：`computeColumnPresentationPreview`
 * （columnPresentation.ts），原值与结果同时显示（PRD §3.2 双锚点，
 * 如原值 1705316400000 / Unix 毫秒 / 带时区 ISO / Asia/Shanghai /
 * 预览 2024-01-15T19:00:00+08:00）。
 *
 * 总开关关闭时：允许编辑与保存，但明确提示「尚未启用时间显示」，
 * 并提供显式开启操作；保存绝不静默打开总开关。
 *
 * 无可靠表列来源的查询结果（source 缺省）：「保存为表列覆盖」禁用
 * 并说明原因；仍可预览全局默认 / 仅依赖已知元数据的规则效果。
 * 来源判定以调用方传入的 ColumnSource 为准，不从 SQL 别名猜表来源。
 *
 * 保存的接缝（与 T10 的边界）：默认 `onSave` 经 `saveColumnOverride`
 * 写持久化（`updateValuePresentationSettings`，见 columnOverrides.ts）：
 * 先乐观写会话存储，保存成功后清除同一键的会话条目，失败则就地报错、
 * 面板保持打开、会话条目保留、旧配置继续生效。调用方可传自己的
 * `onSave` 覆盖默认行为（此时面板只调回调、不碰持久化）。
 *
 * 文本输入空格沿用 T07 裁决：IANA 名 trim 后接受，名内空白非法。
 * 严格 TS：禁 any；不出现 `new Date(string)`。
 */

import { useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import type {
  ColumnPresentationOverride,
  ColumnSource,
  PresentationValue,
  ValuePresentationRegistry,
} from '@datazen/extension-points';
import { valuePresentationRegistry } from '@datazen/extension-points';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import type { CellTypeHints } from '../../lib/cellTypes';
import {
  TIMESTAMP_CONTRIBUTION_ID,
  buildColumnPresentationPlans,
  columnOverrideFromSnapshot,
  computeColumnPresentationPreview,
  fallbackSampleForColumn,
  formatColumnPresentationSampleValue,
  isValidIanaTimeZoneName,
  isValidTimeZoneName,
  saveColumnOverride,
  snapshotWithoutColumnOverride,
  validateColumnPresentationOverride,
  validateTimestampOptions,
  withTimestampRenderer,
  type TimestampOptions,
  type TimestampOutputFormat,
  type TimeZoneSpec,
  type ValuePresentationSnapshot,
} from '../../lib/valuePresentation/index';
import { parseStoredValuePresentationSettings } from '../../lib/valuePresentation/settings';
import { resolvePresentationSemanticKind } from '../../lib/valuePresentation/presentationPlan';

const INPUT_CLASS =
  'h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/25';

type PresentationMode = 'inherit' | 'raw' | 'custom';
type InterpretationChoice = 'native' | 'unix-seconds' | 'unix-milliseconds' | 'text';
type TimeZoneKindChoice = 'local' | 'utc' | 'iana';
type SourceTimeZoneChoice = 'none' | 'local' | 'utc' | 'iana';

const INTERPRETATIONS: readonly InterpretationChoice[] = [
  'native',
  'unix-seconds',
  'unix-milliseconds',
  'text',
];
const OUTPUT_FORMATS: readonly TimestampOutputFormat[] = ['iso', 'iso-tz', 'locale'];
const TIME_ZONE_KINDS: readonly TimeZoneKindChoice[] = ['local', 'utc', 'iana'];
const SOURCE_TIME_ZONE_CHOICES: readonly SourceTimeZoneChoice[] = ['none', 'local', 'utc', 'iana'];

/** 面板草稿：模式 + 时间选项的表单状态（IANA 名为原始输入，保存时 trim）。 */
export interface ColumnPresentationDraft {
  mode: PresentationMode;
  interpretation: InterpretationChoice;
  format: TimestampOutputFormat;
  displayTimeZoneKind: TimeZoneKindChoice;
  displayIanaName: string;
  sourceTimeZoneKind: SourceTimeZoneChoice;
  sourceIanaName: string;
}

function timeZoneSpec(kind: TimeZoneKindChoice, ianaName: string): TimeZoneSpec {
  return kind === 'iana' ? { kind: 'iana', name: ianaName.trim() } : { kind };
}

function sourceTimeZoneSpec(
  kind: SourceTimeZoneChoice,
  ianaName: string,
): TimeZoneSpec | undefined {
  if (kind === 'none') return undefined;
  return timeZoneSpec(kind, ianaName);
}

/**
 * 草稿 → TimestampOptions（custom 模式用）。IANA 名只做 trim，不在这里
 * 判格式/已知性——校验由 `validateColumnPresentationDraft` 统一做，
 * 预览与保存共用同一结果。
 */
export function draftToTimestampOptions(
  draft: Readonly<ColumnPresentationDraft>,
): TimestampOptions {
  const interpretation: TimestampOptions['interpretation'] =
    draft.interpretation === 'unix-seconds'
      ? { kind: 'unix', unit: 'seconds' }
      : draft.interpretation === 'unix-milliseconds'
        ? { kind: 'unix', unit: 'milliseconds' }
        : draft.interpretation === 'text'
          ? { kind: 'text' }
          : { kind: 'native' };
  const displayTimeZone = timeZoneSpec(draft.displayTimeZoneKind, draft.displayIanaName);
  const sourceTimeZone =
    draft.interpretation === 'unix-seconds' || draft.interpretation === 'unix-milliseconds'
      ? undefined
      : sourceTimeZoneSpec(draft.sourceTimeZoneKind, draft.sourceIanaName);
  return sourceTimeZone === undefined
    ? { interpretation, format: draft.format, displayTimeZone }
    : { interpretation, format: draft.format, displayTimeZone, sourceTimeZone };
}

/** 草稿 → 列覆盖（非 custom 模式不带 options）。 */
export function draftToColumnOverride(
  draft: Readonly<ColumnPresentationDraft>,
): ColumnPresentationOverride {
  if (draft.mode === 'inherit') return { mode: 'inherit' };
  if (draft.mode === 'raw') return { mode: 'raw' };
  return {
    mode: 'custom',
    rendererId: TIMESTAMP_CONTRIBUTION_ID,
    options: draftToTimestampOptions(draft),
  };
}

/**
 * 草稿校验（保存与预览共用）：IANA 名 trim 后判格式与已知性
 * （T07 裁决：名内空白非法）；其余走 validateTimestampOptions 与
 * validateColumnPresentationOverride。返回问题列表，空即合法。
 */
export function validateColumnPresentationDraft(
  draft: Readonly<ColumnPresentationDraft>,
): ReadonlyArray<{ path: string; message: string }> {
  if (draft.mode !== 'custom') return [];
  const problems: Array<{ path: string; message: string }> = [];
  const checkIana = (
    kind: TimeZoneKindChoice | SourceTimeZoneChoice,
    name: string,
    path: string,
  ) => {
    if (kind !== 'iana') return;
    const trimmed = name.trim();
    if (trimmed === '' || !isValidIanaTimeZoneName(trimmed)) {
      problems.push({ path, message: 'expected a valid IANA time zone name' });
    } else if (!isValidTimeZoneName(trimmed)) {
      problems.push({ path, message: 'unknown time zone name' });
    }
  };
  checkIana(draft.displayTimeZoneKind, draft.displayIanaName, 'displayTimeZone.name');
  if (draft.interpretation === 'text' || draft.interpretation === 'native') {
    checkIana(draft.sourceTimeZoneKind, draft.sourceIanaName, 'sourceTimeZone.name');
  }
  if (problems.length > 0) return problems;
  const validatedOptions = validateTimestampOptions(draftToTimestampOptions(draft));
  if (!validatedOptions.ok) return validatedOptions.problems;
  const validatedOverride = validateColumnPresentationOverride(draftToColumnOverride(draft));
  if (!validatedOverride.ok) return validatedOverride.problems;
  return [];
}

/** 从当前覆盖 + 全局默认还原草稿（面板每次打开时重建）。 */
export function draftFromColumnOverride(
  current: Readonly<ColumnPresentationOverride>,
  globalOptions: TimestampOptions | undefined,
  semanticKind: 'number' | 'instant' | 'local-datetime' | 'date' | 'time' | 'text' | 'other',
): ColumnPresentationDraft {
  const base: ColumnPresentationDraft = {
    mode: 'inherit',
    // BIGINT 默认显式选 Unix 毫秒（下拉里可见、可改，不是按位数推断）；
    // 原生时间列默认 native；其余列默认 native（预览会说明不适用）。
    interpretation: semanticKind === 'number' ? 'unix-milliseconds' : 'native',
    format: globalOptions?.format ?? 'iso-tz',
    displayTimeZoneKind:
      globalOptions?.displayTimeZone.kind === 'iana'
        ? 'iana'
        : (globalOptions?.displayTimeZone.kind ?? 'local'),
    displayIanaName:
      globalOptions?.displayTimeZone.kind === 'iana'
        ? globalOptions.displayTimeZone.name
        : 'Asia/Shanghai',
    sourceTimeZoneKind: 'none',
    sourceIanaName: 'Asia/Shanghai',
  };
  if (current.mode !== 'custom') {
    return { ...base, mode: current.mode };
  }
  const validated = validateTimestampOptions(current.options);
  if (!validated.ok) return { ...base, mode: 'custom' };
  const options = validated.options;
  const interpretation: InterpretationChoice =
    options.interpretation.kind === 'unix'
      ? options.interpretation.unit === 'seconds'
        ? 'unix-seconds'
        : 'unix-milliseconds'
      : options.interpretation.kind;
  const sourceTimeZone = options.sourceTimeZone;
  return {
    mode: 'custom',
    interpretation,
    format: options.format,
    displayTimeZoneKind:
      options.displayTimeZone.kind === 'iana' ? 'iana' : options.displayTimeZone.kind,
    displayIanaName:
      options.displayTimeZone.kind === 'iana' ? options.displayTimeZone.name : 'Asia/Shanghai',
    sourceTimeZoneKind:
      sourceTimeZone === undefined
        ? 'none'
        : sourceTimeZone.kind === 'iana'
          ? 'iana'
          : sourceTimeZone.kind,
    sourceIanaName:
      sourceTimeZone !== undefined && sourceTimeZone.kind === 'iana'
        ? sourceTimeZone.name
        : 'Asia/Shanghai',
  };
}

function isInterpretationChoice(value: string): value is InterpretationChoice {
  return (INTERPRETATIONS as readonly string[]).includes(value);
}

function isOutputFormat(value: string): value is TimestampOutputFormat {
  return value === 'iso' || value === 'iso-tz' || value === 'locale';
}

function isTimeZoneKindChoice(value: string): value is TimeZoneKindChoice {
  return value === 'local' || value === 'utc' || value === 'iana';
}

function isSourceTimeZoneChoice(value: string): value is SourceTimeZoneChoice {
  return value === 'none' || value === 'local' || value === 'utc' || value === 'iana';
}

/** 全局默认时间选项（快照 configs 里 datazen.timestamp 一项，非法时缺省）。 */
export function globalTimestampOptionsFromSnapshot(
  snapshot: Readonly<ValuePresentationSnapshot>,
): TimestampOptions | undefined {
  const config = snapshot.configs[TIMESTAMP_CONTRIBUTION_ID];
  if (config === undefined) return undefined;
  const validated = validateTimestampOptions(config.config);
  return validated.ok ? validated.options : undefined;
}

export interface ColumnPresentationDialogProps {
  open: boolean;
  columnName: string;
  columnType?: string;
  cellTypeHints?: CellTypeHints;
  /** 表身份；缺省表示查询结果等无来源列——保存覆盖禁用（PRD §3.2）。 */
  source?: Readonly<Omit<ColumnSource, 'column'>>;
  /** 合并了会话覆盖的快照（DataTable / DetailPanel 经 hook 传入）。 */
  snapshot: ValuePresentationSnapshot;
  /** 预览样本：调用方给真实行值；缺省时按语义种给 PRD 双锚点样本。 */
  sampleValue?: PresentationValue;
  /** 当前覆盖；缺省时从快照取。 */
  currentOverride?: ColumnPresentationOverride;
  /**
   * 保存回调；缺省为写会话存储（T09 会话生效接缝）。无来源列时
   * 保存按钮禁用，本回调不会被调用。
   */
  onSave?: (override: ColumnPresentationOverride) => void;
  onClose: () => void;
  /**
   * 显式开启总开关的操作；缺省为经 settingsStore 把
   * datazen.timestamp 置 enabled（只改这一项，不动其它配置，
   * 且只在用户点「启用时间显示」时执行，保存不触发）。
   */
  onEnableTimeDisplay?: () => void | Promise<void>;
  /** 缺省为全局 valuePresentationRegistry。 */
  registry?: ValuePresentationRegistry;
}

export function ColumnPresentationDialog({
  open,
  columnName,
  columnType,
  cellTypeHints,
  source,
  snapshot,
  sampleValue,
  currentOverride,
  onSave,
  onClose,
  onEnableTimeDisplay,
  registry = valuePresentationRegistry,
}: ColumnPresentationDialogProps) {
  const { t } = useI18n();
  const semanticKind = resolvePresentationSemanticKind({
    name: columnName,
    dataType: columnType,
    hints: cellTypeHints,
  });
  const effectiveCurrentOverride: ColumnPresentationOverride =
    currentOverride ?? columnOverrideFromSnapshot(snapshot, source, columnName);
  const globalOptions = useMemo(() => globalTimestampOptionsFromSnapshot(snapshot), [snapshot]);
  const [draft, setDraft] = useState<ColumnPresentationDraft>(() =>
    draftFromColumnOverride(effectiveCurrentOverride, globalOptions, semanticKind),
  );
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [enabling, setEnabling] = useState(false);
  const [enableError, setEnableError] = useState<string | null>(null);

  // 每次打开（或换列）时用当前覆盖重建草稿；关闭期间的草稿不保留。
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (!open) return;
    setDraft(draftFromColumnOverride(effectiveCurrentOverride, globalOptions, semanticKind));
    setError(null);
    setEnableError(null);
  }, [open, columnName]);

  const timeEnabled = snapshot.enabledRendererIds.includes(TIMESTAMP_CONTRIBUTION_ID);
  // T11：订阅注册中心修订——贡献在面板打开期间停用/重启用时，
  // 缺失提示即时出现/消失，不依赖父级快照重建才重渲染。
  const contributionRevision = useSyncExternalStore(
    (onStoreChange: () => void): (() => void) => registry.subscribe(onStoreChange),
    () => registry.revision,
    () => registry.revision,
  );
  const contribution = useMemo(
    () => registry.get(TIMESTAMP_CONTRIBUTION_ID),
    [registry, contributionRevision],
  );
  const sample: PresentationValue =
    sampleValue !== undefined
      ? sampleValue
      : (fallbackSampleForColumn({
          name: columnName,
          dataType: columnType,
          hints: cellTypeHints,
        }) ?? null);
  const hasSample = sample !== null || sampleValue !== undefined;
  const candidate = draftToColumnOverride(draft);
  const draftProblems = validateColumnPresentationDraft(draft);
  const preview = useMemo(
    () =>
      computeColumnPresentationPreview({
        candidate,
        column: { name: columnName, dataType: columnType, hints: cellTypeHints },
        source,
        snapshot,
        environment: snapshot.environment,
        sampleValue: sample,
        registry,
      }),
    // candidate 由 draft 派生，draft 即全部输入；sample / snapshot 同理。
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [draft, columnName, columnType, source, snapshot, sample, registry],
  );
  // 当前生效计划（inherit 预览与生效来源提示共用同一计划）。草稿为
  // inherit 时按「清除覆盖后」建计划（T09-BUG-001：直接拿合并快照
  // 建计划会让待清除的覆盖优先于规则）；custom / raw 草稿的来源
  // 本就是列覆盖，标签不依赖该计划。
  const currentPlan = useMemo(() => {
    const plans = buildColumnPresentationPlans(
      {
        columns: [{ name: columnName, dataType: columnType, hints: cellTypeHints }],
        source,
        snapshot:
          draft.mode === 'inherit'
            ? snapshotWithoutColumnOverride(snapshot, source, columnName)
            : snapshot,
      },
      registry,
    );
    return plans[0];
  }, [columnName, columnType, cellTypeHints, source, snapshot, registry, draft.mode]);

  // 来源标签按草稿 mode 取值（T09-BUG-001）：custom / raw 保存后即
  // 为列覆盖；inherit 保存后清除覆盖，来源为剥离后的计划所命中的
  // 规则或全局默认——不看保存前的 effectiveCurrentOverride。
  const effectiveSourceLabel =
    draft.mode === 'custom' || draft.mode === 'raw'
      ? t('columnPresentation.source.override')
      : currentPlan?.matchedRuleName !== undefined
        ? t('columnPresentation.source.rule', { name: currentPlan.matchedRuleName })
        : t('columnPresentation.source.global');

  function updateDraft(patch: Partial<ColumnPresentationDraft>): void {
    setDraft((prev) => ({ ...prev, ...patch }));
    setError(null);
  }

  async function handleSave(): Promise<void> {
    if (source === undefined) return;
    if (draftProblems.length > 0) {
      setError(
        `${t('settings.valuePresentation.invalidConfig')}: ${draftProblems
          .map((problem) => `${problem.path}: ${problem.message}`)
          .join('; ')}`,
      );
      return;
    }
    const validated = validateColumnPresentationOverride(candidate);
    if (!validated.ok) {
      setError(
        `${t('settings.valuePresentation.invalidConfig')}: ${validated.problems
          .map((problem) => `${problem.path}: ${problem.message}`)
          .join('; ')}`,
      );
      return;
    }
    if (onSave !== undefined) {
      onSave(validated.override);
      onClose();
      return;
    }
    // 默认保存路径（T10）：经 settingsStore 写持久化。保存成功后
    // 会话条目已清除（saveColumnOverride 内），快照即持久值；失败
    // 则就地报错、面板保持打开（不 onClose）、会话条目保留、旧配
    // 置继续生效，用户可重试。
    setSaving(true);
    setError(null);
    try {
      const store = useSettingsStore.getState();
      const stored = parseStoredValuePresentationSettings(store.settings.valuePresentation);
      const result = await saveColumnOverride({
        stored,
        source,
        columnName,
        override: validated.override,
        persist: store.updateValuePresentationSettings,
      });
      if (!result.ok) {
        setError(t('settings.valuePresentation.saveFailed'));
        return;
      }
    } catch {
      setError(t('settings.valuePresentation.saveFailed'));
      return;
    } finally {
      setSaving(false);
    }
    onClose();
  }

  async function handleEnableTimeDisplay(): Promise<void> {
    if (onEnableTimeDisplay !== undefined) {
      setEnabling(true);
      setEnableError(null);
      try {
        await onEnableTimeDisplay();
      } catch {
        setEnableError(t('settings.valuePresentation.saveFailed'));
      } finally {
        setEnabling(false);
      }
      return;
    }
    // 默认开启路径：只把 datazen.timestamp 置 enabled（T11：updater
    // 形式——store 对保存前重读的权威值替换这一项，规则与列覆盖取
    // 权威原文，不因本窗口基底陈旧而丢弃；未知版本字段也原样保留）。
    // options 沿用快照里的合法全局值，否则用默认 native / iso / local。
    // 保存列配置本身绝不走这条路。
    setEnabling(true);
    setEnableError(null);
    try {
      const store = useSettingsStore.getState();
      const options: TimestampOptions = globalOptions ?? {
        interpretation: { kind: 'native' },
        format: 'iso',
        displayTimeZone: { kind: 'local' },
      };
      await store.updateValuePresentationSettings((fresh) =>
        withTimestampRenderer(fresh, true, options),
      );
    } catch {
      setEnableError(t('settings.valuePresentation.saveFailed'));
    } finally {
      setEnabling(false);
    }
  }

  const showSourceTimeZone = draft.interpretation === 'text' || draft.interpretation === 'native';
  const saveDisabled = source === undefined;

  return (
    <Dialog
      open={open}
      title={`${t('columnPresentation.title')} — ${columnName}`}
      onClose={onClose}
      testId="column-presentation-dialog"
    >
      <div className="min-w-80 space-y-4" data-testid="column-presentation-dialog-body">
        {!timeEnabled && (
          <div
            role="status"
            data-testid="column-presentation-not-enabled"
            className="rounded-md border border-edge bg-surface-alt px-3 py-2 text-sm text-fg-secondary"
          >
            <p>{t('columnPresentation.notEnabled')}</p>
            <div className="mt-2">
              <Button
                variant="primary"
                disabled={enabling}
                onClick={() => void handleEnableTimeDisplay()}
                data-testid="column-presentation-enable"
              >
                {t('columnPresentation.enable')}
              </Button>
              {enableError !== null && (
                <p role="alert" className="mt-1 text-xs text-red-500">
                  {enableError}
                </p>
              )}
            </div>
          </div>
        )}

        {contribution === undefined && (
          <p
            role="status"
            data-testid="column-presentation-contribution-missing"
            className="rounded-md border border-edge bg-surface-alt px-3 py-2 text-sm text-fg-secondary"
          >
            {t('settings.valuePresentation.contributionMissing')}
          </p>
        )}

        <div
          className="text-sm text-fg-secondary"
          data-testid="column-presentation-effective-source"
        >
          {t('columnPresentation.effectiveSource')}: {effectiveSourceLabel}
        </div>

        <fieldset className="space-y-2">
          <legend className="text-sm font-medium text-fg">{t('columnPresentation.mode')}</legend>
          {(['inherit', 'raw', 'custom'] as const).map((mode) => (
            <label key={mode} className="flex items-start gap-2 text-sm text-fg-secondary">
              <input
                type="radio"
                name="column-presentation-mode"
                value={mode}
                checked={draft.mode === mode}
                onChange={() => updateDraft({ mode })}
                data-testid={`column-presentation-mode-${mode}`}
                className="mt-1"
              />
              <span>
                <span className="font-medium text-fg">{t(`columnPresentation.mode.${mode}`)}</span>
                <span className="block text-xs text-fg-muted">
                  {t(`columnPresentation.mode.${mode}.hint`)}
                </span>
              </span>
            </label>
          ))}
        </fieldset>

        {draft.mode === 'custom' && (
          <div className="space-y-3">
            <div>
              <span className="mb-1 block text-sm font-medium text-fg">
                {t('settings.valuePresentation.rules.editor.interpretation')}
              </span>
              <Select
                value={draft.interpretation}
                options={INTERPRETATIONS.map((interpretation) => ({
                  value: interpretation,
                  label: t(`settings.valuePresentation.preview.source.${interpretation}`),
                }))}
                onChange={(next) => {
                  if (isInterpretationChoice(next)) {
                    updateDraft({ interpretation: next });
                  }
                }}
                title={t('settings.valuePresentation.rules.editor.interpretation')}
                data-testid="column-presentation-interpretation"
              />
            </div>

            <div>
              <span className="mb-1 block text-sm font-medium text-fg">
                {t('settings.valuePresentation.format')}
              </span>
              <Select
                value={draft.format}
                options={OUTPUT_FORMATS.map((format) => ({
                  value: format,
                  label: t(`settings.valuePresentation.format.${format}`),
                }))}
                onChange={(next) => {
                  if (isOutputFormat(next)) {
                    updateDraft({ format: next });
                  }
                }}
                title={t('settings.valuePresentation.format')}
                data-testid="column-presentation-format"
              />
            </div>

            <div>
              <span className="mb-1 block text-sm font-medium text-fg">
                {t('settings.valuePresentation.displayTimeZone')}
              </span>
              <Select
                value={draft.displayTimeZoneKind}
                options={TIME_ZONE_KINDS.map((kind) => ({
                  value: kind,
                  label: t(`settings.valuePresentation.timeZone.${kind}`),
                }))}
                onChange={(next) => {
                  if (isTimeZoneKindChoice(next)) {
                    updateDraft({ displayTimeZoneKind: next });
                  }
                }}
                title={t('settings.valuePresentation.displayTimeZone')}
                data-testid="column-presentation-display-timezone"
              />
            </div>

            {draft.displayTimeZoneKind === 'iana' && (
              <div>
                <label
                  htmlFor="column-presentation-display-iana-name"
                  className="mb-1 block text-sm font-medium text-fg"
                >
                  {t('settings.valuePresentation.ianaName')}
                </label>
                <input
                  id="column-presentation-display-iana-name"
                  type="text"
                  value={draft.displayIanaName}
                  placeholder="Asia/Shanghai"
                  onChange={(event) => updateDraft({ displayIanaName: event.target.value })}
                  className={INPUT_CLASS}
                  data-testid="column-presentation-display-iana-name"
                />
              </div>
            )}

            {showSourceTimeZone && (
              <div>
                <span className="mb-1 block text-sm font-medium text-fg">
                  {t('columnPresentation.sourceTimeZone')}
                </span>
                <Select
                  value={draft.sourceTimeZoneKind}
                  options={SOURCE_TIME_ZONE_CHOICES.map((kind) => ({
                    value: kind,
                    label:
                      kind === 'none'
                        ? t('settings.valuePresentation.rules.editor.sourceTimeZone.none')
                        : t(`settings.valuePresentation.timeZone.${kind}`),
                  }))}
                  onChange={(next) => {
                    if (isSourceTimeZoneChoice(next)) {
                      updateDraft({ sourceTimeZoneKind: next });
                    }
                  }}
                  title={t('columnPresentation.sourceTimeZone')}
                  data-testid="column-presentation-source-timezone"
                />
              </div>
            )}

            {showSourceTimeZone && draft.sourceTimeZoneKind === 'iana' && (
              <div>
                <label
                  htmlFor="column-presentation-source-iana-name"
                  className="mb-1 block text-sm font-medium text-fg"
                >
                  {t('settings.valuePresentation.ianaName')}
                </label>
                <input
                  id="column-presentation-source-iana-name"
                  type="text"
                  value={draft.sourceIanaName}
                  placeholder="Asia/Shanghai"
                  onChange={(event) => updateDraft({ sourceIanaName: event.target.value })}
                  className={INPUT_CLASS}
                  data-testid="column-presentation-source-iana-name"
                />
              </div>
            )}
          </div>
        )}

        <div
          className="rounded-md border border-edge bg-surface-alt p-3 text-sm"
          data-testid="column-presentation-preview"
        >
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-fg-muted">{t('columnPresentation.original')}</span>
            <code className="font-mono text-fg" data-testid="column-presentation-original">
              {formatColumnPresentationSampleValue(sample)}
            </code>
            {draft.mode === 'custom' && draft.interpretation === 'unix-seconds' && (
              <span
                data-testid="column-presentation-preview-unix-badge"
                className="rounded bg-accent/15 px-1.5 py-0.5 text-[11px] font-medium text-accent"
              >
                {t('settings.valuePresentation.preview.unixSeconds')}
              </span>
            )}
            {draft.mode === 'custom' && draft.interpretation === 'unix-milliseconds' && (
              <span
                data-testid="column-presentation-preview-unix-badge"
                className="rounded bg-accent/15 px-1.5 py-0.5 text-[11px] font-medium text-accent"
              >
                {t('settings.valuePresentation.preview.unixMilliseconds')}
              </span>
            )}
          </div>
          <div className="mt-1.5">
            {!hasSample && (
              <span data-testid="column-presentation-preview-result" className="text-fg-muted">
                {t('columnPresentation.preview.noSample')}
              </span>
            )}
            {hasSample && preview.kind === 'text' && (
              <span data-testid="column-presentation-preview-result" className="font-mono text-fg">
                {preview.text}
              </span>
            )}
            {hasSample && preview.kind === 'unchanged' && (
              <span data-testid="column-presentation-preview-result" className="text-fg-muted">
                {t('columnPresentation.preview.unchanged')}
                {preview.reason !== undefined ? `: ${preview.reason}` : ''}
              </span>
            )}
            {hasSample && preview.kind === 'invalid' && (
              <span data-testid="column-presentation-preview-result" className="text-red-500">
                {t('settings.valuePresentation.preview.invalid')}
                {': '}
                {preview.problems
                  .map((problem) => `${problem.path}: ${problem.message}`)
                  .join('; ')}
              </span>
            )}
            {hasSample && preview.kind === 'unavailable' && (
              <span data-testid="column-presentation-preview-result" className="text-fg-muted">
                {t('settings.valuePresentation.preview.unavailable')}
              </span>
            )}
          </div>
        </div>

        {saveDisabled && (
          <p
            role="status"
            data-testid="column-presentation-no-source"
            className="rounded-md border border-edge bg-surface-alt px-3 py-2 text-sm text-fg-secondary"
          >
            {t('columnPresentation.noSource')}
          </p>
        )}

        {error !== null && (
          <p role="alert" data-testid="column-presentation-error" className="text-sm text-red-500">
            {error}
          </p>
        )}

        <div className="flex items-center gap-2">
          <Button
            variant="primary"
            disabled={saveDisabled || saving}
            onClick={() => void handleSave()}
            data-testid="column-presentation-save"
          >
            {t('columnPresentation.save')}
          </Button>
          <Button variant="ghost" onClick={onClose} data-testid="column-presentation-cancel">
            {t('common.cancel')}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
