/**
 * presentationPlan — 宿主展示管线与列展示计划（issue-32 T04，设计 §7）。
 *
 * 纯逻辑模块：无 React / DOM / Store / IPC / 文件依赖。宿主在面板层为
 * 每一列构建一份 {@link ColumnPresentationPlan}（prepare 在建计划时做
 * 一次，不逐格调用），GridCell 与 DetailPanel（T05 接线）只经
 * `plan.present(value)` 消费同一结果。
 *
 * 决策顺序（先覆盖、后默认路由；裁决写进注释与测试，不自行发明）：
 *
 * 1. 覆盖：source 存在时按 {@link columnOverrideKey} 查
 *    `snapshot.overrides`；source 缺省（查询结果等无来源列）时覆盖不可用。
 * 2. 覆盖 `raw` → 状态 `raw`，不调 prepare（原值直出）。
 * 3. 覆盖 `custom` → 用目标贡献的 `validateConfig` 校验
 *    `override.options`，失败为 `invalid-config`；成功则生效配置取
 *    校验后 config。显式 custom 覆盖 **bypass enabled 门禁**：具体选择
 *    胜过全局开关（裁决）。
 * 4. 覆盖 `inherit` 或无覆盖 → 默认路由：时间语义列（instant /
 *    local-datetime / date / time）默认渲染器为 `datazen.timestamp`；
 *    number / text / other 无默认渲染器 → 状态 `default`。默认路由依次
 *    检查 enabled 门禁（不在 `enabledRendererIds` → `disabled`）、贡献
 *    缺失（→ `missing-renderer`）、配置缺失（→ `missing-config`）。
 * 5. `effectiveConfig.configVersion !== contribution.configVersion` 时
 *    不调 prepare，直接 `invalid-config`（未知版本不覆盖旧配置，设计
 *    §4 / §6）。
 * 6. prepare 抛异常（契约要求不抛，但宿主隔离，设计 §7）→ 捕获为
 *    状态 `error`、原因 `plan-error`，不记录原始行值。
 *
 * 分工（T03 / T04 边界）：
 * - T04 只用覆盖选渲染器（custom 的 rendererId）与决定是否走默认路由；
 * - T03 的 prepare 内部再按原样透传的 `columnOverride` 选 options
 *   （全局配置 vs custom 选项），T04 不替它解释 options。
 *
 * 严格 TS：禁 any，不用断言跳过验证；不出现 `new Date(string)`；
 * present / prepare 调用不记录原始行值与凭据。
 */

import type {
  ColumnPresentationOverride,
  ColumnSource,
  PresentationColumnContext,
  PresentationEnvironment,
  PresentationResult,
  PresentationValue,
  PreparedPresentation,
  ValuePresentationContribution,
  ValuePresentationRegistry,
  VersionedPresentationConfig,
} from '@datazen/extension-points';
import { valuePresentationRegistry } from '@datazen/extension-points';
import type { CellType, CellTypeHints } from '@datazen/extension-points';
import { isZonedCellType, resolveCellType } from '../cellTypes';
import { TIMESTAMP_CONTRIBUTION_ID } from './timestampContribution';

/** 展示语义种：与 PresentationColumnContext['semanticKind'] 同义。 */
export type PresentationSemanticKind = PresentationColumnContext['semanticKind'];

/**
 * 列输入（自有最小类型，不从 components/TableHeader 导入 ColumnDef——
 * lib 不依赖 components；T05 接线时再做适配）。
 */
export interface PresentationPlanColumn {
  readonly name: string;
  readonly dataType?: string;
  readonly cellType?: CellType;
  readonly hints?: CellTypeHints;
}

/**
 * 语义种映射表（写进注释与测试）：
 *
 * - datetime → isZonedCellType(dataType, hints) ? 'instant' : 'local-datetime'
 * - date → 'date'；time → 'time'；number → 'number'
 * - text / enum / uuid → 'text'
 * - bool / json / bytes / unknown → 'other'
 *
 * cellType 缺省时用 resolveCellType(dataType, hints) 推导。
 */
export function resolvePresentationSemanticKind(
  column: Readonly<PresentationPlanColumn>,
): PresentationSemanticKind {
  const cellType: CellType = column.cellType ?? resolveCellType(column.dataType, column.hints);
  switch (cellType) {
    case 'datetime':
      return isZonedCellType(column.dataType, column.hints) ? 'instant' : 'local-datetime';
    case 'date':
      return 'date';
    case 'time':
      return 'time';
    case 'number':
      return 'number';
    case 'text':
    case 'enum':
    case 'uuid':
      return 'text';
    case 'bool':
    case 'json':
    case 'bytes':
    case 'unknown':
      return 'other';
  }
}

/** 由列输入与完整来源组装列上下文。 */
export function buildPresentationColumnContext(
  column: Readonly<PresentationPlanColumn>,
  source?: Readonly<ColumnSource>,
): PresentationColumnContext {
  return {
    columnName: column.name,
    databaseTypeName: column.dataType,
    semanticKind: resolvePresentationSemanticKind(column),
    source,
  };
}

/**
 * 表列覆盖键：JSON.stringify([connectionId, database ?? null, namespace ?? null,
 * table, column])，不做大小写归一——保留驱动返回的标识符大小写语义
 * （设计 §6）；缺省字段用 null、空字符串保留为空串，二者键不同
 * （设计 §6「缺省字段与空字符串不能无意碰撞」）。
 */
export function columnOverrideKey(source: Readonly<ColumnSource>): string {
  return JSON.stringify([
    source.connectionId,
    source.database ?? null,
    source.namespace ?? null,
    source.table,
    source.column,
  ]);
}

/**
 * 展示环境快照解析：locale 缺省取 navigator.language（不存在时 'en-US'），
 * localTimeZone 取 Intl.DateTimeFormat().resolvedOptions().timeZone
 * （不存在 / 空串时 'UTC'）。不依赖机器隐式默认值（设计 §8.7）。
 */
export function resolvePresentationEnvironment(locale?: string): PresentationEnvironment {
  const resolvedLocale =
    locale !== undefined && locale !== ''
      ? locale
      : typeof navigator !== 'undefined' &&
          typeof navigator.language === 'string' &&
          navigator.language !== ''
        ? navigator.language
        : 'en-US';
  let localTimeZone = 'UTC';
  try {
    const timeZone = new Intl.DateTimeFormat().resolvedOptions().timeZone;
    if (typeof timeZone === 'string' && timeZone !== '') {
      localTimeZone = timeZone;
    }
  } catch {
    // Intl 不可用时回退 UTC，不抛异常。
    localTimeZone = 'UTC';
  }
  return { locale: resolvedLocale, localTimeZone };
}

/**
 * 展示快照（T06 持久化将来填充的内存内契约，T04 只定义形状与默认快照）：
 * 不可变快照，配置修订以 revision 标识；T06 从 store 提供真实快照。
 */
export interface ValuePresentationSnapshot {
  readonly revision: number;
  readonly environment: PresentationEnvironment;
  readonly configs: Readonly<Record<string, VersionedPresentationConfig>>;
  readonly enabledRendererIds: ReadonlyArray<string>;
  readonly overrides: Readonly<Record<string, ColumnPresentationOverride>>;
}

/**
 * 默认快照（revision 0、configs {}、enabled []、overrides {}），对应
 * 设计 §6 加载态 1「初次加载中使用同一个默认快照」——此时时间列一律
 * 'disabled'，走默认展示。
 */
export function createDefaultValuePresentationSnapshot(
  environment?: PresentationEnvironment,
): ValuePresentationSnapshot {
  return {
    revision: 0,
    environment: environment ?? resolvePresentationEnvironment(),
    configs: {},
    enabledRendererIds: [],
    overrides: {},
  };
}

/** 列展示计划状态。 */
export type ColumnPresentationStatus =
  | 'ready'
  | 'default'
  | 'raw'
  | 'disabled'
  | 'missing-renderer'
  | 'missing-config'
  | 'invalid-config'
  | 'error';

/**
 * 无 prepared 时 present 的稳定回退原因（按状态取值）：
 * default → 'no-renderer'、raw → 'raw-override'、disabled → 'disabled'、
 * missing-renderer → 'missing-renderer'、missing-config → 'missing-config'、
 * invalid-config → 'invalid-config'、error → 'plan-error'。
 */
export function fallbackPresentationReason(status: ColumnPresentationStatus): string {
  switch (status) {
    case 'ready':
      // ready 一定携带 prepared，不应走到回退；防御性取值与 error 一致。
      return 'plan-error';
    case 'default':
      return 'no-renderer';
    case 'raw':
      return 'raw-override';
    case 'disabled':
      return 'disabled';
    case 'missing-renderer':
      return 'missing-renderer';
    case 'missing-config':
      return 'missing-config';
    case 'invalid-config':
      return 'invalid-config';
    case 'error':
      return 'plan-error';
  }
}

/** 一列的展示计划：prepare 的结果与统一 present 入口。 */
export interface ColumnPresentationPlan {
  readonly columnName: string;
  readonly column: PresentationColumnContext;
  readonly status: ColumnPresentationStatus;
  readonly rendererId?: string;
  readonly reason?: string;
  readonly prepared?: PreparedPresentation;
  present(value: PresentationValue): PresentationResult;
}

/** buildColumnPresentationPlans 的输入。 */
export interface BuildColumnPresentationPlansInput {
  readonly columns: ReadonlyArray<PresentationPlanColumn>;
  /** 表身份；缺省表示查询结果等无来源列，覆盖不可用。 */
  readonly source?: Readonly<Omit<ColumnSource, 'column'>>;
  readonly snapshot: ValuePresentationSnapshot;
}

/** 构造一份非 ready 计划（无 prepared；present 一律回退）。 */
function makeFallbackPlan(
  columnName: string,
  column: PresentationColumnContext,
  status: Exclude<ColumnPresentationStatus, 'ready'>,
  rendererId?: string,
): ColumnPresentationPlan {
  const reason = fallbackPresentationReason(status);
  return {
    columnName,
    column,
    status,
    rendererId,
    reason,
    present: (): PresentationResult => ({ kind: 'unchanged', reason }),
  };
}

/** 构造一份 ready 计划（present 直接调 prepared.present，内部永不抛异常）。 */
function makeReadyPlan(
  columnName: string,
  column: PresentationColumnContext,
  rendererId: string,
  prepared: PreparedPresentation,
): ColumnPresentationPlan {
  return {
    columnName,
    column,
    status: 'ready',
    rendererId,
    prepared,
    present: (value: PresentationValue): PresentationResult => prepared.present(value),
  };
}

/**
 * prepare 阶段（共享）：版本校验在先，prepare 异常隔离在后。
 *
 * - effectiveConfig.configVersion 与贡献声明不一致 → 不调 prepare，
 *   直接 `invalid-config`（未知版本不覆盖旧配置，设计 §4 / §6）；
 * - prepare 抛异常 → 捕获为 `error` / `plan-error`，不记录原始行值
 *   （不要把 value 写进 reason / 日志）。
 */
function preparePlan(
  columnName: string,
  column: PresentationColumnContext,
  rendererId: string,
  contribution: ValuePresentationContribution,
  effectiveConfig: VersionedPresentationConfig,
  columnOverride: Readonly<ColumnPresentationOverride> | undefined,
  environment: PresentationEnvironment,
): ColumnPresentationPlan {
  if (effectiveConfig.configVersion !== contribution.configVersion) {
    return makeFallbackPlan(columnName, column, 'invalid-config', rendererId);
  }
  try {
    const prepared = contribution.prepare({
      config: effectiveConfig,
      columnOverride,
      column,
      environment,
    });
    return makeReadyPlan(columnName, column, rendererId, prepared);
  } catch {
    // 宿主隔离：贡献异常不扩散到调用方，计划本身仍可用（回退展示）。
    return makeFallbackPlan(columnName, column, 'error', rendererId);
  }
}

/** 为一列构建展示计划（决策顺序见文件头）。 */
function buildOneColumnPlan(
  column: Readonly<PresentationPlanColumn>,
  source: Readonly<Omit<ColumnSource, 'column'>> | undefined,
  snapshot: ValuePresentationSnapshot,
  registry: ValuePresentationRegistry,
): ColumnPresentationPlan {
  const columnContext = buildPresentationColumnContext(
    column,
    source === undefined ? undefined : { ...source, column: column.name },
  );

  // a. 查覆盖：source 存在时按键取；source 缺省时无覆盖。
  const override: Readonly<ColumnPresentationOverride> | undefined =
    source === undefined
      ? undefined
      : snapshot.overrides[columnOverrideKey({ ...source, column: column.name })];

  // b. 覆盖 mode 'raw' → 状态 'raw'，不调 prepare。
  if (override !== undefined && override.mode === 'raw') {
    return makeFallbackPlan(column.name, columnContext, 'raw');
  }

  // c. 覆盖 mode 'custom' → 显式选择目标渲染器，bypass enabled 门禁。
  if (override !== undefined && override.mode === 'custom') {
    const rendererId = override.rendererId;
    const contribution = registry.get(rendererId);
    if (contribution === undefined) {
      return makeFallbackPlan(column.name, columnContext, 'missing-renderer', rendererId);
    }
    let validated: ReturnType<ValuePresentationContribution['validateConfig']>;
    try {
      validated = contribution.validateConfig(override.options);
    } catch {
      // validateConfig 契约要求不抛；防御性兜底视为无效配置。
      return makeFallbackPlan(column.name, columnContext, 'invalid-config', rendererId);
    }
    if (!validated.ok) {
      return makeFallbackPlan(column.name, columnContext, 'invalid-config', rendererId);
    }
    const effectiveConfig: VersionedPresentationConfig = {
      configVersion: contribution.configVersion,
      config: validated.config,
    };
    return preparePlan(
      column.name,
      columnContext,
      rendererId,
      contribution,
      effectiveConfig,
      override,
      snapshot.environment,
    );
  }

  // d. 覆盖 'inherit' 或无覆盖 → 默认路由。
  const semanticKind = columnContext.semanticKind;
  const isTemporal =
    semanticKind === 'instant' ||
    semanticKind === 'local-datetime' ||
    semanticKind === 'date' ||
    semanticKind === 'time';
  if (!isTemporal) {
    // number / text / other 无默认渲染器 → 状态 'default'。
    return makeFallbackPlan(column.name, columnContext, 'default');
  }
  const rendererId = TIMESTAMP_CONTRIBUTION_ID;
  if (!snapshot.enabledRendererIds.includes(rendererId)) {
    return makeFallbackPlan(column.name, columnContext, 'disabled', rendererId);
  }
  const contribution = registry.get(rendererId);
  if (contribution === undefined) {
    return makeFallbackPlan(column.name, columnContext, 'missing-renderer', rendererId);
  }
  const effectiveConfig = snapshot.configs[rendererId];
  if (effectiveConfig === undefined) {
    return makeFallbackPlan(column.name, columnContext, 'missing-config', rendererId);
  }
  return preparePlan(
    column.name,
    columnContext,
    rendererId,
    contribution,
    effectiveConfig,
    override,
    snapshot.environment,
  );
}

/**
 * 为全部列构建展示计划。
 *
 * - 计划按输入 columns 顺序返回；输入 columns 为空返回空数组；
 * - 同一列名出现两次时两项独立成计划（不去重、不抛错）；
 * - registry 缺省为全局 valuePresentationRegistry。
 */
export function buildColumnPresentationPlans(
  input: Readonly<BuildColumnPresentationPlansInput>,
  registry: ValuePresentationRegistry = valuePresentationRegistry,
): ReadonlyArray<ColumnPresentationPlan> {
  return input.columns.map((column) =>
    buildOneColumnPlan(column, input.source, input.snapshot, registry),
  );
}

/** computePresentationPlanCacheKey 的输入。 */
export interface PresentationPlanCacheKeyInput {
  readonly source?: Readonly<Omit<ColumnSource, 'column'>>;
  readonly columns: ReadonlyArray<PresentationPlanColumn>;
  readonly configRevision: number;
  readonly contributionRevision: number;
  readonly locale: string;
  readonly localTimeZone: string;
  readonly columnsRevision?: number | string;
}

/**
 * 计划缓存键：稳定 JSON 字符串（键顺序固定，不依赖对象插入顺序以外的
 * 任何东西）。
 *
 * 覆盖设计 §7 的全部失效因子：来源身份、列元数据（每列 name + dataType +
 * semanticKind）、配置修订、贡献修订、locale、localTimeZone；columnsRevision
 * 提供时一并纳入。
 */
export function computePresentationPlanCacheKey(
  input: Readonly<PresentationPlanCacheKeyInput>,
): string {
  const sourcePart =
    input.source === undefined
      ? null
      : [
          input.source.connectionId,
          input.source.database ?? null,
          input.source.namespace ?? null,
          input.source.table,
        ];
  const columnsPart = input.columns.map((column) => [
    column.name,
    column.dataType ?? '',
    resolvePresentationSemanticKind(column),
  ]);
  return JSON.stringify([
    sourcePart,
    columnsPart,
    input.configRevision,
    input.contributionRevision,
    input.locale,
    input.localTimeZone,
    input.columnsRevision ?? null,
  ]);
}
