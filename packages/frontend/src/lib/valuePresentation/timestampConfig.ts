/**
 * timestampConfig — 时间展示器配置类型与纯验证（issue-32 设计 §5，T02）。
 *
 * 这些类型将持久化为 JSON（宿主用户偏好存储），全部为可序列化结构，
 * 不含函数与类实例。验证以 unknown 入参做完整结构检查（禁 any / 断言
 * 跳过），错误路径化（如 `interpretation.unit`），未知字段一律拒绝，
 * 防止脏配置静默入库。
 *
 * 语义约束（设计 §5「源时区只出现一处」）：
 * - `sourceTimeZone` 只对「无时区文本」（interpretation=text）与「原生
 *   local-datetime」（interpretation=native）有意义；native 的 instant 值
 *   与带偏移文本以自身时刻为准，不用它重解释。
 * - `interpretation.kind === 'unix'` 携带 sourceTimeZone 是配置错误
 *   （路径化错误 `sourceTimeZone`）——Unix 数值本身无墙上时间语义。
 *
 * IANA 名只做格式校验（isValidIanaTimeZoneName：非空、无空白、合理字符集、
 * 长度上限）；完整有效性（是否真实存在）由使用时的 Intl try/catch 兜底
 * （temporalMath.isValidTimeZoneName / invalid-time-zone）。
 */

import {
  isValidIanaTimeZoneName,
  type TimeZoneSpec,
  type TimestampOutputFormat,
} from './timestampFormat';

export type TimestampInterpretation =
  | { kind: 'native' }
  | { kind: 'unix'; unit: 'seconds' | 'milliseconds' }
  | { kind: 'text' };

export interface TimestampOptions {
  interpretation: TimestampInterpretation;
  format: TimestampOutputFormat;
  displayTimeZone: TimeZoneSpec;
  /** 只出现一处：无时墙钟文本与原生 local-datetime 的源时区；native+instant 与带偏移值不用它。 */
  sourceTimeZone?: TimeZoneSpec;
}

/** 路径化验证问题：path 指向 JSON 输入内的位置（'' 为根），message 说明原因。 */
export interface TimestampOptionsProblem {
  path: string;
  message: string;
}

export type TimestampOptionsValidation =
  | { ok: true; options: TimestampOptions }
  | { ok: false; problems: ReadonlyArray<TimestampOptionsProblem> };

// ---------------------------------------------------------------------------
// 验证辅助（无 any：全部经 unknown 窄化，不用断言）
// ---------------------------------------------------------------------------

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** 读取并验证一个 TimeZoneSpec；发现问题时记入 problems 并返回 undefined。 */
function readTimeZoneSpec(
  value: unknown,
  path: string,
  problems: TimestampOptionsProblem[],
): TimeZoneSpec | undefined {
  if (!isPlainObject(value)) {
    problems.push({ path, message: 'expected an object' });
    return undefined;
  }
  const before = problems.length;

  let spec: TimeZoneSpec | undefined;
  if (value.kind === 'utc' || value.kind === 'local') {
    spec = { kind: value.kind };
  } else if (value.kind === 'iana') {
    if (typeof value.name === 'string' && isValidIanaTimeZoneName(value.name)) {
      spec = { kind: 'iana', name: value.name };
    } else {
      problems.push({
        path: `${path}.name`,
        message: 'expected a non-empty IANA time zone name without whitespace',
      });
    }
  } else {
    problems.push({ path: `${path}.kind`, message: 'expected "utc", "local" or "iana"' });
  }

  for (const key of Object.keys(value)) {
    if (key !== 'kind' && key !== 'name') {
      problems.push({ path: `${path}.${key}`, message: 'unknown field' });
    }
  }
  if (spec !== undefined && spec.kind !== 'iana' && 'name' in value) {
    problems.push({ path: `${path}.name`, message: 'unknown field (only "iana" carries a name)' });
  }

  return problems.length > before ? undefined : spec;
}

/** 读取并验证 interpretation；发现问题时记入 problems 并返回 undefined。 */
function readInterpretation(
  value: unknown,
  path: string,
  problems: TimestampOptionsProblem[],
): TimestampInterpretation | undefined {
  if (!isPlainObject(value)) {
    problems.push({ path, message: 'expected an object' });
    return undefined;
  }
  const before = problems.length;

  let interpretation: TimestampInterpretation | undefined;
  if (value.kind === 'native' || value.kind === 'text') {
    interpretation = { kind: value.kind };
    if ('unit' in value) {
      problems.push({
        path: `${path}.unit`,
        message: `unknown field ("${value.kind}" interpretation has no unit)`,
      });
    }
  } else if (value.kind === 'unix') {
    if (value.unit === 'seconds' || value.unit === 'milliseconds') {
      interpretation = { kind: 'unix', unit: value.unit };
    } else {
      problems.push({ path: `${path}.unit`, message: 'expected "seconds" or "milliseconds"' });
    }
  } else {
    problems.push({ path: `${path}.kind`, message: 'expected "native", "unix" or "text"' });
  }

  for (const key of Object.keys(value)) {
    if (key !== 'kind' && key !== 'unit') {
      problems.push({ path: `${path}.${key}`, message: 'unknown field' });
    }
  }

  return problems.length > before ? undefined : interpretation;
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

/**
 * 完整结构验证（unknown 入参）。
 *
 * 成功：返回重建并深冻结的规范 TimestampOptions（只含已识别字段，
 * sourceTimeZone 缺省时不出现该键，JSON 序列化稳定）。
 * 失败：返回全部路径化问题（不中断于第一个错误，便于设置面板一次展示）。
 */
export function validateTimestampOptions(input: unknown): TimestampOptionsValidation {
  if (!isPlainObject(input)) {
    return { ok: false, problems: [{ path: '', message: 'expected an object' }] };
  }
  const problems: TimestampOptionsProblem[] = [];

  const interpretation = readInterpretation(input.interpretation, 'interpretation', problems);

  let format: TimestampOutputFormat | undefined;
  if (input.format === 'iso' || input.format === 'iso-tz' || input.format === 'locale') {
    format = input.format;
  } else {
    problems.push({ path: 'format', message: 'expected "iso", "iso-tz" or "locale"' });
  }

  const displayTimeZone = readTimeZoneSpec(input.displayTimeZone, 'displayTimeZone', problems);

  let sourceTimeZone: TimeZoneSpec | undefined;
  if (input.sourceTimeZone !== undefined) {
    sourceTimeZone = readTimeZoneSpec(input.sourceTimeZone, 'sourceTimeZone', problems);
    if (interpretation !== undefined && interpretation.kind === 'unix') {
      problems.push({
        path: 'sourceTimeZone',
        message: 'unix interpretation must not carry a source time zone',
      });
    }
  }

  for (const key of Object.keys(input)) {
    if (
      key !== 'interpretation' &&
      key !== 'format' &&
      key !== 'displayTimeZone' &&
      key !== 'sourceTimeZone'
    ) {
      problems.push({ path: key, message: 'unknown field' });
    }
  }

  if (
    problems.length > 0 ||
    interpretation === undefined ||
    format === undefined ||
    displayTimeZone === undefined
  ) {
    return { ok: false, problems };
  }

  const options: TimestampOptions =
    sourceTimeZone === undefined
      ? { interpretation, format, displayTimeZone }
      : { interpretation, format, displayTimeZone, sourceTimeZone };
  // 深冻结：结构是封闭的（interpretation / TimeZoneSpec 两层），逐层冻结即可。
  Object.freeze(options.interpretation);
  Object.freeze(options.displayTimeZone);
  if (options.sourceTimeZone !== undefined) Object.freeze(options.sourceTimeZone);
  Object.freeze(options);
  return { ok: true, options };
}
