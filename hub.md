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

## 本 initiative 的度量口径（防止做反）

**本轮目标是解耦，不是压缩 src-tauri。** 实测已证明：Wave 1 三轨对宿主净变化 ≈ +27 行（A: 0；B: +30；C: −3）。端口化让宿主**新增** adapter 代码、让共享包**去掉**隐藏的具体存储决定 —— 宿主行数不是成功指标。

预期落点：A–G 做完 src-tauri 生产代码 **≈73.8k–74.3k**（比现在略高）；若再做后续「搬纯规则」才会降到 ~72.5k，且那是**搬家不是删除**，全仓总量不变。

**成功的判据换成这四条**：
1. 共享包内**类型名与模块路径**无 `Desktop` 形态味（线协议字面码、冻结契约成员、宿主侧命名三类**不在**本条范围，见 C 的裁决）。
2. `platform-api` 的 18 个端口中，web 必需的那几个（`IdentityResolver` 为最关键）有可运行实现。
3. **共享包内的进程级 `static` 清零**：`schema-diff/reviewed.rs:49`、`data-sync/plan_store/mod.rs:674-675`、`data-transfer/plan_store/mod.rs:440-441` —— 这三处是多租户串味的真实风险源。
4. 不改动任何共享包代码，web 宿主仅实现 trait 即可跑起来。

### ⚠️ F 轨的强制约束（否则会逆向增长宿主）

`packages/data-sync/src/comparison_store/` 共 2,211 行（`disk.rs` 494 + `mod.rs` 729 + `recovery.rs` 462 + 测试 648 + `manifest_cache.rs` 97），其中 `disk.rs`/`mod.rs` 合计 32 处 `fs::`/`Path` 调用。

**裁决：文件系统实现留在共享包，端口只收"根目录/路径"注入。**
- 反例（禁止）：把 `disk.rs` 整块搬进 `src-tauri/src/` —— 那会让宿主 +500 行，等于把解耦做成搬家，且 web 还得再抄一遍。
- 正解：包内保留 `disk.rs`（形态中立：任何有本地文件系统的宿主都能用），把「存储根从哪来」变成构造参数（现在 `mod.rs:48-50` 已收 `root: PathBuf`，说明只差把构造点从宿主传进去）。
- 同理 `SyncPlanStore`/`TransferPlanStore` 的 `static STORE: OnceLock<...>`：改成**宿主注入的实例**（`Arc<dyn ...>` 或包内 `#[derive(Default)]` 的实例由宿主持有），而不是把 HashMap 实现挪进宿主。
- 判定标准：改完后 `packages/data-{sync,transfer}` 的总行数**基本不变**，`src-tauri` 只增加注入点的少量接线。

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

## 合流顺序与冲突面修正（C 交付后）

**我原先的冲突面判断有一处错**：以为 C 只碰 `desktop_host.rs` + `mod.rs`。实际 C 改的是**跨 crate 的公开类型名**，引用点扩散到 `commands/sync/jobs.rs`、`store/app_db_jobs_tests.rs`、`commands/restore_job/tests.rs`、`commands/data_transfer/job_api/runtime.rs`、`packages/feature-core/src/restore_job.rs`、`bootstrap/app_state.rs` —— 与 A、B 均重叠。C 的处理正确：只做逐字标识符替换、零逻辑改动，并在报告里显式申报了这个冲突。

**裁决：C 先合。** 机械改名先落地，后合的轨在「已改名的标识符」上解冲突，远比反向（把改名 re-apply 到已改动内容上）省力且可验证。

合流顺序：`C → A → B`（若 B 先于 A 完成则 `C → B → A`，原则不变：改名类先合）。

### C 的 4 个待裁定项（协调者裁决，不阻塞验收）

| # | 项 | 裁决 |
|---|---|---|
| 1 | `route.rs` 里 4 个 `desktop*` reason code + `LeaseId` 前缀 `desktop-tunnel/` | **保留，不在本计划处理**。这些是已对外可见的线协议值，改名=行为变更，与"纯重命名"授权冲突。若要动，需另立一次带迁移说明的契约变更。 |
| 2 | `ExportSink::DesktopDialogHandle`（platform-api） | **保留**。它是"宿主可提供哪几种导出汇"的契约枚举成员，本质是形态并集；改它属于契约设计变更，不在本 initiative 范围。记为后续候选。 |
| 3 | 宿主侧 `AppState.desktop_job_host` / `DesktopTunnelHost` / `DesktopIdentity` | **保留，正确**。宿主本身就是桌面形态，形态名在宿主里是事实陈述。 |
| 4 | `BudgetConfig::desktop_default()` | **保留**（方法名，形态内预设值，非类型名）。 |

结论：共享内核里**类型名与模块路径**已无 desktop 形态味；剩余的 `desktop` 全部是 (a) 线协议字面码 (b) 宿主侧命名 (c) 冻结契约成员 —— 三者都不该由本轨动。

## 现场状态（会话中断后接手者必读）

### ⚠️ 验证树上有未还原告异的变异
`.worktrees/verify-runtime-naming`（detached @ `a4c54cc0c`）当前有 **1 个未还原变异**：
`packages/runtime/src/tunnel/single_counter_audit/mod.rs:98` 的登记标签被改成 `src/tunnel/ghost.rs`（`include_str!` 保持有效，所以能编译）。
这是 Tester 的 P2 变异 1，**是证据的一部分，不是脏数据**，不要直接 `git checkout .` 抹掉。
- Tester `a1f0c68d` 已两次死在「跑变异 1 闸门」这一步，我已 resume（死亡 2/3 次，按规则 resume 优于换新实例）。
- 若它再次死亡（>3 次），换新验证实例前**必须先还原该文件**，否则新实例会在脏树上得出无效结论。
- 我（协调者）未在该树上跑过任何 cargo 命令，未改动任何文件。

### Wave 1 各轨活性（22:41 实测）
| 轨 | 状态 | 活性证据 |
|---|---|---|
| C | 已交付 `a4c54cc0c`，`READY_FOR_TEST` | Tester 复测中 |
| A `job-org-scope` | running，33 文件未提交 | 最近编辑 22:37（4 分钟前），活跃 |
| B `di-dyn` | running，9 文件未提交 | 最近编辑 22:12（29 分钟前），`progress.md` 已写 → 判断为在共享 target 锁上排队编译，**非停滞**；若再 20 分钟无变化需按 §5 探活 |

### 轨道 A 的范围扩张（可接受，但记为已知冲突）
A 给 `JobDefinition` 加 `organization_id` 后，构造点扩散到了任务书列为禁区的文件：
`packages/feature-core/src/restore_job.rs`、`packages/drivers/{postgres,mysql}/tests/restore_apply_job.rs`。
这与轨道 C 一样属于「契约字段变更的必要连带」，不是越界夹带 —— **接受**。
后果：A 与 C 在 `packages/feature-core/src/restore_job.rs` 上**必然冲突**（C 改了那里的 `JobHost` 标识符，A 改了那里的 `JobDefinition` 构造）。按 `C → A` 顺序合，冲突是机械的（在已改名的标识符上加字段）。
合流后必须由 Tester 独立复验 A，重点确认这些连带点**没有夹带逻辑改动**。

## 合流记录

（每轨合入后追加：轨 → commit → 门禁数字 → 冲突处理。）
