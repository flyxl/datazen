# Track `prh-split-dcmd` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-split-dcmd** 章节。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: e882fec2f
- 测试 commit: —

## 设计决策

- 将 `driver_command.rs`（~1573 行）拆为子目录模块：`mod.rs`（IPC 编排入口）、`types.rs`、`helpers.rs`、`access.rs`、`resolve.rs`、`discovery.rs`、`execute.rs`、`streaming.rs`、`tests.rs`。
- `commands/mod.rs` 保持 `mod driver_command` + `pub use driver_command::*`，IPC 签名与 re-export 不变。
- `query.rs` 仅更新 import 路径指向子模块 re-export。

## 自验结果

| 套件 | 结果 | 备注 |
|------|------|------|
| cargo test -p datazen --lib | 1233 passed; 0 failed; 2 ignored | CARGO_TARGET_DIR=target/cargo-wt |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | — |

## 遗留

—
