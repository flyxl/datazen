# Brief: sql-s6-d — 中央 Editor / Query 装配（S6-A/B/C 合流后串行）

> 依赖：S4-A/B/C/D、S5-A、S6-A/B/C 全部已合流。本轨是**唯一中央装配轨**。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S6-D（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s6-d/progress.md`。

## 独占写锁

可建/可改：`src/components/sql-editor/SqlEditor.tsx`、
`src/components/sql-editor/editorExtensions.ts`、
`src/components/sql-editor/contracts.ts`、`src/components/SqlEditor.tsx` 兼容入口、
`src/windows/connection/query/QueryEditorSection.tsx`、
`src/windows/connection/query/queryDropHandler.ts` 及组合 tests。
**禁止**：leaf 算法、locale、driver meta。

## 步骤

1. S4-A/B/C/D + S5-A 的 factory 按固定优先级装入 compartments：
   statement、completion/signature、intention/hint、hover/navigation、paste/drop/multiple selection。
2. 接入 metadata snapshot、execution state、active target change、navigation/DDL callbacks、INSERT hint setting。
3. `documentVersion` 用 editor StateField 维护；外部 value replacement、undo/redo、component remount 明确定义+测试。
4. table/column drop request 交 `queryDropHandler`：空编辑器保留生成 SELECT，非空插入引用，
   跨连接拒绝；旧 payload 继续兼容。
5. CodeMirror dialect mapping 接入方言族 profile：引入 `@codemirror/lang-sql` MSSQL 支持
  （旧 `CM_DIALECT_MAP` 仅 PG/MySQL/MariaDB/SQLite）；未知方言退回 Standard。
6. 按设置动态装卸 hint；反复 reconfigure 不重复 listener/timer/tooltip。
7. 旧 `sqlCompletions`、`sqlCompletionContext`、旧 SqlEditor import 保持兼容 wrapper；
   确认无第二份函数/scanner 数据源。

## 测试

全部 extension 组合、keymap 冲突、reconfigure/dispose、metadata refresh、setting toggle、
drop→QueryPanel、execution snapshot、MSSQL/Standard mapping。

## 完成定义

- 所有 leaf 功能在真实 SqlEditor 中可见；组合生命周期测试 + 对应功能 E2E 通过。
