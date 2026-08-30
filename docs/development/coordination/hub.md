# DataZen v0.1.x 并行开发总览

> 协调者维护。各轨只写自己的 `tracks/<track-id>/progress.md` 和 `bugs.md`。

## 当前波次

| 轨道 | 范围 | 状态 | 编码 commit | 测试 commit |
|---|---|---|---|---|
| `v01x-query-cancel` | 精确 query execution handle、Driver cancel protocol、QueryExecutionViewModel | 已完成 | 3a14ced5 | 6baa1f17 |
| `v01x-pending-changes` | staged row changes、Preview plan、Commit/Rollback | 已完成 | 583cfc13 | a848caec |

## 合并规则

- 编码代理和测试代理必须是不同的全新实例。
- 编码代理只在自己的 worktree 修改功能代码和本轨进度；测试代理只验证和写本轨 bugs/progress。
- 共享页面、locale、panelStore 接线由协调者在轨道闭环后处理。
- 轨道测试闭环后，协调者合并并运行 `tsc --noEmit`、定向 Vitest 和 Rust 单测。

## 当前风险

- PostgreSQL/MySQL 已改为精确 execution-handle 协议：PostgreSQL 使用目标 backend PID，MySQL 使用目标 thread ID，并通过独立控制连接取消，避免误取消同一会话中的其他查询。
- SQLite、MariaDB、部分兼容驱动和测试注入驱动仍可能不支持精确取消，能力未知或不支持时必须隐藏/禁用取消入口；事务连接也不宣称支持该协议。
- 真实 PostgreSQL/MySQL 取消和桌面 E2E 尚未在本轮执行，需具备 `TEST_MYSQL_*` / `TEST_PG_*` 夹具及桌面自动化环境后补测。
- pending changes 必须以主键或稳定 row identity 为前提；无主键表不能静默执行 UPDATE/DELETE。
