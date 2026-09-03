# Track `prh-panic-policy` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-panic-policy** 章节。

## 状态

- Phase: PASSED
- 编码 commit: 6ef2aee16
- 测试 commit: b0819beef

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
| cargo test -p datazen --lib | pass | 1250 passed; 0 failed; 3 ignored (tester 独立复验) |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | — |

## Tester 强化测试

| 测试 | 模块 | 覆盖路径 |
|------|------|----------|
| `test_tester_connect_lock_poison_returns_internal` | connection_manager | 毒化 `connect_locks` → `ConnectionError::Internal` |
| `test_tester_lock_export_poison_returns_internal` | export | 非 callback `lock_export` 毒化 → `CommandError::Internal` |
| `test_tester_lock_export_stream_recovers_from_poison` | export | callback `lock_export_stream` 毒化 → `into_inner` 恢复 |
| `test_tester_lock_export_concurrent_access_succeeds` | export | 8 线程 × 100 次并发 lock 无错误 |
| `test_tester_canonicalize_sorts_by_identity_key` | data | `sort_by` 回归：按 identity key 排序不 panic |

## 残留 unwrap 审计（生产路径，启发式扫描）

约 **119** 处 `.unwrap()` / `.expect()` 仍在非测试 Rust 源码中。本轮已治理的 4 处关键路径（connect_locks、export mutex、sort_by、save dialog）均已通过新增测试覆盖或既有单测（`save_dialog_commands_rejected_without_interactive_handle`）验证。

**未处理高危点（建议后续轨跟进）**：

| 位置 | 数量 | 风险 |
|------|------|------|
| `extensions/mod.rs` | 9 | RwLock 毒化 → panic（extension registry） |
| `commands/dialog.rs` | 3 | Mutex 毒化 → panic（dialog injection queue） |
| `workflow/scheduler.rs` | 2 | OnceCell 缺失 → panic（启动后应不变量） |
| `bootstrap.rs` | 4 | 启动 fail-fast（可接受，需注释标注） |

**有意保留**：`export.rs` 中 `lock_export_stream` 的 `unwrap_or_else(|poisoned| poisoned.into_inner())` 符合 panic-policy §4（Fn callback 无法返回 Result）。

## 遗留

—
