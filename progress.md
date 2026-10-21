# 轨道 B（di-dyn）进度台账

> 目标：把 `packages/feature-core` 里「存储/端口」的注入方式从泛型（编译期单态化）改为
> `Arc<dyn Trait>`（运行期可替换），使宿主实现类型不再渗进共享包的类型签名。
> 分支 `feature/di-dyn`；基线 `8bf6eb553`（基于 `feature/storage-ports`）。
> 本文件是开发期台账，**验收合并时必须删除**（AGENTS.md「进度台账：开发期间允许，交付即销毁」）。

## 已完成

- [x] **feature-core 去泛型**：`packages/feature-core/src/workflow/registry.rs`
  - `pub struct WorkflowRegistry<S: WorkflowDefinitionStore>` → `pub struct WorkflowRegistry`，
    内部持有 `Arc<dyn WorkflowDefinitionStore>`；`impl<S: …> WorkflowRegistry<S>` → `impl WorkflowRegistry`。
  - `WorkflowRegistry::new(store: Arc<dyn WorkflowDefinitionStore>, builtins: Vec<(&'static str, &'static str)>)`。
  - `store()` 访问器返回 `&dyn WorkflowDefinitionStore`（不再返回宿主具体类型 `&S`）。
  - 测试夹具 `MemoryStore` 改为 `Arc::new(MemoryStore::default())`；需要访问具体类型的两处
    （`the_builtin_workflows_are_seeded_once` / `a_dashboard_builtin_is_published_as_a_user_workflow`）
    改为测试自己持有 `Arc<MemoryStore>` 句柄，断言内容与顺序不变。
- [x] **宿主绑定收敛**：`src-tauri/src/workflow/registry.rs`
  - `AppDbWorkflowStore` 由私有 struct 改为 `pub(crate)`，新增构造入口
    `pub(crate) fn new(app_db: Arc<AppDb>) -> Self`（宿主在组装根构造并注入）。
  - 宿主侧别名 `CoreRegistry` 去掉类型参数：`core: CoreRegistry`（不再是 `CoreRegistry<AppDbWorkflowStore>`）。
  - 宿主 `WorkflowRegistry::new(store: Arc<dyn WorkflowDefinitionStore>, data_dir: PathBuf)`——
    **不再**自行持有 `Arc<AppDb>`；原 `app_db()` 访问器删除（唯一调用点改为读 `state.store.app_db()`，
    与组装根是同一个 `Arc<AppDb>`，行为不变）。
- [x] **组装根**：`src-tauri/src/bootstrap/app_state.rs:205`
  - `workflow_registry: Arc::new(workflow::WorkflowRegistry::new(Arc::new(workflow::registry::AppDbWorkflowStore::new(app_db.clone())), data_dir.clone()))`
  - 核实过 `new` 的真实签名：宿主 `WorkflowRegistry::new` 第二参是 `data_dir`（内部 `join("workflows")`），
    共享 `CoreRegistry::new` 第二参才是 `builtins`（宿主传 `BUILTIN_DEFINITIONS.to_vec()`）。
- [x] **调用点**：`src-tauri/src/commands/workflow.rs`（`workflow_get_yaml` 改读 `state.store.app_db()`）、
  `src-tauri/src/commands/context.rs`（2 处测试装配）、`src-tauri/src/dashboard/create.rs`（测试装配）。
- [x] **范围内顺带清理**：`packages/feature-core` 全仓 `pub struct` 复查——除本次的
  `WorkflowRegistry` 外，**没有任何**带类型参数的公开 struct；带泛型的公开函数全是纯计算
  （`parse_*` / `encode_*` / `value_as_string` 等），无需 dyn 化。
- [x] **文档同步**：
  - `docs/architecture/backend/workflow.md`：共享模块树补上 `registry.rs` 一行（此前共享包树里
    漏列了它，读者会误以为规则仍在宿主），并写明以 `Arc<dyn WorkflowDefinitionStore>` 注入。
  - `docs/architecture/platform/workflow-resource-model.md:67`：本轨道改动了
    `src-tauri/src/workflow/registry.rs`，其 `version: None` 行号由 184 漂移到 190，已同步。

## 进行中 / 未开始

- 无。未开始项：无。

## 门禁实测

### A. 隔离 target 目录（`CARGO_TARGET_DIR=/tmp/datazen-di-dyn-target`）——本轨道的权威结果

| 命令 | 退出码 | 结论行（逐字） |
| --- | --- | --- |
| `cargo fmt --all -- --check` | `0` | 日志为空（0 字节） |
| `cargo clippy -p datazen-feature-core -p datazen --all-targets` | `0` | `error: 0`；`warning: 428`（全部为改动前既有告警；本轨道 5 个改动文件无新增告警，仅 3 处既有告警落在其中：`commands/workflow.rs:120` redundant_closure、`dashboard/create.rs:51` too_many_arguments、`commands/context.rs:308` manual_contains） |
| `cargo check -p datazen-feature-core -p datazen --all-targets` | `0` | `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 1m 32s` |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-feature-core --lib` | `0` | `test result: ok. 367 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.98s` |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib` | `0` | `test result: ok. 1265 passed; 0 failed; 9 ignored; 0 measured; 0 filtered out; finished in 11.37s` |

**已知 flake 的实测**：某一次全量宿主测试在机器高负载（另一轨道同时在编译）时出现 1 例失败——
`platform::native_tool::tests::native_desktop_output_overflow_is_bounded_killed_and_reaped`
（`left: TimedOut` / `right: OutputLimit`，`src-tauri/src/platform/native_tool/tests.rs:87`），
与本轨道改动无关（该用例测的是原生工具输出上限回收，不碰 workflow registry）。
按 AGENTS.md 的既定处置单跑重试：`cargo test -p datazen --lib platform::native_tool::tests::native_desktop_output_overflow_is_bounded_killed_and_reaped`
→ `test result: ok. 1 passed; 0 failed; ...`（退出码 `0`）；随后全量重跑 → `1265 passed; 0 failed`（退出码 `0`）。

### B. 共享 target 目录（`CARGO_TARGET_DIR=.../.worktrees/.shared-target`）——**被并行轨道污染，非本轨道缺陷**

| 命令 | 退出码 | 结论 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | `0` | 通过（fmt 与 target 目录无关） |
| `cargo clippy -p datazen-feature-core -p datazen --all-targets` | `0` | 通过（clippy 产物指纹独立，未被污染） |
| `cargo check -p datazen-feature-core -p datazen --all-targets` | `101` | **失败，但错误全部来自并行轨道的产物**（见下） |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-feature-core --lib` | `0` | `test result: ok. 367 passed; 0 failed; ...`（test profile 产物指纹独立，未被污染） |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib` | `0` | `test result: ok. 1265 passed; 0 failed; 9 ignored; ...` |

**污染证据（逐字）**：共享目录里 `cargo check` 的报错路径指向的是**另一个工作树**的源码：

```text
error[E0063]: missing field `organization_id` in initializer of `JobDefinition`
   --> packages/feature-core/src/restore_job.rs:655:37
error[E0107]: missing generics for struct `datazen_feature_core::workflow::registry::WorkflowRegistry`
  --> src-tauri/src/workflow/registry.rs:120:11
   = note: struct defined here, with 1 generic parameter: `S`
    --> /Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-job-org-scope/packages/feature-core/src/workflow/registry.rs:95:12
```

- 本工作树 `packages/platform-api/src/dto/job.rs` 的 `JobDefinition` **没有** `organization_id`
  （`grep -c organization_id` = 0）；`packages/feature-core/src/workflow/registry.rs` 的 `WorkflowRegistry`
  在本轨道已无类型参数——报错里被引用的 `S` 泛型版本来自
  `.worktrees/datazen-job-org-scope/packages/feature-core/src/workflow/registry.rs:95`（该工作树**未**做本轨道改造）。
- 成因：`datazen-platform-api` / `datazen-feature-core` 是 path 依赖，三条轨道在**同一个**
  `CARGO_TARGET_DIR` 里编译时，cargo 的单元指纹/产物文件名不区分工作树（实测
  `.shared-target/debug/.fingerprint/datazen-platform-api-*` 的 `path` 哈希在多个工作树间相同），
  于是后写的轨道覆盖了先写的轨道产物；后跑的轨道看到「产物比源文件新」就判定为 fresh，
  直接链接**别人工作树**的 rlib。
- 影响面：共享 target 目录下的 `cargo check` 在三条轨道同时改同一批共享包时**不可信**；
  `cargo test`（test profile）与 `cargo clippy`（独立指纹）本次未受影响。
- 建议（需协调者裁定，本轨道未擅自更改约定）：三条轨道各用各的 `CARGO_TARGET_DIR`；
  若必须共享，则每条轨道在跑门禁前 `touch` 自己工作树的共享包源码以强制重建。
  本轨道为此额外跑了一次隔离 target 目录的全量门禁（A 节），结论全绿。

附加核查（非门禁，但对应验收标准第 1/2/3 条）：

- 验收 1（共享包公开签名不再出现宿主实现类型作为泛型参数）：
  `grep -n "^pub struct" packages/feature-core/src/**/*.rs` → 96 处公开 struct，**0 处**带类型参数；
  `WorkflowRegistry` 已无类型参数。
- 验收 2（共享包不依赖宿主 crate）：
  `cargo tree -p datazen-feature-core | grep datazen` → 只出现
  `datazen-ai-api / application / data-sync / data-transfer / driver-api / platform-api / platform-infra / runtime / schema-diff / migration-common`，
  **无宿主 crate `datazen`、无 `tauri`**。
- 验收 3（宿主 `AppState.workflow_registry` 不泄漏 store 类型）：
  `src-tauri/src/commands/mod.rs:167` 仍是 `pub workflow_registry: Arc<WorkflowRegistry>`，
  `WorkflowRegistry` 是宿主自己的壳（字段私有：`core: CoreRegistry` + `workflows_dir`）。

## 遗留与待裁定

1. **共享 `CARGO_TARGET_DIR` 的跨工作树产物污染（需协调者裁定，优先）**：证据见上文 B 节——
   共享目录下 `cargo check` 会链接到 `datazen-job-org-scope` 工作树的 `feature-core`/`platform-api` 产物，
   报出本轨道不可能产生的错误。三条轨道同时改共享包时，共享 target 目录的 `cargo check` 结论不可信。
   建议各轨道改用各自的 `CARGO_TARGET_DIR`（本轨道已用 `/tmp/datazen-di-dyn-target` 跑过一遍全量门禁，全绿）。
2. **共享包模块树补全（超出本轨道范围，未做）**：`docs/architecture/backend/workflow.md` 的
   `packages/feature-core/src/workflow/` 树里除本次补的 `registry.rs` 外，仍缺 `error.rs`、
   `executable.rs`、`history.rs`、`target_plan.rs`（`results.rs` 为私有模块，未列）。
   属于文档欠账，建议由文档归口轨道统一补，本轨道不越界改。
3. **`workflow-resource-model.md:67` 的历史行号漂移（未改）**：同一句里
   `workflow/host.rs:127/168/203`（实际 128/169/204）、`commands/dashboard.rs:460`（实际 463）、
   `commands/ai/integration_tests.rs:372/923/1057/1106`（实际 372 为 `Some("1")`，另三处实际 924/1058/1107）
   都是**本轨道之前**就存在的漂移，涉及的文件本轨道没有改动，故未一并修正。
   本轨道只修正了自己改动的 `workflow/registry.rs:184 → 190`。
4. **未新增 `Arc<dyn …>` 之外的运行期可替换能力**：共享 `WorkflowRegistry` 的 store 在构造后不可换；
   「运行期可替换」在本轨道体现为**注入点由宿主决定**（宿主可选任意实现、可用不同实现构造不同 registry），
   不含运行期热替换 store 的 API。若需要热替换，应另开需求（`ArcSwap` / 构造新 registry）。
