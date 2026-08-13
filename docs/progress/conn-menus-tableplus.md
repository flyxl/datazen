# Connection Window 菜单对齐 TablePlus

> 分支：`feat/conn-menus-tableplus`  
> Worktree：`/Users/wuxiaolong/code/rust-projects/datazen-conn-batch-export`

## 功能清单

| ID | 功能 | 状态 | 测试 |
|----|------|------|------|
| F1 | Schema 树表/视图：Open Structure、Copy DDL、Truncate、Drop、New Query | done | PASS：schemaTreeContextMenu |
| F2 | SQL 编辑器：Run / Run Selected / Format / Comment | done | PASS：sqlEditorContextMenu |
| F3 | Panel Tab：Close to the Right（+ Close to the Left） | done | PASS：connectionTabContextMenu |
| F4 | DataTable：Set NULL、Copy as UPDATE、Copy as CSV；Query 结果禁虚接线 Set NULL | done | PASS：dataTableContextMenu + enableSetNull=false |
| F5 | Schema 库节点/空白：New Table、Import；DDL 视图右键 Copy | done | PASS：schema + DDLView 静态 |
| F6 | i18n + 文档 + merge/push | done | — |

## 变更日志

- 2026-08-13：实现 F1–F4 builder + SqlConnectionView / QueryPanel / DataTable 接线 + 单测 + i18n（en / zh-CN / zh-TW 正式，其它 locale 英文占位）。
- 2026-08-13：F4 修复 QueryPanel `enableSetNull={false}`；F5 库节点 New Table + DDLView 原生 Copy；独立测试全部 PASS。
