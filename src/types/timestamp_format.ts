/**
 * 时间戳的显示格式配置（GitHub issue #32）。
 *
 * 这个文件是 **Wave 11 的跨轨契约**：四条轨并行开发，只有它们共用的**数据形状**
 * 与**接口**放在这里，生产实现分属各轨。本文件不含任何格式化逻辑。
 *
 * 三条设计约定（每条都有理由，不是偏好）：
 *
 * 1. **模式是字符串，不是 `RegExp`。** `RegExp` 不可序列化 ⇒ 存不进 settings
 *    JSON；而且用户要能在界面上编辑它。运行时编译，并必须防 ReDoS。
 * 2. **默认关闭。** issue 的 Backwards Compatibility 明确要求 opt-in：
 *    既有连接与既有设置**逐字节不变**。所以 `enabled` 缺省 `false`，
 *    而「没启用」时渲染结果必须与今天**逐字相同**。
 * 3. **规则匹配是纯前端行为。** 时区来自浏览器的 `Intl`，渲染也发生在浏览器里
 *    ⇒ 规则引擎与格式化本体都不需要 Rust、不需要 IPC、不需要共享 crate。
 *    需要跨形态共享的只有**表级覆盖的存储**（那是宿主数据，走命令）。
 */

/** 一次时间戳的显示格式。 */
export type TimestampFormatKind =
  /** `2026-01-15T14:30:00.000Z` —— 与今天一致。 */
  | 'iso'
  /** `2026-01-15T14:30:00+08:00` —— 带时区偏移。 */
  | 'iso-tz'
  /** 浏览器本地化格式（`toLocaleString()`）。 */
  | 'locale'
  /** 用户自定义格式串。**语义待定，见 `TimestampFormatSetting.customPattern`。 */
  | 'custom';

/** 一个 Unix 数字被当作哪种单位解释。 */
export type TimestampUnixUnit = 'seconds' | 'milliseconds';

/**
 * 大整型列的语义提示。
 *
 * 为什么需要它：issue 要求「BIGINT + 列名像时间 ⇒ 当 Unix 时间戳」，但
 * 一个叫 `at` 的 BIGINT 列可能是计数而不是时间戳。所以**推断必须可见**：
 * 命中时界面要显示「Unix 秒 / Unix 毫秒」，并且允许逐列覆盖。
 */
export type TimestampSemanticHint = 'timestamp' | 'unix_seconds' | 'unix_millis';

/**
 * 一份「怎么显示时间戳」的设置。
 *
 * `kind: 'custom'` 时的 `customPattern` **语义未定**：issue 的示例是 Java 风格
 * （`yyyy-MM-dd HH:mm:ss`），而 JS 生态（`Intl` / dayjs / date-fns）是
 * `YYYY-MM-DD` —— 两者在小写 `yyyy` 上含义相反（JS 的 `yyyy` 是「年内的第几天」）。
 * ⇒ 在语义定死之前，`custom` **不应被实现**，也不应被界面暴露。
 */
export interface TimestampFormatSetting {
  /** 缺省 `'iso'`。 */
  kind: TimestampFormatKind;
  /** 缺省 `false`（opt-in）。 */
  enabled: boolean;
  /** `kind === 'custom'` 时的格式串（语义见上）。 */
  customPattern?: string;
  /** IANA 时区名（如 `Asia/Shanghai`）；缺省 = 浏览器本地时区。 */
  timezone?: string;
  /** 数字被当作 Unix 时间戳时的单位；缺省 = 毫秒（与今天的 `formatTimestamp` 一致）。 */
  unixUnit?: TimestampUnixUnit;
}

/** 一条规则：命中条件 + 应用什么格式。 */
export interface TimestampFormattingRule {
  id: string;
  name: string;
  enabled: boolean;
  /** 数字越小越先命中；同优先级时也按它排序。 */
  priority: number;
  /** 表名模式（**字符串**，运行时编译）。 */
  tableNamePattern?: string;
  /** 列名模式（**字符串**，运行时编译）。 */
  columnNamePattern?: string;
  /** 列类型模式（**字符串**，运行时编译）。 */
  dataTypePattern?: string;
  format: TimestampFormatSetting;
}

/** 表级、列级的覆盖：优先级最高。 */
export interface TimestampColumnOverride {
  /** 持久化连接 id（**绝不是** `dbSessionId` —— 它从不落盘）。 */
  connectionId: string;
  table: string;
  column: string;
  format: TimestampFormatSetting;
  /** 大整型列被解释成什么（Unix 秒 / Unix 毫秒 / 时间戳）。 */
  semanticHint?: TimestampSemanticHint;
  /** 显式关闭这一列的格式化（压过一切规则）。 */
  disabled?: boolean;
}

/** 用户级的格式化配置（存在 settings 里）。 */
export interface TimestampFormattingConfig {
  /** 缺省：`{ kind: 'iso', enabled: false }`。 */
  global: TimestampFormatSetting;
  /** 缺省空数组。 */
  rules: TimestampFormattingRule[];
}

/** 判定「这次渲染该用哪个格式」所需的全部事实。 */
export interface TimestampFormatQuery {
  table?: string;
  column: string;
  dataType?: string;
  config: TimestampFormattingConfig;
  /** 表级覆盖（调用方按 connectionId + table + column 过滤好再传进来）。 */
  overrides?: readonly TimestampColumnOverride[];
}

/** 一次格式化的额外参数。 */
export interface TimestampFormatOptions {
  timezone?: string;
  unixUnit?: TimestampUnixUnit;
  semanticHint?: TimestampSemanticHint;
}

/** 判定结果：格式 + 它是从哪一级来的（界面要能说明「为什么是这个格式」）。 */
export interface ResolvedTimestampFormat {
  setting: TimestampFormatSetting;
  /** 命中的层级：表级覆盖 > 规则 > 全局。 */
  source: 'override' | 'rule' | 'global';
  /** `source === 'rule'` 时命中的规则 id。 */
  ruleId?: string;
  /** 大整型列被推断出来的语义（界面要显示它）。 */
  semanticHint?: TimestampSemanticHint;
}

/**
 * 格式化引擎：**宿主与扩展都只依赖这个接口**。
 *
 * 扩展点（若将来开一个「单元格显示」扩展点）实现它；宿主保留默认实现兜底。
 * 引擎是**无状态**的：配置与覆盖由调用方经 `query` 传入，所以它可以被单测、
 * 可以被替换、不需要在渲染路径上持有任何 store。
 */
export interface TimestampFormatEngine {
  /** 三级优先级：表级覆盖 > 规则 > 全局。 */
  resolve(query: TimestampFormatQuery): ResolvedTimestampFormat;
  /** 把一个值渲染成文本。 */
  format(
    value: unknown,
    setting: TimestampFormatSetting,
    options?: TimestampFormatOptions,
  ): string;
}

/** 网格渲染路径需要的上下文数据（由 Provider 提供）。 */
export interface TimestampFormatContextValue {
  config: TimestampFormattingConfig;
  overrides: readonly TimestampColumnOverride[];
}
