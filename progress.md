# 轨道 D：feature-core 组织作用域（feature/featurecore-org-scope）

> 交付即销毁：本文件随分支流转，验收合并时必须删除。

## 目标

feature-core 的三个存储 trait（`DashboardStore` / `DashboardRunStore` / `WorkflowDefinitionStore`）
加上 `OrganizationScope`，使多租户宿主能在**同一个存储实现**上区分「本组织的数据」与
「别的组织的数据」；桌面形态是单租户，只承认自己那一个组织。

---

# 第 1 轮（实现）

## 已完成

### 1. feature-core（共享层，规则 + 端口）

- `packages/feature-core/src/dashboard/store.rs`
  - `DashboardStore` 的 **10 个方法**全部加 `scope: &OrganizationScope`（`&self` 之后第一个参数）。
  - 规则层自由函数 `list_dashboards` / `get_dashboard` / `save_dashboard` / `delete_dashboard` /
    `set_dashboard_refresh_paused` / `find_workflow_refs` / `assign_import_id` 全部把 scope 传下去。
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
    按其既有契约「删除不存在的 id 不是错误」返回 `Ok(())`（语义见 D-02）。
  - **SQLite 没有加 `organization_id` 列**：桌面是单租户，schema 不变。
- `src-tauri/src/monitor/scheduled.rs`：定时执行的运行记录用 grant 自带请求上下文的作用域。

### 3. 跨组织用例（第 1 轮新增 17 条）

feature-core（规则层 10 条）：`dashboard/store.rs` 4 条、`dashboard/runs.rs` 2 条、
`workflow/registry.rs` 4 条；宿主（存储隔离 7 条）：`dashboard/store_tests.rs` 4 条、
`dashboard/runs.rs` 1 条、`workflow/registry.rs` 2 条。断言一律逐条比对空/None
（`assert!(...is_empty())` / `assert!(...is_none())` / `assert!(matches!(..., Err(NotFound(_)))`），
**没有** `assert!(x.is_err())` 这类模糊断言。

### 4. 文档

- `docs/architecture/platform/workflow-resource-model.md`：本轨道重构后行号漂移，
  同步更新了 `version` 字面量位置的行号引用（内容结论不变）。
- `docs/architecture/platform/shared-boundaries-and-ports.md` 全文没有提到这三个存储 trait，
  无失效描述，未改。

---

# 第 2 轮（修-测循环 · 第 1 轮）

独立 Tester 复测结论 **TEST_FAILED**：1 条阻塞缺陷（D-01）+ 2 条非阻塞（D-02 / D-03）。
Tester 留下三笔**只动测试**的提交，本轨道 cherry-pick 进来：

- `2e3a1fc23`（来源 `3a1ffa622f3ae427dc98e8d13be841778ca490ae`，Tester 的
  `test(verify): 组织作用域轨道的验证覆盖（P0/P1/P3/P7）`）：
  `src-tauri/src/platform/scope_tests.rs`（新，5 条）、`src-tauri/src/commands/org_scope_surface_tests.rs`
  （新，2 条 IPC 载荷守卫）、`src-tauri/src/dashboard/create.rs` 的 3 条 `WidgetCreationHost`
  跨组织用例、`src-tauri/src/workflow/registry.rs` 1 条断言加固。**零生产代码改动。**

## D-01【阻塞·行为回归】IPC 命令因「身份不可用」而整体不可用 —— 已修

**根因**：第 1 轮把 IPC 路径的作用域写成
`AppState::organization_scope()` → `platform_require()` → `begin_call().context()`，
于是 adapter 组装失败（「取不到当前 OS 用户名」）时仪表板 / workflow 命令整体失败。

**修法（协调者裁决：启动期确定一次 → 存进 `AppState` → 请求期直接取）**：

1. `AppState` 新增字段 `pub organization_scope: OrganizationScope`，
   值在**启动期**（`bootstrap/app_state.rs::finish_app_state`）算好并写入；
2. `AppState::organization_scope()` **直接返回该字段**，不再走 `platform_require()`。

改后的实现（逐字）：

```rust
    /// 本次 IPC 调用的**组织作用域**。
    ///
    /// 作用域在**启动期**确定一次并存进 [`AppState::organization_scope`]，请求期
    /// 直接取：命令参数里没有任何字段能覆盖组织身份（INV-01）。
    ///
    /// 桌面形态下这个值永远是桌面自己那一个组织（安装级常量），因此本方法
    /// **不依赖** platform adapter 是否组装成功——「取不到当前 OS 用户名」只影响
    /// `principalId`，不影响组织边界；仪表板 / workflow 命令不会因此整体不可用。
    ///
    /// 保留 `Result` 是因为「作用域从哪来」是宿主形态的自由度：多租户宿主可能
    /// 按请求解析组织，那一步是可以失败的。桌面形态不会失败。
    pub fn organization_scope(&self) -> Result<OrganizationScope, CommandError> {
        Ok(self.organization_scope.clone())
    }
```

字段（逐字）：

```rust
    /// 桌面形态的**组织作用域**：**启动期确定一次**，请求期直接取。
    ///
    /// 桌面是单租户：一份 app data 就是一个组织。组织的值是**安装级常量**
    /// （`crate::platform::LOCAL_ORGANIZATION_ID`），与「当前 OS 用户名能不能取到」
    /// **无关**——后者只决定 `principalId`。因此组织作用域**不依赖** platform
    /// adapter 是否组装成功：身份不可用时，仪表板 / workflow 命令依然可用。
    ///
    /// 作用域只在这里确定一次，请求体里的任何字段都覆盖不了它（INV-01）。
    pub organization_scope: OrganizationScope,
```

装配点（逐字，`bootstrap/app_state.rs::finish_app_state`）：

```rust
        desktop_job_host,
        // 组织作用域在**启动期**确定一次：桌面是单租户，组织的值是安装级常量，
        // 与「当前 OS 用户名能不能取到」无关。请求期直接取这份值（INV-01）。
        organization_scope: crate::platform::local_organization_scope(),
    };
```

**表征用例已反转**（`src-tauri/src/platform/scope_tests.rs`，
`the_ipc_scope_is_unavailable_when_the_platform_adapter_failed_to_assemble` →
`the_ipc_scope_is_still_available_when_the_platform_adapter_failed_to_assemble`）：

```rust
/// 【P0 行为回归·不变量】adapter 组装失败时，IPC 路径的**组织作用域仍然可用**。
///
/// 桌面是单租户，组织的值是**安装级常量**（`LOCAL_ORGANIZATION_ID`），
/// 与「当前 OS 用户名能不能取到」**无关**——后者只决定 `principalId`。
///
/// 因此仪表板 / workflow 命令**不因身份不可用而整体不可用**：作用域在启动期
/// 确定一次并存在 `AppState` 里，请求期直接取，不走 `platform_require()`。
///
/// 这条用例钉住本轮修复的**核心不变量**：任何将来把作用域来源改回「请求期
/// 解析身份」的改动，都会先把它弄红。
#[tokio::test]
async fn the_ipc_scope_is_still_available_when_the_platform_adapter_failed_to_assemble() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let mut state = test.state.clone();
    state.platform = failed_entry();

    let scope = state
        .organization_scope()
        .expect("adapter 组装失败时 IPC 组织作用域仍然可用");
    assert_eq!(
        scope.organization_id().as_str(),
        LOCAL_ORGANIZATION_ID,
        "组织作用域不得依赖 adapter 是否组装成功"
    );
    assert_eq!(
        scope.organization_id(),
        local_organization_scope().organization_id(),
        "IPC 路径与后台路径必须看到同一个组织"
    );
}
```

Tester 的另一条护栏 `the_ipc_scope_and_the_background_scope_name_the_same_organization`
**一字未改**，仍然通过（两条路径都指向 `LOCAL_ORGANIZATION_ID`）。

## D-02【非阻塞·语义歧义】跨组织删除 workflow 返回 `Ok(())` —— 只补契约，不改语义

按协调者裁决：不改语义（改成 `NotFound` 会让「删除本就不存在的 id」也变成错误，
那是改变既有行为）。已在 `packages/feature-core/src/workflow/registry.rs` 的
`WorkflowDefinitionStore::delete_workflow` 契约注释里写清，并同步补在
`WorkflowRegistry::delete` 与桌面实现上：

```rust
    /// Delete one saved workflow. Deleting an unknown id is not an error.
    ///
    /// **跨组织删除与删除一个不存在的 id 一样返回 `Ok(())`**：在本契约下二者
    /// 不可区分——越组织一律按「不存在」处理（CM-05），而「删除不存在的 id 不是
    /// 错误」是既有约定。因此调用方**不应**把这个 `Ok(())` 读作「目标已删除」，
    /// 它只说明「没有需要删除的东西」。
```

Tester 加固的那条断言（跨组织删除前后 `list` 长度不变）保留。

## D-03【非阻塞·台账口径】断言变化计数 —— 已改准（第 3 轮再改一次）

第 1 轮台账写「feature-core 2 条断言变化」，口径夸大；第 2 轮改成「6 条 = 1 + 5」，
**仍不准**：那次数的是「diff 里出现变化的断言**行**」，而多行 `assert*` 宏会被 rustfmt
换行重排，行级计数会漏。**第 3 轮按 Tester 的口径重算**：把多行 `assert*` 宏归一化成
单条语句（拼接 + 折叠空白）后做**多重集配对**，基线取 `65dac1e2c`：

| 位置 | 既有断言**语句**变化 | 性质 |
| --- | --- | --- |
| feature-core `workflow/registry.rs:466` | `assert_eq!(records[0].visibility, …)` → `records[0].1.visibility` | **1 条元组改写**：内存存储改成按组织分区，记录是 `(OrganizationId, record)`。断言语义不变 |
| feature-core `dashboard/runs.rs:270` | `get_run(&store, "d1", "w2", "run-1")` → `get_run(&scope, &store, …)` | 只补 `&scope` 参数 |
| feature-core `dashboard/store.rs:482` | `get_dashboard(&store, "dash-1")` → `get_dashboard(&scope, &store, …)` | 只补 `&scope` 参数 |
| feature-core `workflow/registry.rs:415` | `registry.get("hidden-1")` → `registry.get(&scope, "hidden-1")` | 只补 `&scope` 参数 |
| feature-core `workflow/registry.rs:476` | `registry.get("yaml-1")` → `registry.get(&scope, "yaml-1")` | 只补 `&scope` 参数 |
| host `workflow/registry.rs:286` | `reg.get("hidden-1")` → `reg.get(&scope, "hidden-1")` | 只补 `&scope` 参数 |
| host `dashboard/store_tests.rs:127` | `list_dashboards(&*db)` → `list_dashboards(&scope, &*db)` | 只补 `&scope` 参数 |
| host `dashboard/export.rs:258` | `list_dashboards(&*db)` → `list_dashboards(&scope, &*db)` | 只补 `&scope` 参数 |

**feature-core 5 条 = 1 条元组改写 + 4 条只补 `&scope`；宿主 3 条 = 3 条只补 `&scope`；
合计 8 条 = 1 + 7。断言未改弱，结论不变。**

> 复核注：`src-tauri/src/bootstrap/tests.rs` 里有一处 `assert!(src.contains("…generate_handler!["));`
> 会被朴素的「按 `assert` 起头拼接」解析器误判为变化——它含不平衡的括号（字符串字面量里
> 有 `![`），逐字比对确认**该断言一字未改**，不是第 9 条。

---

# 第 3 轮（修-测循环 · 第 2 轮）

独立 Tester 复测结论 **TEST_PASSED**（D-01 确认已真修好且**只修好了它**：三棵树对照
基线 242 / 修前 248 / 修后 242，失败集合逐条相同；7 条变异全部按预期）。
本轮只做三项**文档/注释**小修。

## D-04【要修·文档】行号引用 —— 已改准

`docs/architecture/platform/workflow-resource-model.md` 的引用逐条核对当前代码后修正：

| 引用 | 原值 | 现值 | 依据 |
| --- | --- | --- | --- |
| `workflow/registry.rs`（宿主侧 `version: None`） | 224 | **226** | 本轨 `a0ea3d1f0` 在其上方加了 2 行 D-02 注释 |
| `packages/feature-core/src/workflow/registry.rs` | 429 | **437** | 本轨 `a0ea3d1f0` 在其上方加了 8 行 D-02 注释 |
| `executor.rs`（§2.4 的两行代码块） | 129 | **130-131** | `workflow_connection` / `workflow_database` 两行 |
| `command_runtime.rs:115-119`（目标定位策略注释） | 115-119 | **`commands/driver_command/execute.rs:59-62`** | 那条注释已随 workflow 执行迁进 feature-core 而**从 `command_runtime.rs` 消失**；它的现居所是 `driver_command/execute.rs` 的 F7 注释（"the target travels with the command input … the host never switches the session's database"） |
| `scheduler.rs`（调度器不包裹 Job） | 176 | **`workflow/scheduler.rs:44-48`** | `src-tauri/src/workflow/scheduler.rs` 只有 92 行，`176` 越界；`commands::workflow_execute_impl(...)` 的调用在 44-48 行，连接参数恒为 `None`（第 48 行） |
| `dashboard/execute.rs`（同样不传连接） | 222 | **71** | `src-tauri/src/dashboard/execute.rs` 第 71 行 `WorkflowExecutor::execute(&workflow, self.app_state, None, …)` |
| `migration.rs`（`job_id` 传 `None`） | 509 | **221** | `src-tauri/src/workflow/migration.rs` 第 221 行 `job_id: None,` |

顺带核对的另外 4 处**基线就已失效**的引用（不是本轨引入）已一并改准：
`executor.rs:129`、`command_runtime.rs:115-119`、`scheduler.rs:176`、`dashboard/execute.rs:222`
—— 见上表；`migration.rs:509` 同属该组。

其余引用逐条核对**仍然正确**，未动：`dashboard/create.rs:40`、`workflow/host.rs:128/169/204`、
`workflow/migration.rs:433`、`commands/dashboard.rs:482`、
`commands/ai/integration_tests.rs:377/932/1070/1123`、`feature-core/workflow/scheduler.rs:223`、
`feature-core/dashboard/archive.rs:185`、`store/settings.rs:15`。

文档里的**事实断言**也复核过：`qualify_sql_target` 的实现方仍是 clickhouse / duckdb /
mysql / postgres / sqlite / sqlserver **6 个** SQL 驱动，mongodb 仍声明 `has_multi_database`
但不走 SQL 路径 —— 与文档表述一致。

## D-05【要修·契约注释】`platform-api` 的「作用域只能由 adapter 构造」—— 已改

`packages/platform-api/src/ports/scope.rs` 的模块级表述被桌面实现（安装级常量）字面推翻，
且与同文件 `OrganizationScope::new` 的文档自相矛盾。已改成「可信来源」口径，并把模块级
表述与 `new` 的文档**统一**：

模块级（逐字）：

```rust
//! * 作用域**只能由可信来源构造**：adapter 组装的请求上下文
//!   （[`OrganizationScope::from_context`]），或宿主形态的**安装级常量**——
//!   桌面是单租户，它的组织 id 是一个进程常量，与「当前 OS 用户名能不能取到」
//!   无关。两种来源都**不是**请求体字段：**任何请求体字段都不得覆盖组织身份**
//!   （INV-01）。
```

`OrganizationScope::new`（逐字）：

```rust
    /// 直接以组织 id 构造作用域。
    ///
    /// 只在调用方已经持有**可信的组织身份**时用：宿主形态的安装级常量
    /// （桌面单租户），或别的已解析好的组织身份。请求体字段**不得**流到这里
    /// （INV-01）——那等于让调用方指定组织，租户边界随之失效。
    /// 其余情况优先用 [`Self::from_context`]。
```

`OrganizationScope::from_context`（逐字）：

```rust
    /// 从请求上下文取作用域。
    ///
    /// 身份只能来自 adapter 构造的上下文（INV-01），因此本方法**不接受**任何
    /// 请求体字段作为入参。宿主形态若不经过 adapter（例如桌面用安装级常量
    /// 直接构造），见 [`Self::new`]。
```

## D-06【非阻塞·台账口径】断言变化计数 —— 再次改准（见上）

feature-core **5 条 = 1 条元组改写 + 4 条只补 `&scope`**；
宿主 **3 条 = 3 条只补 `&scope`**；合计 **8 条 = 1 + 7**。

## 长期文档：桌面组织作用域的来源（已写进正式文档）

新增 `docs/architecture/platform/shared-boundaries-and-ports.md` 的
**§5.2 组织作用域的来源（已实现）**：形态对照表（桌面 = 安装级常量 / server = 认证会话 /
后台 Job = 逐阶段继承）+ 三条结论（组织身份不可用 ≠ 组织边界不可用；IPC 路径与后台路径
必须看到同一个组织；作用域不是 IPC 参数）+ 按组织限定的存储端口的越权约定（CM-05）。

---

## 门禁实测（第 2 轮，HEAD = `2e3a1fc23` + 本轮修复提交之前的工作区）

| 命令 | 退出码 | 结论行 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | 无输出 |
| `cargo clippy -p datazen-feature-core -p datazen --all-targets` | 0 | 无 error；本轨道新增 0 条 warning |
| `cargo check -p datazen-feature-core -p datazen --all-targets` | 0 | 无 error |
| `cargo check -p datazen --features webdriver --all-targets` | 0 | 无 error |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-feature-core --lib` | 0 | `test result: ok. 377 passed; 0 failed; 0 ignored` |
| `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen --lib` | 0 | `test result: ok. 1283 passed; 0 failed; 9 ignored` |
| `env -u USER -u DATAZEN_DESKTOP_USER … cargo test -p datazen --lib` | 101 | `test result: FAILED. 1041 passed; 242 failed; 9 ignored` |

### `env -u USER` 这条的命令行专项核验

- 修复前（Tester 实测）：**248 failed**，其中 **6 条由本轨引入**。
- 修复后（本轨道实测，**连跑两次结果完全一致**）：**242 failed** ⇒ **恰好 6 条恢复**。
- 恢复的 6 条（逐条按名字核对，现已不在失败清单里）：
  `commands::dashboard::tests::dashboard_crud_via_impl`、
  `commands::ai::integration_tests::workflow_save_list_get_delete`、
  `commands::ai::integration_tests::workflow_execute_ai_step_with_wiremock`、
  `commands::ai::integration_tests::workflow_history_clear_after_execute`、
  `commands::ai::integration_tests::dashboard_hidden_workflow_execute_skips_history`、
  `commands::ai::integration_tests::ai_providers_config_prompt_workflow_impl`。
- 剩余 242 条**没有一条属于本轨引入的 6 条**；按模块分布：
  `commands` 195、`services` 24、`mcp` 14、`workflow` 4、`platform` 4、`monitor` 1。
- 它们的失败原因**全部**是「取不到当前桌面登录用户」（`principalId` 不可得），
  即 **178 条 `platform is ready: ApiError { code: Unauthenticated …}`** +
  23 条 `unauthenticated` / `NotConfigured("无法确定当前桌面登录用户…")` +
  其余同类；**没有一条的失败信息提到组织作用域**（日志里 `organization_scope` /
  「组织作用域」零命中）。
- 这 242 条是**改造前既有**的 adapter 依赖（AI / connection / data / data_transfer /
  backup / sync / schema_diff / wapps / mcp / native_tool / 测试夹具要求 platform ready），
  **不在本轨道范围**，本轨道没有为「让它们全绿」去动它们。

### 基线（同一命令、同一工作树，`git stash` 后在 `65dac1e2c` 实测）

| 包 | HEAD 基线 | 第 1 轮之后 | 第 2 轮（+Tester 10 条） |
| --- | --- | --- | --- |
| feature-core | 367 passed | 377 passed（+10） | 377 passed（不变） |
| host | **1266** passed | 1273 passed（+7） | **1283 passed**（+10） |

> 任务书写的 host 基线是 1269；本轨道在 HEAD 上实测为 **1266 passed; 0 failed; 9 ignored**。
> 差 3 条不影响结论（0 failed，且增量与新增用例数一一对应），但**基线数字以实测为准**。

### 既有断言是否被改动

见上面 D-03 的表格：**6 条既有断言行变化 = 1 条元组改写 + 5 条只补 `&scope` 参数**，
断言语义与被断言对象一字未改；其余是 rustfmt 换行。

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
5. `progress.md` 在分支根目录（可入库），**合并时必须删除**。

## 未开始 / 不在本轨道范围

- 未给桌面 SQLite 增加 `organization_id` 列（按要求：桌面单租户，schema 不变）。
- 未改动 `packages/platform-api/**`（`OrganizationScope` 词汇已备好）、`packages/runtime/**`、
  `packages/schema-diff/**`、`packages/data-{sync,transfer}/**`、
  `src-tauri/src/commands/{sync,data_transfer,schema_diff}/**`。
- 未为 `env -u USER` 下那 242 条既有失败做「修复」——它们是改造前的 adapter 依赖，不在本轨范围。
