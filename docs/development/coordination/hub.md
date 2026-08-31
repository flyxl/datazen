# DataZen v0.1.x 并行开发总览

> 协调者维护。各轨只写自己的 `tracks/<track-id>/progress.md` 和 `bugs.md`。

## 当前波次

| 轨道 | 范围 | 状态 | 编码 commit | 测试 commit |
|---|---|---|---|---|
| `v01x-query-cancel` | 精确 query execution handle、Driver cancel protocol、QueryExecutionViewModel | 已完成 | 3a14ced5 | 6baa1f17 |
| `v01x-pending-changes` | staged row changes、Preview plan、Commit/Rollback | 已完成 | 583cfc13 | a848caec |
| `v01x-query-cancel-plus` | 事务连接取消、MariaDB 取消、兼容驱动继承父驱动取消能力 | 测试中 | 79838de7 | 待测试 |
| `v01x-connection-discovery` | 连接搜索排序、Pinned/Recent 优先级和连接表单分层 | 测试中 | 25630125 | 待测试 |
| `v01x-filter-pagination` | 快速过滤表达式、分页重置、请求竞态和菜单分层组件 | 测试中·修复轮 | d8e9c59b | db18a4b4 |
| `v01x-object-actions` | 对象搜索、表定位和生成 SQL action | 已完成 | df1c0ad9 | 61acc880 |
| `v01x-result-workspace` | Table/Chart 统一结果承载组件 | 已完成 | ffae5e54 | cfa056fc |
| `v01x-ai-actions` | Explain/Fix SQL/Retry 快捷动作上下文 | 测试中·修复轮 | 63d8e6df | 37b1bcb5 |
| `v01x-page-integration` | 共享页面、DataTable、QueryPanel 和 i18n 最终接线 | 待开始 | 待编码 | 待测试 |

## 合并规则

- 编码代理和测试代理必须是不同的全新实例。
- 编码代理只在自己的 worktree 修改功能代码和本轨进度；测试代理只验证和写本轨 bugs/progress。
- 共享页面、locale、panelStore 接线由协调者在轨道闭环后处理。
- 轨道测试闭环后，协调者合并并运行 `tsc --noEmit`、定向 Vitest 和 Rust 单测。

## 当前风险

- PostgreSQL/MySQL 已改为精确 execution-handle 协议：普通连接和事务连接都必须使用目标 backend PID/thread ID，并通过独立控制连接取消，避免误取消同一会话中的其他查询。
- MariaDB 与 MySQL 使用同一精确取消实现；兼容驱动必须继承父驱动的精确取消能力，但只有实际委托同一目标绑定和控制逻辑时才可声明支持。
- SQLite 仍需独立的 `sqlite3_interrupt`/连接句柄协议；在该协议完成前不能把 SQLite 宣称为精确可取消。
- 事务取消后的数据库状态必须明确反馈：PostgreSQL 事务可能进入 aborted 状态，需要回滚；MySQL 需验证语句取消后的事务和锁语义。
- 真实 PostgreSQL/MySQL 取消和桌面 E2E 尚未在本轮执行，需具备 `TEST_MYSQL_*` / `TEST_PG_*` 夹具及桌面自动化环境后补测。
- pending changes 必须以主键或稳定 row identity 为前提；无主键表不能静默执行 UPDATE/DELETE。
