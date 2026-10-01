/**
 * 基础标量镜像：Id / Counter / Timestamp。
 *
 * 与 Rust 侧 `packages/platform-api/src/id.rs` 逐字对应：
 * - `Id` 是**不透明字符串**（`connectionId` / `dbSessionId` / `executionId` / `jobId` /
 *   `streamId` / `artifactId` / `requestId` / `delegationId` …）。Rust 侧是 newtype，
 *   跨进程序列化后退化为字符串，因此 TS 侧不再做品牌区分，只保留别名以便阅读。
 *   **不可混用的两条规则仍然有效**，只是检查发生在 Rust 侧：
 *   `connectionId`（持久化配置 id）与 `dbSessionId`（运行时会话 id，永不落盘）
 *   在本包中共用 `Id` 别名，任何传参顺序错误都必须在 Rust 边界被 `newtype` 拦住。
 * - `Counter` 序列化为**十进制字符串**（`Counter(4)` → `"4"`），不是 JSON number。
 *   前端比较版本时必须按数值比较，不要用字符串字典序。
 * - `Timestamp` 在 Rust 侧是不透明字符串 newtype，由宿主写入，本包不解释其格式。
 */

/** 所有不透明标识符的 wire 形态：JSON 字符串。 */
export type Id = string;

/**
 * 单调计数器。wire 形态是**十进制字符串**（Rust `Counter` 自定义 serde），
 * 用 `parseCounter` / `compareCounters` 读取，不要直接 `Number(...)` 参与展示。
 */
export type Counter = string;

/** 宿主写入的不透明时间戳字符串；本包不解析其格式。 */
export type Timestamp = string;

/** RFC 3339 / ISO 8601 形态的请求时间戳，由宿主适配器生成。 */
export type IsoTimestamp = string;

/** JSON 可序列化值（`publicOptions`、能力快照的 `confirmed` 等自由形状字段）。 */
export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };

/** 键排序的自由形状配置映射。 */
export type JsonObject = { readonly [key: string]: JsonValue };

const DECIMAL = /^(0|[1-9][0-9]*)$/;

/** `Counter` 的 wire 校验：十进制字符串，无前导零、无负号、无小数点。 */
export function isCounter(value: unknown): value is Counter {
  return typeof value === 'string' && DECIMAL.test(value);
}

/** 把 `Counter` 读成数值。非法 wire 值返回 `null`，不抛异常、不静默变成 `NaN`。 */
export function parseCounter(value: unknown): number | null {
  if (!isCounter(value)) return null;
  const parsed = Number.parseInt(value, 10);
  return Number.isSafeInteger(parsed) ? parsed : null;
}

/** 数值比较两个 `Counter`。非法值排在合法值之后，使「未定义」永远排在末尾。 */
export function compareCounters(left: unknown, right: unknown): number {
  const a = parseCounter(left);
  const b = parseCounter(right);
  if (a === null && b === null) return 0;
  if (a === null) return 1;
  if (b === null) return -1;
  return a - b;
}

/** `Counter` 自增，返回新的字符串值；非法输入按 `0` 起点计算。 */
export function incrementCounter(value: unknown, delta = 1): Counter {
  const current = parseCounter(value) ?? 0;
  const next = Math.max(0, current + Math.max(0, Math.trunc(delta)));
  return String(next);
}
