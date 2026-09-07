# Brief: sql-s3-c — Schema tree 拖拽壳层

> 依赖：无（只动 tree 侧，可与 S3-A、S3-B1 同 wave）。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S3-C（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s3-c/progress.md`。

## 独占写锁

可建/可改：`UnifiedSchemaTree.tsx` 及 schema-tree 子模块、拖拽 payload contract。
**禁止**：import editor semantic 模块；不在 tree 侧决定 SQL 文本和 quote。

## 步骤

1. 先拆分 `UnifiedSchemaTree.tsx`（808 行）：保持树展开、搜索、上下文菜单、选择等价。
2. 首版聚焦 Table/View 节点：定义版本化通用 MIME（`SchemaObjectDragPayloadV1`）+
   Table payload，继续发送旧 `application/datazen-table` 兼容数据。
3. 列拖拽限于已有列源（结构视图）或未来树列节点扩展；**不**向树注入可能导致元数据查询风暴的展开列节点。
4. payload 始终携带 connectionId + database/schema/table；有 session 时携带 dbSessionId。

## 测试

table/view payload、namespace identity、legacy payload、tree 原行为回归。

## 完成定义

- 树只表达 schema object，不承担 SQL 生成。
- 相关验收：AC-13 部分（drag payload；drop caret 与 smart quote 由 S5-A/S6-D 做）。
