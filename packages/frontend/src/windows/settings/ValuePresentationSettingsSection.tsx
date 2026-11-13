/**
 * ValuePresentationSettingsSection — 设置 → 表格数据 → 时间显示
 * （issue-32 T07，PRD §3.1/§3.3，设计 §4/§5/§6）。
 *
 * 挂在 TableDataSettingsSection 末尾，随「数据浏览」设置页出现。
 *
 * 设计要点（与 T06 持久化链路、T02/T03 核心的接线方式）：
 *
 * 1. 草稿 + 显式保存：控件只改本地草稿，点「保存」才经
 *    `updateValuePresentationSettings` 走正常保存链路。非法配置
 *    （validateConfig 不通过、IANA 名无效）在保存前拦截，就地说明
 *    原因；保存失败明确提示，旧配置继续生效（草稿保留、不显示成
 *    已保存）。
 * 2. 全局默认只管原生时间值：全局默认的 interpretation 固定为
 *    native（PRD §3.1：开启只让全局默认应用于适用的原生时间值，
 *    绝不把所有数字/字符串尝试转时间）。Unix 秒/毫秒/文本只出现在
 *    预览的「时间来源」选择里，用于看样本效果，不参与保存。
 * 3. 预览与运行时同一条路：预览经 `datazen.timestamp` 贡献的
 *    prepare/present（T03），不另写解析。贡献缺失时预览不可用，
 *    面板提示所需扩展不可用，配置保留、可继续保存（保存校验走
 *    T02 纯验证，不依赖贡献在场）。
 * 4. 规则列表可编辑（T08，PRD §3.1）：新增/编辑/删除/启停/调序，
 *    挂在同一面板的可折叠规则列表内。规则草稿随面板草稿一起保存
 *    （同一份 valuePresentation，经 updateValuePresentationSettings
 *    走正常保存链路）；编辑器见 ValuePresentationRuleEditor。
 * 5. 未启用时不改变既有行为：开关关闭只影响保存的 enabled 位；
 *    本面板挂载、改草稿、看预览都不会写 store，也不会触动网格
 *    与详情的展示（PRD 验收 1）。
 * 6. 跨窗口同步（T11，设计 §6）：保存走 updater 形式——只写用户
 *    实际改过的路径（全局格式/开关、规则列表），其余字段取保存前
 *    重读的权威值；另一窗口保存后本面板若有未保存编辑，不静默丢
 *    弃草稿，只提示外部更新，用户点「重新加载已保存的设置」才覆
 *    盖草稿（规则列表整体是一条路径，两窗同时改规则仍后写胜出，
 *    见 valuePresentationSave.ts 文件头裁决）。
 */

import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import {
  valuePresentationRegistry,
  type PresentationEnvironment,
  type PresentationValue,
  type ValuePresentationContribution,
  type ValuePresentationRegistry,
} from '@datazen/extension-points';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { useSettingsStore } from '../../stores/settingsStore';
import {
  TIMESTAMP_CONTRIBUTION_ID,
  MAX_PATTERN_LENGTH,
  MAX_VALUE_PRESENTATION_RULES,
  describeRuleMatch,
  isValidIanaTimeZoneName,
  isValidTimeZoneName,
  parseStoredValuePresentationSettings,
  resolvePresentationEnvironment,
  validateTimestampOptions,
  validateValuePresentationRule,
  withTimestampRenderer,
  withValuePresentationRules,
  type StoredValuePresentationSettings,
  type TimestampOptions,
  type TimestampOutputFormat,
  type TimeZoneSpec,
  type ValuePresentationRule,
} from '../../lib/valuePresentation/index';
import { ValuePresentationRuleEditor } from './ValuePresentationRuleEditor';
import { SettingRow, ToggleRow } from './settingsUi';

// ---------------------------------------------------------------------------
// 草稿：从 store 的 valuePresentation 读 datazen.timestamp 一项
// ---------------------------------------------------------------------------

/** 全局默认时间选项的初始值：原生解释、UTC ISO、跟随本地时区。 */
export const DEFAULT_TIMESTAMP_OPTIONS: TimestampOptions = {
  interpretation: { kind: 'native' },
  format: 'iso',
  displayTimeZone: { kind: 'local' },
};

/** 面板草稿：开关 + 全局默认选项（interpretation 恒为 native，见文件头 2）。 */
export interface TimestampDisplayDraft {
  enabled: boolean;
  options: TimestampOptions;
}

/**
 * 从存储值还原草稿：无条目 → 默认关闭；条目 options 非法 → 开关保留、
 * 选项回落默认（非法配置不进草稿，保存时写回合法值）。
 */
export function draftFromStored(stored: StoredValuePresentationSettings): TimestampDisplayDraft {
  const entry = stored.renderers[TIMESTAMP_CONTRIBUTION_ID];
  if (entry === undefined) {
    return { enabled: false, options: DEFAULT_TIMESTAMP_OPTIONS };
  }
  const validated = validateTimestampOptions(entry.options);
  return {
    enabled: entry.enabled === true,
    options: validated.ok ? validated.options : DEFAULT_TIMESTAMP_OPTIONS,
  };
}

// ---------------------------------------------------------------------------
// 预览：与运行时同一条路（贡献 prepare / present，T03）
// ---------------------------------------------------------------------------

/** 预览的时间来源选择：只影响预览样本，不参与保存（见文件头 2）。 */
export type PreviewInterpretation = 'native' | 'unix-seconds' | 'unix-milliseconds' | 'text';

const PREVIEW_INTERPRETATIONS: readonly PreviewInterpretation[] = [
  'native',
  'unix-seconds',
  'unix-milliseconds',
  'text',
];

interface PreviewSample {
  value: PresentationValue;
  semanticKind: 'instant' | 'local-datetime' | 'number';
}

/**
 * 预览样本（PRD §4 双锚点）：
 * - native：2026-01-15T14:30:00Z（Asia/Shanghai + iso-tz → 2026-01-15T22:30:00+08:00）；
 * - unix 秒/毫秒：1705316400 / 1705316400000（Asia/Shanghai + iso-tz → 2024-01-15T19:00:00+08:00）；
 * - text：无时区墙上文本 2026-01-15 14:30:00。
 */
export function previewSampleFor(preview: PreviewInterpretation): PreviewSample {
  switch (preview) {
    case 'native':
      return { value: '2026-01-15T14:30:00Z', semanticKind: 'instant' };
    case 'unix-seconds':
      return { value: 1705316400, semanticKind: 'number' };
    case 'unix-milliseconds':
      return { value: 1705316400000, semanticKind: 'number' };
    case 'text':
      return { value: '2026-01-15 14:30:00', semanticKind: 'local-datetime' };
  }
}

/**
 * 由草稿选项派生预览选项：只替换 interpretation。Unix 解释不带
 * sourceTimeZone（validateTimestampOptions 禁止，设计 §5）；文本解释
 * 需要源时区，预览用草稿已有的源时区、缺省为 UTC（仅预览，不保存）。
 */
export function buildPreviewOptions(
  draft: TimestampOptions,
  preview: PreviewInterpretation,
): TimestampOptions {
  const format = draft.format;
  const displayTimeZone = draft.displayTimeZone;
  switch (preview) {
    case 'native':
      return draft.sourceTimeZone === undefined
        ? { interpretation: { kind: 'native' }, format, displayTimeZone }
        : {
            interpretation: { kind: 'native' },
            format,
            displayTimeZone,
            sourceTimeZone: draft.sourceTimeZone,
          };
    case 'unix-seconds':
      return { interpretation: { kind: 'unix', unit: 'seconds' }, format, displayTimeZone };
    case 'unix-milliseconds':
      return { interpretation: { kind: 'unix', unit: 'milliseconds' }, format, displayTimeZone };
    case 'text':
      return {
        interpretation: { kind: 'text' },
        format,
        displayTimeZone,
        sourceTimeZone: draft.sourceTimeZone ?? { kind: 'utc' },
      };
  }
}

export type TimestampPreviewResult =
  | { kind: 'unavailable'; sample: PresentationValue }
  | {
      kind: 'invalid';
      problems: ReadonlyArray<{ path: string; message: string }>;
      sample: PresentationValue;
    }
  | { kind: 'text'; text: string; interpretation: string | undefined; sample: PresentationValue }
  | { kind: 'unchanged'; reason: string | undefined; sample: PresentationValue };

/**
 * 预览计算（与运行时同一条路）：贡献 validateConfig → prepare →
 * present。贡献缺失返回 unavailable；配置无效返回 invalid 与路径化
 * 问题；其余按 present 结果原样返回。
 */
export function computeTimestampPreview(input: {
  contribution: ValuePresentationContribution | undefined;
  options: TimestampOptions;
  previewInterpretation: PreviewInterpretation;
  environment: PresentationEnvironment;
}): TimestampPreviewResult {
  const sample = previewSampleFor(input.previewInterpretation);
  if (input.contribution === undefined) {
    return { kind: 'unavailable', sample: sample.value };
  }
  const validated = input.contribution.validateConfig(input.options);
  if (!validated.ok) {
    return { kind: 'invalid', problems: validated.problems, sample: sample.value };
  }
  const prepared = input.contribution.prepare({
    config: { configVersion: input.contribution.configVersion, config: validated.config },
    column: { columnName: '__preview__', semanticKind: sample.semanticKind },
    environment: input.environment,
  });
  const result = prepared.present(sample.value);
  if (result.kind === 'text') {
    return {
      kind: 'text',
      text: result.text,
      interpretation: result.interpretation,
      sample: sample.value,
    };
  }
  return { kind: 'unchanged', reason: result.reason, sample: sample.value };
}

function formatPreviewSample(value: PresentationValue): string {
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'bigint') return String(value);
  return '';
}

const OUTPUT_FORMATS: readonly TimestampOutputFormat[] = ['iso', 'iso-tz', 'locale'];

type TimeZoneKind = 'local' | 'utc' | 'iana';

const TIME_ZONE_KINDS: readonly TimeZoneKind[] = ['local', 'utc', 'iana'];

function isOutputFormat(value: string): value is TimestampOutputFormat {
  return value === 'iso' || value === 'iso-tz' || value === 'locale';
}

function isTimeZoneKind(value: string): value is TimeZoneKind {
  return value === 'local' || value === 'utc' || value === 'iana';
}

function isPreviewInterpretation(value: string): value is PreviewInterpretation {
  return (
    value === 'native' ||
    value === 'unix-seconds' ||
    value === 'unix-milliseconds' ||
    value === 'text'
  );
}

// ---------------------------------------------------------------------------
// 面板
// ---------------------------------------------------------------------------

export interface ValuePresentationSettingsSectionProps {
  /** 缺省为全局 valuePresentationRegistry；测试传入全新注册中心。 */
  readonly registry?: ValuePresentationRegistry;
}

export function ValuePresentationSettingsSection({
  registry = valuePresentationRegistry,
}: ValuePresentationSettingsSectionProps) {
  const { t, language } = useI18n();
  const settings = useSettingsStore((s) => s.settings);
  const updateValuePresentationSettings = useSettingsStore(
    (s) => s.updateValuePresentationSettings,
  );
  const stored = useMemo(
    () => parseStoredValuePresentationSettings(settings.valuePresentation),
    [settings.valuePresentation],
  );

  // 贡献在场性随注册中心修订而变（停用/重启用扩展时面板即时反映）。
  const subscribeRegistry = useCallback(
    (onStoreChange: () => void): (() => void) => registry.subscribe(onStoreChange),
    [registry],
  );
  const contributionRevision = useSyncExternalStore(
    subscribeRegistry,
    () => registry.revision,
    () => registry.revision,
  );
  const contribution = useMemo(
    () => registry.get(TIMESTAMP_CONTRIBUTION_ID),
    [registry, contributionRevision],
  );

  const [draft, setDraft] = useState<TimestampDisplayDraft>(() => draftFromStored(stored));
  const [ianaName, setIanaName] = useState<string>(() => {
    const initial = draftFromStored(stored);
    return initial.options.displayTimeZone.kind === 'iana'
      ? initial.options.displayTimeZone.name
      : '';
  });
  const [appliedStoredJson, setAppliedStoredJson] = useState<string>(() => JSON.stringify(stored));
  // T11：另一窗口保存后本面板仍有未保存编辑时置 true，只提示、不覆
  // 盖草稿；用户点「重新加载已保存的设置」（即 handleReset）后清零。
  const [externalUpdate, setExternalUpdate] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [saving, setSaving] = useState(false);
  const [previewInterpretation, setPreviewInterpretation] =
    useState<PreviewInterpretation>('native');
  const [rulesOpen, setRulesOpen] = useState(false);
  // 规则草稿（T08）：随面板草稿一起保存；初始值取解析后的 stored.rules
  // （T08 起宽容解析不再丢 rules，T07 的读原始值 workaround 已移除）。
  const [ruleDrafts, setRuleDrafts] = useState<ReadonlyArray<ValuePresentationRule>>(
    () => stored.rules ?? [],
  );
  // 正在编辑的规则：'new' 为新增，{ index } 为编辑第 index 条，null 为未编辑。
  const [editingRule, setEditingRule] = useState<{ index: number } | 'new' | null>(null);

  // T11：草稿最后一次同步的已保存值（appliedStoredJson 的解析结果）。
  // 保存时的按路径脏检查以它为准，而不是以当前 store 值——当前值
  // 可能已被另一窗口改过；以当前值为准会把未动过的路径误判为脏，
  // 反而用陈旧草稿覆盖对方刚保存的字段。
  const appliedStored = useMemo(
    () => parseStoredValuePresentationSettings(JSON.parse(appliedStoredJson) as unknown),
    [appliedStoredJson],
  );
  const appliedDraft = useMemo(() => draftFromStored(appliedStored), [appliedStored]);
  const appliedRules: ReadonlyArray<ValuePresentationRule> = appliedStored.rules ?? [];

  // 待保存的选项：IANA 名取输入框的实时值（去空白）；其余与草稿一致。
  const optionsToSave: TimestampOptions = useMemo(() => {
    const displayTimeZone: TimeZoneSpec =
      draft.options.displayTimeZone.kind === 'iana'
        ? { kind: 'iana', name: ianaName.trim() }
        : draft.options.displayTimeZone;
    return draft.options.sourceTimeZone === undefined
      ? {
          interpretation: draft.options.interpretation,
          format: draft.options.format,
          displayTimeZone,
        }
      : {
          interpretation: draft.options.interpretation,
          format: draft.options.format,
          displayTimeZone,
          sourceTimeZone: draft.options.sourceTimeZone,
        };
  }, [draft, ianaName]);

  const ianaNameTrimmed = ianaName.trim();
  const ianaFormatInvalid =
    draft.options.displayTimeZone.kind === 'iana' &&
    ianaNameTrimmed !== '' &&
    !isValidIanaTimeZoneName(ianaNameTrimmed);
  const ianaUnknown =
    draft.options.displayTimeZone.kind === 'iana' &&
    ianaNameTrimmed !== '' &&
    isValidIanaTimeZoneName(ianaNameTrimmed) &&
    !isValidTimeZoneName(ianaNameTrimmed);

  const environment = useMemo(() => resolvePresentationEnvironment(language), [language]);
  const preview = useMemo(
    () =>
      computeTimestampPreview({
        contribution,
        options: buildPreviewOptions(optionsToSave, previewInterpretation),
        previewInterpretation,
        environment,
      }),
    [contribution, optionsToSave, previewInterpretation, environment],
  );

  const savedDraft = useMemo(() => draftFromStored(stored), [stored]);
  const savedRules: ReadonlyArray<ValuePresentationRule> = stored.rules ?? [];
  const isDirty =
    draft.enabled !== savedDraft.enabled ||
    JSON.stringify(optionsToSave) !== JSON.stringify(savedDraft.options) ||
    JSON.stringify(ruleDrafts) !== JSON.stringify(savedRules);

  // T11：外部 store 变化（跨窗口同步、保存成功回写）时的草稿处理。
  // 按路径判断草稿是否还有未保存编辑——某路径只有「与上次同步值
  // 不同、且与新已保存值也不同」时才算未保存：自己的保存回显（草稿
  // 与新值一致）不算，纯外部更新（草稿与上次同步值一致）也不算。
  // 任一路径未保存 → 只提示、不覆盖草稿，且不同步基线
  // （appliedStoredJson 不动），使保存时的按路径脏检查仍以编辑前
  // 的基线为准；否则用已保存值重建草稿并推进基线。本地编辑本身不
  // 触发本 effect（stored 不变时首行即返回）。
  useEffect(() => {
    const json = JSON.stringify(stored);
    if (json === appliedStoredJson) return;
    const newDraft = draftFromStored(stored);
    const newRules: ReadonlyArray<ValuePresentationRule> = stored.rules ?? [];
    const formatUnsaved =
      (draft.enabled !== appliedDraft.enabled ||
        JSON.stringify(optionsToSave) !== JSON.stringify(appliedDraft.options)) &&
      (draft.enabled !== newDraft.enabled ||
        JSON.stringify(optionsToSave) !== JSON.stringify(newDraft.options));
    const rulesUnsaved =
      JSON.stringify(ruleDrafts) !== JSON.stringify(appliedRules) &&
      JSON.stringify(ruleDrafts) !== JSON.stringify(newRules);
    if (formatUnsaved || rulesUnsaved) {
      setExternalUpdate(true);
      return;
    }
    setAppliedStoredJson(json);
    setExternalUpdate(false);
    const next = newDraft;
    setDraft(next);
    setIanaName(
      next.options.displayTimeZone.kind === 'iana' ? next.options.displayTimeZone.name : '',
    );
    setRuleDrafts(stored.rules ?? []);
    setEditingRule(null);
    setError(null);
    setSaved(false);
  }, [stored, appliedStoredJson, draft, ruleDrafts, optionsToSave, appliedDraft, appliedRules]);

  // T11：保存时按路径的脏检查（以最后一次同步的已保存值为准，见
  // appliedStored 注释）；未动过的路径不进 updater，取保存前重读的
  // 权威原文，不覆盖另一窗口刚保存的字段。
  const formatDirty =
    draft.enabled !== appliedDraft.enabled ||
    JSON.stringify(optionsToSave) !== JSON.stringify(appliedDraft.options);
  const rulesDirty = JSON.stringify(ruleDrafts) !== JSON.stringify(appliedRules);

  function validateDraftForSave(): TimestampOptions | null {
    if (draft.options.displayTimeZone.kind === 'iana') {
      if (ianaNameTrimmed === '' || !isValidIanaTimeZoneName(ianaNameTrimmed)) {
        setError(t('settings.valuePresentation.ianaInvalid'));
        return null;
      }
      if (!isValidTimeZoneName(ianaNameTrimmed)) {
        setError(t('settings.valuePresentation.ianaUnknown'));
        return null;
      }
    }
    const validated = validateTimestampOptions(optionsToSave);
    if (!validated.ok) {
      setError(
        `${t('settings.valuePresentation.invalidConfig')}: ${validated.problems
          .map((problem) => `${problem.path}: ${problem.message}`)
          .join('; ')}`,
      );
      return null;
    }
    if (contribution !== undefined) {
      const contributionValidated = contribution.validateConfig(validated.options);
      if (!contributionValidated.ok) {
        setError(
          `${t('settings.valuePresentation.invalidConfig')}: ${contributionValidated.problems
            .map((problem) => `${problem.path}: ${problem.message}`)
            .join('; ')}`,
        );
        return null;
      }
    }
    return validated.options;
  }

  async function handleSave(): Promise<void> {
    const options = validateDraftForSave();
    if (options === null) return;
    // 规则草稿再验证一次（编辑器只交回验证后的规则，此处为防御性复核；
    // 非法规则禁止保存，就地说明原因）。
    for (const rule of ruleDrafts) {
      const validated = validateValuePresentationRule(rule);
      if (!validated.ok) {
        setError(
          `${t('settings.valuePresentation.invalidConfig')}: ${validated.problems
            .map((problem) => `${problem.path}: ${problem.message}`)
            .join('; ')}`,
        );
        return;
      }
    }
    if (typeof updateValuePresentationSettings !== 'function') {
      setError(t('settings.valuePresentation.saveUnavailable'));
      return;
    }
    setSaving(true);
    setError(null);
    // T11：updater 形式——只写用户实际改过的路径（全局开关/格式、
    // 规则列表），其余字段（其他 renderer、列覆盖、未知版本字段）
    // 取保存前重读的权威原文；未知版本仍原样保留 version（T06），
    // rules 只在 version 1 写（withValuePresentationRules 内裁决）。
    try {
      await updateValuePresentationSettings((fresh) => {
        let next = fresh;
        if (formatDirty) {
          next = withTimestampRenderer(next, draft.enabled, options);
        }
        if (rulesDirty) {
          next = withValuePresentationRules(next, ruleDrafts);
        }
        return next;
      });
      setSaved(true);
      setExternalUpdate(false);
    } catch {
      // 保存失败：旧配置继续生效，草稿保留，就地报错，不显示成已保存。
      setError(t('settings.valuePresentation.saveFailed'));
      setSaved(false);
    } finally {
      setSaving(false);
    }
  }

  function handleReset(): void {
    const next = draftFromStored(stored);
    setDraft(next);
    setIanaName(
      next.options.displayTimeZone.kind === 'iana' ? next.options.displayTimeZone.name : '',
    );
    setRuleDrafts(stored.rules ?? []);
    setEditingRule(null);
    setError(null);
    setSaved(false);
    // T11：重新加载已保存的设置同时清除外部更新提示（同一操作）。
    setExternalUpdate(false);
  }

  function handleRuleSave(rule: ValuePresentationRule): void {
    setRuleDrafts((prev) =>
      editingRule === 'new'
        ? [...prev, rule]
        : prev.map((entry, index) =>
            editingRule !== null && index === editingRule.index ? rule : entry,
          ),
    );
    setEditingRule(null);
    setSaved(false);
  }

  function moveRule(index: number, direction: -1 | 1): void {
    setRuleDrafts((prev) => {
      const target = index + direction;
      if (target < 0 || target >= prev.length) return prev;
      const next = [...prev];
      const entry = next[index];
      const other = next[target];
      if (entry === undefined || other === undefined) return prev;
      next[index] = other;
      next[target] = entry;
      return next;
    });
    setSaved(false);
  }

  function handleFormatChange(value: string): void {
    if (!isOutputFormat(value)) return;
    setDraft((prev) => ({ ...prev, options: { ...prev.options, format: value } }));
    setSaved(false);
  }

  function handleTimeZoneKindChange(value: string): void {
    if (!isTimeZoneKind(value)) return;
    setDraft((prev) => ({
      ...prev,
      options: {
        ...prev.options,
        displayTimeZone:
          value === 'iana' ? { kind: 'iana', name: ianaName.trim() } : { kind: value },
      },
    }));
    setSaved(false);
  }

  function handlePreviewInterpretationChange(value: string): void {
    if (!isPreviewInterpretation(value)) return;
    setPreviewInterpretation(value);
  }

  return (
    <div className="space-y-4 border-t border-edge pt-4" data-testid="value-presentation-settings">
      <div>
        <h3 className="text-sm font-semibold text-fg">{t('settings.valuePresentation.title')}</h3>
        <p className="mt-1 text-xs leading-5 text-fg-muted">
          {t('settings.valuePresentation.description')}
        </p>
      </div>

      {externalUpdate && (
        <div
          role="status"
          data-testid="value-presentation-external-update"
          className="rounded-md border border-edge bg-surface-alt px-3 py-2 text-sm text-fg-secondary"
        >
          <p>{t('settings.valuePresentation.externalUpdate')}</p>
          <div className="mt-2">
            <Button
              variant="ghost"
              onClick={handleReset}
              data-testid="value-presentation-reload-saved"
            >
              {t('settings.valuePresentation.reloadSaved')}
            </Button>
          </div>
        </div>
      )}

      {contribution === undefined && (
        <p
          role="status"
          data-testid="value-presentation-contribution-missing"
          className="rounded-md border border-edge bg-surface-alt px-3 py-2 text-sm text-fg-secondary"
        >
          {t('settings.valuePresentation.contributionMissing')}
        </p>
      )}

      <ToggleRow
        label={t('settings.valuePresentation.enable')}
        hint={t('settings.valuePresentation.enableHint')}
        checked={draft.enabled}
        onChange={(v) => {
          setDraft((prev) => ({ ...prev, enabled: v }));
          setSaved(false);
        }}
        testId="value-presentation-enable"
      />

      <SettingRow label={t('settings.valuePresentation.format')}>
        <Select
          value={draft.options.format}
          options={OUTPUT_FORMATS.map((format) => ({
            value: format,
            label: t(`settings.valuePresentation.format.${format}`),
          }))}
          onChange={handleFormatChange}
          data-testid="value-presentation-format"
        />
      </SettingRow>

      <SettingRow label={t('settings.valuePresentation.displayTimeZone')}>
        <Select
          value={draft.options.displayTimeZone.kind}
          options={TIME_ZONE_KINDS.map((kind) => ({
            value: kind,
            label: t(`settings.valuePresentation.timeZone.${kind}`),
          }))}
          onChange={handleTimeZoneKindChange}
          data-testid="value-presentation-timezone"
        />
      </SettingRow>

      {draft.options.displayTimeZone.kind === 'iana' && (
        <SettingRow label={t('settings.valuePresentation.ianaName')}>
          <input
            type="text"
            value={ianaName}
            placeholder="Asia/Shanghai"
            onChange={(event) => {
              setIanaName(event.target.value);
              setSaved(false);
            }}
            className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/25"
            data-testid="value-presentation-iana-name"
          />
          {ianaFormatInvalid && (
            <p data-testid="value-presentation-iana-error" className="mt-1 text-xs text-red-500">
              {t('settings.valuePresentation.ianaInvalid')}
            </p>
          )}
          {ianaUnknown && (
            <p data-testid="value-presentation-iana-error" className="mt-1 text-xs text-red-500">
              {t('settings.valuePresentation.ianaUnknown')}
            </p>
          )}
        </SettingRow>
      )}

      <div className="space-y-2">
        <SettingRow label={t('settings.valuePresentation.preview.source')}>
          <Select
            value={previewInterpretation}
            options={PREVIEW_INTERPRETATIONS.map((interpretation) => ({
              value: interpretation,
              label: t(`settings.valuePresentation.preview.source.${interpretation}`),
            }))}
            onChange={handlePreviewInterpretationChange}
            data-testid="value-presentation-preview-source"
          />
        </SettingRow>

        <div
          className="rounded-md border border-edge bg-surface-alt p-3 text-sm"
          data-testid="value-presentation-preview"
        >
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-fg-muted">
              {t('settings.valuePresentation.preview.original')}
            </span>
            <code className="font-mono text-fg">{formatPreviewSample(preview.sample)}</code>
            {previewInterpretation === 'unix-seconds' && (
              <span
                data-testid="value-presentation-preview-unix-badge"
                className="rounded bg-accent/15 px-1.5 py-0.5 text-[11px] font-medium text-accent"
              >
                {t('settings.valuePresentation.preview.unixSeconds')}
              </span>
            )}
            {previewInterpretation === 'unix-milliseconds' && (
              <span
                data-testid="value-presentation-preview-unix-badge"
                className="rounded bg-accent/15 px-1.5 py-0.5 text-[11px] font-medium text-accent"
              >
                {t('settings.valuePresentation.preview.unixMilliseconds')}
              </span>
            )}
          </div>
          <div className="mt-1.5">
            {preview.kind === 'text' && (
              <span data-testid="value-presentation-preview-result" className="font-mono text-fg">
                {preview.text}
              </span>
            )}
            {preview.kind === 'unchanged' && (
              <span data-testid="value-presentation-preview-result" className="text-fg-muted">
                {t('settings.valuePresentation.preview.unchanged')}
                {preview.reason !== undefined ? `: ${preview.reason}` : ''}
              </span>
            )}
            {preview.kind === 'invalid' && (
              <span data-testid="value-presentation-preview-result" className="text-red-500">
                {t('settings.valuePresentation.preview.invalid')}
                {': '}
                {preview.problems
                  .map((problem) => `${problem.path}: ${problem.message}`)
                  .join('; ')}
              </span>
            )}
            {preview.kind === 'unavailable' && (
              <span data-testid="value-presentation-preview-result" className="text-fg-muted">
                {t('settings.valuePresentation.preview.unavailable')}
              </span>
            )}
          </div>
        </div>
      </div>

      <div className="rounded-md border border-edge">
        <button
          type="button"
          aria-expanded={rulesOpen}
          data-testid="value-presentation-rules-toggle"
          onClick={() => setRulesOpen((v) => !v)}
          className="flex w-full items-center justify-between px-3 py-2 text-left text-sm text-fg-secondary"
        >
          <span>
            {t('settings.valuePresentation.rules.title')} ({ruleDrafts.length})
          </span>
          <span aria-hidden="true">{rulesOpen ? '▾' : '▸'}</span>
        </button>
        {rulesOpen && (
          <div className="border-t border-edge px-3 py-2">
            {ruleDrafts.length === 0 ? (
              <p data-testid="value-presentation-rules-empty" className="text-sm text-fg-muted">
                {t('settings.valuePresentation.rules.empty')}
              </p>
            ) : (
              <ul
                data-testid="value-presentation-rules-list"
                className="space-y-2 text-sm text-fg-secondary"
              >
                {ruleDrafts.map((rule, index) => (
                  <li
                    key={`${rule.name}-${index}`}
                    data-testid={`value-presentation-rule-${index}`}
                    className="rounded-md border border-edge bg-surface-alt px-3 py-2"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-medium text-fg">{rule.name}</span>
                      <span className="text-xs text-fg-muted">
                        {rule.enabled
                          ? t('settings.valuePresentation.rules.enabled')
                          : t('settings.valuePresentation.rules.disabled')}
                      </span>
                    </div>
                    <p className="mt-1 font-mono text-xs text-fg-muted">
                      {describeRuleMatch(rule.match)}
                    </p>
                    <div className="mt-2 flex flex-wrap items-center gap-1">
                      <button
                        type="button"
                        disabled={editingRule !== null}
                        onClick={() => setEditingRule({ index })}
                        data-testid={`value-presentation-rule-edit-${index}`}
                        className="rounded px-2 py-1 text-xs text-accent hover:bg-accent/10 disabled:opacity-40"
                      >
                        {t('settings.valuePresentation.rules.edit')}
                      </button>
                      <button
                        type="button"
                        disabled={editingRule !== null}
                        onClick={() => setRuleDrafts((prev) => prev.filter((_, i) => i !== index))}
                        data-testid={`value-presentation-rule-delete-${index}`}
                        className="rounded px-2 py-1 text-xs text-red-500 hover:bg-red-500/10 disabled:opacity-40"
                      >
                        {t('settings.valuePresentation.rules.delete')}
                      </button>
                      <button
                        type="button"
                        disabled={editingRule !== null || index === 0}
                        onClick={() => moveRule(index, -1)}
                        data-testid={`value-presentation-rule-up-${index}`}
                        className="rounded px-2 py-1 text-xs text-fg-secondary hover:bg-surface disabled:opacity-40"
                      >
                        {t('settings.valuePresentation.rules.moveUp')}
                      </button>
                      <button
                        type="button"
                        disabled={editingRule !== null || index === ruleDrafts.length - 1}
                        onClick={() => moveRule(index, 1)}
                        data-testid={`value-presentation-rule-down-${index}`}
                        className="rounded px-2 py-1 text-xs text-fg-secondary hover:bg-surface disabled:opacity-40"
                      >
                        {t('settings.valuePresentation.rules.moveDown')}
                      </button>
                      <button
                        type="button"
                        disabled={editingRule !== null}
                        onClick={() =>
                          setRuleDrafts((prev) =>
                            prev.map((entry, i) =>
                              i === index ? { ...entry, enabled: !entry.enabled } : entry,
                            ),
                          )
                        }
                        data-testid={`value-presentation-rule-toggle-${index}`}
                        className="rounded px-2 py-1 text-xs text-fg-secondary hover:bg-surface disabled:opacity-40"
                      >
                        {rule.enabled
                          ? t('settings.valuePresentation.rules.disable')
                          : t('settings.valuePresentation.rules.enable')}
                      </button>
                    </div>
                  </li>
                ))}
              </ul>
            )}
            {editingRule !== null && (
              <ValuePresentationRuleEditor
                initialRule={editingRule === 'new' ? null : (ruleDrafts[editingRule.index] ?? null)}
                otherNames={ruleDrafts
                  .filter((_, i) => editingRule === 'new' || i !== editingRule.index)
                  .map((rule) => rule.name)}
                contribution={contribution}
                environment={environment}
                onSave={handleRuleSave}
                onCancel={() => setEditingRule(null)}
              />
            )}
            <div className="mt-2 flex items-center gap-2">
              <Button
                variant="ghost"
                disabled={editingRule !== null || ruleDrafts.length >= MAX_VALUE_PRESENTATION_RULES}
                onClick={() => setEditingRule('new')}
                data-testid="value-presentation-rule-add"
              >
                {t('settings.valuePresentation.rules.add')}
              </Button>
            </div>
            <p className="mt-2 text-xs text-fg-muted">
              {t('settings.valuePresentation.rules.limits', {
                maxRules: MAX_VALUE_PRESENTATION_RULES,
                maxPattern: MAX_PATTERN_LENGTH,
              })}
            </p>
          </div>
        )}
      </div>

      {error !== null && (
        <p role="alert" data-testid="value-presentation-error" className="text-sm text-red-500">
          {error}
        </p>
      )}

      <div className="flex items-center gap-2">
        <Button
          variant="primary"
          disabled={!isDirty || saving}
          onClick={() => void handleSave()}
          data-testid="value-presentation-save"
        >
          {saving ? t('settings.valuePresentation.saving') : t('settings.valuePresentation.save')}
        </Button>
        <Button
          variant="ghost"
          disabled={!isDirty}
          onClick={handleReset}
          data-testid="value-presentation-reset"
        >
          {t('settings.valuePresentation.reset')}
        </Button>
        {saved && !isDirty && (
          <span data-testid="value-presentation-saved" className="text-xs text-success">
            {t('settings.valuePresentation.saved')}
          </span>
        )}
      </div>
    </div>
  );
}
