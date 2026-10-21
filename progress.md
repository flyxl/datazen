# 轨道 D：feature-core 组织作用域（feature/featurecore-org-scope）

> 交付即销毁：本文件随分支流转，验收合并时必须删除。

## 目标

feature-core 的三个存储 trait（`DashboardStore` / `DashboardRunStore` / `WorkflowDefinitionStore`）
加上 `OrganizationScope`，使多租户宿主能在**同一个存储实现**上区分「本组织的数据」与
「别的组织的数据」；桌面形态是单租户，只承认自己那一个组织。

## 已完成

### 1. feature-core（共享层，规则 + 端口）

- `packages/feature-core/src/dashboard/store.rs`
  - `DashboardStore` 的 **10 个方法**全部加 `scope: &OrganizationScope`（`&self` 之后第一个参数）。
  - 规则层自由函数 `list_dashboards` / `get_dashboard` / `save_dashboard` / `delete_dashboard` /
    `set_dashboard_refresh_paused` / `find_workflow_refs` / `assign_import_id` 全部把 scope 传下去。
  - 内部 `widgets_of` 同样带 scope。
- `packages/feature-core/src/dashboard/runs.rs`
  - `DashboardRunStore` 的 3 个方法全部带 scope；`write_run` / `list_run_index` / `get_run` 带 scope。
- `packages/feature-core/src/dashboard/create.rs`
  - `WidgetCreationHost` 的 5 个方法全部带 scope；三个建 widget 的自由函数带 scope。
  - **额外**（不在原任务清单内，但同属「读/写已存数据」的公开端口，留着就是越权旁路）：
    `WidgetCreationHost` 若不带 scope，多租户宿主就能绕过 `DashboardStore` /
    `WorkflowDefinitionStore` 直接读别的组织的仪表板与 workflow。
- `packages/feature-core/src/workflow/registry.rs`
  - `WorkflowDefinitionStore` 的 4 个方法全部带 scope。
  - `WorkflowRegistry` 的 8 个公开方法（`get` / `list` / `list_definitions` / `save` / `save_yaml` /
    `delete` / `load_all` / `ensure_loaded`）全部带 scope。
  - **内置 workflow 的播种标志由全局一个 `AtomicBool` 改为按组织的 `HashSet<OrganizationId>`**：
    全局标志会让第二个组织永远拿不到自己的内置 workflow（多租户下第二个组织列表为空）。

### 2. 桌面宿主（单租户，只承认自己那一个组织）

- 新增 `src-tauri/src/platform/scope.rs`
  - `local_organization_scope()`：桌面自己的组织作用域（组织 id = `LOCAL_ORGANIZATION_ID`，
    与 `DesktopIdentity::organization_id()` 一致：桌面形态唯一的生产构造入口
    `DesktopIdentity::from_desktop_user` 把组织钉在该常量上）。
  - `is_local_scope(scope)`：作用域是否属于桌面自己那一个组织。
- `src-tauri/src/dashboard/store.rs` / `runs.rs` / `src-tauri/src/workflow/registry.rs`
  - 三个 `impl` 全部加 org 闸门：别的组织的作用域 → 空列表 / `None` / `NotFound`；
    **不返回任何「无权限」类错误**（CM-05）。
  - 写操作（`upsert_*` / `delete_*`）对别的组织一律 `NotFound`；workflow 的 `delete_workflow`
    按其既有契约「删除不存在的 id 不是错误」返回 `Ok(())`。
  - **SQLite 没有加 `organization_id` 列**：桌面是单租户，schema 不变。
- `src-tauri/src/commands/mod.rs`：`AppState::organization_scope()`——IPC 路径的作用域来自
  adapter 组装的请求上下文（`OrganizationScope::from_context(&ctx)`），命令参数无法覆盖身份。
- 无请求上下文的桌面内路径（后台 monitor、MCP、调度器）用 `local_organization_scope()`。
- `src-tauri/src/monitor/scheduled.rs`：定时执行的运行记录用 grant 自带请求上下文的作用域。

### 3. 跨组织用例（新增 17 条）

feature-core（规则层，10 条）：

- `dashboard/store.rs`：`a_dashboard_of_another_organization_is_not_found`、
  `the_widgets_of_another_organization_are_not_found`、
  `two_organizations_keep_their_own_dashboards_apart`、
  `a_dashboard_of_another_organization_cannot_be_changed_or_removed`
- `dashboard/runs.rs`：`the_runs_of_another_organization_are_not_found`、
  `two_organizations_keep_their_own_runs_apart`
- `workflow/registry.rs`：`the_workflows_of_another_organization_are_not_found`、
  `two_organizations_keep_their_own_workflows_apart`、
  `a_workflow_of_another_organization_cannot_be_removed`、
  `the_builtin_workflows_are_seeded_per_organization`

宿主（存储隔离，7 条）：

- `dashboard/store_tests.rs`：`a_dashboard_of_another_organization_is_not_found`、
  `the_widgets_of_another_organization_are_not_found`、
  `a_dashboard_of_another_organization_cannot_be_changed_or_removed`、
  `the_workflow_refs_of_another_organization_are_not_found`
- `dashboard/runs.rs`：`the_runs_of_another_organization_are_not_found`
- `workflow/registry.rs`：`the_workflows_of_another_organization_are_not_found`、
  `a_workflow_of_another_organization_cannot_be_removed`

断言一律逐条比对空/None（`assert!(...is_empty())` / `assert!(...is_none())` /
`assert!(matches!(..., Err(NotFound(_)))`），**没有** `assert!(x.is_err())` 这类模糊断言。

### 4. 文档

- `docs/architecture/platform/workflow-resource-model.md`：本轨道重构后行号漂移，
  同步更新了 `version` 字面量位置的行号引用（内容结论不变）。
- `docs/architecture/platform/shared-boundaries-and-ports.md` 全文没有提到这三个存储 trait，
  无失效描述，未改。

## 门禁实测（HEAD = 65dac1e2cf79fe7099b57988ca4ecfc7095e2ee1）

| 命令 | 退出码 | 结论行 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | 无输出 |
| `cargo clippy -p datazen-feature-core -p datazen --all-targets` | 0 | 无 error；本轨道新增 0 条 warning |
| `cargo check -p datazen-feature-core -p datazen --all-targets` | 0 | 无 error |
| `cargo check -p datazen --features webdriver --all-targets` | 0 | 无 error |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-feature-core --lib` | 0 | `test result: ok. 377 passed; 0 failed; 0 ignored` |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib` | 0 | `test result: ok. 1273 passed; 0 failed; 9 ignored` |

### 基线（同一命令、同一工作树，stash 后在 HEAD 实测）

| 包 | HEAD 基线 | 本轨道之后 | 增量 |
| --- | --- | --- | --- |
| feature-core | 367 passed | 377 passed | +10（本轨道新增） |
| host | **1266** passed | 1273 passed | +7（本轨道新增） |

> 任务书写的 host 基线是 1269；本轨道用 `git stash` 在 HEAD 上实测为 **1266 passed; 0 failed;
> 9 ignored**。差 3 条不影响结论（0 failed，且 +7 与本轨道新增的 7 条宿主用例一一对应），
> 但**基线数字以实测为准**，任务书的 1269 需上游复核。

### 既有断言是否被改动

- 宿主侧**没有任何断言被改写**：所有改动都只是给调用点补上 `&scope` 参数
  （例如 `assert!(list_dashboards(&*db).unwrap().is_empty())` →
  `assert!(list_dashboards(&scope, &*db).unwrap().is_empty())`）。
- feature-core 侧有 2 条断言的写法随内存存储改造变化（内存存储现在按组织分区，
  记录是 `(OrganizationId, record)` 元组）：`assert_eq!(records[0].visibility, ...)` →
  `assert_eq!(records[0].1.visibility, ...)`；断言的**语义不变**。

### clippy

本轨道**新增 0 条 warning**。3 个函数因新增 scope 参数越过 clippy 的参数个数阈值，
按仓库既有惯例（仓库里有 41 处 `#[allow(clippy::too_many_arguments)]`）显式标注：

- `packages/feature-core/src/dashboard/create.rs::create_widget_from_sql`（7 → 8 个参数，**新触发**）
- `src-tauri/src/dashboard/create.rs::create_widget_from_sql`（8 → 9 个参数，原已触发）
- `src-tauri/src/dashboard/execute.rs::persist_error_run`（8 → 9 个参数，原已触发）

## 待裁定项

1. **IPC 命令现在依赖 platform adapter**：仪表板 / workflow 命令的作用域取自
   `AppState::organization_scope()`，adapter 组装失败时这些命令会明确失败
   （`CommandError::Internal`），而改造前它们不依赖 adapter。桌面形态下 adapter 只在
   「取不到当前 OS 用户名」时组装失败（`local_principal()` 返回 `None`），
   且宿主测试里已有命令（`wapps/tests.rs`、`sync/tests.rs`）本就要求 platform ready。
   若上游认为「仪表板命令不应因身份不可用而不可用」，可以把作用域改为
   `local_organization_scope()`，代价是失去「作用域来自 adapter 组装的上下文」这条约束。
2. **`WidgetCreationHost` 也被加了 scope**（超出任务书列出的三个 trait）。
   理由已写在上面的「已完成 §1」里：不加就是越权旁路。若上游认为应另开一轨处理，回退成本很低。
3. **内置 workflow 播种改为按组织**：全局标志会让多租户下第二个组织拿不到内置 workflow。
   这一改动同时改变了「播种次数」的语义（同一 registry 的不同组织各自播种一次）。
4. **宿主基线 1269 vs 实测 1266**：请上游复核基线数字的来源。

## 未开始 / 不在本轨道范围

- 未给桌面 SQLite 增加 `organization_id` 列（按要求：桌面单租户，schema 不变）。
- 未改动 `packages/platform-api/**`（`OrganizationScope` 词汇已备好）、`packages/runtime/**`、
  `packages/schema-diff/**`、`packages/data-{sync,transfer}/**`、
  `src-tauri/src/commands/{sync,data_transfer,schema_diff}/**`。
