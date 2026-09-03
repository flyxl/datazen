# Track `prh-panic-policy` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-panic-policy** 章节。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: 6ef2aee16
- 测试 commit: —

## 设计决策

- 约定文档：`docs/development/panic-policy.md`；`AGENTS.md` 代码风格节、`CONTRIBUTING.md` PR checklist 交叉引用。
- 生产路径替换/加固：
  - `connection_manager.rs`：`connect_locks` 毒化锁 → `ConnectionError::Internal`
  - `commands/export.rs`：导出流 `Mutex` 锁 → `lock_export` / `lock_export_stream`（callback 路径用 `into_inner` + 日志，见 panic-policy）
  - `commands/data.rs`：`sort_by_key` 裸 `expect` → 已验证 key 的 `sort_by`
  - `commands/driver_command/execute.rs`：save dialog 裸 `expect` → `ok_or_else` + `CommandError::Internal`
- `store/**` 生产代码已采用 `with_conn` + `map_err` 模式，本轮无行为变更。

## 自验结果

| 套件 | 结果 | 备注 |
|------|------|------|
| cargo test -p datazen --lib | pass | 1243 passed; 2 ignored |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | — |

## 遗留

—
