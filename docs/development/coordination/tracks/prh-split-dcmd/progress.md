# Track `prh-split-dcmd` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-split-dcmd** 章节。

## 状态

- Phase: PASSED
- 编码 commit: e882fec2f
- 测试 commit: 7865d5d15

## 设计决策

- 将 `driver_command.rs`（~1573 行）拆为子目录模块：`mod.rs`（IPC 编排入口）、`types.rs`、`helpers.rs`、`access.rs`、`resolve.rs`、`discovery.rs`、`execute.rs`、`streaming.rs`、`tests.rs`。
- `commands/mod.rs` 保持 `mod driver_command` + `pub use driver_command::*`，IPC 签名与 re-export 不变。
- `query.rs` 仅更新 import 路径指向子模块 re-export。

## 自验结果

| 套件 | 结果 | 备注 |
|------|------|------|
| cargo test -p datazen --lib（编码自验） | 1233 passed; 0 failed; 2 ignored | CARGO_TARGET_DIR=target/cargo-wt |

## 测试代理复验（2026-09-03，强化轮）

| 验收项 | 结果 |
|--------|------|
| driver_command 降为模块编排入口 | ✅ `mod.rs` 79 行，仅 IPC + 委派 |
| 子模块职责划分 | ✅ types / access / discovery / resolve / execute / streaming / helpers / tests |
| IPC 签名不变 | ✅ 4 个 `#[tauri::command]` 与基线 `e882fec2f~1` 逐行一致；bootstrap 注册未变 |
| commands/mod.rs 导出 | ✅ `mod driver_command` + `pub use driver_command::*` 未变 |
| 无新增 driver_type 硬编码 | ✅ driver_command 子树无 `== "postgres"` 等分支 |
| cargo test -p datazen --lib | ✅ **1255 passed; 0 failed; 3 ignored**（独立重跑） |
| driver_command 模块测试 | ✅ 40 passed（含 10 个 `test_tester_*` 强化用例） |

### 编码自报 vs 独立实测

| 指标 | 编码自报 | 独立实测 | 差异说明 |
|------|----------|----------|----------|
| lib 测试通过数 | 1233 | 1255 | 集成分支已合入其他轨增量测试 |
| lib 测试失败数 | 0 | 0 | — |
| ignored | 2 | 3 | 其他轨新增 ignored |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | — |

## 遗留

—
