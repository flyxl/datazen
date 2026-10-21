# SE-PROD-011 列拖放不可达

**状态**：已知未修复 · 潜伏（不影响现有功能，无崩溃、无错误结果）
**影响面**：仅 SQL Editor Pro · 拖放（`e2e/specs/` 与 Pro E2E 均无覆盖）
**结论**：这是一个**半实现**特性，不是「不存在」的特性。

## 现象

从结构对象树把一个**列**拖进 SQL 编辑器，期望插入列限定名（如 `users.id`）。
UI 上没有可拖拽的列节点，因此该操作无法发起；即使构造出列负载投放到编辑器，
插入的也是表引用而非列引用。

## 链路实测

逐段核对如下，每条都对应当前代码行。

### 1. 消费端（Pro）已实现

`packages/pro-extensions/sql-editor-pro/src/paste/dropCaret.ts`

| 行 | 事实 |
| --- | --- |
| `:24`、`:45` | 负载类型声明为 `kind: 'table' \| 'view' \| 'column'` |
| `:125` | `kind: json.kind ?? 'table'` —— 解析时保留 `column` |
| `:129` | `column: json.column` —— 列名被读出并存入 `ResolvedDropPayload` |

`packages/pro-extensions/sql-editor-pro/src/paste/__tests__/dropCaret.test.ts:40-50`
有用例 `validates column payload structure`，断言一个 `kind: 'column'` / `column: 'id'`
的 V1 负载能被正确解析。**消费端不仅实现了，还有单测。**

### 2. 生产端（Host）缺失

`src/windows/connection/schema-tree/schemaTreeDrag.ts:47`

```ts
kind: 'table' | 'view';
```

联合类型里没有 `column`，宿主无法构造列负载。

`src/windows/connection/navigator/NavigatorTreeRow.tsx` 全文只有 3 处 `draggable`
（`:272`、`:441`、`:625`），其中只有 2 处调用 `setDragPayload`：

- `:446` —— 表 / 视图
- `:639` —— 存储例程 / 函数 / 存储过程 / 触发器

**没有任何列节点是可拖拽的**，生产端根本无法发出列负载。

### 3. 插入端（Host）无列分支

`src/windows/connection/query/queryDropHandler.ts` 只遍历 `payload.tables`。
该文件出现 `column` 的 2 处（`:32`、`:121`）是关于「生成 SELECT 时的列解析」的注释，
与列拖放无关。

### 4. 静默降级点

`packages/pro-extensions/sql-editor-pro/src/proFeatures.ts:210-216`

```ts
onDrop: (resolved, pos) => {
  const legacyPayload: DroppedTablePayload = {
    tables: [{ tableName: resolved.table, schema: resolved.schema }],
    connectionId: resolved.connectionId,
    databaseType: resolved.databaseType,
  };
```

`ResolvedDropPayload` → `DroppedTablePayload` 的适配**只取 `resolved.table` 与
`resolved.schema`，`resolved.column` 被静默丢弃**。这是列信息消失的确切位置：
即使有列负载送达，插入结果也是表限定名而非列限定名，且不报错。

## 为什么长期没被发现

曾经存在覆盖该场景的 E2E 用例（SE-PROD-011），但它的断言是恒真的：

```ts
const hasColumnName = editorContent.includes('name') || editorContent.includes('"name"');
expect(typeof hasColumnName).toBe('boolean');
```

`typeof` 断言对任何值都成立，永远通过。同一批用例里还有另外 4 条同形断言
（另有错误的 MIME 类型 `application/json`、永远匹配不上的 `connectionId`）。
恒真断言让「负载类型联合里多了一个 `column`、但没人能发出它」这个半实现状态
完全隐形。该用例已删除（见下），相关用例改写到 Pro 包并改为实断言。

## 修复需要同时满足的四点

1. `DragPayloadOptions.kind` 增加 `'column'`，并给 `schemaTreeDrag` 补
   `column?: string`；
2. `NavigatorTreeRow` 的列节点接上 `draggable` + `setDragPayload`，
   按 AGENTS.md「数据属性解耦」用 `data-*` 标识，不用视口几何坐标；
3. `queryDropHandler` 增加列分支，决定插入 `table.column` 还是裸 `column`；
4. `proFeatures.ts` 的适配层透传 `resolved.column`，否则前三步都会被它抹掉。

## 关联测试现状

- **无任何 E2E 覆盖**：本条不可达，所以没有用例能失败，也就无从回归。
- 修复 1-4 之后，应补一条 Pro E2E：拖列 → 断言文档包含 `表.列` 限定名。
  断言必须针对具体文档文本，`expect(typeof …)` 一律不算。
