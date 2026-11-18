/**
 * valuePresentation 贡献契约 — 字段展示扩展公共类型（issue-32 设计 §4）。
 *
 * 首版只开放 text 输出：PresentationResult 只有 'text' 与 'unchanged' 两个
 * kind，不提前接受 link/image 或任意 ReactNode（设计 §11 留待后续按需扩展，
 * 不允许未知 kind 被当前宿主误处理）。
 *
 * 配置与公共契约必须是 JSON 可序列化结构；PreparedPresentation 中的函数仅
 * 存在于进程内，不持久化。全部类型不使用 any，不以断言跳过验证。
 */

/**
 * 单元格原始值。bigint 属于值输入契约，不代表现有 transport 需要新增
 * bigint 编码（宿主优先消费原有整数字符串）。
 */
export type PresentationValue =
  | null
  | undefined
  | string
  | number
  | boolean
  | bigint
  | readonly unknown[]
  | Readonly<Record<string, unknown>>;

/** 值的来源定位（宿主负责来源身份，不含凭据）。 */
export interface ColumnSource {
  connectionId: string;
  database?: string;
  namespace?: string;
  table: string;
  column: string;
}

/** 准备列展示计划时的列上下文。 */
export interface PresentationColumnContext {
  columnName: string;
  databaseTypeName?: string;
  semanticKind: 'instant' | 'local-datetime' | 'date' | 'time' | 'number' | 'text' | 'other';
  source?: Readonly<ColumnSource>;
}

/** 展示环境快照（不依赖机器隐式默认值）。 */
export interface PresentationEnvironment {
  locale: string;
  localTimeZone: string;
}

/** 展示结果：输出文本或明确回退（普通非法值回退，不算扩展异常）。 */
export type PresentationResult =
  | { kind: 'text'; text: string; interpretation?: string }
  | { kind: 'unchanged'; reason?: string };

/** prepare 产出的进程内展示器：规则与 Intl 格式器在 prepare 阶段创建。 */
export interface PreparedPresentation {
  present(value: PresentationValue): PresentationResult;
}

/**
 * JSON 可序列化的配置值（递归结构）。
 *
 * validateConfig 成功时返回的 config、以及持久化的展示配置都必须满足本类型；
 * 函数与类实例不允许进入配置。
 */
export type PresentationConfigValue =
  | null
  | boolean
  | number
  | string
  | readonly PresentationConfigValue[]
  | { readonly [key: string]: PresentationConfigValue };

/** 路径化验证问题：path 指向 JSON 输入内的位置，message 说明原因。 */
export interface ConfigValidationProblem {
  path: string;
  message: string;
}

/**
 * 配置验证结果判别联合。
 *
 * - 成功：携带验证后的 JSON 可序列化配置，宿主可直接持久化；
 * - 失败：携带路径化问题列表；不抛异常、不返回部分配置。
 */
export type ConfigValidationResult =
  | { ok: true; config: PresentationConfigValue }
  | { ok: false; problems: ReadonlyArray<ConfigValidationProblem> };

/**
 * 已验证的版本化配置快照。
 *
 * configVersion 与贡献声明的 configVersion 一致。JSON 输入在宿主边界交给
 * validateConfig 验证；未知版本不覆盖旧配置（由宿主执行，贡献只负责验证与
 * 纯迁移声明）。
 */
export interface VersionedPresentationConfig {
  readonly configVersion: number;
  readonly config: PresentationConfigValue;
}

/**
 * 表列覆盖（设计 §5）。与格式选项分离组织：表定位信息由宿主持有，
 * 覆盖只声明展示模式；custom 的 options 是未经校验的原始 JSON，
 * 由宿主交给目标 rendererId 的 validateConfig 验证。
 */
export type ColumnPresentationOverride =
  | { readonly mode: 'inherit' }
  | { readonly mode: 'raw' }
  | {
      readonly mode: 'custom';
      readonly rendererId: string;
      readonly options: unknown;
    };

/**
 * prepare 输入：宿主在面板列展示计划阶段组装的不可变快照。
 * EP 激活阶段只注册能力，不读取 Store、文件或接口；配置由宿主加载、
 * 验证后传入（设计 §6）。
 */
export interface PresentationPrepareInput {
  /** 已验证的版本化配置（全局配置与列覆盖解析后的结果）。 */
  readonly config: VersionedPresentationConfig;
  /** 命中当前列的表列覆盖；未配置或未命中时缺省。 */
  readonly columnOverride?: Readonly<ColumnPresentationOverride>;
  /** 列上下文。 */
  readonly column: PresentationColumnContext;
  /** 环境快照（locale 与本地时区）。 */
  readonly environment: PresentationEnvironment;
}

/**
 * 字段展示贡献。id 采用「提供方.展示器」（如 'datazen.timestamp'）；
 * 扩展点能力名 `data.valuePresentation` 由宿主侧声明，不在此重复定义。
 */
export interface ValuePresentationContribution {
  /** 贡献唯一标识（如 'datazen.timestamp'）。 */
  id: string;
  /** 配置 schema 版本；未知版本不覆盖旧配置。 */
  configVersion: number;
  /** 在宿主边界验证 JSON 输入；不抛异常，失败返回路径化问题。 */
  validateConfig(input: unknown): ConfigValidationResult;
  /** 为一列构建展示计划（进程内，不持久化）。 */
  prepare(input: PresentationPrepareInput): PreparedPresentation;
}
