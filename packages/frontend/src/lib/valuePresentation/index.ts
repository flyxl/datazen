/**
 * @datazen/frontend lib/valuePresentation — 字段展示扩展（issue-32）。
 *
 * T02 交付的纯逻辑核心（无 React、无 DOM、无 Store 依赖）：
 * - temporalMath：时刻 / 墙上时间解析与转换（DST 双向校验、微秒纳秒精度）。
 * - timestampFormat：iso / iso-tz / locale 三种输出格式。
 * - timestampConfig：时间展示器配置类型与纯验证（JSON 可序列化）。
 *
 * T03（展示器贡献注册）及之后的管线模块在本目录追加；展示器契约类型见
 * `@datazen/extension-points`（T01 已交付）。
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
