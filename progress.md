# F2 轨进度台账：data-sync 的计划与比较存储端口化

> 本文件是并行开发期间的进度台账，**交付合并时必须删除**。
> 分支：`feature/sync-store-ports`（基线 `65dac1e2c`）。

## 目标

把 data-sync 的两处存储改成**端口 + 宿主注入的实例**，去掉进程级全局可变状态：

1. `packages/data-sync/src/comparison_store/`：落盘实现**留在包内**，只把「存储根从哪来」变成构造参数。
2. `packages/data-sync/src/plan_store/`：删除 `static STORE`，改为 `SyncPlanStore` 端口 + 包内 `InMemorySyncPlanStore` 实现。
3. 端口方法带 `datazen_platform_api::ports::scope::OrganizationScope`；越组织访问按「**不存在**」处理（CM-05），不得返回「无权限」。

## 已完成

| 步骤 | 内容 | 状态 |
|---|---|---|
| ① | `comparison_store` 端口化（包内）：新增 `comparison_store/port.rs`（`ComparisonStoreRoot` 端口 + `LocalComparisonStoreRoot` 实现 + `default_comparison_store_root()`）；`ComparisonStore::from_comparison` / `StreamingComparisonStoreWriter::new` 改为接收根与作用域；`recovery.rs` 的 owner 租约缓存改为**按根**缓存（原先是「首个根赢」的无键全局） | ✅ |
| ② | `SyncPlanStore` 端口化（包内）：新增 `plan_store/store.rs`（`SyncPlanStore` 端口 + `SyncPlanIssue` 请求结构 + `InMemorySyncPlanStore` 实现）；`plan_store/mod.rs` 删掉 `static STORE`、`global_store()` 与 5 个全局自由函数；自由函数 `default_profile_selection` / `preview_for_plan` / `get_comparison_page` 改为接收 `(store, scope)` | ✅ |
| ③ | 宿主接线：`AppState.sync_stores: SyncStorePorts`（组装期创建实例）、`AppState::organization_scope()`、调用点传作用域；删除 `src-tauri/src/commands/sync/mod.rs:6` 的转发门面 `pub(crate) use datazen_data_sync::comparison_store;`；更新 `docs/architecture/backend/data-sync.md` | ✅ |
| ④ | 第 2 轮（修-测循环）：cherry-pick Tester 的验证提交 `11d9e4cca` → `261118e90`；**D-02** 计划注册表的键改成 `(OrganizationId, String)`（组织维度进键，分区成为结构而不是约定）+ 重复签发改为拒绝；**D-04** 比较侧端口文档改成「不可用」以对齐实际措辞（错误文案不动）；**D-06** 文档 §2/§7 改口径 + §3.1 补默认根与回收机制 | ✅ |

## 关键决策

- **落盘实现不搬家**：`disk.rs` / `recovery.rs` / `manifest_cache.rs` 全部留在 `packages/data-sync`。本地文件系统是形态中立的；宿主只注入「根目录」。宿主因此只 +104 行（`src-tauri/src/commands/sync/`），而不是 +500 行。
- **作用域来源**：桌面是单租户，组织维度是进程常量 `platform::identity::LOCAL_ORGANIZATION_ID`。`AppState::organization_scope()` 优先用 `OrganizationScope::from_context(adapter.begin_call().context())`（INV-01：身份只能由 adapter 组装）；桌面身份组装失败（拿不到登录用户）时按**同一个常量**给出作用域——变的是主体，不是「哪个组织」。这样 `env -u USER` 场景下同步存储依然可用，且没有新增失败。
- **`ComparisonStore` 由调用方构造**：计划端口的 `issue` 接收**已冻结的** `ComparisonStore`，因此计划注册表不需要知道比较存储的根；宿主在 `apply.rs` 里用注入的根构造它。
- **计划归属**：键是 `(OrganizationId, String)`（D-02）——组织维度进键，「按组织分区」是**结构**而不是「读出来再比一次 value 上的字段」的约定。`StoredSyncPlan.organization_id` 保留为第二道防线。越组织的 `peek` / `claim` / `attach_endpoint_identities` 落到另一个键，与「不存在」是同一条路径，措辞逐字不变。
- **一个 id 一份计划**：同一个 `(组织, 计划 id)` 重复签发被**拒绝**（不覆盖）；过期的同名计划在签发前先被清掉，因此不占名额；另一个组织用同一个 id 签发仍然成功（占的是另一个格子）。

## 门禁实测（`CARGO_TARGET_DIR` = 本工作树 target）

| # | 命令 | 退出码 | 结论 |
|---|---|---|---|
| 1 | `cargo fmt --all -- --check` | 0 | 干净 |
| 2 | `cargo clippy -p datazen-data-sync -p datazen --all-targets` | 0 | 无新增告警（改动文件零告警） |
| 3 | `cargo check -p datazen-data-sync -p datazen --all-targets` | 0 | 通过 |
| 4 | `cargo check -p datazen --features webdriver --all-targets` | 0 | 通过 |
| 5 | `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-data-sync --lib` | 0 | **242 passed / 0 failed**（235 基线 → +3 本轨 → +2 Tester → +2 D-02） |
| 6 | `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib` | 0 | **1267 passed / 0 failed / 9 ignored**（1266 基线 → +1 Tester） |
| 7 | `env -u USER -u DATAZEN_DESKTOP_USER … cargo test -p datazen --lib` | 101 | **1025 passed / 242 failed / 9 ignored**（失败数 = 基线 242，失败清单逐行相同） |

第 7 条的失败清单与基线**逐行相同**（`diff` 无差异）：242 条全部落在 AI / connection / data / data_transfer / backup 及要求 platform ready 的测试夹具上，**没有新增**。

## 行数对比（验证「没搬家」）

| 范围 | 改前 | 改后 | 差 |
|---|---|---|---|
| `packages/data-sync`（全部 .rs） | 19,108 | 19,592 | **+484** |
| `packages/data-sync/src` | 16,916 | 17,400 | +484 |
| `packages/data-sync/src/comparison_store/` | 2,998 | 3,258 | +260（落盘实现**留在**包内并变大） |
| `packages/data-sync/src/plan_store/` | 2,025 | 2,249 | +224 |
| `src-tauri/src/commands/sync/` | 9,322 | 9,426 | +104（宿主**没有** +500 行） |
| `src-tauri/src` | 98,111 | 98,329 | +218 |

## 新增测试

- `comparison_store::port::tests`：根只对自己的组织可见；**另一个组织拿不到根**（而不是「无权限」）。
- `plan_store::store::tests::another_organization_sees_no_plan_at_all`：跨组织的 `peek` / `claim` / `attach_endpoint_identities` 与「计划不存在」**同词**，且不泄漏存在性；跨组织的 `claim` 不能把计划摘走。
- `plan_store::store::tests::two_organizations_issue_the_same_plan_id_without_overwriting_each_other`（D-02）：两个组织用同一个 `plan_id` 各自签发，互不覆盖。
- `plan_store::store::tests::a_second_issue_under_the_same_plan_id_is_refused_instead_of_overwriting`（D-02）：重复签发被拒绝，原计划仍在。
- Tester 的三条：`comparison_store::tests::another_organization_gets_no_comparison_storage_at_all`（越组织失败关闭且不触碰文件系统）、`comparison_store::tests::owner_lease_is_cached_per_root_so_each_root_can_reclaim_its_own_stale_stores`、`commands::sync::tests::desktop_sync_store_ports_admit_only_the_local_organization`。

## 遗留 / 待裁定

- **F1 的前提需要复核**：协调者说「并行轨 F1 的 `InMemoryTransferPlanStore`（键含组织）同构」。实测 F1 分支（`feature/migration-store-ports` @ `f866f5b1f`）的 `packages/data-transfer/src/plan_store/memory.rs` 仍是 `plans: Mutex<HashMap<String, StoredTransferPlan>>`——**键只有 plan_id**，组织同样是 value 上的字段，靠 `owns(scope, owner)` 辅助函数比较。本轨按裁决把键改成了 `(OrganizationId, String)`，**两条轨目前形状不同**；是否让 F1 也对齐，待协调者裁定。
- **宿主侧还有一组按 plan_id 键的进程内 map**：`src-tauri/src/commands/sync/host/state.rs` 的 `ARTIFACTS` / `SELECTIONS` / `CONFIRMED_SELECTIONS` / `PENDING_PREVIEWS` / `FAILURES`。它们**不是**计划存储端口（是宿主自己的冻结 ChangeSet / 选区 / Job 失败记录），本轮范围外；若将来出现多租户宿主，需要同样的口径复核。
- `docs/architecture/backend/services.md` 未提到计划存储，无需改动（已核对）。
- `get_data_sync_comparison_page` 的 Tauri 命令签名新增了 `State<'_, AppState>`；JS 侧 IPC 入参不变（`{ request }`），命令名不变。
- 中间提交 ①② 只改共享包，**单独 cherry-pick 不能编译**（宿主适配在 ③）。按本轨与并行迁移轨的文件边界拆分，合流时应整体取用。
