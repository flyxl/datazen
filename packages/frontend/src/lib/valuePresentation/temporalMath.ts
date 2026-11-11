/**
 * temporalMath — 时刻与墙上时间的纯数学核心（issue-32 设计 §8，T02）。
 *
 * 零新依赖实现（前置核查结论，见任务汇报）：
 * - IANA 时区解析与「时刻 → 目标时区墙上分量」转换：ECMA-402
 *   `Intl.DateTimeFormat` 的 `timeZone` 选项 + `formatToParts`。Desktop
 *   WebView2 / WKWebView 与现代浏览器、Node（Linux 全量 tzdata）均支持。
 * - 微秒/纳秒精度：字符串分段解析 + `InstantParts` 组合数学，不经
 *   `Date`（其毫秒截断会丢精度）。
 * - DST 歧义检测：设计规定的「分量 → 时刻 → 回读分量」双向校验法
 *   （把墙上时间用前/中/后三个偏移试解，回读不一致即不存在或歧义）。
 *
 * 精度表示（InstantParts 不变量，务必遵守）：
 * - `epochMs` 是该时刻按**毫秒向下取整**的整数部分（安全整数）。对负时刻，
 *   floor 比 trunc 更负一毫秒，例如 -1234.000567ms 的 epochMs 是 -1235。
 * - `subSecondNanos ∈ [0, 999999]` 是**毫秒内**的纳秒余数（不是秒内！
 *   名称沿用任务约定，范围以 0–999999 为准），恒非负。
 * - 时刻 = epochMs + subSecondNanos × 10⁻⁶ 毫秒。该分解在不变量约束下
 *   唯一，因此 `compareInstants` 直接比较两组分量即严格全序。
 * - 墙上秒内纳秒 = positiveMod(epochMs, 1000) × 10⁶ + subSecondNanos；
 *   负 epochMs 时 civil 毫秒（如 -1235ms → 765ms）负责进位，subSecondNanos
 *   只叠加在 civil 毫秒之后，绝不产生借位。
 *
 * 支持范围（文档化的日期范围，任务允许自定义）：年份 [-100000, 100000]
 * （含），即 epochMs ∈ [MIN_SUPPORTED_EPOCH_MS, MAX_SUPPORTED_EPOCH_MS]。
 * 越界返回稳定原因码 `out-of-range`，不猜测不截断。
 *
 * 本模块是纯逻辑：无 React、无 DOM、无 Store 访问；Intl 格式器按
 * timeZone 做进程内缓存（缓存只影响性能，不影响结果——同一输入恒同一输出）。
 */

/** 精确时刻：epoch 毫秒（安全整数）+ 毫秒内纳秒余数（0–999999）。 */
export interface InstantParts {
  /** 该时刻按毫秒 floor 后的整数部分（负时刻比 trunc 更负一毫秒）。 */
  epochMs: number;
  /** 毫秒内纳秒余数，[0, 999999]；与 epochMs 分解唯一。 */
  subSecondNanos: number;
  /**
   * 源小数秒位数（0–9），供 iso/iso-tz 输出「不补零不截断」地保留源精度：
   * `.100` 输出 `.100` 而不是 `.1`，`.123456789` 输出 9 位。
   * 缺省时按「最小精确位数」推导（尾零裁剪，如 123ms → `.123`）。
   * 若调用方给出的位数小于实际亚毫秒精度，输出按位数截断（解析器不会
   * 产生这种状态，仅手工构造 InstantParts 时可能）。
   */
  fractionDigits?: number;
}

/** 墙上日期分量（无时区语义，公历，月份 1–12）。 */
export interface WallDate {
  year: number;
  month: number;
  day: number;
}

/** 墙上时间分量（无时区语义；nanos 是秒内纳秒 [0, 999999999]）。 */
export interface WallTime {
  hour: number;
  minute: number;
  second: number;
  nanos: number;
  /** 源小数秒位数（0–9），供输出保留源精度；缺省按最小精确位数推导。 */
  fractionDigits?: number;
}

/** 完整墙上日期时间分量（date + time）。 */
export interface WallComponents extends WallDate, WallTime {}

// ---------------------------------------------------------------------------
// 稳定错误原因码（判别联合，供 i18n / 上层引用；全部为字符串字面量）
// ---------------------------------------------------------------------------

/** 文本 / 数值解析类错误（parseZonedTimestamp / parseUnixValue / parseWallClockText）。 */
export type TemporalParseError =
  | { readonly reason: 'empty-input' }
  | { readonly reason: 'not-a-string' }
  | { readonly reason: 'not-a-number-or-string' }
  | { readonly reason: 'invalid-format' }
  | { readonly reason: 'invalid-integer-syntax' }
  | { readonly reason: 'not-safe-integer' }
  | { readonly reason: 'invalid-month' }
  | { readonly reason: 'invalid-day' }
  | { readonly reason: 'invalid-hour' }
  | { readonly reason: 'invalid-minute' }
  | { readonly reason: 'invalid-second' }
  | { readonly reason: 'invalid-offset' }
  | { readonly reason: 'fraction-digits-out-of-range' }
  | { readonly reason: 'out-of-range' };

/** 墙上时间 ↔ 时刻转换类错误（wallClockToInstant / instantToWallParts）。 */
export type TemporalConversionError =
  | { readonly reason: 'invalid-time-zone' }
  | { readonly reason: 'invalid-components' }
  | { readonly reason: 'invalid-instant' }
  | { readonly reason: 'out-of-range' }
  | { readonly reason: 'dst-ambiguous' }
  | { readonly reason: 'dst-invalid' }
  /** 防御性：Intl 回读分量不完整（有效输入下不可达，仅供上层兜底稳定码）。 */
  | { readonly reason: 'readback-failed' };

/** 格式化类错误（timestampFormat 使用）。 */
export type TemporalFormatError =
  | { readonly reason: 'invalid-instant' }
  | { readonly reason: 'invalid-time-zone' }
  | { readonly reason: 'invalid-locale' }
  | { readonly reason: 'out-of-range' };

/** 全部时间域稳定原因码的并集。 */
export type TemporalError = TemporalParseError | TemporalConversionError | TemporalFormatError;

/** 时刻解析结果（任务契约：ZonedTextParse 的形状，供带时区文本与 Unix 值共用）。 */
export type InstantParseResult =
  | { kind: 'ok'; instant: InstantParts }
  | { kind: 'error'; reason: TemporalParseError };

/** 任务指定的带时区文本解析结果名（与 InstantParseResult 同构）。 */
export type ZonedTextParse = InstantParseResult;

/** Unix 数值解析结果。 */
export type UnixValueParse = InstantParseResult;

/** 无时区文本解析结果：date / time / datetime 三种形状，均不做任何时区解释。 */
export type WallClockTextParse =
  | { kind: 'ok'; shape: 'datetime'; wall: WallComponents }
  | { kind: 'ok'; shape: 'date'; date: WallDate }
  | { kind: 'ok'; shape: 'time'; time: WallTime }
  | { kind: 'error'; reason: TemporalParseError };

/**
 * 墙上时间 + IANA 源时区 → 时刻 的结果。
 *
 * 拒绝猜测（PRD §4）：秋季回拨的重复墙上时间返回 `dst-ambiguous`，春季
 * 跳变的不存在墙上时间返回 `dst-invalid`，均不自动选择偏移。`invalid-components`
 * 与 `out-of-range` 是超出任务三原因码的文档化扩展（运行时垃圾输入防御）。
 */
export type WallToInstantResult =
  | { ok: true; instant: InstantParts }
  | {
      ok: false;
      reason:
        | 'dst-ambiguous'
        | 'dst-invalid'
        | 'invalid-time-zone'
        | 'invalid-components'
        | 'out-of-range';
    };

/**
 * 时刻 → 目标时区墙上分量 的结果。
 *
 * - `offsetSeconds`：该时刻在目标时区的实际 UTC 偏移总秒数（精确值；
 *   历史时区如 1850 年 Asia/Shanghai 的 LMT 偏移 +08:05:43 = 29143 秒）。
 * - `offsetMinutes`：`Math.trunc(offsetSeconds / 60)`，对现代整分钟偏移精确；
 *   亚分钟历史偏移只在 `offsetSeconds` 中精确（任务签名的便利字段）。
 */
export type InstantToWallResult =
  | { ok: true; wall: WallComponents; offsetSeconds: number; offsetMinutes: number }
  | {
      ok: false;
      reason: 'invalid-time-zone' | 'invalid-instant' | 'out-of-range' | 'readback-failed';
    };

// ---------------------------------------------------------------------------
// 支持范围常量
// ---------------------------------------------------------------------------

const MAX_SUPPORTED_YEAR = 100_000;
const GREGORIAN_400_YEARS_MS = 146_097 * 86_400_000; // 400 公历年 = 146097 天，精确

/** 支持的下界：-100000-01-01T00:00:00.000Z。 */
export const MIN_SUPPORTED_EPOCH_MS: number = Date.UTC(-MAX_SUPPORTED_YEAR, 0, 1);

/** 支持的上界：100000-12-31T23:59:59.999Z（含 100000 年全年）。 */
export const MAX_SUPPORTED_EPOCH_MS: number = Date.UTC(MAX_SUPPORTED_YEAR + 1, 0, 1) - 1;

const MIN_EPOCH_BI = BigInt(MIN_SUPPORTED_EPOCH_MS);
const MAX_EPOCH_BI = BigInt(MAX_SUPPORTED_EPOCH_MS);

const DAY_MS = 86_400_000;

// ---------------------------------------------------------------------------
// 基础算术
// ---------------------------------------------------------------------------

/** 数学取模，结果恒非负：positiveMod(-1235, 1000) === 765。 */
function positiveMod(value: number, modulus: number): number {
  return ((value % modulus) + modulus) % modulus;
}

/** 公历闰年（proleptic，与 Date/Intl 一致；负年份按同规则）。 */
function isLeapYear(year: number): boolean {
  return (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
}

function daysInMonth(year: number, month: number): number {
  switch (month) {
    case 1:
    case 3:
    case 5:
    case 7:
    case 8:
    case 10:
    case 12:
      return 31;
    case 4:
    case 6:
    case 9:
    case 11:
      return 30;
    case 2:
      return isLeapYear(year) ? 29 : 28;
    default:
      return 0;
  }
}

/**
 * 墙上分量（视为 UTC）→ epoch 毫秒。
 *
 * `Date.UTC` 会把 0–99 的年份映射为 1900–1999（构造器遗留行为），这里用
 * 「+400 公历年再减回」的精确修正（400 公历年 = 146097 天，无世纪例外）。
 * 超出 Date 表示域（约 ±275760 年）时返回 NaN，由调用方做范围检查。
 */
function epochMsFromUtcParts(
  year: number,
  monthIndex: number,
  day: number,
  hour: number,
  minute: number,
  second: number,
  millisecond: number,
): number {
  if (year >= 0 && year <= 99) {
    return (
      Date.UTC(year + 400, monthIndex, day, hour, minute, second, millisecond) -
      GREGORIAN_400_YEARS_MS
    );
  }
  return Date.UTC(year, monthIndex, day, hour, minute, second, millisecond);
}

// ---------------------------------------------------------------------------
// Intl 回读（时刻 → 目标时区墙上分量）
// ---------------------------------------------------------------------------

/**
 * 回读格式器：locale 固定 en-US（机器读回，非用户可见文案），字段全显式、
 * hourCycle h23（避免午夜 "24"）、era 显式（BC 年份的回读需要 "501 BC" →
 * 天文年 -500）。按 timeZone 缓存，避免逐格构造。
 */
const READBACK_LOCALE = 'en-US';
const FORMATTER_CACHE_LIMIT = 64;
const readbackFormatterCache = new Map<string, Intl.DateTimeFormat>();

function getReadbackFormatter(timeZone: string): Intl.DateTimeFormat {
  const cached = readbackFormatterCache.get(timeZone);
  if (cached !== undefined) return cached;
  const created = new Intl.DateTimeFormat(READBACK_LOCALE, {
    timeZone,
    year: 'numeric',
    era: 'short',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hourCycle: 'h23',
  });
  if (readbackFormatterCache.size >= FORMATTER_CACHE_LIMIT) {
    const oldest = readbackFormatterCache.keys().next().value;
    if (oldest !== undefined) readbackFormatterCache.delete(oldest);
  }
  readbackFormatterCache.set(timeZone, created);
  return created;
}

/** IANA 时区名是否可用（无效名在 Intl.DateTimeFormat 构造时抛 RangeError）。 */
export function isValidTimeZoneName(timeZone: string): boolean {
  if (typeof timeZone !== 'string' || timeZone === '') return false;
  try {
    getReadbackFormatter(timeZone);
    return true;
  } catch {
    return false;
  }
}

interface ReadbackWall {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
  millisecond: number;
}

/**
 * 时刻 → 目标时区墙上分量（含 civil 毫秒）。
 *
 * civil 毫秒不经 Intl（`millisecond` 选项在部分引擎不产生部件）：偏移量
 * 恒为整秒，故任意时区的 civil 毫秒 === positiveMod(epochMs, 1000)。
 */
function readWallParts(epochMs: number, formatter: Intl.DateTimeFormat): ReadbackWall | null {
  let year: number | null = null;
  let month = 0;
  let day = 0;
  let hour = 0;
  let minute = 0;
  let second = 0;
  let eraBc = false;
  try {
    const parts = formatter.formatToParts(new Date(epochMs));
    for (const part of parts) {
      switch (part.type) {
        case 'year': {
          const digits = Number(part.value.replace(/[^0-9]/g, ''));
          if (Number.isFinite(digits)) year = digits;
          break;
        }
        case 'month':
          month = Number(part.value);
          break;
        case 'day':
          day = Number(part.value);
          break;
        case 'hour':
          hour = Number(part.value);
          break;
        case 'minute':
          minute = Number(part.value);
          break;
        case 'second':
          second = Number(part.value);
          break;
        case 'era':
          eraBc = part.value.toUpperCase().includes('BC');
          break;
        default:
          break;
      }
    }
  } catch {
    return null;
  }
  if (year === null) return null;
  // Intl 显示「BC 公元前 501 年」对应天文（ISO）年 -500：year = 1 - 显示值。
  const isoYear = eraBc ? 1 - year : year;
  return {
    year: isoYear,
    month,
    day,
    hour,
    minute,
    second,
    millisecond: positiveMod(epochMs, 1000),
  };
}

/** 时刻在目标时区的实际偏移秒数（由回读分量与 epochMs 的精确差得出）。 */
function readOffsetSecondsAt(epochMs: number, formatter: Intl.DateTimeFormat): number | null {
  const rb = readWallParts(epochMs, formatter);
  if (rb === null) return null;
  const asUtc = epochMsFromUtcParts(
    rb.year,
    rb.month - 1,
    rb.day,
    rb.hour,
    rb.minute,
    rb.second,
    rb.millisecond,
  );
  if (!Number.isFinite(asUtc)) return null;
  return (asUtc - epochMs) / 1000;
}

// ---------------------------------------------------------------------------
// 解析：带时区文本
// ---------------------------------------------------------------------------

/**
 * 带时区时间戳文本的严格语法。手工分段解析，不交给 `new Date(string)`
 * （其对非标准形式宽容且各引擎不一致）。接受：
 * - 日期：`YYYY-MM-DD`，可选符号的 4–6 位年（扩展年，如 `-0500-…`、`+10000-…`）。
 * - 分隔：`T`、`t` 或单个空格（数据库文本常见 `2026-01-15 14:30:00+08:00`）。
 * - 时间：`HH:MM`（秒可省）、`HH:MM:SS`、小数秒 1–9 位（超出报
 *   `fraction-digits-out-of-range`，不接受 >9 位）。
 * - 时区：`Z`/`z`、`±HH:MM`、`±HHMM`、`±HH`（Postgres 风格短偏移）。
 * 拒绝：24:00（end-of-day 形式）、:60 闰秒（数据库不产出，明确不支持）、
 * 月份/日期/分秒越界、前导尾随空白。
 */
const ZONED_TIMESTAMP_RE =
  /^([+-]?\d{4,6})-(\d{2})-(\d{2})[Tt ](\d{2}):(\d{2})(?::(\d{2})(?:\.(\d+))?)?(Z|z|[+-]\d{2}(?::?\d{2})?)$/;

/** 解析带时区文本为精确时刻（保留全部小数秒精度）。 */
export function parseZonedTimestamp(text: string): ZonedTextParse {
  if (typeof text !== 'string') return { kind: 'error', reason: { reason: 'not-a-string' } };
  if (text === '') return { kind: 'error', reason: { reason: 'empty-input' } };
  const match = ZONED_TIMESTAMP_RE.exec(text);
  if (match === null) return { kind: 'error', reason: { reason: 'invalid-format' } };

  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const hour = Number(match[4]);
  const minute = Number(match[5]);
  const second = match[6] !== undefined ? Number(match[6]) : 0;
  const fraction = match[7];
  const zone = match[8];

  if (month < 1 || month > 12) return { kind: 'error', reason: { reason: 'invalid-month' } };
  if (day < 1 || day > daysInMonth(year, month))
    return { kind: 'error', reason: { reason: 'invalid-day' } };
  if (hour > 23) return { kind: 'error', reason: { reason: 'invalid-hour' } };
  if (minute > 59) return { kind: 'error', reason: { reason: 'invalid-minute' } };
  if (second > 59) return { kind: 'error', reason: { reason: 'invalid-second' } };

  let fractionDigits = 0;
  let nanosWithinSecond = 0;
  if (fraction !== undefined) {
    if (fraction.length > 9) {
      return { kind: 'error', reason: { reason: 'fraction-digits-out-of-range' } };
    }
    fractionDigits = fraction.length;
    nanosWithinSecond = Number(fraction.padEnd(9, '0'));
  }

  // 时区偏移（毫秒）。Z → 0；±HH(:MM)? 由数字段构造。
  let offsetMs = 0;
  if (zone !== 'Z' && zone !== 'z') {
    const sign = zone[0] === '-' ? -1 : 1;
    const offsetHour = Number(zone.slice(1, 3));
    const minutePart = zone.length > 3 ? Number(zone.replace(/[^0-9]/g, '').slice(2)) : 0;
    if (offsetHour > 23 || minutePart > 59) {
      return { kind: 'error', reason: { reason: 'invalid-offset' } };
    }
    offsetMs = sign * (offsetHour * 3_600_000 + minutePart * 60_000);
  }

  // 毫秒位并入 epochMs（见下方拆分说明），毫秒内纳秒余数留在 subSecondNanos。
  const epochMs =
    epochMsFromUtcParts(
      year,
      month - 1,
      day,
      hour,
      minute,
      second,
      Math.floor(nanosWithinSecond / 1_000_000),
    ) - offsetMs;
  if (!Number.isFinite(epochMs) || !Number.isSafeInteger(epochMs)) {
    return { kind: 'error', reason: { reason: 'out-of-range' } };
  }
  if (epochMs < MIN_SUPPORTED_EPOCH_MS || epochMs > MAX_SUPPORTED_EPOCH_MS) {
    return { kind: 'error', reason: { reason: 'out-of-range' } };
  }
  // 拆分不变量：小数秒的 9 位纳秒按「前 3 位进 epochMs、后 6 位作 subSecondNanos」
  // 分解。offsetMs 恒为整秒（±HH:MM 偏移），不破坏毫秒对齐，因此 epochMs 始终
  // 等于该时刻的 floor（负时刻同样成立，如 -0.000001ms → epochMs -1）。
  return {
    kind: 'ok',
    instant: { epochMs, subSecondNanos: nanosWithinSecond % 1_000_000, fractionDigits },
  };
}

// ---------------------------------------------------------------------------
// 解析：Unix 数值
// ---------------------------------------------------------------------------

const INTEGER_STRING_RE = /^[+-]?\d+$/;

/**
 * Unix 时间戳解析。
 *
 * - number：必须 `Number.isSafeInteger`（NaN/±Infinity/2^53 越界均拒绝）。
 * - string：先验证整数语法（可选前导 `+`/`-`，禁空白、小数、指数、非数字），
 *   再 BigInt 精确解析；秒乘 1000 在 BigInt 域完成，不做浮点近似。
 * - 结果转 InstantParts 前检查支持范围（±100000 年，见文件头）。
 * - 0 与负数是正常值（1970 前的 1969 等），不猜测不截断。
 */
export function parseUnixValue(
  value: number | string,
  unit: 'seconds' | 'milliseconds',
): UnixValueParse {
  let msBigInt: bigint;
  if (typeof value === 'number') {
    if (!Number.isSafeInteger(value))
      return { kind: 'error', reason: { reason: 'not-safe-integer' } };
    msBigInt = BigInt(value) * (unit === 'seconds' ? 1000n : 1n);
  } else if (typeof value === 'string') {
    if (!INTEGER_STRING_RE.test(value))
      return { kind: 'error', reason: { reason: 'invalid-integer-syntax' } };
    msBigInt = BigInt(value) * (unit === 'seconds' ? 1000n : 1n);
  } else {
    return { kind: 'error', reason: { reason: 'not-a-number-or-string' } };
  }
  if (msBigInt < MIN_EPOCH_BI || msBigInt > MAX_EPOCH_BI) {
    return { kind: 'error', reason: { reason: 'out-of-range' } };
  }
  const epochMs = Number(msBigInt);
  if (!Number.isSafeInteger(epochMs)) {
    return { kind: 'error', reason: { reason: 'out-of-range' } };
  }
  // BUG-003：毫秒余数真实存在时不得谎报 fractionDigits 0，否则 iso/iso-tz
  // 组装会被显式 0 截断（resolveFractionDigits 显式优先），静默丢弃 .123。
  // 正模使负 epochMs（如 -1 → 余数 999）同样得到 3 位；秒单位的 epochMs
  // 恒为 1000 倍数，余数 0，fractionDigits 保持 0。
  const fractionDigits = positiveMod(epochMs, 1000) === 0 ? 0 : 3;
  return { kind: 'ok', instant: { epochMs, subSecondNanos: 0, fractionDigits } };
}

// ---------------------------------------------------------------------------
// 解析：无时区文本（墙上分量，不做任何时区解释）
// ---------------------------------------------------------------------------

const DATE_ONLY_RE = /^([+-]?\d{4,6})-(\d{2})-(\d{2})$/;
const TIME_ONLY_RE = /^(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d+))?)?$/;
const WALL_DATETIME_RE =
  /^([+-]?\d{4,6})-(\d{2})-(\d{2})[Tt ](\d{2}):(\d{2})(?::(\d{2})(?:\.(\d+))?)?$/;

/**
 * 解析无时区文本为墙上分量。三种形状：
 * - date：`2026-03-01`
 * - datetime：`2026-03-01 00:15:30.123`（`T`/`t`/空格分隔，秒与小数秒可省）
 * - time：`00:15:30.123`（`14:30` 分钟精度也接受）
 *
 * 只做语法与分量校验，绝不解释为任何时区（不隐式漂移）；带偏移的形式
 * 属于 parseZonedTimestamp 的职责，这里返回 invalid-format。
 */
export function parseWallClockText(text: string): WallClockTextParse {
  if (typeof text !== 'string') return { kind: 'error', reason: { reason: 'not-a-string' } };
  if (text === '') return { kind: 'error', reason: { reason: 'empty-input' } };

  const datetimeMatch = WALL_DATETIME_RE.exec(text);
  if (datetimeMatch !== null) {
    const wall = buildWallComponents(datetimeMatch);
    if ('kind' in wall) return wall;
    return { kind: 'ok', shape: 'datetime', wall };
  }

  const dateMatch = DATE_ONLY_RE.exec(text);
  if (dateMatch !== null) {
    const year = Number(dateMatch[1]);
    const month = Number(dateMatch[2]);
    const day = Number(dateMatch[3]);
    if (month < 1 || month > 12) return { kind: 'error', reason: { reason: 'invalid-month' } };
    if (day < 1 || day > daysInMonth(year, month))
      return { kind: 'error', reason: { reason: 'invalid-day' } };
    return { kind: 'ok', shape: 'date', date: { year, month, day } };
  }

  const timeMatch = TIME_ONLY_RE.exec(text);
  if (timeMatch !== null) {
    const time = buildWallTime(timeMatch);
    if ('kind' in time) return time;
    return { kind: 'ok', shape: 'time', time };
  }

  return { kind: 'error', reason: { reason: 'invalid-format' } };
}

/** datetime 正则分组 → 完整墙上分量；失败返回解析错误（窄化辅助）。 */
function buildWallComponents(
  match: RegExpExecArray,
): WallComponents | { kind: 'error'; reason: TemporalParseError } {
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const hour = Number(match[4]);
  const minute = Number(match[5]);
  const second = match[6] !== undefined ? Number(match[6]) : 0;
  if (month < 1 || month > 12) return { kind: 'error', reason: { reason: 'invalid-month' } };
  if (day < 1 || day > daysInMonth(year, month))
    return { kind: 'error', reason: { reason: 'invalid-day' } };
  if (hour > 23) return { kind: 'error', reason: { reason: 'invalid-hour' } };
  if (minute > 59) return { kind: 'error', reason: { reason: 'invalid-minute' } };
  if (second > 59) return { kind: 'error', reason: { reason: 'invalid-second' } };
  const fraction = match[7];
  let fractionDigits = 0;
  let nanos = 0;
  if (fraction !== undefined) {
    if (fraction.length > 9)
      return { kind: 'error', reason: { reason: 'fraction-digits-out-of-range' } };
    fractionDigits = fraction.length;
    nanos = Number(fraction.padEnd(9, '0'));
  }
  return { year, month, day, hour, minute, second, nanos, fractionDigits };
}

/** time 正则分组 → 墙上时间分量；失败返回解析错误（窄化辅助）。 */
function buildWallTime(
  match: RegExpExecArray,
): WallTime | { kind: 'error'; reason: TemporalParseError } {
  const hour = Number(match[1]);
  const minute = Number(match[2]);
  const second = match[3] !== undefined ? Number(match[3]) : 0;
  if (hour > 23) return { kind: 'error', reason: { reason: 'invalid-hour' } };
  if (minute > 59) return { kind: 'error', reason: { reason: 'invalid-minute' } };
  if (second > 59) return { kind: 'error', reason: { reason: 'invalid-second' } };
  const fraction = match[4];
  let fractionDigits = 0;
  let nanos = 0;
  if (fraction !== undefined) {
    if (fraction.length > 9)
      return { kind: 'error', reason: { reason: 'fraction-digits-out-of-range' } };
    fractionDigits = fraction.length;
    nanos = Number(fraction.padEnd(9, '0'));
  }
  return { hour, minute, second, nanos, fractionDigits };
}

// ---------------------------------------------------------------------------
// 转换：墙上时间 + 时区 → 时刻（DST 双向校验）
// ---------------------------------------------------------------------------

function checkWallComponents(wall: WallComponents): { reason: 'invalid-components' } | null {
  if (wall === null || typeof wall !== 'object') return { reason: 'invalid-components' };
  const { year, month, day, hour, minute, second, nanos, fractionDigits } = wall;
  if (
    !Number.isInteger(year) ||
    !Number.isInteger(month) ||
    !Number.isInteger(day) ||
    !Number.isInteger(hour) ||
    !Number.isInteger(minute) ||
    !Number.isInteger(second) ||
    !Number.isInteger(nanos)
  ) {
    return { reason: 'invalid-components' };
  }
  if (month < 1 || month > 12) return { reason: 'invalid-components' };
  if (day < 1 || day > daysInMonth(year, month)) return { reason: 'invalid-components' };
  if (hour < 0 || hour > 23) return { reason: 'invalid-components' };
  if (minute < 0 || minute > 59) return { reason: 'invalid-components' };
  if (second < 0 || second > 59) return { reason: 'invalid-components' };
  if (nanos < 0 || nanos > 999_999_999) return { reason: 'invalid-components' };
  if (
    fractionDigits !== undefined &&
    (!Number.isInteger(fractionDigits) || fractionDigits < 0 || fractionDigits > 9)
  ) {
    return { reason: 'invalid-components' };
  }
  return null;
}

/**
 * 墙上时间 + IANA 源时区 → 时刻。
 *
 * DST 双向校验法（设计 §8.5 / 任务规定）：把墙上分量视为 UTC 得到试探时刻
 * `asUtc`，取 asUtc 前一天 / 当时 / 后一天三个探测时刻的实际偏移作为候选
 * 集合；对每个候选偏移 o 计算候选时刻 t = asUtc − o，再回读 t 的墙上分量，
 * 与输入分量完全一致才算有效解释。
 * - 0 个有效 → 该墙上时间不存在（春季跳变）→ `dst-invalid`；
 * - ≥2 个有效 → 该墙上时间出现两次（秋季回拨）→ `dst-ambiguous`，拒绝猜测；
 * - 恰 1 个 → 唯一时刻。
 *
 * 候选集取自 ±24h 探测，覆盖 tzdata 全部现存时区（含 24 小时整日跳变如
 * 2011 Pacific/Apia）；若某历史时区在 24h 内发生两次偏移变更，可能漏判
 * 有效解释（误报 dst-invalid）——已知 tzdata 无此情形，属文档化边界。
 *
 * 精度透传：wall.nanos 的毫秒位并入 epochMs，毫秒内余数作 subSecondNanos
 * （与 instantToWallParts 互逆）；`fractionDigits` 原样透传——输入缺省时
 * 输出也缺省，由格式层按「最小精确位数」推导（见 InstantParts.fractionDigits
 * 契约）。不虚构 0：对 nanos ≠ 0 的缺省输入补 0 会构成「位数小于实际精度」
 * 的截断态，违反 InstantParts 不变量。
 */
export function wallClockToInstant(wall: WallComponents, timeZone: string): WallToInstantResult {
  const componentError = checkWallComponents(wall);
  if (componentError !== null) return { ok: false, reason: componentError.reason };

  let formatter: Intl.DateTimeFormat;
  try {
    formatter = getReadbackFormatter(timeZone);
  } catch {
    return { ok: false, reason: 'invalid-time-zone' };
  }

  const asUtc = epochMsFromUtcParts(
    wall.year,
    wall.month - 1,
    wall.day,
    wall.hour,
    wall.minute,
    wall.second,
    Math.floor(wall.nanos / 1_000_000),
  );
  if (!Number.isFinite(asUtc) || !Number.isSafeInteger(asUtc)) {
    return { ok: false, reason: 'out-of-range' };
  }
  // 探测点允许越界一天（±24h），最终时刻再按支持范围收紧。
  if (asUtc < MIN_SUPPORTED_EPOCH_MS - DAY_MS || asUtc > MAX_SUPPORTED_EPOCH_MS + DAY_MS) {
    return { ok: false, reason: 'out-of-range' };
  }

  const candidateOffsets = new Set<number>();
  for (const probe of [asUtc - DAY_MS, asUtc, asUtc + DAY_MS]) {
    const offset = readOffsetSecondsAt(probe, formatter);
    if (offset !== null) candidateOffsets.add(offset);
  }

  const wallMsPart = Math.floor(wall.nanos / 1_000_000);
  const matches: number[] = [];
  for (const offsetSeconds of candidateOffsets) {
    const candidateMs = asUtc - offsetSeconds * 1000;
    const rb = readWallParts(candidateMs, formatter);
    if (rb === null) continue;
    if (
      rb.year === wall.year &&
      rb.month === wall.month &&
      rb.day === wall.day &&
      rb.hour === wall.hour &&
      rb.minute === wall.minute &&
      rb.second === wall.second &&
      rb.millisecond === wallMsPart
    ) {
      matches.push(candidateMs);
    }
  }

  if (matches.length === 0) return { ok: false, reason: 'dst-invalid' };
  if (matches.length > 1) return { ok: false, reason: 'dst-ambiguous' };
  const epochMs = matches[0];
  if (epochMs < MIN_SUPPORTED_EPOCH_MS || epochMs > MAX_SUPPORTED_EPOCH_MS) {
    return { ok: false, reason: 'out-of-range' };
  }
  return {
    ok: true,
    instant: {
      epochMs,
      subSecondNanos: wall.nanos % 1_000_000,
      fractionDigits: wall.fractionDigits,
    },
  };
}

// ---------------------------------------------------------------------------
// 转换：时刻 → 目标时区墙上分量
// ---------------------------------------------------------------------------

/**
 * InstantParts 运行时校验（类型之外的防御检查，供本模块与 timestampFormat
 * 共用；T03 构造 InstantParts 后也可复用）。返回 null 表示通过。
 */
export function validateInstantParts(
  instant: InstantParts,
): { reason: 'invalid-instant' } | { reason: 'out-of-range' } | null {
  if (instant === null || typeof instant !== 'object') return { reason: 'invalid-instant' };
  const { epochMs, subSecondNanos, fractionDigits } = instant;
  if (typeof epochMs !== 'number' || !Number.isSafeInteger(epochMs)) {
    return { reason: 'invalid-instant' };
  }
  if (
    typeof subSecondNanos !== 'number' ||
    !Number.isInteger(subSecondNanos) ||
    subSecondNanos < 0 ||
    subSecondNanos > 999_999
  ) {
    return { reason: 'invalid-instant' };
  }
  if (
    fractionDigits !== undefined &&
    (!Number.isInteger(fractionDigits) || fractionDigits < 0 || fractionDigits > 9)
  ) {
    return { reason: 'invalid-instant' };
  }
  if (epochMs < MIN_SUPPORTED_EPOCH_MS || epochMs > MAX_SUPPORTED_EPOCH_MS) {
    return { reason: 'out-of-range' };
  }
  return null;
}

/**
 * 时刻 → 目标时区墙上分量 + 实际偏移。
 *
 * subSecondNanos 原样带回：wall.nanos = civil 毫秒 × 10⁶ + subSecondNanos
 * （见文件头不变量），fractionDigits 一并透传。
 */
export function instantToWallParts(instant: InstantParts, timeZone: string): InstantToWallResult {
  const instantError = validateInstantParts(instant);
  if (instantError !== null) return { ok: false, reason: instantError.reason };

  let formatter: Intl.DateTimeFormat;
  try {
    formatter = getReadbackFormatter(timeZone);
  } catch {
    return { ok: false, reason: 'invalid-time-zone' };
  }

  const rb = readWallParts(instant.epochMs, formatter);
  if (rb === null) return { ok: false, reason: 'readback-failed' };

  const asUtc = epochMsFromUtcParts(
    rb.year,
    rb.month - 1,
    rb.day,
    rb.hour,
    rb.minute,
    rb.second,
    rb.millisecond,
  );
  if (!Number.isFinite(asUtc)) return { ok: false, reason: 'readback-failed' };
  const offsetSeconds = (asUtc - instant.epochMs) / 1000;

  return {
    ok: true,
    wall: {
      year: rb.year,
      month: rb.month,
      day: rb.day,
      hour: rb.hour,
      minute: rb.minute,
      second: rb.second,
      nanos: rb.millisecond * 1_000_000 + instant.subSecondNanos,
      fractionDigits: instant.fractionDigits,
    },
    offsetSeconds,
    offsetMinutes: Math.trunc(offsetSeconds / 60),
  };
}

// ---------------------------------------------------------------------------
// 工具
// ---------------------------------------------------------------------------

/**
 * 偏移分钟 → `±HH:MM` 文本（如 480 → `+08:00`，-300 → `-05:00`）。
 * 输入应为整分钟（instantToWallParts.offsetMinutes 满足）；非整数分钟按
 * 向零截断处理（文档化行为，不做四舍五入猜测）。
 */
export function formatOffsetMinutes(offsetMinutes: number): string {
  const truncated = Math.trunc(offsetMinutes);
  const sign = truncated < 0 ? '-' : '+';
  const abs = Math.abs(truncated);
  const hours = Math.floor(abs / 60);
  const minutes = abs % 60;
  return `${sign}${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}`;
}

/**
 * 偏移秒数 → `±HH:MM`（整分钟时）或 `±HH:MM:SS`（亚分钟历史偏移，如
 * 29143 → `+08:05:43`）。iso-tz 输出使用本函数以保历史偏移精确。
 */
export function formatOffsetSeconds(offsetSeconds: number): string {
  const truncated = Math.trunc(offsetSeconds);
  const sign = truncated < 0 ? '-' : '+';
  const abs = Math.abs(truncated);
  const hours = Math.floor(abs / 3600);
  const minutes = Math.floor((abs % 3600) / 60);
  const seconds = abs % 60;
  const head = `${sign}${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}`;
  return seconds === 0 ? head : `${head}:${String(seconds).padStart(2, '0')}`;
}

/**
 * 严格全序比较两个时刻：-1 / 0 / 1。
 * InstantParts 分解在不变量约束下唯一，逐分量比较即等值比较。
 */
export function compareInstants(a: InstantParts, b: InstantParts): number {
  if (a.epochMs !== b.epochMs) return a.epochMs < b.epochMs ? -1 : 1;
  if (a.subSecondNanos !== b.subSecondNanos) return a.subSecondNanos < b.subSecondNanos ? -1 : 1;
  return 0;
}
