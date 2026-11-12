/**
 * columnPresentation — 列配置面板的纯逻辑（issue-32 T09，设计 §5/§9，PRD §3.2）。
 *
 * 本模块无 React / DOM / Store / IPC 依赖，只做三件事：
 *
 * 1. 校验列覆盖（`ColumnPresentationOverride`，设计 §5）：
 *    `inherit` / `raw` 不带额外字段；`custom` 只接受时间展示器
 *    `datazen.timestamp`，其 options 走 T02 `validateTimestampOptions`
 *    校验——非法 options 禁存（调用方就地报错），不写进任何存储。
 * 2. 预览：与运行时同一条路。`inherit` 走宿主计划
 *    `buildColumnPresentationPlans` 的 `present`；`raw` 直接回退原值；
 *    `custom` 走贡献的 `validateConfig → prepare → present`（T03）。
 *    预览不猜来源：无来源列的 `inherit` 预览就是全局默认 / 规则的
 *    计划结果，custom 预览只解释候选覆盖本身，不伪造表来源。
 * 3. 样本值：调用方优先给真实行值（网格取本列首个非空值，aside 取
 *    当前行值）；缺省时按语义种给 PRD §3.2 双锚点样本（Unix 毫秒
 *    1705316400000 / 原生 2026-01-15T14:30:00Z / 墙上文本
 *    2026-01-15 14:30:00），不编造行数据。
 *
 * 与 T10 的边界（见 columnPresentationSession.ts 文件头）：本模块只
 * 定义覆盖值的形状与校验，不做持久化；会话生效的接缝是
 * `columnPresentationSession.ts` 的会话覆盖存储，T10 接持久化时
 * 替换那一层，本模块的形状与校验保持不变。
 *
 * 严格 TS：禁 any；不出现 `new Date(string)`。
 */

import type {
  ColumnPresentationOverride,
  ColumnSource,
  PresentationColumnContext,
  PresentationEnvironment,
  PresentationResult,
  PresentationValue,
  ValuePresentationContribution,
  ValuePresentationRegistry,
} from '@datazen/extension-points';
import { valuePresentationRegistry } from '@datazen/extension-points';
import {
  buildColumnPresentationPlans,
  buildPresentationColumnContext,
  columnOverrideKey,
  resolvePresentationSemanticKind,
  type PresentationPlanColumn,
  type ValuePresentationSnapshot,
} from './presentationPlan';
import { TIMESTAMP_CONTRIBUTION_ID } from './timestampContribution';
import { validateTimestampOptions, type TimestampOptions } from './timestampConfig';

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** 路径化验证问题（与 T02 的 TimestampOptionsProblem 同形）。 */
export interface ColumnPresentationOverrideProblem {
  path: string;
  message: string;
}

export type ColumnPresentationOverrideValidation =
  | { ok: true; override: ColumnPresentationOverride }
  | { ok: false; problems: ReadonlyArray<ColumnPresentationOverrideProblem> };

/**
 * 完整结构验证（unknown 入参）。成功返回深冻结的规范覆盖（只含已识别
 * 字段）；失败返回全部路径化问题。
 *
 * - `inherit` / `raw`：不接受除 mode 外的任何字段；
 * - `custom`：rendererId 为去空白后的非空标识（名内空白非法，沿用
 *   T07 文本输入空格裁决）；目前只支持时间展示器
 *   `datazen.timestamp`——其他 rendererId 无法在本面板校验其 options，
 *   拒绝而不是存一个无法验证的配置；
 * - `custom` 的 options 经 `validateTimestampOptions` 校验，成功返回
 *   规范化后的 TimestampOptions（不是原样透传的 unknown）。
 */
export function validateColumnPresentationOverride(
  input: unknown,
): ColumnPresentationOverrideValidation {
  if (!isPlainObject(input)) {
    return { ok: false, problems: [{ path: '', message: 'expected an object' }] };
  }
  const problems: ColumnPresentationOverrideProblem[] = [];
  const mode = input.mode;
  if (mode === 'inherit' || mode === 'raw') {
    for (const key of Object.keys(input)) {
      if (key !== 'mode') {
        problems.push({ path: key, message: 'unknown field' });
      }
    }
    if (problems.length > 0) return { ok: false, problems };
    const override: ColumnPresentationOverride = { mode };
    Object.freeze(override);
    return { ok: true, override };
  }
  if (mode !== 'custom') {
    return {
      ok: false,
      problems: [{ path: 'mode', message: 'expected "inherit", "raw" or "custom"' }],
    };
  }
  let rendererId: string | undefined;
  if (typeof input.rendererId !== 'string') {
    problems.push({ path: 'rendererId', message: 'expected a string' });
  } else {
    const trimmed = input.rendererId.trim();
    if (trimmed === '') {
      problems.push({ path: 'rendererId', message: 'must not be empty' });
    } else if (/\s/.test(trimmed)) {
      problems.push({ path: 'rendererId', message: 'must not contain whitespace' });
    } else {
      rendererId = trimmed;
    }
  }
  let options: TimestampOptions | undefined;
  if (rendererId !== undefined && rendererId !== TIMESTAMP_CONTRIBUTION_ID) {
    problems.push({ path: 'rendererId', message: 'unknown renderer' });
  } else if (rendererId !== undefined) {
    const validated = validateTimestampOptions(input.options);
    if (!validated.ok) {
      for (const problem of validated.problems) {
        problems.push({ path: `options.${problem.path}`, message: problem.message });
      }
    } else {
      options = validated.options;
    }
  }
  for (const key of Object.keys(input)) {
    if (key !== 'mode' && key !== 'rendererId' && key !== 'options') {
      problems.push({ path: key, message: 'unknown field' });
    }
  }
  if (problems.length > 0 || rendererId === undefined || options === undefined) {
    return { ok: false, problems };
  }
  const override: ColumnPresentationOverride = { mode: 'custom', rendererId, options };
  Object.freeze(override);
  return { ok: true, override };
}

/**
 * 从快照取一列的当前覆盖：无来源（查询结果）或无条目时为 `inherit`
 * （跟随规则 / 全局默认）。来源判定以调用方传入的 ColumnSource 为
 * 准，不从 SQL 别名猜表来源（PRD §3.2）。
 */
export function columnOverrideFromSnapshot(
  snapshot: Readonly<ValuePresentationSnapshot>,
  source: Readonly<Omit<ColumnSource, 'column'>> | undefined,
  columnName: string,
): ColumnPresentationOverride {
  if (source === undefined) return { mode: 'inherit' };
  return (
    snapshot.overrides[columnOverrideKey({ ...source, column: columnName })] ?? {
      mode: 'inherit',
    }
  );
}

/**
 * 按语义种给预览样本（PRD §3.2 双锚点；调用方有真实行值时优先用
 * 真实行值，本函数只在无行值时兜底）：
 * - number → 1705316400000（Unix 毫秒锚点；秒/毫秒由用户显式选，
 *   不以位数自动决定）；
 * - instant → '2026-01-15T14:30:00Z'；local-datetime → '2026-01-15 14:30:00'；
 * - date → '2026-01-15'；time → '14:30:00'；
 * - text / other → undefined（无可预览的时间样本）。
 */
export function fallbackSampleForColumn(
  column: Readonly<PresentationPlanColumn>,
): PresentationValue | undefined {
  switch (resolvePresentationSemanticKind(column)) {
    case 'number':
      return 1705316400000;
    case 'instant':
      return '2026-01-15T14:30:00Z';
    case 'local-datetime':
      return '2026-01-15 14:30:00';
    case 'date':
      return '2026-01-15';
    case 'time':
      return '14:30:00';
    case 'text':
    case 'other':
      return undefined;
  }
}

/** 样本值的展示文本：NULL 仍为 NULL；对象用 JSON，不抛异常。 */
export function formatColumnPresentationSampleValue(value: PresentationValue): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'bigint' || typeof value === 'boolean') {
    return String(value);
  }
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
}

/** 列配置预览结果：与 T07 的 TimestampPreviewResult 同形。 */
export type ColumnPresentationPreview =
  | { kind: 'unavailable'; sample: PresentationValue }
  | {
      kind: 'invalid';
      problems: ReadonlyArray<ColumnPresentationOverrideProblem>;
      sample: PresentationValue;
    }
  | { kind: 'text'; text: string; interpretation: string | undefined; sample: PresentationValue }
  | { kind: 'unchanged'; reason: string | undefined; sample: PresentationValue };

function mapPresentationResult(
  result: PresentationResult,
  sample: PresentationValue,
): ColumnPresentationPreview {
  if (result.kind === 'text') {
    return {
      kind: 'text',
      text: result.text,
      interpretation: result.interpretation,
      sample,
    };
  }
  return { kind: 'unchanged', reason: result.reason, sample };
}

/** computeColumnPresentationPreview 的输入。 */
export interface ComputeColumnPresentationPreviewInput {
  /** 候选覆盖（面板草稿）；`inherit` 时预览当前生效计划。 */
  readonly candidate: Readonly<ColumnPresentationOverride>;
  readonly column: Readonly<PresentationPlanColumn>;
  /** 表身份；缺省表示查询结果等无来源列（覆盖不可用，inherit 即全局/规则）。 */
  readonly source?: Readonly<Omit<ColumnSource, 'column'>>;
  readonly snapshot: ValuePresentationSnapshot;
  readonly environment: PresentationEnvironment;
  readonly sampleValue: PresentationValue;
  /** 缺省为全局 valuePresentationRegistry。 */
  readonly registry?: ValuePresentationRegistry;
}

/**
 * 列配置预览（与运行时同一条路，不另写解析）：
 *
 * - `inherit`：用快照为该列建计划并 `present(sample)`——即全局默认 /
 *   规则命中后的同一结果；
 * - `raw`：不调 prepare，直接 `unchanged`（原因 `raw-override`），
 *   命中规则的列选原始值后仍显示原值（PRD 验收 4）；
 * - `custom`：贡献缺失 → `unavailable`；`validateConfig` 失败 →
 *   `invalid` 与路径化问题；成功则 `prepare`（候选覆盖经
 *   columnOverride 传入，与 T04 计划构建器同一调用形状）后
 *   `present(sample)`。
 */
export function computeColumnPresentationPreview(
  input: Readonly<ComputeColumnPresentationPreviewInput>,
): ColumnPresentationPreview {
  const registry: ValuePresentationRegistry = input.registry ?? valuePresentationRegistry;
  const sample = input.sampleValue;
  const candidate = input.candidate;
  if (candidate.mode === 'inherit') {
    const plans = buildColumnPresentationPlans(
      { columns: [input.column], source: input.source, snapshot: input.snapshot },
      registry,
    );
    const plan = plans[0];
    if (plan === undefined) return { kind: 'unchanged', reason: 'no-plan', sample };
    return mapPresentationResult(plan.present(sample), sample);
  }
  if (candidate.mode === 'raw') {
    return { kind: 'unchanged', reason: 'raw-override', sample };
  }
  const contribution: ValuePresentationContribution | undefined = registry.get(
    candidate.rendererId,
  );
  if (contribution === undefined) {
    return { kind: 'unavailable', sample };
  }
  const validated = contribution.validateConfig(candidate.options);
  if (!validated.ok) {
    return { kind: 'invalid', problems: validated.problems, sample };
  }
  const columnContext: PresentationColumnContext = buildPresentationColumnContext(
    input.column,
    input.source === undefined ? undefined : { ...input.source, column: input.column.name },
  );
  const prepared = contribution.prepare({
    config: { configVersion: contribution.configVersion, config: validated.config },
    columnOverride: candidate,
    column: columnContext,
    environment: input.environment,
  });
  return mapPresentationResult(prepared.present(sample), sample);
}
