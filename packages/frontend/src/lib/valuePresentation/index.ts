/**
 * @datazen/frontend lib/valuePresentation — 字段展示扩展（issue-32）。
 *
 * T02 交付的纯逻辑核心（无 React、无 DOM、无 Store 依赖）：
 * - temporalMath：时刻 / 墙上时间解析与转换（DST 双向校验、微秒纳秒精度）。
 * - timestampFormat：iso / iso-tz / locale 三种输出格式。
 * - timestampConfig：时间展示器配置类型与纯验证（JSON 可序列化）。
 *
 * T03 交付的展示器贡献（本文件追加）：
 * - timestampContribution：`datazen.timestamp` 贡献对象（validateConfig /
 *   prepare），装配 T02 核心供宿主展示管线消费；接线 CellRenderer /
 *   DetailPanel 属 T05，不在本目录。
 *
 * T04（宿主展示管线）及之后的管线模块在本目录追加；展示器契约类型见
 * `@datazen/extension-points`（T01 已交付）。
 *
 * T04 交付的宿主展示管线（本文件追加）：
 * - presentationPlan：展示快照、语义种映射、列展示计划构建与缓存键；
 *   纯逻辑，宿主接线 hook 在 `packages/frontend/src/hooks/useValuePresentation.ts`。
 */

export type {
  InstantParts,
  WallDate,
  WallTime,
  WallComponents,
  TemporalParseError,
  TemporalConversionError,
  TemporalFormatError,
  TemporalError,
  InstantParseResult,
  ZonedTextParse,
  UnixValueParse,
  WallClockTextParse,
  WallToInstantResult,
  InstantToWallResult,
} from './temporalMath';
export {
  MIN_SUPPORTED_EPOCH_MS,
  MAX_SUPPORTED_EPOCH_MS,
  isValidTimeZoneName,
  parseZonedTimestamp,
  parseUnixValue,
  parseWallClockText,
  wallClockToInstant,
  instantToWallParts,
  validateInstantParts,
  formatOffsetMinutes,
  formatOffsetSeconds,
  compareInstants,
} from './temporalMath';

export type {
  TimestampOutputFormat,
  TimeZoneSpec,
  TimestampFormatOptions,
  TimestampFormatResult,
} from './timestampFormat';
export {
  isValidIanaTimeZoneName,
  resolveTimeZoneName,
  formatTimestampOutput,
} from './timestampFormat';

export type {
  TimestampInterpretation,
  TimestampOptions,
  TimestampOptionsProblem,
  TimestampOptionsValidation,
} from './timestampConfig';
export { validateTimestampOptions } from './timestampConfig';

export {
  TIMESTAMP_CONTRIBUTION_ID,
  TIMESTAMP_CONTRIBUTION_CONFIG_VERSION,
  timestampContribution,
  timestampOptionsToConfigValue,
} from './timestampContribution';

export type {
  PresentationSemanticKind,
  PresentationPlanColumn,
  ValuePresentationSnapshot,
  ColumnPresentationStatus,
  ColumnPresentationPlan,
  BuildColumnPresentationPlansInput,
  PresentationPlanCacheKeyInput,
} from './presentationPlan';
export {
  resolvePresentationSemanticKind,
  buildPresentationColumnContext,
  columnOverrideKey,
  resolvePresentationEnvironment,
  createDefaultValuePresentationSnapshot,
  fallbackPresentationReason,
  buildColumnPresentationPlans,
  computePresentationPlanCacheKey,
} from './presentationPlan';

// T08（全局规则，设计 §5 / PRD §3.1）：规则模型、受限正则验证、匹配。
export type {
  ValuePresentationNamePattern,
  ValuePresentationRuleMatch,
  ValuePresentationRule,
  ValuePresentationRuleContext,
  ValuePresentationRuleProblem,
  ValuePresentationRuleValidation,
  RestrictedPatternValidation,
} from './valuePresentationRules';
export {
  MAX_VALUE_PRESENTATION_RULES,
  MAX_PATTERN_LENGTH,
  MAX_RULE_NAME_LENGTH,
  MAX_MATCH_VALUE_LENGTH,
  validateRestrictedPattern,
  matchNamePattern,
  describeNamePattern,
  validateValuePresentationRule,
  validateValuePresentationRules,
  findDuplicateRuleName,
  matchValuePresentationRule,
  findFirstMatchingRule,
  describeRuleMatch,
} from './valuePresentationRules';

// T06（设置持久化链路，设计 §5/§6）：存储形状、宽容解析、快照构建与
// 核心内置贡献的启动注册。设置 UI / 规则 / 列覆盖持久化不在 T06。
export type { StoredRendererSettings, StoredValuePresentationSettings } from './settings';
export {
  DEFAULT_VALUE_PRESENTATION_SETTINGS,
  parseStoredValuePresentationSettings,
  buildValuePresentationSnapshot,
} from './settings';
// T09（列配置面板，设计 §5/§9，PRD §3.2）：覆盖校验与预览（纯逻辑）、
// 会话生效接缝（进程内存储 + 合并点；持久化是 T10 的活，见
// columnPresentationSession.ts 文件头）。
export type {
  ColumnPresentationOverrideProblem,
  ColumnPresentationOverrideValidation,
  ColumnPresentationPreview,
  ComputeColumnPresentationPreviewInput,
} from './columnPresentation';
export {
  validateColumnPresentationOverride,
  columnOverrideFromSnapshot,
  snapshotWithoutColumnOverride,
  fallbackSampleForColumn,
  formatColumnPresentationSampleValue,
  computeColumnPresentationPreview,
} from './columnPresentation';
export type { ColumnPresentationSessionState } from './columnPresentationSession';
export {
  getColumnPresentationSessionState,
  subscribeColumnPresentationSession,
  setSessionColumnOverride,
  clearSessionColumnOverride,
  resetColumnPresentationSession,
  applySessionColumnOverrides,
} from './columnPresentationSession';
// T10（表列覆盖持久化，设计 §5/§6，PRD §5/§6 验收 6）：覆盖记录
// 验证、存储值应用与面板保存链路（乐观写会话 → 持久化 → 成功后
// 清除会话条目）。
export type { SaveColumnOverrideInput, SaveColumnOverrideResult } from './columnOverrides';
export {
  MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES,
  validateStoredColumnOverrides,
  withColumnOverride,
  saveColumnOverride,
} from './columnOverrides';
// T11（跨窗口同步与扩展停用回退，设计 §6 / PRD §5/§6 验收 10/12）：
// 路径化保存更新（整份对象或只改拥有路径的 updater），见
// valuePresentationSave.ts 文件头裁决。
export type { ValuePresentationSettingsUpdate } from './valuePresentationSave';
export {
  applyValuePresentationUpdate,
  withTimestampRenderer,
  withValuePresentationRules,
} from './valuePresentationSave';
export { registerBundledValuePresentationContributions } from './registerBundledContributions';
