# BUG-006：database 缺省时 grid 传 ''、aside 传 undefined，同一覆盖一侧命中一侧不命中

- 状态：已修复，待 tester 回归验证
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

- 验证人：tester（独立回归，待验证）
- 方法：待 tester 填写
- 结论：待 tester 填写
