# BUG-004：列覆盖键缺省字段与空字符串碰撞

- 状态：已修复，待 tester 回归验证
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

- 验证人：tester（独立回归，未修改任何被测代码）
- 日期：待验证
- 方法：待 tester 独立探针 + 门禁复跑后补记。
- 结论：**待 tester 回归验证。**本文件状态在验证通过前保持「已修复，待 tester 回归验证」，勿自称已关闭。
