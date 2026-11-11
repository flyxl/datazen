/**
 * timestampContribution — 时间展示器贡献 `datazen.timestamp`（issue-32 T03）。
 *
 * 设计 §4 的首个展示贡献：把 T02 的纯逻辑核心（timestampConfig /
 * timestampFormat / temporalMath）装配成 `ValuePresentationContribution`，
 * 供宿主展示管线（T04+）在面板列展示计划阶段经 `prepare` 消费。
 *
 * 本模块是纯逻辑：不读 Store / DOM / 文件 / 接口，不新增依赖。
 * `present` 永不抛异常（内部 try/catch 兜底为 `unexpected-error`）；
 * `prepare` 本身同样做防御性兜底，输入快照结构异常时也返回可用 stub。
 *
 * 裁决记录（任务规格未明之处，按此实现并在测试中锁定）：
 * 1. 解析优先级（按任务规格列出的顺序）：configVersion → 全局配置有效性 →
 *    列覆盖。全局配置无效时一律 `invalid-config`，即使列覆盖为 `raw`；
 *    版本不匹配时一律 `unsupported-config-version`，不校验配置体。
 * 2. `local` 时区一律在 prepare 阶段解析为
 *    `{ kind: 'iana', name: environment.localTimeZone }`，绝不调用
 *    `formatTimestampOutput` 的运行时机器时区解析（设计 §8.7：不依赖
 *    机器隐式默认值）；`locale` 同样取 `environment.locale`。
 * 3. `text` / `native` 回退中的无日期 `time` 文本（如 `14:30:00`）以
 *    1970-01-01（epoch 日）为锚定日期；无时间 `date` 文本取当日 00:00。
 *    不用「当前日期」——那会引入系统时钟依赖，破坏纯逻辑与可测试性。
 * 4. `native` 回退的失败原因码：墙上文本解析失败时取墙上解析的原因码，
 *    转换失败时取转换的原因码；其余失败一律取底层 T02 稳定原因码原样
 *    透传，不自造新码（present 层自有原因码见下）。
 * 5. `unix` 的 bigint 值经 `String(value)` 交给 `parseUnixValue`
 *   （其签名只接受 number | string）；十进制字符串转换精确无损，
 *    不做 Number() 近似，避免超过安全整数时静默失真。
 * 6. semanticKind 门禁在值解释之前判定：`other` 列与「非 unix 解释的
 *    number 列」先于 null 检查回退（null 在 `other` 列上得到
 *    `not-applicable` 而非 `null`）。
 */

import type {
  ConfigValidationResult,
  PresentationConfigValue,
  PresentationPrepareInput,
  PresentationResult,
  PresentationValue,
  PreparedPresentation,
  ValuePresentationContribution,
} from '@datazen/extension-points';
import {
  parseUnixValue,
  parseWallClockText,
  parseZonedTimestamp,
  wallClockToInstant,
  type InstantParts,
  type WallClockTextParse,
  type WallComponents,
} from './temporalMath';
import { formatTimestampOutput, type TimeZoneSpec } from './timestampFormat';
import { validateTimestampOptions, type TimestampOptions } from './timestampConfig';

/** 贡献唯一标识（设计 §2：提供方.展示器）。 */
export const TIMESTAMP_CONTRIBUTION_ID = 'datazen.timestamp';

/** 配置 schema 版本（v1：TimestampOptions JSON 形状，无 rules 字段）。 */
export const TIMESTAMP_CONTRIBUTION_CONFIG_VERSION = 1;

/** present 层自有原因码（底层 T02 原因码之外，任务规格明确列出者）。 */
const UNCHANGED_NULL: PresentationResult = { kind: 'unchanged', reason: 'null' };
const UNCHANGED_NOT_APPLICABLE: PresentationResult = {
  kind: 'unchanged',
  reason: 'not-applicable',
};

function unchanged(reason: string): PresentationResult {
  return { kind: 'unchanged', reason };
}

// ---------------------------------------------------------------------------
// validateConfig：委托 T02 验证，成功返回 JSON 可序列化配置
// ---------------------------------------------------------------------------

/**
 * 把验证后的 TimestampOptions 重建成 PresentationConfigValue。
 *
 * 不用断言：TimestampOptions 是具名 interface，TypeScript 不给它隐式
 * 索引签名，直接赋值给 `{ readonly [key: string]: … }` 会失败；此处按
 * 字段逐一重建为对象字面量（只含已验证字段，sourceTimeZone 缺省时
 * 不出现该键，与 validateTimestampOptions 的规范形状一致）。
 */
function toPresentationConfigValue(options: TimestampOptions): PresentationConfigValue {
  const interpretation: PresentationConfigValue =
    options.interpretation.kind === 'unix'
      ? { kind: 'unix', unit: options.interpretation.unit }
      : { kind: options.interpretation.kind };
  const displayTimeZone: PresentationConfigValue =
    options.displayTimeZone.kind === 'iana'
      ? { kind: 'iana', name: options.displayTimeZone.name }
      : { kind: options.displayTimeZone.kind };
  const sourceTimeZone: PresentationConfigValue | undefined =
    options.sourceTimeZone === undefined
      ? undefined
      : options.sourceTimeZone.kind === 'iana'
        ? { kind: 'iana', name: options.sourceTimeZone.name }
        : { kind: options.sourceTimeZone.kind };
  return sourceTimeZone === undefined
    ? { interpretation, format: options.format, displayTimeZone }
    : { interpretation, format: options.format, displayTimeZone, sourceTimeZone };
}

function validateTimestampConfig(input: unknown): ConfigValidationResult {
  const validated = validateTimestampOptions(input);
  if (!validated.ok) {
    // 路径与 message 原样透传，不抛异常、不返回部分配置。
    return { ok: false, problems: validated.problems };
  }
  return { ok: true, config: toPresentationConfigValue(validated.options) };
}

// ---------------------------------------------------------------------------
// 时区解析（prepare 阶段一次完成，present 阶段不再解析）
// ---------------------------------------------------------------------------

/** `local` 改按环境快照的 localTimeZone 解析；其余 kind 原样保留。 */
function resolveLocalSpec(spec: TimeZoneSpec, localTimeZone: string): TimeZoneSpec {
  return spec.kind === 'local' ? { kind: 'iana', name: localTimeZone } : spec;
}

/** 已解析的源时区 spec → wallClockToInstant 接受的 IANA 名。 */
function specToZoneName(spec: TimeZoneSpec): string {
  return spec.kind === 'iana' ? spec.name : 'UTC';
}

// ---------------------------------------------------------------------------
// 墙上分量 → 时刻（text / native 回退共用）
// ---------------------------------------------------------------------------

/**
 * 墙上文本解析结果 → 完整墙上分量。
 *
 * - datetime：原样；
 * - date：当日 00:00（午夜）；
 * - time：锚定 1970-01-01（见文件头裁决 3）。
 */
function wallParseToComponents(parsed: WallClockTextParse): WallComponents | null {
  if (parsed.kind !== 'ok') return null;
  switch (parsed.shape) {
    case 'datetime':
      return parsed.wall;
    case 'date':
      return {
        year: parsed.date.year,
        month: parsed.date.month,
        day: parsed.date.day,
        hour: 0,
        minute: 0,
        second: 0,
        nanos: 0,
      };
    case 'time':
      return {
        year: 1970,
        month: 1,
        day: 1,
        hour: parsed.time.hour,
        minute: parsed.time.minute,
        second: parsed.time.second,
        nanos: parsed.time.nanos,
        fractionDigits: parsed.time.fractionDigits,
      };
  }
  // 静态类型已穷尽三种 shape；防御性兜底（运行时垃圾输入不抛异常）。
  return null;
}

/**
 * 墙上文本 → 时刻（源时区名已解析）。
 *
 * 返回 ok 时携带时刻；失败时携带底层稳定原因码字符串（解析失败取
 * 解析原因码，转换失败取转换原因码）。
 */
function wallTextToInstant(
  text: string,
  sourceZoneName: string,
): { ok: true; instant: InstantParts } | { ok: false; reason: string } {
  const parsed = parseWallClockText(text);
  if (parsed.kind !== 'ok') return { ok: false, reason: parsed.reason.reason };
  const wall = wallParseToComponents(parsed);
  if (wall === null) return { ok: false, reason: 'invalid-format' };
  const converted = wallClockToInstant(wall, sourceZoneName);
  if (!converted.ok) return { ok: false, reason: converted.reason };
  return { ok: true, instant: converted.instant };
}

// ---------------------------------------------------------------------------
// prepare：解析一次配置与覆盖，产出列展示器
// ---------------------------------------------------------------------------

/**
 * 解析 prepare 输入，得到生效配置或固定的回退原因。
 *
 * - `options`：生效的 TimestampOptions（全局或 custom 覆盖）；
 * - `reason`：解析失败时 present 一律返回的 unchanged 原因
 *  （`unsupported-config-version` / `invalid-config` / `raw-override` /
 *   `override-target-mismatch`），此时无生效配置。
 */
function resolveEffectiveOptions(
  input: PresentationPrepareInput,
): { options: TimestampOptions; reason: null } | { options: null; reason: string } {
  if (input.config.configVersion !== TIMESTAMP_CONTRIBUTION_CONFIG_VERSION) {
    return { options: null, reason: 'unsupported-config-version' };
  }
  const global = validateTimestampOptions(input.config.config);
  if (!global.ok) {
    return { options: null, reason: 'invalid-config' };
  }

  const override = input.columnOverride;
  if (override === undefined || override.mode === 'inherit') {
    return { options: global.options, reason: null };
  }
  if (override.mode === 'raw') {
    return { options: null, reason: 'raw-override' };
  }
  if (override.mode === 'custom') {
    if (override.rendererId !== TIMESTAMP_CONTRIBUTION_ID) {
      return { options: null, reason: 'override-target-mismatch' };
    }
    const custom = validateTimestampOptions(override.options);
    if (!custom.ok) {
      return { options: null, reason: 'invalid-config' };
    }
    return { options: custom.options, reason: null };
  }
  // 运行时传入的未知 mode（静态类型已穷尽）：不猜测，视为无效配置。
  return { options: null, reason: 'invalid-config' };
}

/** 固定回退的展示器（配置/覆盖解析失败时 present 一律返回该原因）。 */
function fixedUnchangedPresentation(reason: string): PreparedPresentation {
  return { present: () => unchanged(reason) };
}

/** 格式化成功时刻；失败时返回底层格式原因码。 */
function formatInstant(
  instant: InstantParts,
  options: TimestampOptions,
  displayTimeZone: TimeZoneSpec,
  locale: string,
): PresentationResult {
  const formatted = formatTimestampOutput(instant, {
    format: options.format,
    displayTimeZone,
    locale,
  });
  if (!formatted.ok) return unchanged(formatted.reason.reason);
  return { kind: 'text', text: formatted.text, interpretation: options.interpretation.kind };
}

/**
 * 按生效配置解释一个值。
 *
 * semanticKind 门禁先于值解释；null / 非时间值随后回退；三种
 * interpretation 各走 T02 解析路径，成功即格式化输出。
 */
function presentValue(
  value: PresentationValue,
  options: TimestampOptions,
  semanticKind: PresentationPrepareInput['column']['semanticKind'],
  displayTimeZone: TimeZoneSpec,
  sourceZoneName: string | undefined,
  locale: string,
): PresentationResult {
  // 列适用性门禁（在解释之前）：other 列永不适用；number 列只在 unix
  // 解释下适用（native 不把数字自动解释成 Unix，设计 §5）。
  if (semanticKind === 'other') return UNCHANGED_NOT_APPLICABLE;
  if (semanticKind === 'number' && options.interpretation.kind !== 'unix') {
    return UNCHANGED_NOT_APPLICABLE;
  }

  // NULL 回退（T03 范围，T02 报告已注明属 T03+）；其余非时间值不适用。
  if (value === null || value === undefined) return UNCHANGED_NULL;
  if (typeof value === 'boolean' || Array.isArray(value) || typeof value === 'object') {
    return UNCHANGED_NOT_APPLICABLE;
  }

  switch (options.interpretation.kind) {
    case 'native': {
      if (typeof value !== 'string') {
        // native 不把数字自动解释成 Unix（设计 §5）。
        return UNCHANGED_NOT_APPLICABLE;
      }
      const zoned = parseZonedTimestamp(value);
      if (zoned.kind === 'ok') {
        return formatInstant(zoned.instant, options, displayTimeZone, locale);
      }
      // 带时区解析失败：有源时区时尝试无时区墙上时间回退。
      if (sourceZoneName === undefined) return unchanged(zoned.reason.reason);
      const fallback = wallTextToInstant(value, sourceZoneName);
      if (!fallback.ok) return unchanged(fallback.reason);
      return formatInstant(fallback.instant, options, displayTimeZone, locale);
    }
    case 'unix': {
      // bigint 经十进制字符串精确传入（见文件头裁决 5）。
      const parsed =
        typeof value === 'bigint'
          ? parseUnixValue(value.toString(), options.interpretation.unit)
          : parseUnixValue(value, options.interpretation.unit);
      if (parsed.kind !== 'ok') return unchanged(parsed.reason.reason);
      return formatInstant(parsed.instant, options, displayTimeZone, locale);
    }
    case 'text': {
      if (typeof value !== 'string') {
        // text 解释只接受字符串。
        return UNCHANGED_NOT_APPLICABLE;
      }
      const parsed = parseWallClockText(value);
      if (parsed.kind !== 'ok') return unchanged(parsed.reason.reason);
      if (sourceZoneName === undefined) {
        return unchanged('missing-source-time-zone');
      }
      const wall = wallParseToComponents(parsed);
      if (wall === null) return unchanged('invalid-format');
      const converted = wallClockToInstant(wall, sourceZoneName);
      if (!converted.ok) return unchanged(converted.reason);
      return formatInstant(converted.instant, options, displayTimeZone, locale);
    }
  }
}

/** 时间展示器贡献 `datazen.timestamp`。 */
export const timestampContribution: ValuePresentationContribution = {
  id: TIMESTAMP_CONTRIBUTION_ID,
  configVersion: TIMESTAMP_CONTRIBUTION_CONFIG_VERSION,

  validateConfig(input: unknown): ConfigValidationResult {
    return validateTimestampConfig(input);
  },

  prepare(input: PresentationPrepareInput): PreparedPresentation {
    try {
      const resolved = resolveEffectiveOptions(input);
      if (resolved.options === null) {
        return fixedUnchangedPresentation(resolved.reason);
      }
      const options = resolved.options;
      // local 时区在 prepare 阶段按环境快照解析，不用运行机器时区。
      const displayTimeZone = resolveLocalSpec(
        options.displayTimeZone,
        input.environment.localTimeZone,
      );
      const sourceZoneName =
        options.sourceTimeZone === undefined
          ? undefined
          : specToZoneName(
              resolveLocalSpec(options.sourceTimeZone, input.environment.localTimeZone),
            );
      const semanticKind = input.column.semanticKind;
      const locale = input.environment.locale;
      return {
        present(value: PresentationValue): PresentationResult {
          try {
            return presentValue(
              value,
              options,
              semanticKind,
              displayTimeZone,
              sourceZoneName,
              locale,
            );
          } catch {
            // present 永不抛异常：未知运行时异常统一回退。
            return unchanged('unexpected-error');
          }
        },
      };
    } catch {
      // prepare 防御性兜底：快照结构异常也不抛给宿主。
      return fixedUnchangedPresentation('unexpected-error');
    }
  },
};
