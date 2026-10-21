# 存储层端口化 — 跨轨台账（协调者维护）

> 本文件是开发期台账，随分支流转；**验收合并后必须删除**，不得存活在 `main` 上。
> 设计结论与缺陷必须变成代码、测试与 `docs/` 正式文档里的事实。

## 目标

按「web / desktop 共享架构模型」把 DataZen 的存储层彻底端口化：

1. **存储层由 Repository trait 抽象**，runtime / feature-core 面向接口编程，不面对具体实现。
2. **具体实现由 web 与 desktop 宿主各自提供**。
3. **初始化时构造具体 Repository 并注入**（依赖注入）。
4. **web / desktop 各保留形态差异**（HTTP server / Tauri app / 本地 I/O）。

## 现状基线（协调者实测，2026-10-21）

| 判据 | 状态 |
|---|---|
| 端口契约层 `packages/platform-api/src/ports/` | 18 个 trait，其中 13 个有实现 |
| **无人实现的端口** | `IdentityResolver`、`EventSink`、`SubmissionTokenIssuer`、`ClusterNodeBudgetPort`、`ControlResourcePort` |
| feature-core 存储 trait | `DashboardStore`、`DashboardRunStore`、`WorkflowDefinitionStore`（**方法签名无 ctx / org**） |
| 迁移三件套的"存储" | **未抽象**：`data-sync/plan_store`（具体 struct）、`data-transfer/plan_store`（具体 struct）、`data-sync/comparison_store`（**直接绑 `std::fs`**）、`schema-diff/reviewed.rs`（**进程级 `static PLANS`**） |
| DI 风格 | **两套并存**：`WorkflowRegistry<S>` 泛型 / `DashboardStore` 为 `&dyn` |
| 共享内核的形态命名泄漏 | `runtime/src/job/desktop_host.rs`、`runtime/src/tunnel/desktop.rs` |
| 多租户词汇 | `RequestContext.organization_id`、`ProfileScope::from_context`、`SessionOwner.organization_id`、`PolicyService.isolation_key` 已就位；**`JobDefinition`/`JobFilter`/`OwnerRef` 无 org** |
| 转发门面残留 | `src-tauri/src/commands/sync/mod.rs:6` 的 `pub(crate) use datazen_data_sync::comparison_store;`、`src-tauri/src/wapps/mod.rs` 的 kernel re-export 块 |

## 波次与轨道（按文件冲突面划分）

### Wave 1（并行，冲突面互斥）

| 轨 | 分支 | worktree | 范围 | 状态 |
|---|---|---|---|---|
| A | `feature/job-org-scope` | `.worktrees/datazen-job-org-scope` | platform-api 的 `JobDefinition`/`JobFilter` 加 `organization_id`；runtime 与 SQLite 仓储按 org 过滤 | ⏳ 进行中 |
| B | `feature/di-dyn` | `.worktrees/datazen-di-dyn` | `WorkflowRegistry<S>` 泛型 → `Arc<dyn WorkflowDefinitionStore>` | ⏳ 进行中 |
| C | `feature/runtime-naming` | `.worktrees/datazen-runtime-naming` | `DesktopJobHost` / `DesktopNetworkProvider` / `DesktopRouteCatalog` 改名去形态味 | ⏳ 进行中 |

**冲突面裁决**
- A 与 C 都落在 `packages/runtime/src/job/` 下：A 只碰 `repository.rs` / `runtime_repository.rs` / `memory_runtime_repository.rs` / `owner_claims.rs` / `runtime_methods.rs`；C 只碰 `desktop_host.rs`（重命名）+ `mod.rs` 导出。**互斥文件已写进各自简报**。
- A 与 B 都不碰对方文件（A 碰 platform-api/runtime-job/host-jobs；B 碰 feature-core/workflow-registry + host workflow-registry + app_state）。A 的简报明确禁止改 `bootstrap/app_state.rs`。
- B 与 C 完全无交集。
- 共享 `CARGO_TARGET_DIR=.worktrees/.shared-target`（cargo 文件锁会串行化，属预期）。

### Wave 2（并行，依赖 Wave 1 合流）

| 轨 | 分支 | 范围 | 依赖 |
|---|---|---|---|
| D | `feature/featurecore-org-scope` | feature-core 三个存储 trait 加 org 作用域（`ProfileScope::from_context` 样板） | B（先 dyn 化再改签名，避免同一文件两轮冲突） |
| E | `feature/schema-diff-reviewed-port` | `schema-diff/reviewed.rs` 的进程全局 → 端口 + 宿主实例 | A（reviewed plan 要带 org） |
| F | `feature/migration-store-ports` | `data-sync/plan_store` + `data-transfer/plan_store` + `data-sync/comparison_store` → 端口；顺手清 `commands/sync/mod.rs:6` 的转发门面 | A |
| G | `feature/identity-resolver` | 实现 `IdentityResolver`（桌面版） | A（要用 org 词汇） |

### Wave 3

R 阶段全量回归 → 合流 `main` → PR。

## 门禁基线（Wave 1 开工前，2026-10-21，`main` @ 8bf6eb553）

- `cargo fmt --all -- --check` → EXIT=0
- `cargo clippy -p datazen --all-targets` → EXIT=0，0 errors
- `cargo check --workspace --all-targets` → EXIT=0
- `cargo test -p datazen --lib` → **1265 passed / 0 failed / 9 ignored**
- `cargo test -p datazen-feature-core --lib` → 367 passed
- `cargo test -p datazen-runtime --lib` → （以各轨实测为准）
- `cargo test -p datazen-platform-api --lib` → （以各轨实测为准）
- 前端 `tsc --noEmit` clean；`vitest run` → 608 files / 6341 tests passed

## 待裁定项

（暂无。若各轨提出，按 §7 归属裁决：默认不并入已关闭功能。）

## 合流记录

（每轨合入后追加：轨 → commit → 门禁数字 → 冲突处理。）
