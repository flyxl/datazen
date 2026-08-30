# DataZen v0.1.x 并行开发总览

> 协调者维护。各轨只写自己的 `tracks/<track-id>/progress.md` 和 `bugs.md`。

## 当前波次

| 轨道 | 范围 | 状态 | 编码 commit | 测试 commit |
|---|---|---|---|---|
| `v01x-query-cancel` | 精确 query execution handle、Driver cancel protocol、QueryExecutionViewModel | 协议重构中 | ecfa2bcf | 5d23c50d |
| `v01x-pending-changes` | staged row changes、Preview plan、Commit/Rollback | 修复中 | 2a6ff456 | 32bbdd7e |

## 合并规则

- 编码代理和测试代理必须是不同的全新实例。
- 编码代理只在自己的 worktree 修改功能代码和本轨进度；测试代理只验证和写本轨 bugs/progress。
- 共享页面、locale、panelStore 接线由协调者在轨道闭环后处理。
- 轨道测试闭环后，协调者合并并运行 `tsc --noEmit`、定向 Vitest 和 Rust 单测。

## 当前风险

- 现有 PostgreSQL/MySQL `cancel_query` 实现会取消连接范围内的活动查询，后续需要单独评估是否满足“当前查询”语义。
- SQLite、部分驱动和测试注入驱动的取消能力可能是 no-op 或未知，不能统一展示为可取消。
- pending changes 必须以主键或稳定 row identity 为前提；无主键表不能静默执行 UPDATE/DELETE。
