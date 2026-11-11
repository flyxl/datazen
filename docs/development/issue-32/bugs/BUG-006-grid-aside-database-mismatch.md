# BUG-006：database 缺省时 grid 传 ''、aside 传 undefined，同一覆盖一侧命中一侧不命中

- 状态：已关闭（2026-11-12 独立回归验证通过）
- 严重程度：轻微（覆盖命中不一致；仅 database 缺省时出现）
- 发现阶段：issue-32 T05 独立测试验证（计划层复现，由协调方复核代码确认）
- 归属判定：**T05 贡献模块引入**（T05 的两处 source 组装对同一缺省用了两种表示）

## 现象

两者皆缺时，grid 的 `database` 为 `''`，aside 的为 `undefined`：同一份 database 缺省的覆盖，aside 侧命中、grid 侧不命中（或反之，取决于覆盖以哪一侧的键写成），grid 与 aside 对同一列读出不同文本。

## 复现步骤

1. 面板 database 与会话当前 database 皆缺省（`panel.database` 与 `currentDatabase` 均为缺省）。
2. 写一份 database 缺省的列覆盖（key 中 `database` 为 `undefined`，即 aside 侧 `ContentViewDrawers.tsx:317` 组装出的 source）。
3. 修复前：aside 侧命中该覆盖，grid 侧不命中——grid 的 source 是 `TableView.tsx` 组装出的 `database: ''`，`columnOverrideKey` 与覆盖的 key 不等。

根因位置：

- grid 侧：`PanelContentRenderer.tsx:230` / `:319` 的 `?? ''` 使 `TableView` 的 `database` prop（类型即 `string`）为 `''`，`TableView.tsx:1271-1281`（修复前行号）把该值原样放进 `presentationSource`。
- aside 侧：`ContentViewDrawers.tsx:317-332` 用 `activePanel.database ?? currentDatabase ?? undefined`，缺省时为 `undefined`。

## 期望行为

同一缺省在两处组装出同一 source，命中同一覆盖；真实 database 值不受影响。

## 根因分析

`columnOverrideKey` 区分 `undefined` 与 `''`（BUG-004 的防碰撞语义正在于此：缺省绝不伪造成空串）。两处组装对同一缺省用了两种表示，key 随之分叉。`TableView` 的 `database` prop 类型是 `string`，缺省只能是 `''`，错不在 prop 类型，在 source 组装处未做归一。

## 修复记录

只改 `TableView.tsx` 的 `presentationSource` 组装一处（提交 `78f71928c`，`TableView.tsx:1276-1285`）：

```ts
presentationSource={
  connectionId
    ? {
        connectionId,
        database: database === '' ? undefined : database,
        namespace: schema ?? undefined,
        table: tableName,
      }
    : undefined
}
```

- `connectionId` 缺省时 source 仍为 `undefined`（T05 已有行为不动）。
- 只改 source 组装，不改 `TableView` 的 `database` prop 类型与其他用途。
- 未反向把 aside 改为 `?? ''`——那会重引 BUG-004 碰撞（缺省伪造成空串）。

回归测试（与 code 提交分开提交）：

- `packages/frontend/src/components/DataTable/__tests__/DetailPanel.displayState.test.tsx` 新增 `grid/aside source parity (issue-32 BUG-006)` 小节（2 项）：以复现 `TableView` 组装逻辑的 `buildGridSource`（`database === '' ? undefined : database`）验证 `database ''` 的 key 与 aside 侧 `undefined` 的 key 相等，且经真实 plan 路径命中同一覆盖（aside 侧 key 的快照 + grid 组装的 source → 呈示文本出现）；另验真实 database 值（`'shop'`）命中不受影响，且 `''` 不命中 `'shop'` 的覆盖。
- 未改动任何既有断言。

## 回归验证

- 验证人：tester（独立回归验证）
- 日期：2026-11-12
- 验证基线：分支 `feature/issue-32-value-presentation`，HEAD `dacfccf48`；修复提交 `78f71928c`（fix）+ 回归测试提交 `dacfccf48`（test）。
- 方法：独立探针（一次性项目 `/tmp/probe-bug56`，复用仓库测试脚手架、被测源码以绝对路径导入，不污染仓库工作区；用后删除）+ 仓库门禁复跑 + 代码审读。
- 探针结果：
  - R6(a) 计划层（真实 `buildColumnPresentationPlans` 路径）：修复后 grid source（`database ''` 归一为 `undefined`）与 aside source（`database undefined`）的覆盖键相等，两侧均命中同一 database 缺省覆盖，`present(1705316400123)` 均为 `2024-01-15T19:00:00.123+08:00`；修复前口径（`database ''`）的键与缺省覆盖键不等、不命中（`unchanged`）；真实 database 值 `'shop'` 命中不受影响，且 `''` 不命中 `'shop'` 的覆盖。
  - R6(b) 真实 `TableView` 挂载：见下「TableView:1280 覆盖缺口」。
- TableView:1280 覆盖缺口（如实记录）：
  - fixer 的回归测试确如其所承认：BUG-006 回归测试用复现组装逻辑的 `buildGridSource`（`database === '' ? undefined : database`）构建 source，`TableView.tsx:1280` 的组装行本身未被仓库自带测试直接执行——仓库回归套件层面该行仍无直接覆盖。
  - 独立探针 R6(b) 补上了这一执行：以真实 `TableView` 挂载（`database=''`、`connectionId='conn-1'`、快照的 `epoch_ms` 覆盖以 database 缺省为键、时间贡献注册进全局 registry、真实 `tableDataStore` 种子、无 `DataTable` mock），grid 的 `epoch_ms` 单元格呈示为 `2024-01-15T19:00:00.123+08:00`、`created_at` 呈示为 `2026-01-15T22:30:00+08:00`——`TableView.tsx:1280` 行真实执行且 `''` 已归一为 `undefined` 并命中覆盖。
  - 判断：**可接受关闭。** 缺口不在行为层面而在仓库回归套件层面：行为已由独立真实挂载验证；建议后续（T06 或整理时）把 R6(b) 的真实 `TableView` 挂载吸收为仓库回归测试，使该行获得直接覆盖；不阻塞本次关闭与 T05 收尾。
- 门禁结果（与 BUG-005 同批复跑，全部真实结果）：G1 758/758（退出码 0）、G2 1197/1197（退出码 0）、G3 `pnpm typecheck` 与 `pnpm typecheck:tsc` 双引擎均 OK（退出码均为 0）、G4 命名门禁 ok（退出码 0）、G5 代码审读：`TableView.tsx` 的改动仅为该组装行一处（`database: database === '' ? undefined : database`）加注释，`database` prop 类型与其他用途未动；未反向把 aside 改为 `?? ''`（未重引 BUG-004 碰撞）。BUG-006 回归测试未改动任何既有断言。
- 结论：**验证通过，BUG-006 关闭。** 同一缺省在 grid 与 aside 两处组装出同一 source、命中同一覆盖；真实 database 值不受影响。
