/**
 * timestampFormat — 时间展示三种输出格式（issue-32 设计 §8.7，T02）。
 *
 * - `iso`：UTC ISO 8601（`2026-01-15T14:30:00.123456789Z`），保留源小数秒
 *   位数（InstantParts.fractionDigits，不补零不截断）。
 * - `iso-tz`：目标时区墙上时间 + 该时刻实际偏移
 *   （`2026-01-15T22:30:00.123456789+08:00`）；`local` 用运行环境时区。
 *   历史亚分钟偏移（LMT）输出 `±HH:MM:SS`（如 1850 年上海 `+08:05:43`）。
 * - `locale`：Intl.DateTimeFormat(locale, { timeZone, 显式完整字段 })，语言
 *   与目标时区均显式传入，不依赖机器隐式默认。
 *
 * locale 格式的亚毫秒精度决策（设计 §8.7「无法实现时回退」的二选一，此处
 * 选择截断并文档化）：Intl 的 fractionalSecondDigits 最高 3 位（毫秒），微秒/
 * 纳秒源精度在 locale 格式下**截断到毫秒**（向过去截断，不四舍五入），不
 * 返回错误——错误会让 locale 格式对微秒精度值（MySQL(6)/Postgres 常见）完全
 * 不可用；需要完整精度时 iso / iso-tz 均无损。无小数秒的源不追加 `.000`。
 *
 * 失败模式：无论 format 为何，displayTimeZone 都先经完整校验（无效 IANA 名
 * 一律失败，配置错误在预览与实际展示中表现一致，不因格式而异）；locale
 * 字符串本身非法（如空串）返回 `invalid-locale`——结构合法但无数据的语言
 * 标签按 ICU 行为回退默认语言（文档化边界，由上层设置校验尽量前置）。
 */

import {
  type InstantParts,
  type TemporalFormatError,
  instantToWallParts,
  isValidTimeZoneName,
  validateInstantParts,
} from './temporalMath';

export type TimestampOutputFormat = 'iso' | 'iso-tz' | 'locale';

export type TimeZoneSpec = { kind: 'utc' } | { kind: 'local' } | { kind: 'iana'; name: string };

export interface TimestampFormatOptions {
  format: TimestampOutputFormat;
  displayTimeZone: TimeZoneSpec;
  locale: string;
}

export type TimestampFormatResult =
  | { ok: true; text: string }
  | { ok: false; reason: TemporalFormatError };

/**
 * IANA 时区名的格式校验（非空、无空白、合理字符集、长度上限）。
 * 完整有效性（是否真实存在）由使用时的 Intl 构造 try/catch 兜底
 * （isValidTimeZoneName / invalid-time-zone）。timestampConfig 复用本函数。
 */
export function isValidIanaTimeZoneName(name: string): boolean {
  if (typeof name !== 'string' || name === '') return false;
  if (name.length > 100) return false;
  return /^[A-Za-z0-9_+\-/]+$/.test(name);
}

/** 运行环境时区（{ kind: 'local' } 的解析结果；无法确定时回退 'UTC'）。 */
function localTimeZoneName(): string {
  const tz = new Intl.DateTimeFormat().resolvedOptions().timeZone;
  return typeof tz === 'string' && tz !== '' ? tz : 'UTC';
}

/**
 * TimeZoneSpec → Intl 可用的时区名。
 * iana 只做格式校验（此处不查询 Intl，完整校验由调用方决定是否执行）。
 */
export function resolveTimeZoneName(
  spec: TimeZoneSpec,
): { ok: true; name: string } | { ok: false; reason: { reason: 'invalid-time-zone' } } {
  if (spec === null || typeof spec !== 'object')
    return { ok: false, reason: { reason: 'invalid-time-zone' } };
  switch (spec.kind) {
    case 'utc':
      return { ok: true, name: 'UTC' };
    case 'local':
      return { ok: true, name: localTimeZoneName() };
    case 'iana':
      if (typeof spec.name !== 'string' || !isValidIanaTimeZoneName(spec.name)) {
        return { ok: false, reason: { reason: 'invalid-time-zone' } };
      }
      return { ok: true, name: spec.name };
    default:
      return { ok: false, reason: { reason: 'invalid-time-zone' } };
  }
}

// ---------------------------------------------------------------------------
// 小数秒与年份的文本装配
// ---------------------------------------------------------------------------

function pad2(value: number): string {
  return String(value).padStart(2, '0');
}

/**
 * ISO 年份文本（对齐 Date.toISOString 的扩展年约定）：
 * 负年 `-0500`，4 位内补零 `0050`，>9999 带显式 `+100000`。
 */
function formatIsoYear(year: number): string {
  if (year < 0) return `-${String(-year).padStart(4, '0')}`;
  if (year > 9999) return `+${String(year)}`;
  return String(year).padStart(4, '0');
}

/** 秒内纳秒 → 9 位小数串（123456789 ns → "123456789"）。 */
function nanosToDigits9(nanos: number): string {
  return String(nanos).padStart(9, '0');
}

/**
 * 实际输出的小数秒位数：显式 fractionDigits 优先（0–9，按源精度不补零
 * 不截断）；缺省时按最小精确位数推导（123ms → 3 位，1ns → 9 位，0 → 0 位）。
 */
function resolveFractionDigits(fractionDigits: number | undefined, nanos: number): number {
  if (fractionDigits !== undefined) return fractionDigits;
  if (nanos === 0) return 0;
  const digits = nanosToDigits9(nanos);
  let end = 9;
  while (end > 0 && digits[end - 1] === '0') end -= 1;
  return end;
}

/** 组装 iso / iso-tz 的日期时间与小数秒部分（不含偏移后缀）。 */
function formatIsoBody(
  year: number,
  month: number,
  day: number,
  hour: number,
  minute: number,
  second: number,
  fractionDigits: number | undefined,
  subSecondNanos: number,
): string {
  const useDigits = resolveFractionDigits(fractionDigits, subSecondNanos);
  const fraction = useDigits === 0 ? '' : `.${nanosToDigits9(subSecondNanos).slice(0, useDigits)}`;
  return `${formatIsoYear(year)}-${pad2(month)}-${pad2(day)}T${pad2(hour)}:${pad2(minute)}:${pad2(second)}${fraction}`;
}

/** 时刻的 UTC 墙上分量（Date UTC 访问器对 0–99 年无 1900 映射问题）。 */
function utcWallOf(instant: InstantParts): {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
  nanos: number;
} {
  const date = new Date(instant.epochMs);
  return {
    year: date.getUTCFullYear(),
    month: date.getUTCMonth() + 1,
    day: date.getUTCDate(),
    hour: date.getUTCHours(),
    minute: date.getUTCMinutes(),
    second: date.getUTCSeconds(),
    // civil 毫秒 + 毫秒内纳秒 = 秒内纳秒（文件头不变量）。
    nanos: date.getUTCMilliseconds() * 1_000_000 + instant.subSecondNanos,
  };
}

// ---------------------------------------------------------------------------
// 三种格式
// ---------------------------------------------------------------------------

function formatIsoUtc(instant: InstantParts): string {
  const wall = utcWallOf(instant);
  return `${formatIsoBody(
    wall.year,
    wall.month,
    wall.day,
    wall.hour,
    wall.minute,
    wall.second,
    instant.fractionDigits,
    wall.nanos,
  )}Z`;
}

function formatIsoTz(instant: InstantParts, timeZoneName: string): TimestampFormatResult {
  const converted = instantToWallParts(instant, timeZoneName);
  if (!converted.ok) {
    // instant 与 zone 在 formatTimestampOutput 入口已校验，此分支仅防御性
    // 兜底。转换层的 readback-failed 在格式层无同名原因码（有效输入下
    // 不可达，见 TemporalConversionError 注释），按兜底约定归入
    // invalid-instant；其余三个原因码与格式层同名，原样透传。
    return converted.reason === 'readback-failed'
      ? { ok: false, reason: { reason: 'invalid-instant' } }
      : { ok: false, reason: { reason: converted.reason } };
  }
  const body = formatIsoBody(
    converted.wall.year,
    converted.wall.month,
    converted.wall.day,
    converted.wall.hour,
    converted.wall.minute,
    converted.wall.second,
    instant.fractionDigits,
    converted.wall.nanos,
  );
  // ±HH:MM（整分钟偏移）；历史亚分钟偏移输出 ±HH:MM:SS。
  const sign = converted.offsetSeconds < 0 ? '-' : '+';
  const abs = Math.abs(converted.offsetSeconds);
  const hours = Math.floor(abs / 3600);
  const minutes = Math.floor((abs % 3600) / 60);
  const seconds = abs % 60;
  const offsetText =
    seconds === 0
      ? `${sign}${pad2(hours)}:${pad2(minutes)}`
      : `${sign}${pad2(hours)}:${pad2(minutes)}:${pad2(seconds)}`;
  return { ok: true, text: `${body}${offsetText}` };
}

function formatLocale(
  instant: InstantParts,
  locale: string,
  timeZoneName: string,
): TimestampFormatResult {
  // 空串/非字符串 locale 会落入 ICU 默认语言（机器隐式默认），显式拒绝。
  if (typeof locale !== 'string' || locale === '') {
    return { ok: false, reason: { reason: 'invalid-locale' } };
  }
  const wall = utcWallOf(instant);
  const options: Intl.DateTimeFormatOptions = {
    timeZone: timeZoneName,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    // 偏移标记让用户能确认显示时区（PRD §3.3 生效反馈）。
    timeZoneName: 'shortOffset',
  };
  if (wall.nanos > 0) {
    // Intl 最高毫秒 3 位：微秒/纳秒源精度截断到毫秒（见文件头决策）。
    options.fractionalSecondDigits = 3;
  }
  try {
    const formatter = new Intl.DateTimeFormat(locale, options);
    return { ok: true, text: formatter.format(new Date(instant.epochMs)) };
  } catch {
    return { ok: false, reason: { reason: 'invalid-locale' } };
  }
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

/**
 * 按配置格式化时刻。DST 跨日、正负偏移、非整分钟偏移（+05:45）均由
 * instantToWallParts 的实际偏移计算保证正确。
 */
export function formatTimestampOutput(
  instant: InstantParts,
  opts: TimestampFormatOptions,
): TimestampFormatResult {
  const instantError = validateInstantParts(instant);
  if (instantError !== null) return { ok: false, reason: instantError };

  const zone = resolveTimeZoneName(opts.displayTimeZone);
  if (!zone.ok) return { ok: false, reason: zone.reason };
  // 无论何种格式都完整校验时区名（无效配置在预览与展示中一致失败）。
  if (!isValidTimeZoneName(zone.name))
    return { ok: false, reason: { reason: 'invalid-time-zone' } };

  switch (opts.format) {
    case 'iso':
      return { ok: true, text: formatIsoUtc(instant) };
    case 'iso-tz':
      return formatIsoTz(instant, zone.name);
    case 'locale':
      return formatLocale(instant, opts.locale, zone.name);
    default:
      return { ok: false, reason: { reason: 'invalid-locale' } };
  }
}
