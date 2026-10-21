# 存储层端口化 — 协调台账与完整开发计划

> **本文件是开发期台账，交付即销毁。** 验收合并时必须删除，不得存活在 `main` 上（AGENTS.md「进度台账」条）。
> 设计结论与缺陷必须变成代码、测试与 `docs/` 正式文档里的事实；台账消失后结论仍要能从代码读出来。
> 基线：`main` @ `8bf6eb553`（PR #81 已合入）。集成分支 `feature/storage-ports`，worktree `.worktrees/datazen-storage-ports`。

---

## 0. 目标架构模型（本 initiative 的判准）

用户确立的四条 + 协调者补的第五条：

1. **存储层由 Repository trait 提供抽象**给 runtime / feature-core 等；共享层面向接口编程，不面对具体实现。
2. **具体实现由 web 与 desktop 宿主各自提供**。
3. **初始化时构造具体 Repository 并注入**（依赖注入）。
4. **web / desktop 各保留形态差异**（HTTP server / Tauri app / 本地 I/O）。
5. **（补）每个存储端口的方法要么带 `&RequestContext`，要么在构造时绑定 org 作用域。** 桌面单租户不需要它，但一旦 trait 签名里没有 org，web 就只能"按组织各建一个实例"，接口层就丧失了显式表达租户边界的能力。样板：`packages/platform-api/src/ports/profile.rs:24-40` 的 `ProfileScope::from_context(ctx)`，越权语义为**返回 `None`/空而非错误**（CM-05，不泄漏"存在但无权"）。

### 现状盘点（实测，非推断）

| 项 | 实测 |
|---|---|
| `packages/platform-api/src/ports/` | **18 个 trait**，13 个有实现 |
| **无人实现的端口** | `IdentityResolver`、`EventSink`、`SubmissionTokenIssuer`、`ClusterNodeBudgetPort`、`ControlResourcePort` |
| 共享包内的进程级 `static`（多租户串味源） | 3 处：`schema-diff/reviewed.rs:49`、`data-sync/plan_store/mod.rs:674-675`、`data-transfer/plan_store/mod.rs:440-441` |
| 共享包内绑文件系统的存储 | `data-sync/src/comparison_store/`（`disk.rs` 494 + `mod.rs` 729，合计 32 处 `fs::`/`Path`） |
| 未抽象的具体存储 | `data-sync/plan_store`（2,025 行）、`data-transfer/plan_store`（541 行） |
| DI 风格 | **两套并存**：`WorkflowRegistry<S>` 泛型 / `DashboardStore` 为 `&dyn` |
| 多租户词汇 | 已就位：`RequestContext.organization_id`、`ProfileScope`、`SessionOwner.organization_id`、`PolicyService.isolation_key`；**缺**：`JobDefinition`/`JobFilter` 无 org |
| 宿主残留转发门面 | `src-tauri/src/commands/sync/mod.rs:6`、`src-tauri/src/wapps/mod.rs:26-34`、`src-tauri/src/monitor/channels.rs:14`、`src-tauri/src/lib.rs:8/20/27` |

---

## 1. 度量口径（防止把解耦做成搬家）

**本 initiative 不压缩 src-tauri，它降低耦合。** 实测已证明：Wave 1 三轨对宿主净变化 **≈ +27 行**（A: 0；B: +30；C: −3）。

| 阶段 | src-tauri 生产行数 |
|---|---|
| 基线 main | **73,363**（测试文件 24,987 / 总 98,350） |
| Wave 1 (A/B/C) | +27 → ≈73,390 |
| Wave 2 (D/E/F/G) | +400～900 → **≈73.8k–74.3k** |
| （后续 Wave 4 搬纯规则） | −1,000～1,600 → ≈72.5k，**是搬家不是删除**，全仓总量不变 |

**src-tauri 有约 70k 的地板**：SQLite/AppDb 持久化、Tauri IPC + 窗口/菜单/托盘、AES-GCM + keyring、SSH 隧道、原生对话框、桌面身份装配、迁移三件套的 job 生命周期接线（20.5k 里实测 83% 是 job/驱动执行，不是可搬规则）、25k 宿主测试。**不要用"还剩多少行"验收。**

### 成功判据（四条，均可机械核验）

1. 共享包内**类型名与模块路径**无 `Desktop` 形态味。例外三类**不在本条范围**：线协议字面码、冻结契约成员、宿主侧命名（见 §4 C 轨裁决）。
2. web 必需的端口有可运行实现 —— 关键是 `IdentityResolver`（5 个无人实现里唯一阻塞 web 的）。
3. **共享包内的进程级 `static` 清零**（上述 3 处）。
4. **不改动任何共享包代码**，web 宿主仅实现 trait 即可跑起来。

### 反模式（明确禁止）

- ❌ 把 `disk.rs` 整块搬进 `src-tauri/` —— 宿主 +500 行，等于把解耦做成搬家，web 还得再抄一遍。
  ✅ 正解：fs 实现留在包内（**任何有本地文件系统的宿主都能用**，本就形态中立），端口只收"存储根/实例"注入。`comparison_store/mod.rs:48-50` 已经收 `root: PathBuf`，说明只差构造点接线。
  **核验标准：改完后 `packages/data-{sync,transfer}` 总行数基本不变，`src-tauri` 只增加少量注入接线。**
- ❌ 用兼容别名冒充改名完成（`pub use host::JobHost as DesktopJobHost`）—— 验收不成立。
- ❌ 把 `JobFilter.organization_id` 做成 `Option` 来回避"列举必须按组织过滤"。

---

## 2. 波次划分（按**文件冲突面**，不是功能相邻度）

```
Wave 1 (并行 3)   A: job-org-scope     B: di-dyn        C: runtime-naming
                     ↓                    ↓                 ↓
                  合流 C → B → A（改名类先合）
                     ↓
Wave 2 (并行 4)   D: featurecore-org   E: reviewed-port  F: migration-store  G: identity-resolver
                     ↓                    ↓                 ↓                    ↓
                  合流 E → F → D → G
                     ↓
Wave 3            R 阶段全量回归 → 合入 main → 删台账 → PR
Wave 4 (可选,不在本目标内)  搬纯规则 / WappCatalog 端口 / 其余 4 个端口
```

### 依赖与排序理由

- **D 依赖 B**：两者都改 `feature-core/src/workflow/registry.rs` 与宿主 `dashboard/create.rs`。B 先把 `WorkflowRegistry<S>` 变 `Arc<dyn>`，D 再往签名里加 scope，避免同一文件两轮结构变更。
- **E/F/G 依赖 A**：A 提供 org 词汇（`JobDefinition`/`JobFilter.organization_id`），E/F 的端口化要用它做租户边界，G 的实现直接消费它。
- **E / F / G 之间无文件交集**，可完全并行。
- **合流顺序 `C → B → A`**：协调者原先判断"C 只碰 desktop_host.rs + mod.rs"是**错的** —— C 改的是跨 crate 公开类型名，引用点扩散到 6 个宿主文件，与 A/B 重叠。机械改名先落地，后合的轨在**已改名的标识符**上解冲突，远比反向省力。

---

## 3. 各轨范围、落点与验收

### Wave 1

| 轨 | 分支 / commit | 范围 | 验收 |
|---|---|---|---|
| **A** `job-org-scope` | `feature/job-org-scope` / **`5ea292d8a` + `5bde303eb`**（35 改 + 3 新），`READY_FOR_TEST` | `platform-api/dto/job.rs:73 JobDefinition`、`:280 JobFilter` 加 `organization_id`（**必填非 Option**）；`ports/job.rs` 契约补语义；`runtime/job/repository.rs` 的 `InMemoryJobRepository` 按 org 过滤（跨 org 返回"不存在"）；`runtime_repository.rs`/`memory_runtime_repository.rs`/`owner_claims.rs`/`runtime_methods.rs`；宿主 `store/app_db/jobs/`（`admission.rs:146`）加列 + 迁移 + 过滤；`platform/ipc.rs` FROZEN_SIGNATURES 同提交更新 | 桌面单租户**行为完全不变**（1265 条断言不许动）；≥3 条组织隔离单测；跨 org 不泄漏"存在但无权" |
| **B** `di-dyn` | `feature/di-dyn` / `d235f7d3` + `062aa0fde`（Tester 补测）+ `13f3df9bd`（修 D1 + 销毁台账）→ **已合入 `056b35834`** | `feature-core/src/workflow/registry.rs:95/103` 去泛型 → `Arc<dyn WorkflowDefinitionStore>`；宿主 `workflow/registry.rs`（`AppDbWorkflowStore` 提可见性 + 构造入口）、`bootstrap/app_state.rs:205`、`commands/mod.rs:78/167`、`dashboard/create.rs:3/11/53/77/124`、`commands/workflow.rs`、`commands/context.rs`；包内测试夹具 `MemoryStore` | 共享包公开签名里**不出现宿主类型参数**；`WorkflowRegistry` 无类型参数；feature-core 仍不依赖宿主 crate；行为不变 |
| **C** `runtime-naming` | `feature/runtime-naming` / **`a4c54cc0c`** + `52225ed04`（Tester 补测）+ `b46db6468`（销毁 progress.md）→ **已合入 `e267fad3f`** | `git mv`：`job/desktop_host.rs`→`job/host.rs`（`DesktopJobHost`→`JobHost`）、`tunnel/desktop.rs`→`tunnel/route.rs`（`DesktopNetworkProvider`→`RouteNetworkProvider`、`DesktopRouteCatalog`→`RouteCatalog`）、`tests/tunnel_desktop_network_provider.rs`→`tunnel_route_network_provider.rs`；导出面、`single_counter_audit` 的 `AUDITED` 登记表、宿主/feature-core 引用点、11 份文档 | 三类型名 0 命中；无 `desktop.rs`/`desktop_host.rs`；**断言逐条未改**；历史保留 |

### Wave 2 —— **工作树已全部建好并注入热 target**

四棵 Wave 2 工作树已用 `scripts/new-feature-worktree.sh` 建好（base = `feature/storage-ports` @ `9c724c8e8`），各自注入 11G 热 target（APFS 克隆）+ touch path 依赖：

| 轨 | 分支 | worktree | 热 target |
|---|---|---|---|
| D | `feature/featurecore-org-scope` | `.worktrees/datazen-featurecore-org-scope` | ✅ 11G | ⏳ 已派发 `9b9f1166` |
| E | `feature/schema-diff-reviewed-port` | `.worktrees/datazen-schema-diff-reviewed-port` | ✅ 11G | ✅ 已交付 `33416cb72` → Tester `0da32120` 复测中 |
| F | `feature/migration-store-ports` | `.worktrees/datazen-migration-store-ports` | ✅ 11G | ⏳ 已派发 `29467fd8` |
| G | `feature/identity-resolver` | `.worktrees/datazen-identity-resolver` | ✅ 11G | ⏳ 已派发 `e035cc8f` |

**派发前必做**：Wave 1 合流进 `feature/storage-ports` 后，把集成分支 merge 进这四棵树的分支，再**重新 touch** path 依赖。**已执行**（四树均推进到 `65dac1e2c` = C + B + `OrganizationScope`）。

### Wave 2 的共用前置：`OrganizationScope`（协调者已先行落地 `65dac1e2c`）

四轨都需要「按组织限定存储访问」的作用域。**若让四轨各造各的，会得到四套并行的租户边界机制** —— 那比不做更糟。因此协调者先在契约层落了一个共用词汇：

`packages/platform-api/src/ports/scope.rs` → `OrganizationScope { organization_id }`，带 `from_context(ctx)`。
- 身份**只能**由 adapter 构造的 `RequestContext` 派生（INV-01），不接受任何请求体字段。
- 越组织访问一律按「**不存在**」处理，不返回「无权限」（CM-05，避免泄漏"存在但无权"）。
- **不复用** `ProfileScope` —— 后者限定的是「某组织的连接档案」，语义不同；两个作用域并存是刻意的。
- **不要求桌面 SQLite 加 organization_id 列**：桌面是单租户，它的实现**只承认自己那一个组织**，传别的组织的 scope 按「不存在」处理。这让隔离成为**真实的**而不是名义上的，且 schema 不变。

四轨简报里都写明：**用这个类型，不要自己造**。


#### D `featurecore-org-scope`（依赖 B）
**落点（实测）**
- `feature-core/src/dashboard/store.rs:35-76` `DashboardStore` **10 方法**：`list_dashboard_rows`/`dashboard_row`/`upsert_dashboard_row`/`delete_dashboard_row`/`set_dashboard_refresh_paused`/`list_widget_rows`/`widget_row`/`upsert_widget_rows`/`delete_widget_row`/`find_workflow_refs`
- 包内规则层自由函数签名也要带 scope：`:209 list_dashboards`、`:220 get_dashboard`、`:234 save_dashboard`、`:266 delete_dashboard`、`:272 set_dashboard_refresh_paused`、`:280 find_workflow_refs`、`:288 assign_import_id`
- `feature-core/src/dashboard/runs.rs:28` `DashboardRunStore`
- `feature-core/src/workflow/registry.rs:35` `WorkflowDefinitionStore`（4 方法）；`WorkflowRegistry` 公开方法 `get:188`/`list:209`/`list_definitions:218`/`save:247`/`save_yaml:254`/`delete:261`/`load_all:268`/`ensure_loaded:273` 需把 scope 传下去
- 宿主实现：`src-tauri/src/dashboard/store.rs:24`、`dashboard/runs.rs:15`、`workflow/registry.rs:39`
- 宿主调用：`src-tauri/src/monitor/engine.rs:10/63/67`、`dashboard/export.rs:10/24`、`dashboard/execute.rs:24`、`commands/dashboard.rs`

**验收**：三 trait 每个方法都能表达 org 作用域（`&RequestContext` 或构造期绑定 `Scope`，二选一并在全仓保持一致）；桌面行为与 1265 条断言不变；新增跨 org 用例证明 dashboard/workflow 不可互见。

#### E `schema-diff-reviewed-port`（依赖 A）
**落点**
- `packages/schema-diff/src/reviewed.rs`（524 行）。**病灶 `:49 static PLANS: OnceLock<Mutex<HashMap<String, ReviewedPlan>>>` + `:53 fn plans()`**
- 公共入口：`:57 identity`、`:63 validate_source_connection_snapshot`、`:364 same_endpoint`、`:387 physical_database_scope`、`:451 validate_snapshot`、`:501 validate_object_snapshot`（`:324 validate`、`:358 normalized_scope` 私有）
- 类型：`:19 ReviewedPlan`、`:38 ReviewedSourceSnapshot`
- 宿主调用**全在** `src-tauri/src/commands/schema_diff/`：`object_plan_commands.rs:54/150/214/279/328/403/452/527`、`unified_plan/revalidation.rs:9/23`
- 文件头注释「the plan registry lives for the process」**就是要被打破的语义**，改造时同步改写

**验收**：`grep "static PLANS" packages/schema-diff/src/` 0 命中；宿主持有一个注入的 store 实例；包行数基本不变。

#### F `migration-store-ports`（依赖 A，⚠️ 见 §1 反模式约束）
**落点**
- `packages/data-sync/src/plan_store/mod.rs`（**2,025 行**）：`:505 SyncPlanStore { plans: Mutex<HashMap> }`、**`:674-675 static STORE`**；`global_store()` 被 **6 个自由函数**调用：`issue_plan:680`、`issue_plan_with_store_and_id:718`、`peek:754`、`attach_endpoint_identities:762`、`claim:783`（内部 `:695/:734`）
- `packages/data-transfer/src/plan_store/mod.rs`（541 行）：`:166 TransferPlanStore { plans, checkpoints }`、**`:440-441 static STORE`**
- `packages/data-sync/src/comparison_store/`：`disk.rs` 494（**留在包内**）、`mod.rs` 729（`:48-50` 已收 `root/directory/manifest: PathBuf`）、`recovery.rs` 462、`manifest_cache.rs` 97、测试 890+326
- 顺手删残留门面 `src-tauri/src/commands/sync/mod.rs:6 pub(crate) use datazen_data_sync::comparison_store;`
- 宿主调用：`commands/data_transfer/{exec.rs:14, preview.rs:427/832, job_api/mod.rs:58, job_api/assembly.rs:22}`、`commands/sync/{exec.rs:16/859, host/state.rs:26, mod.rs:32}`
- **端口先例可参照**：`packages/data-sync/src/job/host.rs:78 async fn open_endpoint(...)`

**验收**：两处 `static STORE` 清零；`SyncPlanStore`/`TransferPlanStore` 的实例与存储根由宿主在组装根注入；包行数基本不变（核验 §1）。

#### G `identity-resolver`（依赖 A）
**落点**
- 端口 `packages/platform-api/src/ports/identity.rs`：`ExecutionIdentity { organization_id, connection_id, execution_identity_key, policy_isolation_key, credential_revision }`；`resolve(&ctx, &ExecutionTarget)` / `resolve_delegation(&ctx, DelegationRef)`
- **端口自带 5 条单测（`:71-190`）即验收依据**：身份键不得由客户端传入 principal 派生；轮换凭据只动 `credential_revision` 不动 key；`resolve` 与 `resolve_delegation` 是两条路径而非一个 flag；debug 输出不含 password/material 字样
- 桌面原料（均已存在，**不要新建**）：
  - `src-tauri/src/platform/identity.rs:66 DesktopIdentity` → `organization_id():127`、`principal_id():131`、`client_instance_id():135`、`request_context():111`、`next_request_context():123`
  - `src-tauri/src/platform/repositories.rs:279 impl SecretProvider for DesktopProfiles` → `revision():358` = `credential_revision` 来源
  - `src-tauri/src/platform/policy.rs` → `PolicyService::isolation_key(ctx)` = `policy_isolation_key` 来源
  - `packages/platform-api/src/id.rs` → `ExecutionIdentityKey`/`PolicyIsolationKey`/`CredentialRevision` newtype
- 硬约束（端口注释原文）：「同一 `(organization, connection, isolation)` 必须稳定返回同一 `execution_identity_key`，否则会话连续性（INV-02）与池复用都不成立」→ **确定性派生**（runtime 已有 `hmac`/`sha2` 依赖可参照 `packages/runtime/src/job/` 的做法），禁止随机、禁止把凭据材料放进身份键

**验收**：`grep "IdentityResolver for"` 在宿主命中一条真实实现；5 条端口单测语义在宿主实现上重验（尤其确定性：同输入两次调用得同 key）；`ExecutionIdentity` 不含任何秘密材料。

---

## 4. 已裁决项（不阻塞验收）

| # | 项 | 裁决 |
|---|---|---|
| 1 | C 轨 `route.rs` 里 4 个 `desktop*` reason code + `LeaseId` 前缀 `desktop-tunnel/` | **保留**。已对外可见的线协议值，改名=行为变更。要动需另立带迁移说明的契约变更。 |
| 2 | `ExportSink::DesktopDialogHandle`（`platform-api/ports/artifact.rs:128`，被 `runtime/src/artifact/mod.rs:638` 生产路径引用） | **保留**。冻结契约成员，属"宿主可提供的导出汇"形态并集。记为后续候选。 |
| 3 | 宿主 `AppState.desktop_job_host`、`DesktopTunnelHost`、`DesktopIdentity`、`DesktopRestoreGateway`、`commands/schema_diff/job/desktop.rs` | **保留，正确**。宿主本身就是桌面形态，形态名在宿主里是事实陈述。 |
| 4 | `BudgetConfig::desktop_default()`（方法名，形态内预设） | **保留**。 |
| 5 | A/B 轨"越界"改禁区文件（`feature-core/restore_job.rs`、`drivers/{postgres,mysql}/tests/restore_apply_job.rs`、`store/app_db_jobs_tests.rs`、`commands/sync/jobs.rs`） | **接受**。契约字段/类型变更的必要连带，非夹带。后果：A 与 C 在 `feature-core/src/restore_job.rs` **必然冲突**（C 改那里的 `JobHost`，A 改那里的 `JobDefinition` 构造）——按 `C→A` 合，冲突是机械的。合流后 Tester 须独立复验这些连带点**没有夹带逻辑改动**。 |

---

## 5. 现场状态（会话中断后接手者必读）

### 🔧 标准流程：热 target 克隆（Wave 2 各树必须用，避免冷编译）

**热源（全程保留，收工才删）**：`/tmp/datazen-di-dyn-target`（9.6G，由 B 轨用 `--drivers=all` 全量构建，已验证完整可用）。

**每个新工作树建好后立即执行**（实测克隆 9.7G 只需 **4–7 秒**，APFS 写时复制 `cp -c`）：
```
# 1) 克隆热 target 进本树
cp -c -R /tmp/datazen-di-dyn-target <worktree>/target
# 2) 强制 path 依赖针对本树源码重新检查（registry 依赖不动）
find <worktree>/packages -name '*.rs' -path '*/src/*' -exec touch {} +
find <worktree>/src-tauri/src -name '*.rs' -exec touch {} +
```
**实测效果**：集成树 `cargo check -p datazen --all-targets` = **3m18s**，日志只出现 `Checking datazen-feature-core (.../datazen-storage-ports/packages/feature-core)` —— 即 **path 依赖按本树源码重新检查**（日志路径指向本树，不会链接别的工作树），**registry 依赖（tauri/tokio/reqwest 等编译大头）全部复用**。冷编译约 15–25 分钟。
**touch 必须做**：万一指纹撞车，touch 强制重建，杜绝跨工作树串产物。

**代理的 target 目录约定**：一律用**自己工作树内的 `target/`**（如上克隆所得）；**不用** `/tmp/datazen-<track>-target` 另建，**更不用**已作废的 `.shared-target`。各树之间互不共享 → 既热又隔离。

### ⚠️ 共享 `CARGO_TARGET_DIR` 已作废 —— 各轨必须用隔离 target 目录

**这是协调者的判断失误**：我最初让三条轨道共用 `.worktrees/.shared-target`，实测证明**会跨工作树串产物** —— cargo 对 path 依赖（本仓库 `packages/*`）的指纹与产物文件名**不区分工作树路径**，多工作树互相覆盖同一 crate 的 rlib；后跑的轨道若判定"产物比源文件新"就 fresh 复用，**实际链接的是别人工作树的产物**。

实证（B 轨实测上报）：共享目录下 `cargo check` 退出码 101，报错路径直接指向 `.worktrees/datazen-job-org-scope/packages/feature-core/src/workflow/registry.rs:95`（`struct … with 1 generic parameter: S`）与该轨道独有的 `JobDefinition.organization_id` —— 即链接了 job-org-scope 工作树的产物。

**处置**：
- 共享目录已改名为 `.worktrees/.shared-target-VOID`，**禁止再使用**（含各轨的 `export CARGO_TARGET_DIR=...shared-target`）。
- **每条轨道 / 每个验证树各自建隔离目录**：`/tmp/datazen-<track>-target`（约 10G/条，磁盘 229 GiB free，可承受）。
- **后果追溯**：在共享目录上跑过的门禁结论**一律不可信**。已要求 A 轨（仍在跑）与 C 的 Tester（已 resume）**在隔离目录下重跑门禁**。C 轨编码代理自验的结论同样受影响，其 Tester 的 P5 全量门禁已在隔离目录重跑。
- B 轨 `d235f7d3` 的门禁**本来就是隔离目录跑的**（`/tmp/datazen-di-dyn-target`），结论可信。

### ⚠️ 验证树上有一个未还原告异的变异
`.worktrees/verify-runtime-naming`（detached @ `a4c54cc0c`，独立建树 + 补齐 gitignored codegen + node_modules 软链）：
`packages/runtime/src/tunnel/single_counter_audit/mod.rs:98` 的登记标签被改成 `("src/tunnel/ghost.rs", include_str!("../route.rs"))` —— `include_str!` 保持有效故可编译。

**这是 Tester 的 P2 变异 1，是证据，不是脏数据。不要 `git checkout .` 抹掉。**
- Tester `a1f0c68d` 已两次死在"跑变异 1 闸门"这步，已 resume（死亡 2/3 次；>3 次则换新实例，但**换新前必须先还原该文件**，否则新实例在脏树上结论无效）。
- 变异验证的判准：标签指向不存在的路径 → 闸门**必须点红**（R6 的立论是"每个模块都得登记"）；静默通过 = **R6 形同虚设，记缺陷**。
- 还要做变异 2：**整条删掉** `route.rs` 登记行 → 同样必须点红（"闸门自己不能被裁小"）。注意删掉一条 `include_str!` 会让该源码不再被扫，可能顺带触发别的规则断言 —— 要逐字读失败消息，确认是**因 R6 红**才算闸门有效。
- 每次变异跑完立即还原并复验 EXIT=0。

### 三轨活性（22:50 实测）
| 轨 | 状态 | 活性证据 |
|---|---|---|
| C | **已验收 TEST_PASSED → 已合入集成 `e267fad3f`** | Tester `a1f0c68d`（3 次尝试后完成） |
| B | **已验收 TEST_PASSED → 已合入 `056b35834`** | Tester `cda75c30` |
| A | 第 1 轮修复完成 `951f5ef68`（D1–D5 全处置）→ **全新 Tester 复测中** `64877b2a`（验证树 `verify-job-org-scope-v2`） | 上一轮 Tester `b285d2f2`：组织维度逐方法核实贯通，8 组变异证明新断言有效 |

### 环境
- `export CARGO_TARGET_DIR=/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/.shared-target`（三轨共用；会打印 `Blocking waiting for file lock` —— **正常，等它**，禁止各轨改 target 目录）
- 各 worktree 的 `node_modules` 是软链，**禁 `pnpm install`**；驱动集 `NEW_WT_DRIVERS=all`（与 CI 对齐）
- 磁盘 229 GiB free
- 已知 flake：`platform::native_tool::tests::native_desktop_output_overflow_is_bounded_killed_and_reaped` 高负载下 `TimedOut`（期望 `OutputLimit`），单跑重试即绿，**不要为此改代码**
- 长输出一律落 `mktemp` 文件再 `tail`/`grep`；退出码单独打印

---

### A 轨的裁决（6 项）

| # | 项 | 裁决 |
|---|---|---|
| 1 | 越界改 `packages/feature-core/src/restore_job.rs` 2 行（DTO 必填字段，不改则该 crate lib test 无法编译） | **接受**。与 C 轨同文件冲突 → 按 `C→A` 合，机械可解。 |
| 2 | 简报前提错误：jobs 表**早有** `organization_id`（`JOB_SCHEMA_V2`），无需迁移 | **记录为协调者简报失误**。代理核实后如实上报而非照做 —— 正确行为。 |
| 3 | `InMemoryJobRepository::accept` 重复 jobId：**静默覆盖 → 报错** | **有条件接受**。条件：Tester 须独立验证 ①改前是否真是跨组织覆盖入口（旧实现是否按 jobId 覆盖整行）②是否有既有测试断言过"重复即覆盖"而被改弱或删除 ③错误类型与 SQLite 主键冲突口径真的一致 ④无调用方依赖静默覆盖。**任一条不成立则回退该行为变更**，只保留字段与过滤。 |
| 4 | `JobRuntimeRepository::renew` 无 `ctx` | **接受**，条件：端口注释须明确写"续约由 claim fencing 兜底"，否则后来者会误以为 renew 也有 org 隔离。 |
| 5 | `RecoveryFilter` 未加组织字段 | **接受**（恢复扫描按 ctx 的组织过滤即组织内）。跨组织恢复扫描需显式 opt-in 类型 → 记入 Wave 4。 |
| 6 | `persistence-model.md` §4.1 写 SCHEMA_VERSION=3，代码是 4 | 若确为**既有漂移** → 归 Wave 3 文档收口（非阻塞）；若本轨引入 → 缺陷。Tester P7 独立核实。 |

### C 轨验收结论（Tester `a1f0c68d`）

**TEST_PASSED**。取证质量值得记录：**变异自证了新增测试非空洞** —— 删掉 `host.rs` Running 分支的 `persist_recovery` 调用后，它自己补的集成测点红，而既有 host AppState 旅程**保持绿**；机制查明为 SQLite 仓储 `mark_pending_for`（`runtime_ops.rs:228-236`）自己落 verdict JSON，**遮蔽了宿主层的漏调**。这条测试钉住的正是既有测试**结构上测不到**的那半不变量。

| 项 | 结论 | 关键证据 |
|---|---|---|
| P0 改名真实性 | 通过 | 无 `as Desktop*` 别名、无 `#[deprecated]`；三旧类型名 + 旧模块路径全仓 0 命中 |
| P1 历史保留 | 通过 | 三处 `git log --follow` 均为真 rename（R094/R090/R093），历史连续 |
| P2 机械闸门 | 通过 | 隔离 target 下变异 1（标签→ghost）与变异 2（整条删登记）**双双 EXIT=101 点红**，失败消息逐字为 R6 本因（非顺带红）；每次还原复验 19 passed |
| P3 行为不变 | 通过 | perl 反替换 + diff：断言计数旧=新（`assert!`24 / `assert_eq!`30 / 13 测试函数）、**断言行逐字节 diff=0**、无新增 `#[ignore]`；`route.rs` +30 行**全为注释**；5 个对外字面码逐字保留 |
| P4 覆盖补齐 | 完成 | 新增 `packages/runtime/tests/job_host_restart_verification.rs`（201 行 / 1 测试 / 零生产代码） |
| P5 全量门禁 | 通过 | 隔离 target 下 9 项 EXIT 全 0（runtime 630 / 44 target 全 ok / 13 / host 1265 / feature-core 367 / platform-api 221） |
| P6 文档 | 通过 | 11 份 diff 逐份核对干净；docs/ 反向 grep 0 残留 |

**C 的缺陷清单处置**：
- `ENV-01`（共享 target 串产物，环境级）→ **已处置**（隔离 target + 热克隆流程已固化于 §5）。
- `DOC-01`（progress.md 的 grep 准则只覆盖类型名、漏模块路径名）→ **实质无缺口**（Tester 补扫确认路径名也 0 命中），且 progress.md 已随交付销毁 → **无需行动**。
- `INFRA-02`（SQLite 双写遮蔽：宿主层漏调 `persist_recovery` 在 host 旅程里测不出）→ 风险已被 `52225ed04` 闭合；**提示归 Wave 3 文档收口**（写进正式文档，提醒后续改 `host.rs` 的人不要依赖 host 测守这条）。

### B 轨验收结论（Tester `cda75c30`）

**TEST_PASSED**，无阻塞缺陷。取证里两处做法值得留在台账：

- **M1 / M1′ 双变异证明断言强度等价**：把 `seed_builtins_if_empty` 改成跳过播种，**改后**与**基线**上失败的是**同一对**用例（`365 passed; 2 failed`）→ 断言强度未被削弱，而不是"改完仍然绿"就算过。
- **M3 证明新不变量承重**：把组装根注入的 store 换成另一个 `AppDb` → **稳定**失败 3 条，其中一条是 Tester 新加的 `finish_app_state_shares_one_app_db_between_the_registry_and_the_store`，报 `the registry must write into the store's own app db: NotFound(...)`。
  **这条不变量是 B 引入的**（改前 registry 自持 `Arc<AppDb>` 时天然成立，改后靠装配约定维持，改前无任何测试钉住）。Tester 补的集成测试（`src-tauri/src/bootstrap/tests.rs`，仅测试文件）已随 B 合入。

| 项 | 结论 |
|---|---|
| P0 去泛型彻底 | 通过。feature-core 仅 3 个公开类型带类型参数且**全是生命周期**；`AppDbWorkflowStore` 为 `pub(crate)`，不出现在任何公开签名；`cargo tree` 无宿主 crate、无 tauri |
| P1 断言保真 | 通过（M1/M1′ 双变异） |
| P2 API 影响面 | 通过。`store()` 生产代码**零调用点**；`app_db()` 无残留；**已查实是同一个 `Arc<AppDb>`**（`Store::app_db()` 是 `Arc::clone`，组装根注入同一实例） |
| P3 行为不变 | 通过（367 未减少；5 个函数体逐字节 IDENTICAL） |
| P4 IPC 未变 | 通过（`run.rs` 逐字节相同） |
| P5 文档 | **2 个非阻塞缺陷**（D1/D2） |
| P6 门禁 | 全绿（隔离 target） |
| P7 E2E | 不适用；唯一需端到端证明的不变量改用组装根集成测试覆盖，并由 M3 证明其有效性 |

**B 的缺陷清单处置**：
- `D1`（`workflow-resource-model.md:67` 同行 8 处失效行号 + 计数 10→13 + 漏列 3 个 feature-core 写入点）→ **已在 B 分支修复并随 B 合入**（Tester 给了精确正确值，协调者已独立逐条核实）。
- `D2`（`backend/workflow.md` 共享模块树漏 `error.rs`/`executable.rs`/`history.rs`/`target_plan.rs`；`results.rs` 是私有模块不列是对的）→ 归 **Wave 3 文档收口**（与本轨道无关，Tester 已核实属实）。
- `D3`（**先存**竞态：`platform::live_source::tests::*` 的 `DATAZEN_DESKTOP_USER` 进程级 env 竞态）→ Tester 用 M4 证伪本轨道责任（回退 B 的 6 个文件后 5 次跑，第 4 次同样 `1262 passed; 3 failed`，失败正是同族用例）→ **归 Wave 4，但优先级高**（它让 CI 周期性红）。
- `D4`（编码代理自述数字与实况不符：自称 9 文件 +215/−45，实为 9 文件 +222/−45，排除 progress.md 则 8 文件 +93/−45）→ 信息项，无行动。
- `D5`（宿主壳硬编码 `BUILTIN_DEFINITIONS`，web 宿主想要自定义 builtins 只能用 `CoreRegistry` 从而失去宿主壳的 `workflows_dir()`/`save_workflow()` 命名）→ 设计观察，归 **Wave 4**（建议：宿主构造器改名或 builtins 参数化）。

### A 轨验证结论（Tester `b285d2f2`）—— TEST_FAILED

**核心实现判定通过**：10 个 `JobRepository` + 17 个 `JobRuntimeRepository` 方法逐个核实，内存与 SQLite **错误形状完全一致**（不存在→`NotFound`，越组织→`StaleClaim`），**没有"内存漏过滤、SQLite 已过滤"的半贯通**。8 组变异（M1/M2/M3/M4/M6/M8/M9）逐条按预期变红，证明新断言非空转。Tester 写了一个**会自己变红的守卫测试**（扫全仓 `JobFilter` 字面量，缺 `organization_id` 即报错并给出行号）。

**阻塞缺陷 D1 —— 暴露的是协调者门禁集的盲区，不是实现问题**：
`JobFilter` 移除 `Default` 派生后，`src-tauri/src/commands/dashboard.rs:585-588` 这个**被 `#[cfg(feature = "webdriver")]` 门控**的调用点没跟上 → `cargo check -p datazen --features webdriver --all-targets` **EXIT=101**。
**默认门禁不编译 feature 分支**，所以 fmt/clippy/check/三套件全绿也照样漏掉 → `pnpm tauri:build:webdriver` / `pnpm e2e` 整条 E2E 流水线会红。
→ **已把 `cargo check -p datazen --features webdriver --all-targets` 补进合流门禁集**（§6），并要求 A 轨自验也跑这一条。**这是本轮最有价值的发现之一：门禁集自身的盲区比实现缺陷更危险。**

其余缺陷处置：
- `D2`（端口与架构文档的"跨组织访问"措辞只覆盖读路径，写路径实为 `StaleClaim`）→ **要求 A 修**（两处）。
- `D3`（`renew` 的组织兜底只写在内存实现，端口契约零注释、SQLite 侧零注释，且没出现"claim fencing"字样）→ **要求 A 修**（端口 + SQLite 两侧）。
- `D4`（`persistence-model.md` 的 `SCHEMA_VERSION` 3 vs 4）→ Tester 用 `git merge-base --is-ancestor 4fbee80f7 5ea292d8a^` **独立证实早于本轨** → Wave 4 文档收口。
- `D5`（`progress.md` 已入库）→ 合并收尾删除。
- `D6`（claim 写路径的"存在性预言机"：不存在→`NotFound` vs 属他组织→`StaleClaim`，可区分 jobId 是否存在，与 CM-05 不完全吻合）→ **SQLite 侧改前即如此**，内存侧本次忠实镜像 → 既有设计取舍 → Wave 4。
- `D7`（SQLite 侧 `admission.rs:44` 的盖章行**非承重**，真正承重的是 INSERT 绑定 `admission.rs:107`；内存侧同名行承重）→ **要求 A 加注释说明**（低成本，防后来人误判）。
- `D8`（`accept` 重复 jobId 新行为合并前**零覆盖** —— 整段删掉 630 条 runtime 用例仍全绿）→ **已由 Tester 的 `f6c089372` 补上**。

**Tester 对 A 自述的校正（已采信）**：`accept` 从"静默覆盖"改报错**接受**。改前确实是裸 `inner.jobs.insert()` 覆盖整行、且**根本没有组织概念** —— 覆盖之所以变成"跨组织顶替入口"是**本轨引入组织维度后的必要收口**，不是无关收紧。且 `InMemoryJobRepository` 在**生产代码里只以 `#[cfg(test)]` 出现**（桌面走 SQLite，其重复 jobId 改前就已报错）→ **桌面行为零变化**。

### Wave 2 派发时补进各轨简报的三条纪律

1. **门禁必须含 `cargo check -p datazen --features webdriver --all-targets`** —— A 轨的阻塞缺陷 D1 正是"默认门禁不编译 feature 分支"造成的，这条现在是四轨的必跑项。
2. **A 轨待合入会带来 7 个文件的机械重叠**（`packages/schema-diff/tests/job_handler.rs`、`packages/data-transfer/src/job/tests/kernel_cancel.rs`、`commands/data_transfer/job_api/{queries,runtime}.rs`、`commands/data_transfer/job_api/tests/job_lifecycle.rs`、`commands/schema_diff/job/desktop.rs`、`commands/sync/jobs.rs`）—— A 在这些文件上只是「补 `organization_id` 字段」。E/F 两轨已被告知：**不要改这些文件的 job 相关部分**，若非改不可必须保持增量并在报告中点名，便于合流解冲突。
3. **F 轨的强制约束**（防"把解耦做成搬家"）：`comparison_store/disk.rs`（494 行、32 处 `std::fs`）**留在包内**，端口只收「存储根/实例」注入；`SyncPlanStore`/`TransferPlanStore` 的 `static STORE` 改成宿主注入的实例，HashMap 实现本身留在包内。**判定标准：`packages/data-{sync,transfer}` 改动前后总行数基本不变。**

### A 轨修-测第 1 轮（编码代理 `951f5ef68`）

D1–D5 全部处置，并 cherry-pick 上一轮 Tester 的三笔测试提交（`d522efeba` / `d87569b0e` / `78d0cc128`）。自验全绿，**含新增的 `cargo check -p datazen --features webdriver --all-targets` → EXIT=0**。

| 项 | 处置 | 要点 |
|---|---|---|
| D1（阻塞） | `src-tauri/src/commands/dashboard.rs` 的 feature 门控 `JobFilter` 字面量改为显式 6 字段 | **根因已固化为守卫**：`d87569b0e` 全仓扫描，任何缺 `organization_id` 的 `JobFilter` 字面量即报错并给出行号 —— feature 门控分支再也藏不住 |
| D2 | `ports/job.rs` 与 `shared-boundaries-and-ports.md` 两处措辞拆成「**读路径按不存在** / **写路径由 claim fencing 兜底按 StaleClaim**」 | 两处同步；端口 trait 内各方法的行级注释本就是读路径口径，未改 |
| D3 | `renew` 的组织兜底：端口契约 + SQLite `renew_claim` + 内存 `renew` 三处补注释 | 均出现「claim fencing」字样，并注明**续约自身不做组织比对** |
| D4 | `admission.rs` 盖章行注明「SQLite 侧是防御性冗余，承重处在 INSERT 绑定」；内存侧镜像注明承重 | 选注释而非加断言，理由：**可观测不变量已由既有 host 用例** `admission_stamps_the_context_organization_onto_the_persisted_row` 钉住（它伪造 `definition.organization_id = "forged-org"` 并断言落库列 == ctx 组织）；`admit_job` 私有且 `definition` 在返回路径上被 `read_record` 重建 → **该行本身黑盒不可观测**，再加断言只能测实现细节 |
| D5 | `git rm progress.md` | 台账已销毁 |

**复测（全新 Tester `64877b2a`）的重点**：
1. **守卫是否非空洞** —— 变异验证：删掉某处 `JobFilter` 的 `organization_id` → 必须失败且给对行号；**把 `organization_id` 改成常量 → 也必须失败**（若守卫只查"字段存在"不查"值来自上下文"，它挡不住"绕过隔离"这一类夹带）；以及守卫自身有无易绕过手法。
2. **D3 的注释是不是事实陈述** —— 注释现在声称「`renew` 自身不做组织比对，由 claim fencing 兜底」。必须核对 SQLite `renew_claim` 的 SQL **真的没有** `organization_id` 条件、且 claim 真的只在按 ctx 组织核对过的行上铸出。**若因果链有一环不成立，这是"文档说谎"，比措辞瑕疵严重。**
3. D4 的注释与变异结果是否一致（删掉盖章行后既有 host 用例是否仍通过 —— 上一轮 Tester 说会通过，若这次变红则上一轮结论错、注释就是错的）。
4. 三笔 cherry-pick 的测试提交**在 cherry-pick 途中未被削弱**（原提交在 `track/a-tester-commits`，已随 A cherry-pick 完成而删除锚定分支，但 A 分支内已含其内容，原始对象仍在 git 对象库中直到 gc）。
5. 组织隔离完整性未被修复破坏（内存/SQLite 错误形状仍完全一致、`list` 双条件过滤仍在）。

### E 轨交付（编码代理 `33416cb72`）—— 交付，待 Tester 复测

`packages/schema-diff/src/reviewed.rs` 的 `static PLANS` → 存储端口 `ReviewedPlanStore`（`put`/`get`/`remove`，方法带 `scope: &OrganizationScope`），实例由宿主组装期创建并注入（`src-tauri/src/commands/schema_diff/reviewed_store.rs`，桌面单租户进程内存）。包内**规则**（身份指纹 / 快照复核 / 目录比对 / 有效期）全留。

**协调者已独立核实**：TTL 30min 与上限 128 **不是新增行为** —— 改造前 `main:reviewed.rs:260` 有 `plans.retain(|_, p| p.created.elapsed() < Duration::from_secs(1800))`、`:262` 有 `if plans.len() >= 128`，本次只是把魔数提取成命名常量 `REVIEWED_PLAN_TTL` / `REVIEWED_PLAN_MAX_ENTRIES`。

**但有一处真正的语义改动**，已列为 Tester 的重点：冻结时刻从 `Instant::now()`（单调、不可持久化）改成 Unix 毫秒（可持久化、**受系统时钟影响**）。改造的必要性成立（端口化后存储层要自己按 TTL 裁剪，必须知道冻结时刻，且规则层与存储层必须读同一份时刻），但**退化面**需独立评估：时钟前跳会让刚冻结的计划立刻过期、回拨会让本该过期的重新有效。Tester 还被要求核实「存储层裁剪与规则层判定是否读同一份时刻」以及 `created_at_ms` 的**单位换算无 off-by-1000**。

**E 的待裁定项裁决**：桌面评审计划**保持进程内存，不落 SQLite**。理由：改造前就是进程内存，落盘会让「一次性评审计划」变成跨会话残留，需要额外处理过期与并发消费 —— 那是行为变更，不该由端口化顺带引入。

**E 轨 Tester 复测重点**（`0da32120`）：
1. **一次性消费语义**（端口化最容易改坏的地方），三条各做变异验证：校验失败**不烧掉计划**、并发**只有一个删除成功**、**取出后再校验**（避免两次调用间的竞态窗口）。
2. 跨组织隔离的**读/写/删三条路径**逐条精确断言，且**不得**返回「无权限」。
3. 规则函数签名与函数体逐字未变。
4. **A 轨重叠文件未触碰**（独立核对 diff）。

## 6. 门禁与流程

### 每轨 coder 自验 + tester 独立复跑
```
cargo fmt --all -- --check
cargo clippy -p <涉及 crate> --all-targets          # 0 errors
cargo check -p <涉及 crate> --all-targets
DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p <crate> --lib
```
### 每次合流后（集成分支）健全性校验
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cargo check --workspace --all-targets
cargo check -p datazen --features webdriver --all-targets   # ← 必须有：feature 门控代码不被默认门禁编译
DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib            # 基线 1265 passed
DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-feature-core --lib # 基线 367
cargo test -p datazen-platform-api --lib
cargo test -p datazen-data-sync --lib / -p datazen-data-transfer --lib / -p datazen-schema-diff --lib   # 235/226/148
```
**门禁首尾各记一次 HEAD 与工作区 sha**，证明运行期间无人改动。

### 修-测循环（缺陷时）
`Tester 完整上报缺陷 + TEST_FAILED` → resume **原 coder**（带完整缺陷清单 + "仅修这些，不做范围外改动"）→ **全新 Tester** 完整复测 → 通过才闭环。单轨最多 **5 轮**，超限时向用户汇报并给出选项与代价。coder 自验**不能**替代 tester 复测。

### 合流操作（每轨通过验收后，在 `.worktrees/datazen-storage-ports`）
```
git merge --no-ff feature/<track> -m "merge(<track>): ..."
# 解冲突 → 跑上面的合流门禁 → 提交
git worktree remove --force .worktrees/datazen-<track>
git branch -d feature/<track>
git worktree remove --force .worktrees/verify-<track>   # 验证树用完即删
```

---

## 7. Wave 3：R 阶段与交付

1. **全量回归**（一次性，在集成分支）：Rust 全套门禁 + 六个驱动套件 + 前端 `./node_modules/.bin/tsc --noEmit -p tsconfig.json` 与 `./node_modules/.bin/vitest run`（基线 608 files / 6341 tests）。
2. **核验 §1 四条成功判据**，逐条给命令与命中数（不接受"看起来没问题"）。
3. **IPC 表面核对**：`#[tauri::command]` 数量 275 与 `invoke_handler` 270 条目须与 main 逐字节一致；若某轨动到，必须像 PR #81 那样把差异分类为"类型路径拼写"或"有意的契约变更"并举证。
4. **文档收口**：把结论改写为"已实现的事实"并入 `docs/architecture/`（候选：`shared-boundaries-and-ports.md`、`connection-management.md`、`backend/services.md`、`backend/workflow.md`、`platform/system-overview.md`）；反向 grep 清掉指向已改名/已删除文件的引用；自查 markdown 相对链接可达（CI 不校验链接）。
5. **删台账**：`progress.md`（各轨已随分支合入，合流后逐一删除）与 `hub.md` 必须在交付提交里消失。
6. 合入 `main` → push → `gh pr create --base main`（或本 initiative 的 PR）→ 清理全部 worktree 与分支 → 释放磁盘。

## 8. Wave 4：后续 backlog（**不在本 initiative 目标内，勿顺手做**）

| 项 | 实测落点 | 说明 |
|---|---|---|
| 搬宿主剩余纯规则 | 毛量 **3,399 行**：`job_api/admission.rs` 100（自述 pure）、`job_api/assembly.rs` 538、`schema_diff/job.rs` 700、`sync/jobs.rs` 879（DispatchClaim/preaccept）、`sync/jobs_contract.rs` 537、`sync/jobs_cancel_contract.rs` 530、`schema_diff/unified_plan/revalidation.rs` 115 | 真能搬约 1,000–1,600；**是搬家不是删除** |
| `workspace-apps` 端口化 | `packages/workspace-apps` **0 个 port trait**；`manager.rs`/`storage.rs`/`install.rs` 直绑 `std::fs`+`PathBuf` | web 支持插件的前置：`WappCatalog` + `WappStore`；规则侧（manifest/zip 守卫/白名单）已共享，不用动 |
| 残留转发门面清扫 | `commands/sync/mod.rs:6`（归 F）、`wapps/mod.rs:26-34`（实测仅 `validate_wapp_dir`、`Permission` 两名仍在用）、`monitor/channels.rs:14`、`lib.rs:8/20/27` | 小、低风险 |
| **先存 CI flake（新发现，优先）** | `src-tauri/src/platform/live_source_tests.rs:481-482` 的 `std::env::set_var("DATAZEN_DESKTOP_USER", ...)` 是**进程级全局**竞态 → `platform::live_source::tests::*` 偶发 `PermissionDenied: connection permission denied` / `target delegation scope denied`。**已用基线证伪与本 initiative 无关**（回退 6 个文件后 5 次跑仍复现）。修法：把该 env 写入收进进程内串行守卫，或改用注入点而非 env | 会让 CI 周期性红（ Tester 5 次跑中第 4 次命中） |
| 其余 4 个未实现端口 | `EventSink`、`SubmissionTokenIssuer`、`ClusterNodeBudgetPort`、`ControlResourcePort` | 与 web 多实例/预算治理相关，另立 initiative |
| 线协议字面码去形态化 | `route.rs` 的 4 个 reason code + `desktop-tunnel/` 前缀；`ExportSink::DesktopDialogHandle` | 契约变更，需迁移说明 |
