# BUG-003：Unix 毫秒值经 iso / iso-tz 输出时毫秒余数被静默丢弃

- 状态：已修复，待 tester 回归验证（勿自称已关闭）
- 严重程度：主要（数据展示失真，违反设计 §8.6「不静默截断」）
- 发现阶段：issue-32 T03 独立测试验证（见 `docs/development/issue-32/testing/T03-timestamp-contribution.md` 的 S3，由协调方复核代码确认）
- 归属判定：**库外 bug，先于 T03 存在，非 T03 贡献模块引入**（T03 模块经 tester 确认 0 缺陷；根因在 T02 的 `temporalMath.ts`，属 T01 先例允许的顺带修复，须在提交信息与报告中申报）

## 现象

Unix 毫秒值经 `iso` / `iso-tz` 输出时毫秒余数被静默丢弃，同一输入的 `locale` 输出却保留余数，三格式不一致：

```ts
present(1705316400123) // iso / iso-tz → `2024-01-15T19:00:00+08:00`，应为 `…19:00:00.123+08:00`
present(-1)            // iso / iso-tz 丢 `.999`
present(1705316400123) // locale 保留 `.123`
```

## 复现步骤

```ts
import { parseUnixValue, formatTimestampOutput } from '../index';

const instant = parseUnixValue(1705316400123, 'milliseconds').instant;
// iso
formatTimestampOutput(instant, { format: 'iso', displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' }, locale: 'en-US' });
// → 修复前 `2024-01-15T11:00:00Z`；修复后 `2024-01-15T11:00:00.123Z`
// iso-tz
formatTimestampOutput(instant, { format: 'iso-tz', displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' }, locale: 'en-US' });
// → 修复前 `2024-01-15T19:00:00+08:00`；修复后 `2024-01-15T19:00:00.123+08:00`
// locale（修复前后一致）
formatTimestampOutput(instant, { format: 'locale', displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' }, locale: 'en-US' });
// → 含 `07:00:00.123 PM`

const neg = parseUnixValue(-1, 'milliseconds').instant;
// iso → 修复前 `1969-12-31T23:59:59Z`；修复后 `1969-12-31T23:59:59.999Z`
// iso-tz → 修复前 `1970-01-01T07:59:59+08:00`；修复后 `1970-01-01T07:59:59.999+08:00`
```

## 期望行为

毫秒余数非 0 时三格式一致保留 `.123`（负值 `-1` 保留 `.999`）；余数 0 时不虚构小数秒。

## 根因分析

`packages/frontend/src/lib/valuePresentation/temporalMath.ts:503`，`parseUnixValue` 成功时恒返回 `{ epochMs, subSecondNanos: 0, fractionDigits: 0 }`。毫秒位确实在 `epochMs` 里，而 `packages/frontend/src/lib/valuePresentation/timestampFormat.ts:158` 在组装 iso/iso-tz 小数秒时 `nanos = date.getUTCMilliseconds() * 1_000_000 + subSecondNanos`，只被 `fractionDigits = 0` 截断（`resolveFractionDigits` 显式 0 优先，`timestampFormat.ts:114-121`）。`locale` 走 Intl 直接用 `epochMs`，故不受影响。

## 修复记录

只改 `parseUnixValue`，不改 `resolveFractionDigits` 的显式 0 优先语义（`InstantParts` 文档已写明手工构造时位数小于精度即按位数截断）：

- `packages/frontend/src/lib/valuePresentation/temporalMath.ts:503` 附近：毫秒余数非 0 时返回 `fractionDigits: 3`，否则 0。余数用文件已有的 `positiveMod(epochMs, 1000)` 计算（`temporalMath.ts:184-186`），负 `epochMs` 如 `-1` → 余数 999 → `fractionDigits` 3 → 输出 `.999`。秒单位的 Unix 值 `epochMs` 恒为 1000 倍数，余数 0，`fractionDigits` 保持 0。

同类问题审计（`InstantParts` 全部生产者）：

- `parseZonedTimestamp`（`temporalMath.ts:461`）：`fractionDigits` 取源小数秒实际位数（无小数秒为 0），与 `epochMs`/`subSecondNanos` 的「前 3 位进毫秒、后 6 位作余数」拆分一致，无同类问题。
- `wallClockToInstant`（`temporalMath.ts:721-725`）：`fractionDigits` 原样透传输入（缺省时保持缺省，由格式层按最小精确位数推导，不虚构 0），无同类问题。
- `timestampContribution.ts:178-185`：仅转发上游 `converted.instant`，不构造 `InstantParts`，无同类问题。
- 全库 `grep fractionDigits: 0` 仅 `temporalMath.ts:503` 一处生产代码命中（其余为测试断言），已修复。

回归测试（与 code 提交分开提交）：

- `packages/frontend/src/lib/valuePresentation/__tests__/temporalMath.test.ts` 新增 `parseUnixValue: millisecond remainder (BUG-003)` 小节（5 项）：`1705316400123` → `fractionDigits` 3；`-1` → 3；余数 0（`1705316400000`）→ 0；秒单位（`1705316400`）→ 0；number 与 string 路径同值。
- `packages/frontend/src/lib/valuePresentation/__tests__/timestampFormat.test.ts` 新增 `formatTimestampOutput: unix millisecond remainder (BUG-003)` 小节（2 项）：以 `parseUnixValue` 得到的 instant 调 `formatTimestampOutput`，三格式一致断言（iso 含 `.123`、iso-tz 为 `2024-01-15T19:00:00.123+08:00`、locale 含 `.123`；`-1` 的 iso/iso-tz 含 `.999`）。
- 未改动任何既有断言。

## 回归验证

- 验证人：tester（独立回归，未修改任何被测代码）——待验证
- 方法：待 tester 填写
- 结论：待 tester 填写
