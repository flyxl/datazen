# BUG-004：列覆盖键缺省字段与空字符串碰撞

- 状态：已关闭（2026-11-12 回归验证通过，详见「回归验证」段）
- 严重程度：轻微（现实影响可忽略：空串标识符不出现；但 T06 持久化后该键成为存储身份，存在串用风险）
- 发现阶段：issue-32 T04 独立测试验证（见 `docs/development/issue-32/testing/T04-host-pipeline.md` 的 P9，由协调方复核代码确认）
- 归属判定：**T04 引入的 bug**（缺陷在 T04 提交引入；T01–T03 的键/注册逻辑不受影响），属 T01/BUG-003 先例允许的顺带修复，须在提交信息与报告中申报

## 现象

`columnOverrideKey` 对「缺省字段」与「空字符串字段」返回同一键，违反设计 §6「缺省字段与空字符串不能无意碰撞」：

```ts
columnOverrideKey({ connectionId: 'c', table: 't', column: 'x' })
// → 修复前 '["c","","","t","x"]'
columnOverrideKey({ connectionId: 'c', database: '', namespace: '', table: 't', column: 'x' })
// → 修复前 '["c","","","t","x"]'（与上一行同一键）
```

## 复现步骤

```ts
import { columnOverrideKey, computePresentationPlanCacheKey } from '../presentationPlan';

const missing = columnOverrideKey({ connectionId: 'c', table: 't', column: 'x' });
const empty = columnOverrideKey({ connectionId: 'c', database: '', namespace: '', table: 't', column: 'x' });
// 修复前 missing === empty（均为 '["c","","","t","x"]'）；
// 修复后 missing === '["c",null,null,"t","x"]'，empty === '["c","","","t","x"]'，二者不同。

// 缓存键来源段同样碰撞（修复前同键，修复后不同）：
const cacheBase = {
  columns: [{ name: 'x' }],
  configRevision: 1,
  contributionRevision: 1,
  locale: 'en-US',
  localTimeZone: 'UTC',
} as const;
computePresentationPlanCacheKey({ ...cacheBase, source: { connectionId: 'c', table: 't' } });
computePresentationPlanCacheKey({ ...cacheBase, source: { connectionId: 'c', database: '', table: 't' } });
```

## 期望行为

缺省字段与空字符串字段得不同键：缺省用 `null`、空串保留为空串；大小写仍不归一（该行为不变）。

## 根因分析

`packages/frontend/src/lib/valuePresentation/presentationPlan.ts`：

- `columnOverrideKey`（修复前约 :122-132）：`database ?? ''` / `namespace ?? ''` 把「字段缺省」与「字段为空串」映射为同一空串分量。
- `computePresentationPlanCacheKey` 的来源段（修复前约 :447-448）：同样的两处 `?? ''` 归一，与覆盖键一致地碰撞（两者无错位，但同错）。
- 文件头注释（修复前约 :118-121）称「键形状固定，不碰撞」与事实不符——键字符串本身就是碰撞的证据。
- 为什么 coder 自带测试没抓住：该归一行为被自带测试显式锁定为期望（`presentationPlan.test.ts:248`，`it('缺省字段与空字符串归一为同一键')`），即有意设计而非疏漏；但该有意设计与设计文档 §6 的句子直接冲突，测试锁定的是一条与设计不符的行为。

## 修复记录

只改两处 `?? ''` → `?? null`，不改其他归一语义：

- `packages/frontend/src/lib/valuePresentation/presentationPlan.ts:126-127`（`columnOverrideKey`）：`database ?? ''` → `database ?? null`，`namespace ?? ''` → `namespace ?? null`。键形如 `["c",null,null,"t","x"]` vs `["c","","","t","x"]`，不再碰撞。
- `packages/frontend/src/lib/valuePresentation/presentationPlan.ts:448-449`（`computePresentationPlanCacheKey` 的来源段）：同样的两处 `?? ''` → `?? null`，与 `columnOverrideKey` 一致，无错位。
- 同步修正 `columnOverrideKey` 的文件头注释（`:117-122`）：写明缺省用 `null`、空串保留为空串、二者键不同，仍不做大小写归一。
- 刻意不动：`computePresentationPlanCacheKey` 的 columns 段 `dataType ?? ''` 不动——`dataType` 不是身份字段，不在设计 §6 该条规则范围内。
- 修复提交（code）：`7f083faa2`，`fix(frontend): distinguish missing vs empty override key fields (issue-32 BUG-004)`，仅 `presentationPlan.ts` 一个文件。

同类问题审计：全库 `?? ''` 归一中，仅上述两处涉及身份字段（`database` / `namespace`）；`resolvePresentationEnvironment` 的空串回退（locale / timeZone）是环境默认值而非身份键，不在设计 §6 该条规则范围内，无同类问题。

回归测试（与 code 提交分开提交）：

- `packages/frontend/src/lib/valuePresentation/__tests__/presentationPlan.test.ts:248`：按协调方裁决修正 1 条既有断言（与 BUG-003 的 `:421` 裁决同理：修正错误期望，不是放宽门禁）——用例改名 `it('缺省字段与空字符串 → 键不同（BUG-004）')`，期望由 `toBe` 改为 `not.toBe`；并在同一用例内补一处缓存键对照断言（source 缺省 `database` 与空串 `database` 的缓存键不同）。其余既有断言一字不动。
- 回归测试提交：与 code 提交分开的 test 提交（`test(frontend): override key missing-vs-empty regression, correct :248 assertion (issue-32 BUG-004)`，见 git log），仅测试文件 + 本文件。

## 回归验证

- 验证人：tester（独立回归，未修改任何被测代码；验证分支 `feature/issue-32-value-presentation`，HEAD `1efa2cd41`；修复提交 `7f083faa2` + 回归测试提交 `1efa2cd41`）
- 日期：2026-11-12
- 方法：独立探针 `/tmp/probe-bug004`（11 项，R1–R6；`node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码以绝对路径导入；不重跑仓库自带测试当作探针；用后已删除）+ 门禁复跑（G1–G4）+ 代码审读。探针以 `buildColumnPresentationPlans` + `raw` 覆盖走完整覆盖查找路径，并以 `columnOverrideKey` / `computePresentationPlanCacheKey` 直接调用做对照。
- 探针结果（11/11 通过，退出码 0）：
  - R1 原始复现 ✅：缺省键 `["c",null,null,"t","x"]`，空串键 `["c","","","t","x"]`，二者不同。
  - R2 单字段对照 ✅：仅 database 缺省 `["c",null,"n","t","x"]` vs 空串 `["c","","n","t","x"]`；仅 namespace 缺省 `["c","d",null,"t","x"]` vs 空串 `["c","d","","t","x"]`；两者齐备且相同 → 同键 `["c","d","n","t","x"]`。
  - R3 缓存键对照 ✅：source 缺省 database 的缓存键 `[["c",null,null,"t"],[["x","","other"]],1,1,"en-US","UTC",null]` 与空串 database 的 `[["c","",null,"t"],[["x","","other"]],1,1,"en-US","UTC",null]` 不同；同输入复算键稳定；namespace 缺省 vs 空串的缓存键亦不同。
  - R4 覆盖查找端到端 ✅：只存缺省键时空串 database 的列查不到该覆盖（命中侧 `raw`、未命中侧走默认路由 `ready`，不串用）；只存空串键时缺省列同样查不到（命中侧 `raw`、未命中侧 `ready`）。
  - R5 无回归 ✅：大小写不同仍键不同（database `DB` vs `db`、表名 `T` vs `t`）；完全相同输入键相同；PRD 锚点经 `plan.present` 仍通过——`2026-01-15T14:30:00Z` → `2026-01-15T22:30:00+08:00`（含 22:30 与 +08:00）；Unix `1705316400123` → `2024-01-15T19:00:00.123+08:00` 含 `.123`，`1705316400000` → `2024-01-15T19:00:00+08:00` 不含 `.000`（BUG-003 不回归）。
  - R6 `:248` 裁决复核 ✅：旧期望（`it('缺省字段与空字符串归一为同一键')` + `toBe`）在修复前代码上成立、修复后必失败，编码的正是与设计 §6 冲突的 bug 行为；新期望（键不同 + 缓存键对照）正确，与设计 §6「缺省字段与空字符串不能无意碰撞」一致；`git diff` 审读确认测试文件除 `:248` 用例外其余既有断言一字未动（仅该用例改名、`toBe` → `not.toBe` 并补一条缓存键对照断言）。
- 门禁结果：
  - G1 ✅：`npx vitest run packages/frontend/src/lib/valuePresentation packages/frontend/src/hooks/__tests__/useValuePresentation.test.tsx --retry=0`，**283/283** 通过（presentationPlan 75、useValuePresentation 6、timestampContribution 46、timestampConfig 26、temporalMath 96、timestampFormat 34），退出码 0。
  - G2 ✅：`pnpm typecheck`（tsrs 0.18.0）与 `pnpm typecheck:tsc`（tsc 7.0.2）双引擎均 OK（root / scripts / pack-ep 三程序），退出码均为 0（经 `npx -y pnpm@12.18.0` 等价调用）。
  - G3 ✅：`node scripts/check-test-module-naming.mjs` ok（1837 Rust 文件、256 cfg(test) 模块；2561 TS 文件、8 wdio spec 树），退出码 0。
  - G4 代码审读 ✅：`7f083faa2` 的 diff 最小——仅 `presentationPlan.ts` 一个文件，7 行增 / 6 行删：`columnOverrideKey` 与缓存键来源段各两处 `?? ''` → `?? null` + 注释修正；columns 段 `dataType ?? ''` 未被动（fix diff 中 0 行 `dataType` 变化）；注释与事实一致（缺省用 `null`、空串保留为空串、二者键不同，仍不做大小写归一）。
- 结论：**BUG-004 验证通过，关闭。**缺省与空串字段在覆盖键与缓存键来源段均得不同键，覆盖查找端到端不再串用；大小写语义、PRD 锚点与 BUG-003 均无回归。
