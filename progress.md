# 轨道 C：runtime-naming 进度台账

> 临时台账，合并后必须删除（见 AGENTS.md「进度台账：开发期间允许，交付即销毁」）。

## 目标

清理共享内核 `packages/runtime/` 里带 "desktop" 形态命名的实现，让共享包里不出现形态名。
命名表达**职责**（宿主 job host / 路由网络提供器），不表达**形态**。

## 已完成

- [x] `packages/runtime/src/job/desktop_host.rs` → `packages/runtime/src/job/host.rs`（`git mv`）
  - `DesktopJobHost` → `JobHost`；`impl` 块、方法名（`mark_restart_candidates_for_verification`
    等）随类型改名，无残留 `Desktop` 前缀标识符。
  - 文件头文档由「Public desktop facade over the durable job repository and runtime.」
    改写为职责描述「Host-side facade over the durable job repository and runtime.」+
    「名字表达的是职责而不是形态」的说明。
- [x] `packages/runtime/src/tunnel/desktop.rs` → `packages/runtime/src/tunnel/route.rs`（`git mv`）
  - `DesktopNetworkProvider` → `RouteNetworkProvider`；`DesktopRouteCatalog` → `RouteCatalog`。
  - 模块头「桌面侧 `NetworkProvider` 实现」→ 「路由侧 `NetworkProvider` 实现」；
    「此前桌面连接路径」→ 「此前各连接路径」；`/// 桌面 [`NetworkProvider`]。`→ 职责描述。
- [x] `packages/runtime/tests/tunnel_desktop_network_provider.rs` →
  `packages/runtime/tests/tunnel_route_network_provider.rs`（`git mv`）
  - `DesktopNetworkProvider` / `DesktopRouteCatalog` → `RouteNetworkProvider` / `RouteCatalog`；
    测试替身 `ScriptedCatalog` → `ScriptedRouteCatalog`（与新 trait 名配对）。
  - **断言逐条未改**，仅改标识符与 import 顺序。
- [x] 导出面同步：
  - `packages/runtime/src/job/mod.rs`：`pub mod desktop_host` → `pub mod host`；
    `pub use desktop_host::DesktopJobHost` → `pub use host::JobHost`。
  - `packages/runtime/src/tunnel/mod.rs`：`mod desktop` → `mod route`；
    `pub use desktop::{…}` → `pub use route::{RouteCatalog, RouteNetworkProvider}`；
    模块头 `[`desktop`]` 链接 → `[`route`]}`。
  - `packages/runtime/src/lib.rs`：无相关导出，无需改动（已核对）。
- [x] 审计登记表同步（`packages/runtime/src/tunnel/single_counter_audit/mod.rs`）：
  `AUDITED` 表里 `src/tunnel/desktop.rs` → `src/tunnel/route.rs`、
  `tests/tunnel_desktop_network_provider.rs` → `tests/tunnel_route_network_provider.rs`
  （含 `include_str!` 路径）。R6「登记表不许被裁小」用例因此仍绿。
- [x] 引用点全量更新（Grep 全仓搜 `DesktopJobHost` / `DesktopNetworkProvider` /
  `DesktopRouteCatalog` / `job::desktop_host` / `tunnel::desktop`）：
  - `src-tauri/src/bootstrap/app_state.rs`
  - `src-tauri/src/commands/mod.rs`
  - `src-tauri/src/commands/data_transfer/job_api/{mod,runtime}.rs`
  - `src-tauri/src/commands/restore_job/tests.rs`
  - `src-tauri/src/commands/schema_diff/job/desktop.rs`
  - `src-tauri/src/commands/sync/jobs.rs`、`src-tauri/src/commands/sync/host/state.rs`（仅文档行）
  - `src-tauri/src/store/app_db_jobs_tests.rs`
  - `src-tauri/src/tunnel/network_provider.rs`
  - `packages/feature-core/src/restore_job.rs`（仅测试模块的 import / 调用）
  - 上述 `src-tauri/src/store/**`、`src-tauri/src/commands/sync/**`、`packages/feature-core/**`
    属于「本轨禁区」清单，但**类型改名是 runtime 公开 API 变更的必然结果**，
    不改则整个 workspace 编译不过。故只做**逐字标识符替换**，零逻辑改动。
- [x] 文档同步（类型名引用，引用路径已核对代码）：
  `docs/architecture/backend/{ai,data-sync,data-transfer,schema-diff,services,store}.md`、
  `docs/architecture/platform/{data-migration-jobs,feature-core,shared-boundaries-and-ports,team-server-and-auth}.md`、
  `docs/development/platform-development-plan.md`。

## 门禁实测（逐字结论行 + 退出码）

共享 target：`CARGO_TARGET_DIR=/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/.shared-target`

| 命令 | 退出码 | 结论行 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | 无输出（`cargo fmt --all` 应用后复查为 0） |
| `cargo clippy -p datazen-runtime -p datazen --all-targets` | 0 | `Finished dev profile …`，`grep -cE "^error"` = **0** |
| `cargo check -p datazen-runtime -p datazen --all-targets` | 0 | `Finished dev profile …` |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-runtime --lib` | 0 | `test result: ok. 630 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.94s` |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib` | 0（首跑 101 → 重跑 0） | `test result: ok. 1265 passed; 0 failed; 9 ignored; 0 measured; 0 filtered out; finished in 15.52s` |
| `cargo test -p datazen-runtime --test tunnel_route_network_provider` | 0 | `test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s` |

补充（不在指定门禁内，为确认「行为完全不变」而跑）：

| 命令 | 退出码 | 结论行 |
| --- | --- | --- |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-runtime`（全 target） | 0 | 全部 `test result: ok`，0 failed |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-feature-core --lib` | 0 | `test result: ok. 367 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.91s` |

**已知 flake 复现记录**：`DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib`
首跑退出码 **101**，`test result: FAILED. 1264 passed; 1 failed; 9 ignored`，
唯一失败项 `platform::native_tool::tests::native_desktop_output_overflow_is_bounded_killed_and_reaped`
（`assertion left == right failed / left: TimedOut / right: OutputLimit`，
`src-tauri/src/platform/native_tool/tests.rs:87`）。
该用例与本轨道无引用关系（宿主 native tool 进程测试）。单跑重试退出码 **0**：
`test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1273 filtered out; finished in 0.45s`；
整包重跑退出码 **0**：`test result: ok. 1265 passed; 0 failed; 9 ignored`。

## 验收标准逐条核对

1. `grep -rn "DesktopJobHost\|DesktopNetworkProvider\|DesktopRouteCatalog" packages/ src-tauri/src/`
   → **0 命中**（grep 退出码 1）。
2. `packages/runtime/src/` 下名为 `desktop.rs` / `desktop_host.rs` 的文件 → **不存在**；
   `grep -rn "mod desktop\|desktop_host\|tunnel::desktop" packages/runtime/src/` → **0 命中**。
3. 行为不变：见上表全部测试（runtime lib 630 / host lib 1265 / 重命名后的集成测试 13 /
   runtime 全 target / feature-core lib 367）均绿，**断言逐条未改**。
4. 冻结契约清单：`src-tauri/src/platform/ipc.rs` 的 `FROZEN_SIGNATURES` 只钉
   `commands/connection.rs` 的 15 个 `#[tauri::command]` + 14 个 `*_impl` 签名，
   **不含**本轨三个类型名 → **无需更新**（已核对，未改动该文件）。
   另一处契约清单是 `packages/runtime/src/tunnel/single_counter_audit/mod.rs` 的 `AUDITED`
   登记表（R6 机械门禁）→ **已同步更新**。
5. 门禁全绿 → 见上表。

## 遗留项 / 待裁定项

1. **`packages/runtime/src/tunnel/route.rs` 里 5 个仍带 `desktop` 的稳定字面码**：
   `desktopRouteUnknown`、`desktopTunnelReferenceNotHeld`、`desktopTunnelEndpointUnavailable`
   （出现 2 次）、`desktopTunnelTeardownUnconfirmed`（trace reason）、
   以及 `LeaseId` 前缀 `desktop-tunnel/`。
   它们都是**已对外可见的值**（进 `PortError` 消息、进 trace reason、进租约文本），
   改名即行为变更，超出「纯重命名」授权，故**逐字保留**，并在模块头写明保留理由。
   → **待裁定**：是否另开一条「字面码去形态化」变更（届时需同步更新调用方与日志检索）。
2. **`ExportSink::DesktopDialogHandle`**（`packages/platform-api/src/ports/artifact.rs:128`）
   是 `packages/runtime/src/artifact/mod.rs:638` 生产路径上引用的**平台冻结契约变体**。
   `packages/platform-api/**` 是本轨禁区且是冻结上游，改名要动平台契约 → **未改**，
   作为「共享包里仍有一个以 Desktop 开头的类型名」的已知例外上报。
3. **宿主侧字段名 `AppState.desktop_job_host`**（`src-tauri/src/commands/mod.rs:179`）
   与宿主类型 `DesktopTunnelHost` / `DesktopTunnelSource` / `DesktopIdentity` /
   `DesktopRestoreGateway` **保持原样**：它们在宿主 crate 内，而宿主本身就是桌面形态，
   不属于「共享内核里的形态名」；且改名会波及本轨禁区的 `src-tauri/src/store/**`、
   `src-tauri/src/commands/sync/**` 与多份架构文档。→ **待裁定**：是否由宿主轨另行处理。
4. `packages/runtime/src/budget/quotas.rs` 的 `BudgetConfig::desktop_default()`（方法名，
   非类型名）与 `packages/runtime/src/job/time.rs`、`runtime_repository.rs`、
   `sql_guard/keyword.rs` 里描述「桌面宿主如何接线」的注释**保持原样**：
   前者是预算档位名（形态中立语义之外的产品口径），后者是事实描述，按任务书允许。
