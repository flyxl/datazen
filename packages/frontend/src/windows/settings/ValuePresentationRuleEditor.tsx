/**
 * ValuePresentationRuleEditor — 单条全局规则的编辑表单（issue-32 T08，
 * PRD §3.1，设计 §5）。挂在 ValuePresentationSettingsSection 的可折叠
 * 规则列表内，新增/编辑时展开；只改本地草稿，点「保存规则」才把
 * 验证后的规则交回面板（面板保存时才写 store）。
 *
 * 设计要点：
 *
 * 1. 表单 → 规则的唯一真源是 `validateValuePresentationRule`：保存与
 *    样本预览都先把表单拼成 unknown 再验证，非法表达式或配置禁止
 *    保存，并就地列出路径化原因（行内错误，不是全局 toast）。
 * 2. 样本预览与运行时同一条路：验证后的规则 options 经贡献
 *    prepare/present 计算命中结果与预览（computeRuleSamplePreview）。
 *    样本全空时仍允许配置，显示「无样本」提示，不编造真实数据。
 * 3. 下拉框用原生 <select>（密集表单，data-testid 可直测）；面板
 *    其它控件沿用共享 Select，两者视觉样式保持一致。
 * 4. 正则大小写开关：regex 模式下「忽略大小写」对应 flags 'i'，
 *    其余 flags 一律不提供（受限子集，见 valuePresentationRules）。
 */

import { useMemo, useState } from 'react';
import type {
  PresentationEnvironment,
  PresentationValue,
  ValuePresentationContribution,
} from '@datazen/extension-points';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import {
  matchValuePresentationRule,
  resolvePresentationSemanticKind,
  timestampOptionsToConfigValue,
  validateValuePresentationRule,
  type TimestampOutputFormat,
  type TimeZoneSpec,
  type ValuePresentationRule,
  type ValuePresentationRuleContext,
} from '../../lib/valuePresentation/index';
import { SettingRow } from './settingsUi';

const INPUT_CLASS =
  'h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/25';

type PatternMode = 'exact' | 'regex';
type InterpretationChoice = 'native' | 'unix-seconds' | 'unix-milliseconds' | 'text';
type SourceTimeZoneChoice = 'none' | 'local' | 'utc' | 'iana';

interface RuleFormState {
  name: string;
  enabled: boolean;
  connectionId: string;
  database: string;
  namespace: string;
  tableMode: PatternMode;
  tableValue: string;
  tableIgnoreCase: boolean;
  columnMode: PatternMode;
  columnValue: string;
  columnIgnoreCase: boolean;
  typeMode: PatternMode;
  typeValue: string;
  typeIgnoreCase: boolean;
  interpretation: InterpretationChoice;
  format: TimestampOutputFormat;
  displayTimeZoneKind: 'local' | 'utc' | 'iana';
  displayIanaName: string;
  sourceTimeZoneKind: SourceTimeZoneChoice;
  sourceIanaName: string;
  sampleValue: string;
  sampleColumn: string;
  sampleType: string;
  sampleConnection: string;
  sampleDatabase: string;
  sampleNamespace: string;
  sampleTable: string;
}

const DEFAULT_FORM: RuleFormState = {
  name: '',
  enabled: true,
  connectionId: '',
  database: '',
  namespace: '',
  tableMode: 'exact',
  tableValue: '',
  tableIgnoreCase: false,
  columnMode: 'regex',
  columnValue: '',
  columnIgnoreCase: false,
  typeMode: 'exact',
  typeValue: '',
  typeIgnoreCase: false,
  interpretation: 'unix-milliseconds',
  format: 'iso-tz',
  displayTimeZoneKind: 'local',
  displayIanaName: '',
  sourceTimeZoneKind: 'none',
  sourceIanaName: '',
  sampleValue: '',
  sampleColumn: '',
  sampleType: '',
  sampleConnection: '',
  sampleDatabase: '',
  sampleNamespace: '',
  sampleTable: '',
};

function formFromRule(rule: ValuePresentationRule): RuleFormState {
  const match = rule.match;
  const options = rule.options;
  return {
    ...DEFAULT_FORM,
    name: rule.name,
    enabled: rule.enabled,
    connectionId: match.connectionId ?? '',
    database: match.database ?? '',
    namespace: match.namespace ?? '',
    tableMode: match.table?.kind ?? 'exact',
    tableValue:
      match.table === undefined
        ? ''
        : match.table.kind === 'exact'
          ? match.table.value
          : match.table.source,
    tableIgnoreCase: match.table?.kind === 'regex' && match.table.flags === 'i',
    columnMode: match.column?.kind ?? 'regex',
    columnValue:
      match.column === undefined
        ? ''
        : match.column.kind === 'exact'
          ? match.column.value
          : match.column.source,
    columnIgnoreCase: match.column?.kind === 'regex' && match.column.flags === 'i',
    typeMode: match.databaseTypeName?.kind ?? 'exact',
    typeValue:
      match.databaseTypeName === undefined
        ? ''
        : match.databaseTypeName.kind === 'exact'
          ? match.databaseTypeName.value
          : match.databaseTypeName.source,
    typeIgnoreCase:
      match.databaseTypeName?.kind === 'regex' && match.databaseTypeName.flags === 'i',
    interpretation:
      options.interpretation.kind === 'unix'
        ? options.interpretation.unit === 'seconds'
          ? 'unix-seconds'
          : 'unix-milliseconds'
        : options.interpretation.kind,
    format: options.format,
    displayTimeZoneKind: options.displayTimeZone.kind,
    displayIanaName: options.displayTimeZone.kind === 'iana' ? options.displayTimeZone.name : '',
    sourceTimeZoneKind: options.sourceTimeZone?.kind ?? 'none',
    sourceIanaName: options.sourceTimeZone?.kind === 'iana' ? options.sourceTimeZone.name : '',
  };
}

function patternInput(
  mode: PatternMode,
  value: string,
  ignoreCase: boolean,
): Record<string, unknown> | undefined {
  if (value.trim() === '') return undefined;
  if (mode === 'exact') return { kind: 'exact', value: value.trim() };
  return ignoreCase
    ? { kind: 'regex', source: value.trim(), flags: 'i' }
    : { kind: 'regex', source: value.trim() };
}

function timeZoneSpec(kind: 'local' | 'utc' | 'iana', ianaName: string): TimeZoneSpec {
  return kind === 'iana' ? { kind: 'iana', name: ianaName.trim() } : { kind };
}

/** 表单拼成规则输入（unknown），唯一验证入口是 validateValuePresentationRule。 */
export function formToRuleInput(form: Readonly<RuleFormState>): unknown {
  const match: Record<string, unknown> = {};
  if (form.connectionId.trim() !== '') match.connectionId = form.connectionId.trim();
  if (form.database.trim() !== '') match.database = form.database.trim();
  if (form.namespace.trim() !== '') match.namespace = form.namespace.trim();
  const table = patternInput(form.tableMode, form.tableValue, form.tableIgnoreCase);
  if (table !== undefined) match.table = table;
  const column = patternInput(form.columnMode, form.columnValue, form.columnIgnoreCase);
  if (column !== undefined) match.column = column;
  const type = patternInput(form.typeMode, form.typeValue, form.typeIgnoreCase);
  if (type !== undefined) match.databaseTypeName = type;

  const interpretation =
    form.interpretation === 'unix-seconds'
      ? { kind: 'unix', unit: 'seconds' }
      : form.interpretation === 'unix-milliseconds'
        ? { kind: 'unix', unit: 'milliseconds' }
        : { kind: form.interpretation };
  const options: Record<string, unknown> = {
    interpretation,
    format: form.format,
    displayTimeZone: timeZoneSpec(form.displayTimeZoneKind, form.displayIanaName),
  };
  if (form.sourceTimeZoneKind !== 'none') {
    options.sourceTimeZone = timeZoneSpec(form.sourceTimeZoneKind, form.sourceIanaName);
  }
  return { name: form.name, enabled: form.enabled, match, options };
}

// ---------------------------------------------------------------------------
// 样本预览：与运行时同一条路（贡献 prepare / present）
// ---------------------------------------------------------------------------

export type RuleSamplePreview =
  | { kind: 'no-sample' }
  | { kind: 'unavailable' }
  | { kind: 'not-matched' }
  | { kind: 'matched'; text: string; interpretation: string | undefined }
  | { kind: 'matched-unchanged'; reason: string | undefined };

export function computeRuleSamplePreview(input: {
  rule: ValuePresentationRule;
  sampleValue: string;
  sampleColumn: string;
  sampleTypeName?: string;
  sampleConnectionId?: string;
  sampleDatabase?: string;
  sampleNamespace?: string;
  sampleTable?: string;
  contribution: ValuePresentationContribution | undefined;
  environment: PresentationEnvironment;
}): RuleSamplePreview {
  const value = input.sampleValue.trim();
  const column = input.sampleColumn.trim();
  if (value === '' || column === '') return { kind: 'no-sample' };
  const context: ValuePresentationRuleContext = {
    connectionId: input.sampleConnectionId,
    database: input.sampleDatabase,
    namespace: input.sampleNamespace,
    table: input.sampleTable,
    column,
    databaseTypeName: input.sampleTypeName,
  };
  if (!matchValuePresentationRule(input.rule, context)) return { kind: 'not-matched' };
  if (input.contribution === undefined) return { kind: 'unavailable' };
  const typeName = input.sampleTypeName;
  const source =
    input.sampleTable !== undefined && input.sampleConnectionId !== undefined
      ? {
          connectionId: input.sampleConnectionId,
          database: input.sampleDatabase,
          namespace: input.sampleNamespace,
          table: input.sampleTable,
          column,
        }
      : undefined;
  const prepared = input.contribution.prepare({
    config: {
      configVersion: input.contribution.configVersion,
      config: timestampOptionsToConfigValue(input.rule.options),
    },
    column: {
      columnName: column,
      databaseTypeName: typeName,
      semanticKind: resolvePresentationSemanticKind({
        name: column,
        dataType: typeName,
      }),
      source,
    },
    environment: input.environment,
  });
  // 样本值按原样字符串传入：Unix 解析接受整数字符串，native/text 本来
  // 就是字符串；与运行时 present 的解释路径一致。
  const result = prepared.present(value as PresentationValue);
  if (result.kind === 'text') {
    return { kind: 'matched', text: result.text, interpretation: result.interpretation };
  }
  return { kind: 'matched-unchanged', reason: result.reason };
}

// ---------------------------------------------------------------------------
// 编辑器
// ---------------------------------------------------------------------------

export interface ValuePresentationRuleEditorProps {
  /** null 表示新增；非 null 表示编辑该规则（表单据此初始化）。 */
  readonly initialRule: ValuePresentationRule | null;
  /** 列表中除正在编辑者外的其它规则名（重名检查用）。 */
  readonly otherNames: ReadonlyArray<string>;
  readonly contribution: ValuePresentationContribution | undefined;
  readonly environment: PresentationEnvironment;
  readonly onSave: (rule: ValuePresentationRule) => void;
  readonly onCancel: () => void;
}

export function ValuePresentationRuleEditor({
  initialRule,
  otherNames,
  contribution,
  environment,
  onSave,
  onCancel,
}: ValuePresentationRuleEditorProps) {
  const { t } = useI18n();
  const [form, setForm] = useState<RuleFormState>(() =>
    initialRule === null ? DEFAULT_FORM : formFromRule(initialRule),
  );
  const [error, setError] = useState<string | null>(null);

  const validated = useMemo(() => validateValuePresentationRule(formToRuleInput(form)), [form]);

  const samplePreview = useMemo((): RuleSamplePreview => {
    if (!validated.ok) return { kind: 'no-sample' };
    return computeRuleSamplePreview({
      rule: validated.rule,
      sampleValue: form.sampleValue,
      sampleColumn: form.sampleColumn,
      sampleTypeName: form.sampleType.trim() === '' ? undefined : form.sampleType.trim(),
      sampleConnectionId:
        form.sampleConnection.trim() === '' ? undefined : form.sampleConnection.trim(),
      sampleDatabase: form.sampleDatabase.trim() === '' ? undefined : form.sampleDatabase.trim(),
      sampleNamespace: form.sampleNamespace.trim() === '' ? undefined : form.sampleNamespace.trim(),
      sampleTable: form.sampleTable.trim() === '' ? undefined : form.sampleTable.trim(),
      contribution,
      environment,
    });
  }, [validated, form, contribution, environment]);

  function update(patch: Partial<RuleFormState>): void {
    setForm((prev) => ({ ...prev, ...patch }));
    setError(null);
  }

  function handleSaveRule(): void {
    if (!validated.ok) {
      setError(
        `${t('settings.valuePresentation.rules.editor.invalidRule')}: ${validated.problems
          .map((problem) => `${problem.path}: ${problem.message}`)
          .join('; ')}`,
      );
      return;
    }
    if (otherNames.includes(validated.rule.name)) {
      setError(t('settings.valuePresentation.rules.editor.duplicateName'));
      return;
    }
    onSave(validated.rule);
  }

  const showSourceTimeZone = form.interpretation === 'native' || form.interpretation === 'text';
  const interpretationOptions: ReadonlyArray<{ value: InterpretationChoice; label: string }> = [
    { value: 'native', label: t('settings.valuePresentation.preview.source.native') },
    { value: 'unix-seconds', label: t('settings.valuePresentation.preview.source.unix-seconds') },
    {
      value: 'unix-milliseconds',
      label: t('settings.valuePresentation.preview.source.unix-milliseconds'),
    },
    { value: 'text', label: t('settings.valuePresentation.preview.source.text') },
  ];

  return (
    <div
      className="mt-2 space-y-3 rounded-md border border-edge bg-surface p-3"
      data-testid="value-presentation-rule-editor"
    >
      <h4 className="text-sm font-semibold text-fg">
        {initialRule === null
          ? t('settings.valuePresentation.rules.editor.newTitle')
          : t('settings.valuePresentation.rules.editor.editTitle')}
      </h4>

      <SettingRow label={t('settings.valuePresentation.rules.editor.name')}>
        <input
          type="text"
          value={form.name}
          onChange={(event) => update({ name: event.target.value })}
          className={INPUT_CLASS}
          data-testid="value-presentation-rule-name"
        />
      </SettingRow>

      <label className="flex items-center gap-2 text-sm text-fg-secondary">
        <input
          type="checkbox"
          checked={form.enabled}
          onChange={(event) => update({ enabled: event.target.checked })}
          data-testid="value-presentation-rule-enabled"
        />
        {t('settings.valuePresentation.rules.editor.enabled')}
      </label>

      <fieldset className="space-y-2">
        <legend className="text-sm font-medium text-fg">
          {t('settings.valuePresentation.rules.editor.matchTitle')}
        </legend>
        <SettingRow label={t('settings.valuePresentation.rules.editor.connection')}>
          <input
            type="text"
            value={form.connectionId}
            onChange={(event) => update({ connectionId: event.target.value })}
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-connection"
          />
        </SettingRow>
        <SettingRow label={t('settings.valuePresentation.rules.editor.database')}>
          <input
            type="text"
            value={form.database}
            onChange={(event) => update({ database: event.target.value })}
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-database"
          />
        </SettingRow>
        <SettingRow label={t('settings.valuePresentation.rules.editor.namespace')}>
          <input
            type="text"
            value={form.namespace}
            onChange={(event) => update({ namespace: event.target.value })}
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-namespace"
          />
        </SettingRow>
        <PatternField
          label={t('settings.valuePresentation.rules.editor.table')}
          mode={form.tableMode}
          value={form.tableValue}
          ignoreCase={form.tableIgnoreCase}
          onModeChange={(mode) => update({ tableMode: mode })}
          onValueChange={(value) => update({ tableValue: value })}
          onIgnoreCaseChange={(v) => update({ tableIgnoreCase: v })}
          modeTestId="value-presentation-rule-table-mode"
          valueTestId="value-presentation-rule-table-value"
          ignoreCaseTestId="value-presentation-rule-table-ignore-case"
          t={t}
        />
        <PatternField
          label={t('settings.valuePresentation.rules.editor.column')}
          mode={form.columnMode}
          value={form.columnValue}
          ignoreCase={form.columnIgnoreCase}
          onModeChange={(mode) => update({ columnMode: mode })}
          onValueChange={(value) => update({ columnValue: value })}
          onIgnoreCaseChange={(v) => update({ columnIgnoreCase: v })}
          modeTestId="value-presentation-rule-column-mode"
          valueTestId="value-presentation-rule-column-value"
          ignoreCaseTestId="value-presentation-rule-column-ignore-case"
          t={t}
        />
        <PatternField
          label={t('settings.valuePresentation.rules.editor.typeName')}
          mode={form.typeMode}
          value={form.typeValue}
          ignoreCase={form.typeIgnoreCase}
          onModeChange={(mode) => update({ typeMode: mode })}
          onValueChange={(value) => update({ typeValue: value })}
          onIgnoreCaseChange={(v) => update({ typeIgnoreCase: v })}
          modeTestId="value-presentation-rule-type-mode"
          valueTestId="value-presentation-rule-type-value"
          ignoreCaseTestId="value-presentation-rule-type-ignore-case"
          t={t}
        />
      </fieldset>

      <fieldset className="space-y-2">
        <legend className="text-sm font-medium text-fg">
          {t('settings.valuePresentation.rules.editor.optionsTitle')}
        </legend>
        <SettingRow label={t('settings.valuePresentation.rules.editor.interpretation')}>
          <select
            value={form.interpretation}
            onChange={(event) =>
              update({ interpretation: event.target.value as InterpretationChoice })
            }
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-interpretation"
          >
            {interpretationOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </SettingRow>
        <SettingRow label={t('settings.valuePresentation.format')}>
          <select
            value={form.format}
            onChange={(event) => update({ format: event.target.value as TimestampOutputFormat })}
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-format"
          >
            {(['iso', 'iso-tz', 'locale'] as const).map((format) => (
              <option key={format} value={format}>
                {t(`settings.valuePresentation.format.${format}`)}
              </option>
            ))}
          </select>
        </SettingRow>
        <SettingRow label={t('settings.valuePresentation.displayTimeZone')}>
          <select
            value={form.displayTimeZoneKind}
            onChange={(event) =>
              update({
                displayTimeZoneKind: event.target.value as 'local' | 'utc' | 'iana',
              })
            }
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-display-timezone"
          >
            {(['local', 'utc', 'iana'] as const).map((kind) => (
              <option key={kind} value={kind}>
                {t(`settings.valuePresentation.timeZone.${kind}`)}
              </option>
            ))}
          </select>
        </SettingRow>
        {form.displayTimeZoneKind === 'iana' && (
          <SettingRow label={t('settings.valuePresentation.ianaName')}>
            <input
              type="text"
              value={form.displayIanaName}
              placeholder="Asia/Shanghai"
              onChange={(event) => update({ displayIanaName: event.target.value })}
              className={INPUT_CLASS}
              data-testid="value-presentation-rule-display-iana-name"
            />
          </SettingRow>
        )}
        {showSourceTimeZone && (
          <SettingRow label={t('settings.valuePresentation.rules.editor.sourceTimeZone')}>
            <select
              value={form.sourceTimeZoneKind}
              onChange={(event) =>
                update({ sourceTimeZoneKind: event.target.value as SourceTimeZoneChoice })
              }
              className={INPUT_CLASS}
              data-testid="value-presentation-rule-source-timezone"
            >
              <option value="none">
                {t('settings.valuePresentation.rules.editor.sourceTimeZone.none')}
              </option>
              {(['local', 'utc', 'iana'] as const).map((kind) => (
                <option key={kind} value={kind}>
                  {t(`settings.valuePresentation.timeZone.${kind}`)}
                </option>
              ))}
            </select>
          </SettingRow>
        )}
        {showSourceTimeZone && form.sourceTimeZoneKind === 'iana' && (
          <SettingRow label={t('settings.valuePresentation.ianaName')}>
            <input
              type="text"
              value={form.sourceIanaName}
              placeholder="Asia/Shanghai"
              onChange={(event) => update({ sourceIanaName: event.target.value })}
              className={INPUT_CLASS}
              data-testid="value-presentation-rule-source-iana-name"
            />
          </SettingRow>
        )}
      </fieldset>

      <fieldset className="space-y-2">
        <legend className="text-sm font-medium text-fg">
          {t('settings.valuePresentation.rules.editor.sampleTitle')}
        </legend>
        <SettingRow label={t('settings.valuePresentation.rules.editor.sampleValue')}>
          <input
            type="text"
            value={form.sampleValue}
            onChange={(event) => update({ sampleValue: event.target.value })}
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-sample-value"
          />
        </SettingRow>
        <SettingRow label={t('settings.valuePresentation.rules.editor.column')}>
          <input
            type="text"
            value={form.sampleColumn}
            onChange={(event) => update({ sampleColumn: event.target.value })}
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-sample-column"
          />
        </SettingRow>
        <SettingRow label={t('settings.valuePresentation.rules.editor.typeName')}>
          <input
            type="text"
            value={form.sampleType}
            onChange={(event) => update({ sampleType: event.target.value })}
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-sample-type"
          />
        </SettingRow>
        <SettingRow label={t('settings.valuePresentation.rules.editor.table')}>
          <input
            type="text"
            value={form.sampleTable}
            onChange={(event) => update({ sampleTable: event.target.value })}
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-sample-table"
          />
        </SettingRow>
        <SettingRow label={t('settings.valuePresentation.rules.editor.connection')}>
          <input
            type="text"
            value={form.sampleConnection}
            onChange={(event) => update({ sampleConnection: event.target.value })}
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-sample-connection"
          />
        </SettingRow>
        <SettingRow label={t('settings.valuePresentation.rules.editor.database')}>
          <input
            type="text"
            value={form.sampleDatabase}
            onChange={(event) => update({ sampleDatabase: event.target.value })}
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-sample-database"
          />
        </SettingRow>
        <SettingRow label={t('settings.valuePresentation.rules.editor.namespace')}>
          <input
            type="text"
            value={form.sampleNamespace}
            onChange={(event) => update({ sampleNamespace: event.target.value })}
            className={INPUT_CLASS}
            data-testid="value-presentation-rule-sample-namespace"
          />
        </SettingRow>
        <div
          className="rounded-md border border-edge bg-surface-alt p-3 text-sm"
          data-testid="value-presentation-rule-sample-preview"
        >
          {samplePreview.kind === 'no-sample' && (
            <span className="text-fg-muted">
              {t('settings.valuePresentation.rules.editor.sampleHint')}
            </span>
          )}
          {samplePreview.kind === 'unavailable' && (
            <span className="text-fg-muted">
              {t('settings.valuePresentation.preview.unavailable')}
            </span>
          )}
          {samplePreview.kind === 'not-matched' && (
            <span data-testid="value-presentation-rule-sample-match" className="text-fg-muted">
              {t('settings.valuePresentation.rules.editor.matchMiss')}
            </span>
          )}
          {samplePreview.kind === 'matched' && (
            <span>
              <span data-testid="value-presentation-rule-sample-match" className="text-fg">
                {t('settings.valuePresentation.rules.editor.matchHit')}
              </span>{' '}
              <code className="font-mono text-fg">{samplePreview.text}</code>
            </span>
          )}
          {samplePreview.kind === 'matched-unchanged' && (
            <span>
              <span data-testid="value-presentation-rule-sample-match" className="text-fg">
                {t('settings.valuePresentation.rules.editor.matchHit')}
              </span>{' '}
              <span className="text-fg-muted">
                {t('settings.valuePresentation.preview.unchanged')}
                {samplePreview.reason !== undefined ? `: ${samplePreview.reason}` : ''}
              </span>
            </span>
          )}
        </div>
      </fieldset>

      {error !== null && (
        <p
          role="alert"
          data-testid="value-presentation-rule-error"
          className="text-sm text-red-500"
        >
          {error}
        </p>
      )}

      <div className="flex items-center gap-2">
        <Button
          variant="primary"
          onClick={handleSaveRule}
          data-testid="value-presentation-rule-save"
        >
          {t('settings.valuePresentation.rules.editor.saveRule')}
        </Button>
        <Button variant="ghost" onClick={onCancel} data-testid="value-presentation-rule-cancel">
          {t('settings.valuePresentation.rules.editor.cancel')}
        </Button>
      </div>
    </div>
  );
}

function PatternField({
  label,
  mode,
  value,
  ignoreCase,
  onModeChange,
  onValueChange,
  onIgnoreCaseChange,
  modeTestId,
  valueTestId,
  ignoreCaseTestId,
  t,
}: {
  label: string;
  mode: PatternMode;
  value: string;
  ignoreCase: boolean;
  onModeChange: (mode: PatternMode) => void;
  onValueChange: (value: string) => void;
  onIgnoreCaseChange: (v: boolean) => void;
  modeTestId: string;
  valueTestId: string;
  ignoreCaseTestId: string;
  t: (key: string) => string;
}) {
  return (
    <SettingRow label={label}>
      <div className="flex items-center gap-2">
        <select
          value={mode}
          onChange={(event) => onModeChange(event.target.value as PatternMode)}
          className="h-9 shrink-0 rounded-md border border-edge bg-surface px-2 text-sm text-fg outline-none focus:border-accent"
          data-testid={modeTestId}
          aria-label={label}
        >
          <option value="exact">{t('settings.valuePresentation.rules.editor.exact')}</option>
          <option value="regex">{t('settings.valuePresentation.rules.editor.regex')}</option>
        </select>
        <input
          type="text"
          value={value}
          onChange={(event) => onValueChange(event.target.value)}
          className={INPUT_CLASS}
          data-testid={valueTestId}
        />
        {mode === 'regex' && (
          <label className="flex shrink-0 items-center gap-1 text-xs text-fg-secondary">
            <input
              type="checkbox"
              checked={ignoreCase}
              onChange={(event) => onIgnoreCaseChange(event.target.checked)}
              data-testid={ignoreCaseTestId}
            />
            {t('settings.valuePresentation.rules.editor.ignoreCase')}
          </label>
        )}
      </div>
    </SettingRow>
  );
}
