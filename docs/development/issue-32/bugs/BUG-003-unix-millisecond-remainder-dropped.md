# BUG-003：Unix 毫秒值经 iso / iso-tz 输出时毫秒余数被静默丢弃

- 状态：已关闭（2026-11-12 回归验证通过）
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

- 验证人：tester（独立回归，未修改任何被测代码；验证分支 `feature/issue-32-value-presentation`，HEAD `3b7fede54`；修复提交 `7a7091629` + 回归测试提交 `3b7fede54`）
- 日期：2026-11-12
- 方法：独立探针 `/tmp/probe-bug003`（17 项，R1–R6；`node_modules` 符号链接复用仓库依赖，`environment: node`，被测源码以绝对路径导入；不重跑仓库自带测试当作探针；用后已删除）+ 门禁复跑（G1–G4）+ 代码审读。探针经 T03 贡献 `present()` 走完整展示路径（number 列 + unix 解释），并以 `parseUnixValue` / `formatTimestampOutput` 直接调用做对照。
- 探针结果（17/17 通过，退出码 0）：
  - R1 原始复现 ✅：`present(1705316400123)` number / bigint / string 三路径一致——iso-tz `2024-01-15T19:00:00.123+08:00`、iso `2024-01-15T11:00:00.123Z`、locale 含 `07:00:00.123 PM`；直接调 `formatTimestampOutput` 对照一致。
  - R2 负值 ✅：`present(-1)` 三格式含 `.999`——iso `1969-12-31T23:59:59.999Z`、iso-tz `1970-01-01T07:59:59.999+08:00`、locale 含 `07:59:59.999`；`parseUnixValue(-1)` 返回 `fractionDigits: 3`。
  - R3 余数 0 不补零 ✅：`present(1705316400000)` iso-tz `2024-01-15T19:00:00+08:00`、iso `2024-01-15T11:00:00Z`、locale 无小数秒（不输出 `.000`）。
  - R4 秒单位 + `:421` 裁决复核 ✅：`present(1705316400, 秒解释)` 无小数秒；`parseUnixValue('+12', 'milliseconds')` 返回 `fractionDigits: 3`。裁决复核结论：修正正确——`+12` 的毫秒余数确为 12（`positiveMod(12, 1000) = 12`，即 0.012 秒），旧期望 0 编码了 bug 行为（端到端实测修复后 iso 输出 `1970-01-01T00:00:00.012Z`，修复前会丢 `.012`）；同 case 其余断言未动（`-0` 秒仍为 `fractionDigits: 0`）。
  - R5 边界 ✅：999ms 余数 → `.999`；1ms 余数 → `.001`（补零）；`-1000` 余数 0 → 无小数秒；`-1001` 正模余数 999 → `.999`（iso `1969-12-31T23:59:58.999Z`）。
  - R6 无回归 ✅：带时区文本 `2024-01-15T19:00:00.123456789+08:00` 的 9 位精度仍原样（iso-tz / iso 均保留 `.123456789`，T02 P6 行为不受影响）；locale 亚毫秒截断到毫秒不变（9 位与 6 位源均输出 `.123`，不含 `123456`，T02 O-1）。
- 门禁结果：
  - G1 ✅：`npx vitest run packages/frontend/src/lib/valuePresentation --retry=0`，202/202 通过（temporalMath 96、timestampFormat 34、timestampContribution 46、timestampConfig 26），退出码 0。
  - G2 ✅：`pnpm typecheck`（tsrs 0.18.0）与 `pnpm typecheck:tsc`（tsc 7.0.2）双引擎均 OK（root / scripts / pack-ep 三程序），退出码均为 0（经 `npx -y pnpm@12.18.0` 等价调用）。
  - G3 ✅：`node scripts/check-test-module-naming.mjs` ok（1837 Rust 文件、256 cfg(test) 模块；2557 TS 文件、8 wdio spec 树），退出码 0。
  - G4 代码审读 ✅：`7a7091629` 的 diff 最小——仅 `temporalMath.ts`，1 行逻辑（`fractionDigits = positiveMod(epochMs, 1000) === 0 ? 0 : 3`）+ 4 行注释，无其他行为变化；`resolveFractionDigits` 未被改动（`timestampFormat.ts` 在 `e05e50093..HEAD` 间 0 行 diff）；`+12` 断言修改处注释准确（写明余数 12、旧期望编码 bug、iso 丢 `.012`），同 case 其余断言未动（全文件仅 1 行既有断言被删改，即该 `+12` 行，其余均为新增）。
- 记录勘误：本文件「回归测试」段称「未改动任何既有断言」不准确——`3b7fede54` 按协调方裁决修正了 1 条既有断言（`temporalMath.test.ts:421` 的 `+12` 期望 0 → 3），属授权修正而非放宽门禁（见上 R4 复核），特此更正；该段其余内容属实。
- 结论：**BUG-003 验证通过，关闭。**修复（`parseUnixValue` 按毫秒余数声明 `fractionDigits`）根除了 iso/iso-tz 静默丢毫秒余数的问题，三格式一致；余数 0 不虚构小数秒；秒单位与 9 位文本精度路径无回归。
