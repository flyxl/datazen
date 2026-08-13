# Delete Row + Safe Mode 对齐

> 分支：`feat/delete-row-safemode`  
> Worktree：`/Users/wuxiaolong/code/rust-projects/datazen-conn-batch-export`

## Safe Mode 现状（Q1）

| 操作 | 后端 `sql_guard` | 前端菜单 | 结论 |
|------|------------------|----------|------|
| Truncate（含 SQLite `DELETE FROM` 无 WHERE） | Safe Mode **拦截** | Safe Mode 时 **隐藏** Truncate | 已对齐 |
| Drop | Safe Mode **不拦截**（设置文案未包含 DROP） | 仅 `readOnly` 隐藏 | 与设定一致 |
| Delete Row（PK WHERE） | 专用 IPC 与 update 对齐（总有 WHERE） | 右键/选择栏 + confirmOnDelete | 已实现 |

## 功能清单

| ID | 功能 | 状态 | 测试 |
|----|------|------|------|
| F1 | Safe Mode 开启时 Schema 树隐藏 Truncate | done | 静态 PASS |
| F2 | `DatabaseDriver::build_delete_sql` + ReuseDriver | done | trait 默认实现 |
| F3 | `commit_row_deletes` IPC + Rust 单测 | done | cargo 7/7 data tests |
| F4 | `tableDataStore.deleteSelectedRows` / `deleteRows` | done | vitest PASS |
| F5 | DataTable 右键/选择栏 Delete + TableView confirmOnDelete | done | vitest PASS |
| F6 | i18n / 文档 / merge push | done | locales.test PASS |
