# 连接管理重构与桌面 / Web 多形态开发计划

> 状态：P4、P5 已完成（P4 于 2026-10-16、P5 于 2026-10-09）；P6 已启动但尚未关闭（共享 feature 抽取已落地，真实驱动旅程仍待验收）；P7–P10 仍为计划。当前基线：`main`，`c84f21237`（共享层下沉 PR #68 的合并提交，2026-10-16；P6 从此提交起算）。本文按用户明确要求入库，不记录每日进度。
> 设计依据：[系统概要](../architecture/platform/system-overview.md)、[连接管理详细设计](../architecture/platform/connection-management.md)。

## 1. 交付目标与实施原则

最终交付桌面本地、浏览器团队、桌面团队三种方式，共享 Rust 业务内核和 React 产品。连接管理对用户、会话、物理资源和任务分别建模；新增已有能力的 driver 不要求修改宿主业务流程。

先验证资源语义，再迁移 consumer，再增加团队入口和多实例。初期采用模块化单体；远程 agent、独立 driver runner、跨 backend 迁移是独立后续版本，不阻塞首版。

代码变更按阶段拆为可验收的小 PR。每个 PR 包含实现、相关测试和已落地事实文档；不创建进度台账、Bug List 或新方案副本。本计划是阶段顺序与交付门槛，完成的阶段应改为实施事实或合并到开发流程，不无限保留历史方案。

不能把后端重构、Driver API 协议迁移、所有 UI 和 Web 服务一次性切换。旧路径只做有截止点的窄兼容；同一 consumer 同一请求只使用一种资源管理器。

## 2. 阶段依赖

```mermaid
flowchart TD
    P0[P0 基线与契约夹具] --> P1[P1 共享边界与 DTO]
    P1 --> P2[P2 Driver 资源与能力]
    P2 --> P3[P3 Session / Lease / Budget / Gateway]
    P3 --> P4[P4 桌面 tab 与元数据迁移]
    P3 --> P5[P5 Job 与迁移三件套]
    P5 --> P6[P6 Workflow / AI / MCP / Wapp]
    P4 --> P7[P7 单实例团队 Web]
    P6 --> P7
    P7 --> P8[P8 桌面团队客户端与发布]
    P8 --> P9[P9 多 worker 与全局调度]
    P9 --> P10[P10 驱动扩展验收与旧路径清理]
```

P4 与 P5 代码边界独立后可以分别推进，但必须共享已稳定的 P3 契约；图表示依赖，不要求自动启用多个代理。没有用户明确授权时不派发其他聊天任务。

## 3. 每个阶段的统一完成定义

1. 所需 API、DTO、错误和生命周期都有实现，不能只交接口骨架。
2. 详细设计对应测试编号通过；Unsupported 能力有拒绝断言。
3. 生产路径不新增裸 unwrap/expect；测试文件通过 typecheck，无 any 绕过。
4. driver 方言实现及测试在 driver 目录；host 无数据库名称分支。
5. 新协议兼容声明真实，所有参与构建的 path/Git driver 已迁移或明确禁用对应新能力。**自 2026-10 起 P2 第一轮范围收窄为 `basic` 四个驱动**（postgres、mysql、sqlite、redis，即 CI 与默认构建实际编译的集合）；其余 path/Git 驱动移入 P10，由 P10 门槛统一断言。
6. Agent 不读取 `.env` / `.env.test` 内容进入上下文；测试程序读取合法且输出不得泄密，不提交凭据、生成文件或外部子仓库内容。
7. 当前架构文档同步描述已经实现的部分；未实现功能仍标记目标设计。
8. 评审看到清楚的启用条件、关闭/失败行为和回退边界。

## 4. P0：现状基线与契约测试夹具

**目的**：在移动代码前识别逻辑 session、pool handle、transaction handle 的真实语义，建立可比较的行为基线。

输入为当时的 ConnectionManager（现已删除）、Driver API、Query/Table consumer、三件套和 Workflow。重点代码链接见详细设计第 2 节。

交付：

- 清点各 consumer 的连接创建、复用、关闭、取消和配置目标路径；结论放进代码测试与当前服务文档，不写调查台账。
- 建立 transport-neutral fake resource，支持 resourceId、owner、命令 journal、屏障、fake clock 和每阶段失败注入。
- 在 driver 目录建立真实固定会话/事务/目标/清理契约测试；确认不能保证的能力。
- 确定测试环境注入方式，检查启动器/失败输出不回显受保护 env 内容。
- 记录现有 DTO 与持久化格式兼容范围；旧 session ID 不作为迁移数据。
- 用 CM-73 复现并留存“空闲淘汰关闭物理连接、同 ID 重连、事务 map 未清理”的基线；测试作为 P3 强制回归门槛，其断言只依赖行为，不得随旧管理器删除。

负责边界：runtime 负责人定义 fake 契约，driver 维护者验证真实协议，前端负责人验证 tab 行为。

退出门槛：CM-01、04、07 的基线测试，以及 CM-73 对当前事务句柄生命周期的可复现证据；真实 driver CM-08～19 的能力验证能区分已支持和缺失。已知失败作为本轮后续阶段要修复的测试，不能删除断言。

回退：仅增加夹具和测试；不改变生产执行路径。

## 5. P1：抽取共享应用边界和前端传输契约

**目的**：业务可在没有 Tauri 的测试进程中调用。

交付：

- 创建并注册拟新增 application/runtime/platform-api/backend-client 包，明确依赖方向。
- 定义 RequestContext、ID newtype、CanonicalTarget、namespaceShape/操作级 targetRequirements、会话、执行、Job、Artifact DTO 和 ApiError。
- 分开 DurableExecutionRecord 与实时 runtimeBinding，定义能力快照、CancelReceipt 和签名幂等令牌 port；所有持久化路径排除运行时句柄。
- 定义 repositories、SecretProvider、NetworkProvider、ArtifactRepository、EventSink ports。
- Tauri adapter 组装本地身份，调用应用服务；保留现有 IPC 外观的窄适配。
- BackendClient 和 PlatformServices 注入，迁移 SDK 中 invoke/Channel，存量调用用薄再导出过渡。
- 加入依赖护栏：core 不引用 Tauri/HTTP，driver 不引用 host，前端领域不直接绑定平台传输。
- 加入 CI 架构检查：对 server crate 的 normal/build 依赖闭包验证不含 Tauri crate 和 UI runtime，并验证 server 独立构建；依赖检查失败即阻断合并。

退出门槛：CM-01、03～07 的**类型与 schema 断言**、DTO 往返与内核无 Tauri 编译、CI 依赖图检查通过；旧桌面基本连接/查询流程保持可用。同一批用例的 H 层行为断言归 §7（P3）、CM-05～07 的 W1 断言归 §11（P7），阶段归属见 §17，本条不重复计。

回退：Tauri adapter 可仍调用原用例实现，但不得把新 owner 和权限语义伪映射为旧共享 session。新 frontend bridge 在未注入时明确报错。

### 5.1 application 层持有注入的存储端口：已实现的事实与下一步分期

本节记录 §6.2 第 7 步（`ApplicationServices`）的落地状态与后续分期。**已实现的部分是事实**，**未做的部分是下一步**，两者不混写。

本节的事实与数字测于 `b9de8a978`（PR #102 合并后的 `main`）：该 PR 已把 known-hosts 存储端口按 §2.8 改名（`KnownHostsStore` → `KnownHostsRepository`）并给 platform-api 加了端口清单 `packages/platform-api/src/ports/inventory.rs`（端口规模由 `PORT_TRAITS` **派生**，模块头不再手写计数）。下文引用的端口清单一律以那份清单为准。

#### 5.1.1 已实现的事实

**形态中立的用例层现在真的持有注入的存储端口。** `ApplicationServices`（`packages/application/src/lib.rs`）新增两个字段：

| 字段 | 端口类型 | 桌面实现 | 注入点 |
| --- | --- | --- | --- |
| `settings` | `datazen_platform_api::ports::SettingsRepository` | `DesktopRepository`（`src-tauri/src/store/mod.rs`） | `PlatformAdapter::inject`（`src-tauri/src/platform/adapter.rs`） |
| `tunnels` | `datazen_platform_api::ports::tunnel::TunnelRepository` | `DesktopRepository`（`src-tauri/src/store/tunnels.rs`） | 同上（与注入给会话后端的是**同一个实例**） |

要点：

- 端口是**构造参数**而不是 `Option`：漏注入是**编译错误**，不是运行时的 `None`。构造函数对桌面与将来的 web 形态**完全相同**，只有传入的 `Arc<dyn …>` 实现不同。
- `ApplicationServices::settings_service()` 让形态中立的设置用例（`packages/application/src/settings.rs` 的 `SettingsService`）直接跑在聚合持有的端口上，不再借用宿主的 store。
- `AssemblyState::missing_steps()` 里 §6.2-7 的目标状态由 `AwaitingApplicationServices` 改为 `FullyWired`——旧值与当前档位同级，导致「补完第 7 步」在机器上永远不前进。
- 机器可校验的不变式：`packages/application` 侧 6 条（形状 3 条 + 行为 3 条），宿主侧 11 条（装配状态机 2 条 + `AppState` 形状 3 条 + 装配降级 2 条 + 第三步与第五步的宿主态收敛守卫 4 条，见 §5.1.2 的 D1 裁定、第三步与第五步）。

**依赖边结论（为什么 application 层不能直接持有领域包端口）**：`datazen-runtime` 依赖 `datazen-application`，而 `datazen-schema-diff` / `datazen-data-sync` / `datazen-data-transfer` 都依赖 `datazen-runtime`。因此 `application → 领域包` 会闭成 Cargo 环（`cargo metadata` 报 `cyclic package dependency`），门禁连跑都跑不起来。F-02/F-03/F-05 只禁宿主、传输框架与前端标识，**不禁**这条边——挡住它的是依赖图本身，不是门禁。F-04 另外禁止 `platform-api` 依赖领域包，因此「把领域端口搬进 `platform-api`」这条路也要靠**新写窄端口**而不是靠依赖。结论：**领域端口只能由宿主在组装期包成 `platform-api` 层的窄端口再注入**。这一条已写成断言（`the_crate_does_not_depend_on_the_domain_packages`）。

**没有做、也不许顺手做的事**：现有 145 处 `state.store.…` 调用点一个都没搬；命令的签名、载荷与错误文案一个都没改；`AppState` 没有新增 `pub` 字段（字段数上限由断言钉住，当前基线 23：25 起，第三步与第五步各 -1）。

#### 5.1.2 下一步分期

每期都必须满足：形态中立（不引入形态分支）、零行为变化、可独立验收。下表的「少掉」一栏是**验收时应当观测到的数字**。

**第一步（已落地）——聚合持有两个平台端口**

搬走／新增：`ApplicationServices.settings`、`ApplicationServices.tunnels`；`PlatformAdapter::inject` 注入同一实例；`AssemblyState` 第 7 步目标状态修正为 `FullyWired`。

`AppState` 少掉：**0 个字段**（本期只加持有权，不动宿主的字段），访问数不变（`state.store` 仍是 145，`state.tunnels` 仍是 12）。

独立验收：`cargo test -p datazen-application --lib`（形状 3 条 + 行为 3 条）、`cargo test -p datazen --lib`（`completing_step_seven_advances_the_machine_to_fully_wired`、`injection_wires_the_storage_ports_into_the_application_services`、`app_state_*` 3 条）、`node scripts/check-platform-crate-boundaries.mjs`（F-02/F-03/F-04 仍绿）。

风险：`ApplicationServices` 多了必填构造参数 ⇒ 将来任何新的形态都必须提供这两个端口的实现；漏了编译不过，这是**期望**的失败面。注入的实例必须与宿主持有的是同一份（`Arc::ptr_eq` 断言已钉住），否则出现两份事实。

**第二步（已落地 2026-10-23）——设置用例归位到 application 层**

搬走：`src-tauri/src/commands/config.rs` 的两处 `SettingsService::new(state.store.as_ref())`（`get_shared_settings` / `save_shared_settings`）改为 `crate::platform::ipc::require_application_services(state.inner())?` + `ApplicationServices::settings_service()`；错误映射逐字不变（`Internal("Unable to read settings")` / `Validation(message)` / `Internal("Unable to persist settings")`）。`save_shared_settings` 尾部的 `redis_flush_gate::sync_from_settings(&state.store.get_settings().await)` **保留**：它读的是宿主 `AppSettings`，不是形态中立的共享设置，不在本期范围。

`AppState` 少掉：**0 个字段**。生产半边的 `state.store.` 计数（口径：`src-tauri/src` 下 `.rs`、排除 `*_tests.rs`、在首个 `#[cfg(test)]` 处截断）从 **106 减到 104**，其中 `commands/config.rs` 由 **14 减到 12**。早先本节写的「145 → 143」是**含测试半边的宽口径**（同一时刻宽口径实测亦为 143），不可与本期数字直接相减。

独立验收（已实测）：`cargo test -p datazen --lib` **1458 passed / 0 failed**（EXIT=0）；新增 `commands::config::tests::shared_settings_commands_read_the_aggregate_not_the_host_store`——断言生产半边里 `SettingsService::new(` 出现 0 次、`state.store.as_ref()` 出现 0 次、`settings_service()` 出现 2 次、`state.store` 出现 12 次；`src-tauri/src/platform/ipc.rs` 的 15 条 `FROZEN_SIGNATURES` 一字未改。

失败面：按下面的 D1 裁定，聚合取不到时这两条命令报 `NotConfigured`（此前直读 store 恒可用），残留失败面只有 `DesktopIdentity::from_desktop_user` 失败那一档。

**第二步前置裁定 D1（已落地）：可选接线降级，内核不受牵连**

第二期的风险是「设置从可用变 `NotConfigured`」。裁定结果与理由如下——**已实现为事实**，不是待办：

- **做法**：不把 `ApplicationServices` 的构造搬到 `bootstrap/app_state.rs`，而是把 `PlatformAdapter::inject` 里两项**可失败**的接线降级为「缺席能力」：执行身份解析器（`DesktopIdentityResolver::for_desktop`）与结果制品存储（`{dataDir}/result-artifacts`）。它们失败时各自只关闭自己那一项——对应访问器继续报 `ServiceUnavailable`，缺席项由 `PlatformAdapter::degraded_capabilities()` 可枚举，启动各记一条 `warn!`。内核（`profiles` / `policy` / `backend` / `live` / `runtime` / `target_metadata` / `ApplicationServices`）没有失败分支，一装好 `require_application_services()` 就可用。`inject` 的 `Err` 因此只剩「同一进程重复注入」这类装配冲突。
- **为什么不搬到装配根**：聚合的构造材料（`DesktopIdentity`、`DesktopSessionBackend`、`OnceLock` 安装点、Backend 的 publisher/export 接线）全在 adapter 内；搬到 `finish_app_state` 只是把同一段代码挪到上游再传回来，**不改变任何一个失败分支**。真正决定失败面的是「可失败项是否中止整体装配」，不是构造发生在哪一层。另外，搬过去会诱使把聚合塞进 `AppState`，撞上 `app_state_does_not_hold_the_application_services` 守卫。
- **实测依据（说明旧形状的失败半径严重超标）**：`identity_resolver()` 在**生产路径零调用方**（全仓只有 `adapter_tests` 调它），而 `master_key_bytes()` 读的是 `store.encryption_key_b64()`——主密钥在 `DesktopRepository::init_with_path` 里已经生成并持有，因此 `for_desktop` 在 store 就绪后**几乎不可失败**；真实可失败的只有制品目录（`create_dir_all` 撞上同名文件或不可写）。旧形状下这两者任一失败都会让 `profiles` / `runtime` / `services` 一个都装不上，把「制品存不了」放大成「所有依赖平台的 IPC 都是 `NotConfigured`」。
- **残留失败面（写清，不得再悄悄扩大）**：`DesktopIdentity::from_desktop_user` 失败（取不到本机用户名）时没有 adapter，设置命令仍是 `NotConfigured`。这比旧路径（设置直接读 store）多出的**唯一**一档。接受理由：该档里连接列表、建连、工作流、AI 全部 `NotConfigured`，应用实质不可用；为这一档单留一条宿主直读 `store` 的路径，等于重新引入第二事实源。可逆性：将来若产品要求「无身份也能改设置」，正确做法是给聚合增加一条不依赖身份的构造路径（`sessions` 换成惰性端口），而不是回到 `state.store`。

可执行证据：`cargo test -p datazen --lib -- platform::adapter`（17 passed），其中
`injection_wires_the_storage_ports_into_the_application_services`（顺利装配时降级表为空、身份解析器在场）、
`optional_wiring_failure_degrades_only_its_own_capability`（制品目录被同名文件占住时：`inject` 返回 `Ok`、
`require_application_services()` 可用、`degraded_capabilities()` 只列那一项、`artifact_store()` 报
`ServiceUnavailable` 且 IPC 表面仍收敛到 `NotConfigured`）。

**第三步——隧道定义仓储归位（已落地 2026-10-23）**

实际改动（与计划的差异逐条给出）：

| 位置 | 改动 |
|---|---|
| `src-tauri/src/commands/tunnel.rs` | 7 个 `*_impl` 各加一行 `require_application_services(state)?`，`state.tunnels` → `services.tunnels`（8 处） |
| `src-tauri/src/commands/connection.rs` | `test_connection_impl` 同上（1 处） |
| `src-tauri/src/commands/connection_import/ipc.rs` | `materialize_tunnel_refs` 返回 `()`，**不能** `?`：聚合取不到时按既有风格 `warn!` 后 `return`（与「作用域取不到」同档），只降级本次导出的隧道解引用（1 处） |
| `src-tauri/src/commands/mcp.rs` | `clone_app_state()` 的字段逐个克隆里删掉 `tunnels`（1 处） |
| `src-tauri/src/commands/native_tool_tests.rs` | `attach()` 直接 `inject`，原本用 `state.tunnels`；改为 `state.store.clone()`（桌面实现就是存储层本身，与装配根同一做法） |
| `src-tauri/src/commands/mod.rs` | 删掉 `AppState.tunnels` 字段与其 doc 块（聚合里那份 doc 更完整） |
| `src-tauri/src/bootstrap/app_state.rs` | `AppState { … }` 不再带 `tunnels`；局部 `let tunnels` 保留（它要喂 `inject`），注释放明「只经 `ApplicationServices::tunnels` 取用」 |
| `src-tauri/src/commands/app_state_shape_tests.rs` | `BASELINE_FIELD_COUNT` 25 → **24**（守卫自动收紧） |

**实测**：`state.tunnels` 生产与测试源码计数 **1 → 0**（唯一残留是 `native_tool_tests.rs` 的那处，已改为 `state.store`）；`AppState` `pub` 字段 **25 → 24**。

**风险不该变的两件事（已逐条核对）**：① 组织限定仍是端口入参（`Scope` 从 `state.organization_scope()` 来，没降级成字段）；② 同一个仓储实例继续同时喂会话后端与命令侧——`finish_app_state` 里 `let tunnels = store.clone()`，`inject` 拿到的和聚合里的是同一个 `Arc`（`storage_ports_tests.rs` 用 `Arc::ptr_eq` 守这条）。

**第四步——领域包端口的窄化**

搬走／新增：在 `packages/platform-api/src/ports/` 新增三个**窄端口**（纯 trait，不依赖领域包）：
- `ReviewedPlanRepository`（schema-diff 的评审计划存储，现契约在 `packages/schema-diff/src/reviewed.rs`）；
- `SyncPlanRepository`（data-sync 的计划注册表，现契约在 `packages/data-sync/src/plan_store/repository.rs`）；
- `TransferPlanRepository`（data-transfer 的不可变计划与检查点，现契约在 `packages/data-transfer/src/plan_store/port.rs`）。

宿主在组装期实现这些窄端口（转发到领域包端口的实例），再注入给 `ApplicationServices`。

**新增 platform-api 端口必须同时改四处（漏一处即红）**：`packages/platform-api/src/ports/inventory.rs` 的 `PORT_TRAITS`（按 trait 名字的 ASCII 字母序插入）、`packages/platform-api/src/ports/inventory/parser.rs` 的 `SOURCE_FILES`（新端口文件不在这个清单里，它里面的 `pub trait` 会被**静默漏掉**——测试照样绿）、`packages/platform-api/src/ports/mod.rs` 模块头的构成说明（不在概要 §6.4 表里的端口必须在那里被**逐个点名**）、以及（可选）`docs/architecture/platform/system-overview.md` §6.4 的端口表。模块头里**不得**再出现手写端口计数（规模由 `PORT_TRAITS` 派生）。

`AppState` 少掉：**3 个字段**——`schema_diff_reviewed_plans`（9 处访问）、`sync_stores`（7 处）、`transfer_plans`（5 处），合计 **21 处** `state.*` 直读归零；字段数 24 → **21**。

独立验收：`cargo test -p datazen-platform-api --lib`（清单 ↔ 代码 ↔ 文档三条交叉断言 + 窄端口的 DTO 往返断言）+ `cargo test -p datazen --lib`（宿主适配器的转发断言：转发的是同一个领域端口实例）+ `node scripts/check-platform-crate-boundaries.mjs`（F-04 必须仍绿：`platform-api` 不依赖领域包）+ 新增断言「生产源码里 `state.schema_diff_reviewed_plans` / `state.sync_stores` / `state.transfer_plans` 出现 0 次」。

风险（最高的一期）：窄端口的 DTO 一旦与领域包的形状**各写一份**，事实就有两份 ⇒ 窄端口必须只暴露领域包已有的类型；**若某个领域模型无法用 platform-api 的词汇表达，说明该能力本就不该进形态中立层**，此时该窄端口**不应**被创建。这一期也是形态专属的东西最容易漏进中立层的地方——评审必须逐条问「这个方法在 web 形态有对应物吗」。

**第四步的裁定（2026-10-23，P0 落地后）：由「领域包原生端口」取代，不再新建这三个窄端口。**

理由按优先级：

1. **签名里全是领域模型**：`ReviewedPlan`、`StoredSyncPlan`、`ComparisonOptions`、`TransferPlan`、
   `Checkpoint` 都定义在领域包里，而 F-04 规定 `platform-api` 不得依赖领域包 ⇒ 窄端口只能在
   `platform-api` 里复制一份 DTO，事实两份——正是本节风险段自己禁止的事。
2. **P0 断环后这条边已合法**：`ddd-refactor-plan` 的 P0 把 `ApiError`/`ConnectionUseCases`/
   请求 DTO 归位 `platform-api`，`cargo metadata` 实测 0 环、`application → 领域包` 可行
   （`scripts/check-ddd-layers.mjs` 守）。聚合直接持有领域包**自己的**端口 trait 即可，
   宿主不必再写一层转发壳。
3. **剩余价值另有归属**：本节真正想消灭的是「`AppState` 直读领域端口」那 21 处，这件事改由
   第六步按命令族完成——命令不再从 `AppState` 取领域端口，而是从聚合取。
   容器：`SyncRepositoryPorts` 已随第三步改为聚合侧装配。

**第五步——known-hosts 存储端口归位进 platform-api（已落地 2026-10-23）**

搬走：`datazen_tunnel::KnownHostsRepository`（`packages/tunnel/src/known_hosts_repository.rs`）→
`datazen_platform_api::ports::known_hosts::{KnownHostsRepository, KnownHostEntry}`。**改名部分已由 PR #102 完成**
（`KnownHostsStore` → `KnownHostsRepository`），本步做的是**归位 + 错型替换 + 宿主态收敛**：

- **端口搬进契约包**：新文件 `packages/platform-api/src/ports/known_hosts.rs`（trait + 词汇 + 一个内存实现的自证用例）。
  `packages/tunnel/src/known_hosts_repository.rs` 删除，`packages/tunnel/src/lib.rs` 改为 `pub use` 再导出，
  因此 `datazen_tunnel::KnownHostsRepository` 这条路径**仍然可用但已降级为转发**，调用点按新路径逐个改正。
- **错误类型统一**：`KnownHostsRepositoryError` 删除，改用 `PortError`（`save` 失败 → `PortError::BackendUnavailable`；
  `load` 按契约**不失败**，读不到即空集合）。`packages/tunnel/src/ssh_known_hosts.rs` 与宿主
  `src-tauri/src/platform/known_hosts.rs` 同步改型；**文件格式、字段名与两条 warn 文案一字未动**。
- **四处登记**：`ports/inventory.rs` 的 `PORT_TRAITS`（24 → 25）、`ports/inventory/parser.rs` 的 `SOURCE_FILES`、
  `ports/mod.rs` 模块头点名（含同步与 `load` 不失败两处形状差异）、本文 §5.1.2。
- **端口实例归应用层**：`ApplicationServices` 增加第 6 个构造参数 `known_hosts: Arc<dyn KnownHostsRepository>`
  与同名字段；`src-tauri/src/platform/adapter.rs::inject` 用注给会话后端的**同一个实例**构造聚合。
- **宿主态收敛（本步的字段账）**：`AppState` 的 `known_hosts` 字段删除（构造期仍有局部变量交给 `inject`），
  命令侧改经 `ApplicationServices::known_hosts`：`commands/tunnel.rs::test_tunnel_impl`、
  `commands/connection.rs::test_connection_impl`、`commands/mcp.rs::clone_app_state`。字段数 **24 → 23**
  （`app_state_shape_tests.rs` 的 `BASELINE_FIELD_COUNT` 同步）。
- **新增守卫**（都在 `commands/app_state_shape_tests.rs`，与第三步的 `state.tunnels` 守卫同形）：
  1. `the_known_hosts_repository_is_no_longer_reachable_from_the_host_state` —— 全 `src-tauri/src` 扫 `state.known_hosts`，0 命中；
  2. `the_known_hosts_port_is_declared_in_platform_api_only` —— `packages/tunnel/src` 里不得再出现
     `pub trait KnownHostsRepository`，且 `known_hosts_repository.rs` 不得回来；
  3. `app_state_definition_has_no_status_store_fields` —— 定义体里既无 `pub tunnels` 也无 `pub known_hosts`；
  4. `packages/application/src/lib.rs` 的端口断言表与构造签名表加入 `known_hosts`。

实测：`cargo test -p datazen-platform-api --lib` 257 passed（含端口清单三条交叉断言）、
`cargo test -p datazen-tunnel` 27 passed、`cargo test -p datazen --lib` 见下方门禁、`cargo check -p datazen --all-targets` EXIT=0。

风险（已发生并处理）：`packages/tunnel` 的公开 trait 换位置是**破坏性 API 变更**，必须一次迁完
（含 `packages/tunnel/src/ssh.rs`、`src-tauri/src/tunnel/{mod,network_provider}.rs`、
`src-tauri/src/platform/{adapter,session_backend,known_hosts}.rs` 与测试装置）。本机钥匙串 /
`{appData}/ssh_known_hosts.json` 的文件 I/O 仍在宿主实现里，**不进**中立层。

**第六步——`AppState.store` 的归位（按命令族分期；基线 145 处，本期开工时生产半边 104 处）**

搬走：`state.store`（`Arc<DesktopRepository>`）的 145 处访问，按命令族分期迁到 application 层的用例上。命令族与当前访问分布（实测，生产文件）：dashboard 16、config 14、ai/* 合计 27（其中 generate.rs 7）、connection 7、connection_import 6、monitor 6（scheduled 3 + grants 3）、history 3、tray 3、mcp/prompts 3、app_archive 3 等。

`AppState` 少掉：**1 个字段**（`store`），字段数 23 → **22**；`state.store` 的访问数从 145（含测试的
宽口径基线）减到 **0**——生产半边口径是 104 → 0（见上「进度」段的计数规则）。

**进度（2026-10-23）**：第一族「命名视图」已落地——`ApplicationServices` 持
`named_views: Arc<dyn NamedViewRepository>`，`commands/named_views.rs` 生产半边
`state.store` 归零（断言 `the_command_shell_reads_the_aggregate_not_the_host_store`），
全仓生产半边计数 **104 → 101**。口径与 §5.1.2 第二步逐字相同（`src-tauri/src` 下 `.rs`、
排除 `*_tests.rs` 与 `tests.rs`、在首个 `#[cfg(test)]` 处截断）。**不要用裸 `grep` 复算**：
行内 `#[cfg(test)] mod tests` 里的断言针脚本身就会命中 `state.store` 字样（实测裸 grep 得
111，比生产半边多 10）。
附带一条结构性解锁：`application` 现在依赖 `datazen-feature-core`——P0 断环与 P2 收官后
这是正确方向，`check-ddd-layers.mjs` 的镜像不变量改为按**当前**图求值。

独立验收：每期一条「该命令族的生产源码里不再出现 `state.store`」的断言 + `cargo test -p datazen --lib` 全绿 + 该命令族的 IPC 载荷冻结断言不变 + E2E。

风险：这是最大的一期。`DesktopRepository` 里的本地加密落盘、`{appData}` 路径、本机钥匙串都是**桌面专属**，application 层**不能**持有 `DesktopRepository` 本身，只能持有窄端口（如 `SettingsRepository`、`ProfileRepository`、`SecretProvider`、`TunnelRepository`）。每搬一族都要问一句「web 形态这一族有没有对应物」；没有对应物的（如本机钥匙串、桌面 Job 恢复、桌面驱动注册表）就**留在宿主**，不进中立层。

#### 5.1.3 形态专属、不得进中立层的东西（清单）

`DesktopRepository` 的本地 AES-256-GCM 加密落盘、`{appData}` 路径解析、本机钥匙串（`KeyBackend::File` / `Keyring` / `PlatformVault`）、桌面 Job 恢复（`{appData}/datazen.sqlite` 的 `JobHost`）、桌面驱动注册表（`DriverRegistry` 的本机 inventory 装载）、`{appData}/wapps/` 的工作区应用安装目录、桌面组织的安装级常量（`LOCAL_ORGANIZATION_ID`）与本机用户名的解析。这些是**形态的事**：中立层只声明端口，端口的实现留在形态侧。


## 6. P2：Driver 固定资源与可选能力契约

**目的**：把“执行在同一真实资源上”变成可测试保证。

交付：

- opaque resource、ResourceProvider、固定执行、状态观察、reset、close 和精确 cancel 契约。
- namespace/session/transaction/snapshot/data/backup 可选能力注册与运行时能力降低机制。
- 所有实际建连的预算 port，包括 driver pool、集群节点和控制资源。
- 现有 Command 和迁移接口保持领域语义；资源获得方式通过 adapter 演进。
- **第一轮只迁移 `basic` 四个驱动**（postgres、mysql、sqlite、redis —— CI 与默认构建实际编译的集合）：先迁移具有真实会话与事务的代表 driver，再迁移该集合内其余驱动；不支持能力明确拒绝。**其余 path/Git 驱动不在 P2 门槛内**，按 P10 补齐；未迁移的驱动必须在能力注册处显式声明未支持，不得静默回退到旧 Command 路径。
- 修改公共契约时同步 path/Git driver、factory、ReuseDriver、生成选型路径，**同一次交付内全树编译绿**。治理决定（2026-10-21）：仓库无第三方驱动生态，`PROTOCOL_VERSION` 冻结在 6、`MIN` 停在 1，协议兼容门禁族（含规则表与其测试）退役删除；`MIN` 不下调是仍然成立的红线，判据见 [驱动层 §2.5](../architecture/backend/drivers.md#25-协议版本闸门)。

退出门槛：D 层 CM-07～19、22～26、30、45、48；不同 driver 不共享实现库类型；新增能力缺失不会 no-op 成功。**本门槛只对第一轮四个基本驱动断言**；其余驱动的对应断言属于 P10，不得在 P2 记录为已通过。

回退：过渡 adapter 可以支持旧 driver 的独立受控 Command，但不能开放未证明的 statefulSession。

「协议升级与驱动发布是原子兼容门槛，不允许用下调最低版本掩盖 breaking change」这句回退约定随生态现实收敛（治理决定 2026-10-21）：仓库已无第三方驱动生态，协议版本冻结在 6，上述两个机检门禁（`check-driver-protocol-compat.mjs` / `check-driver-protocol-release.mjs` 及其规则表与测试）**整体退役删除**——逐符号四档机检的维护成本在「零外部驱动」前提下不再划算。保留下来的红线是人工纪律：「不得下调 `MIN_PROTOCOL_VERSION` 掩盖不兼容」（`[MIN, PROTOCOL]` 窗口语义在 `driver_registry/registry.rs` 的 `register_from_inventory` 与各处 `protocol_version()` 比对中仍然生效）；契约改动改为「同一次交付内全树迁移 + 全树编译绿」。若未来重新出现外部驱动生态，恢复机检前必须先定案 `pv > PROTOCOL_VERSION` 分支的处置再接入（[驱动层 §2.5](../architecture/backend/drivers.md#25-协议版本闸门)）。

## 7. P3：统一 Session / Lease / Budget / ExecutionGateway

**状态：已完成**（2026-10-13）。单进程 runtime 实现及 CM-02～07、20～32、37～39、54～56、60 单机部分、61～74 的 H 层断言已在 `datazen-runtime` 验收。CM-60 按逐请求总开销 p95 ≤10 ms、失败数为 0、事件无重复/丢失判定，并记录实测环境；不要求固定 CPU / 内存参考规格。桌面旧 consumer 后续由 P4/P5/P6 迁移，连接生命周期统一改走 runtime。

**目的**：建立桌面与 server 都能用的资源运行时。

交付：

- SessionRegistry actor、状态机、owner/runtimeEpoch/contextRevision、独立 attachment 与内部 resourceBindingId。
- ResourceManager lease、pool、cleanup/quarantine、隧道引用。
- 多维单机预算、control/interactive/metadata/job 保留与公平队列、逻辑 session/已连接编辑器额度、真实物理资源计数。
- attachment 令牌与 TTL 最早期限裁决；无消费者有界产物、discard/drain、取消与协议恢复；PoolKey 版本生产者与缓存失效。
- 单进程 SessionDirectory、候选替换提交屏障、过期令牌拒绝与 Clean 双重检查。
- ExecutionGateway：授权、目标校验、schema、幂等 receipt、状态、来源、事件。
- 独立取消路径，取消和关闭先等待执行/协议结束；所有失败阶段 cleanup。
- 显式 session 失效和新 session 恢复；禁止原 ID 透明重连。
- 指标：物理资源数、lease state、预算等待、执行队列、取消耗时、cleanup 失败、Unknown outcome。

退出门槛（已满足）：H 层 CM-02～07、20～32、37～39、54～56、60 单机部分及 CM-61～74 的 H 断言；CM-73、CM-74 转绿，证明会话级句柄（事务/游标）在物理资源释放前已注销，事务状态、物理资源和 session ID 同步失效。race 测试使用 barrier/fake clock，不靠任意 sleep 猜顺序。CM-60 不绑定固定 CPU/内存规格。

回退：新 runtime 仅给已迁移 consumer 使用；不同时维护一条资源的旧 refs 和新 lease。服务暂停/重启前关闭新 session，不能将活事务移回旧 manager。

P3 前置阶段同步修订的 DTO/port 契约现已由 runtime 实现：`ArtifactRepository` 由 `packages/runtime/src/artifact/` 的 `FileArtifactRepository` 实现并接入桌面结果发布；写入期已发布块可读且不可覆盖，取消/失败可显式截断并保留恢复元数据。取消请求与执行终态分离，由 `ExecutionState::resolve_cancel` / `CancelOutcome` 表达并覆盖 CM-22 竞态。CM-60 基准按每个请求的两段附加耗时先求和，再以 nearest-rank 计算 p95；失败样本不从分母剔除。上述行为分别由 runtime artifact journey、CM-22 取消测试与 `cm60-bench` 实现和验证，详见共享边界 §4.6、夹具 §6.3/§11。

P3 收尾阶段曾保留旧的 `ConnectionManager`（当时 Tauri 仍有活跃生产调用方）；P6 迁移完成后它已连同其调用方一并删除，CM-73/74 的行为测试继续保留在 runtime 侧。

## 8. P4（已完成）：桌面 QueryPanel、TablePanel、元数据迁移

**状态：已完成**（2026-10-16，`main` @ `918d82902`）。关闭结论见本节末尾，F 断言的逐项证据见 §8.1；两条首版发布限制（空闲池在真实驱动上恒为空、首版不提供查询结果原地写回）也记在那里。桌面旧 consumer 后续由 P5/P6 迁移，连接生命周期最终统一到 runtime。

**目的**：先在现有产品中完成连接正确性，不等待 Web。

已实现的桌面接线：`QueryPanelSessionRegistry` 按 pane 管理独立 session；编辑器执行与上下文切换经过统一 runtime；TablePanel 的对象读取、ChangeSet 和显式事务分别使用目标操作与表面板 session；元数据缓存绑定身份、配置版本和目标。`ExecutionProjection` 按不可变块身份恢复结果，流重置先清除订阅游标，再读取权威状态和已发布块。页面销毁使用 `rollbackAndClose` 释放所属 session，避免活跃、失败或未知事务令无主 session 的关闭永久被拒；实际回滚与释放结果仍由 runtime 判定。

QueryPanel Explain 已通过同一 pane 的 `EditorSessionController` 提交 `explain` Command，结果通过统一 execution/artifact 读取；旧 `get_explain` IPC 与前端入口已删除。PostgreSQL、MySQL 显式声明该 Command，并在原 retained handle 上生成计划；PG/MySQL 不再为 Explain 从池另借连接。未声明该 Command 的驱动明确拒绝，不回落到旧池路径。SQLite 的底层 driver 仍有池级 `explain()`，但未声明 session-bound Explain 且 QueryPanel 固定 session 被 CM-19 拒绝，因此新 runtime 不会调用该池级入口。未来可通过 ResourceManager 借用一次性 SQLite 执行资源，在借用期间执行 `EXPLAIN QUERY PLAN`、发布 artifact/provenance 并确认释放；不得借此宣称 SQLite 支持跨命令固定 session。Explain 仅规划一条语句，不运行 ANALYZE，也不通过计划执行其中的 DML。

本轮聚焦验证：Host 相关前端 34 个文件/430 个用例、runtime P4 use-case 11 个旅程、Explain 相关 Rust 6 个用例通过。Community 实际应用的通用 SQL 27 个用例及 PG/MySQL retained-resource 旅程各 1 个通过；另有 PostgreSQL 实际 WebDriver UI 旅程在同一编辑器内切换两个可连接数据库 6 次，并在每次切换后执行 `current_database()`，证明会话替换释放没有耗尽默认五个编辑器额度。Pro 实际应用 SQL 27 个、statement 6 个、参数面板 9 个用例分别通过，SQL Editor Pro 包内单元测试 71 个文件/894 个用例通过。参数面板验收按 PostgreSQL 声明的方言策略检查：不将未声明的 `?` / `@name` 当作绑定参数。

补充确定性验收：`p4_usecase_journeys` 通过公开 application API 记录候选失败、旧资源继续执行、候选成功建立、旧资源关闭的物理 journal，并交错双编辑器读取 100 轮（200 次），逐次验证数据库和固定 resource；替换后产物/来源保留、旧 runtimeBinding 失效。另以真实 runtime artifact store 验证漏读已发布块→取消→补块/重读去重→原资源显式新读，只提交原命令一次。CM-72 的 unsupported/requested/alreadyFinished 回执与执行效果分离，registry 非敏感能力版本审计通过；前端拒绝旧 `accepted` 回执、无效大小写、无效状态和异 executionId。结果视图在 binding 失效或换 execution 时关闭已打开输入，并在提交入口再检查可写性。补充前端聚焦 17 文件/218 用例、registry audit 11 用例、SQLite 当前 resource 契约 36 用例与最终 typecheck 通过；这些不能替代相同旅程的真实 UI/驱动层验收。

**关闭依据**（2026-10-16：F 断言已有逐项映射，见 §8.1；下列是本轮补齐的真实旅程与边界结论）：

- §12.1 的「实际 UI 丢块/断线→取消→恢复→再次执行」已有真实故障注入验收：`e2e/specs/journeys/query-fault-recovery-journey.ts`（PostgreSQL，用独立的第二会话执行 `pg_terminate_backend` 杀掉编辑器正在使用的后端）。同一条连续旅程断言：多批部分结果先落地 → 精确取消（取消成功不显示失败，`errorBeforeKill=false`）→ 断线后失败**必须可见**（错误面板非空 **且** 显式「打开新会话」横幅同时出现，静默结束或两者只出现一个都判红）→ 用户显式开新会话（禁止原 ID 透明重连）→ 同一编辑器再次执行成功，且新后端 `backend_start` 严格晚于被杀后端（证明是替代而不是复用）。实测两次均 `1 passing`、EXIT=0。
- SQLite Explain 的拒绝边界由驱动 crate 内 `explain_is_pooled_only_and_not_a_declared_session_command` 钉住（`packages/drivers/sqlite/src/lib.rs`）：池级 `supports_explain()` 仍为真，session-bound `explain` Command 未声明，`execute_command` 对它明确返回 `Unsupported`。
- CM-67 的收口结论见 §17.1 与连接管理 CM-67：H 层已落地并有实测，空闲池在首版真实驱动上恒为空是**能力事实**（全部 path 驱动 `ResetForReuse::Unsupported`）而不是实现缺失，那两条不作为 D 层断言。
- F 断言不再只有汇总：§8.1 逐条给出证据，本轮补齐 5 个真实 UI 旅程（编辑器会话隔离、结果视图消失不取消、查询故障恢复、查询结果写回边界、表格并发与池计数）与 1 个真实驱动旅程（池键与轮换），全部实测通过。同时确认 CM-35/36/61 的**正向**写法在首版不可达：`WritableMapping::Verified` 要求 `context_before.confidence == Confirmed`，而确认要求驱动声明 `ContextObservation::Full`——**当前没有任何内置驱动声明**（postgres / mysql / sqlite / redis 均 `Partial`，其余 `Unsupported`）。真实 UI 层因此只能断言可达的那一半（只读、替换后仍可阅读），"可写结果在替换后失效"留在组件层。
- P4 自己引入的旧取消路径已清理：`panelExecCleanup` 不再向旧 `cancel_query` IPC 发取消，改走该 pane 自己的 `EditorSessionController`；`cancel_query` IPC 本体仍留给旧 `execute_query*` 消费者。
- CM-33 的 F 半补上了：新增只读观测口 `runtime_observability_snapshot`（runtime 侧 `resource/observability.rs`，经 `SessionBackend::observability_snapshot` 投影自有台账），真实 UI 旅程 `e2e/specs/journeys/table-pool-contention-journey.ts` 用它断言"4 个表 tab 逐个刷新期间物理资源不越预算、不随 tab 数增长，编辑器随后仍可执行且观察上下文不变"。在做它之前，那个数字在外部无法归因——同库上旧 `ConnectionManager` 会话与 runtime 会话在 `pg_stat_activity` 里分不开（旧 `ConnectionManager` 已在 P6 迁移后删除）。

**关闭结论（2026-10-16）**：

1. §8.1 的 F 断言映射完成，每一类都有明确落点：`F 已覆盖` / `组件级（已定级）` / `桌面不适用（移交 P7）` / `用例无 F` / `首版发布限制`。
2. 三条残余都已定级（见 §8.1「残余处置」）：CM-16 组件级、CM-63 桌面不适用移交 W1、CM-35/36/61 正向 = 首版不提供查询结果原地写回的产品决定。
3. 受控 WDIO 子集已实跑并逐条归因：`--suite smoke` 26 passed / 2 failed（28 文件）、`--suite db` 20 passed / 7 failed（27 文件）、`--suite connection-pool` 1 passed。本轮新增/修改的旅程全绿；子集内的失败**不属本轮改动**——3 个文件在基线 `f06ad7e5b` 上同样失败（`data-types`、`data-transfer-window`、`host-contract-matrix`），4 个仅套件内失败且隔离跑通过，`export-import` 重复 3 次为 2 通过 / 1 失败（测试自身"文件已建但未 flush"的竞态）。CI 不跑 E2E，这些证据只来自本地实跑。

**关闭时记录的两条首版发布限制**（详见连接管理 CM-67 与 CM-36）：空闲池在真实驱动上恒为空（全部 path 驱动 `ResetForReuse::Unsupported`，不进池、直接关闭）；首版不提供查询结果的原地写回（无驱动声明 `ContextObservation::Full`，`WritableMapping::Verified` 因此不产生）。

**不属 P4（后续阶段或独立清理项）**：旧管理器（`ConnectionManager`）的删除按消费者迁移决定，属 P6/P10；P6 已完成迁移并删除该管理器，Workflow 非 SQL 步骤、Monitor、AI、MCP、迁移三件套、backup/export、旧 `execute_driver_command` 网关随之改走 runtime；`main` 基线自身的三个失败 E2E 文件与 `export-import` 的测试竞态是独立清理项，不阻塞 P4。已存在的单元测试或新 runtime 接线不代表全部验收通过——§8.1 的"F 已覆盖"只表示证据存在且实跑通过，适用层与限制逐条列在该节。

#### 8.1 P4 F 断言逐项证据映射（2026-10 实测）

把 §8 退出门槛列出的 F 断言逐条落到具体证据。`F 已覆盖` = 真实 UI 层有可执行断言；`组件级` = 组件/单测覆盖但未做真实旅程；`用例无 F` = 连接管理 §16 该条本身不含 F 层断言。五个 Host 旅程与一个驱动旅程（全部实测 `1 passing` / `EXIT=0`，跑测于 2026-10）的运行方式：

```bash
pnpm e2e:skip-build -- --spec e2e/specs/journeys/editor-session-isolation-journey.ts      # CM-08/09/11/12/17
pnpm e2e:skip-build -- --spec e2e/specs/journeys/result-view-loss-journey.ts             # CM-64 无消费者
pnpm e2e:skip-build -- --spec e2e/specs/journeys/query-fault-recovery-journey.ts         # CM-56/64 断线→替代→再执行
pnpm e2e:skip-build -- --spec e2e/specs/journeys/temporary-result-writeback-journey.ts   # CM-35/36/61 写回能力边界
pnpm e2e:skip-build -- --spec e2e/specs/journeys/table-pool-contention-journey.ts        # CM-33 表格并发与池计数
pnpm e2e:skip-build -- --spec packages/drivers/postgres/e2e/connection-pool-rotation.ts  # CM-38/67 D 层（或 --suite connection-pool）
```

| 用例 | F 断言落点 | 证据 | 结论 |
| --- | --- | --- | --- |
| CM-08 两个编辑器隔离 | UI 上下文独立、交错读取各自固定 | **新增** `editor-session-isolation-journey`（同一连接两个 tab 各切一库，交错读 5 轮，两个 selector 各保持自己的库）；H：`p4_usecase_journeys::cm08_cm15_cm16_replacement_journal_and_100_interleaved_reads` | F 已覆盖 |
| CM-09 跨调用临时状态 | 原会话可读、另一会话不可访问 | **新增** 同一旅程（同库 temp 表在自身 tab 读到 42，兄弟 tab 明确报缺失关系且之后仍可用）；D：`packages/drivers/postgres/e2e/retained-resource-journey.ts` | F 已覆盖 |
| CM-11 USE 失败 / CM-12 成功后失败 | 失败不移动已观察上下文 | **新增** 同一旅程（限定名读取与失败语句后，selector 与 `current_database()` 仍是原库）；E2E `sql-query.ts SQ-CTX-001`。产品已无 `USE` 切换路径，等价断言即 CM-13 形态 | F 已覆盖（等价形态） |
| CM-13 限定名与补全不切库 | 补全可变、observedContext 不变 | E2E `sql-query.ts` `SQ-CTX-001`（限定名查询后编辑器上下文不变） | F 已覆盖 |
| CM-15 PG 切 database 替换 | 先成功后替换、旧 ID 不能执行 | E2E `sql-query.ts` `SQ-LIFECYCLE-001`（6 次替换 + 每次 `current_database()`，证明旧资源释放）；单测 `sessionReplacement.test.ts` | F 已覆盖 |
| CM-16 替换失败保留旧连接 | UI 不显示 B、旧会话仍有效 | 单测 `sessionReplacement.test.ts::leaves the previous handle bound …`；组件 `queryPanelSessionSwitch.test.tsx::prompts a revision conflict and keeps the previous database`；H：`p4_usecase_journeys::replacement_rollback_revision_commit_and_replay` | **组件级（已定级）**：不做真实 UI 候选失败注入，理由见下 |
| CM-17 手写事务旅程 | UI 同步事务、另一会话不可见 | **新增** 同一旅程（SQL `BEGIN`→`INSERT`→本 tab 读到 1→独立会话读到 0→`ROLLBACK`→两侧 0）；E2E `client-parity.ts`（旧 IPC 三件套）；单测 `TableView.test.tsx` | F 已覆盖 |
| CM-18 aborted/unknown | 不显示为 none、切库被阻止 | 组件 `query.modules.test.tsx`（unclosed/aborted 对话框）、`sqlTransactionGuard.test.ts`；单测 `pageUnload.test.ts`（页面死亡按事务态回滚）；E2E `table-session-loss-recovery.ts`（`resourceLost`+`unknown` 回执→显式新会话） | F 已覆盖 |
| CM-21 切库与执行竞态 | 冲突不吞、提示明确 | 组件 `queryPanelSessionSwitch.test.tsx`（revision 冲突保留旧库）、`sessionPrompts.test.ts`；H：`p4_usecase_journeys` | F 已覆盖 |
| CM-33 多表 tab 共用短操作池 | 编辑器资源/上下文不受表格刷新影响 | **新增** `table-pool-contention-journey.ts`（真实 UI：4 个表 tab 逐个打开并刷新，每次采样都断言物理资源 ≤ 预算且 ≤ 基线 + 2，编辑器随后仍可执行且观察上下文不变）；计数来自 **新增只读口** `runtime_observability_snapshot`（`packages/runtime/src/resource/observability.rs` + `SessionBackend::observability_snapshot`），因为同库上旧 `ConnectionManager` 会话与 runtime 会话无法用 `pg_stat_activity` 区分；H：`pool_capacity_budget.rs`（池容量与预算台账一致） | F 已覆盖 |
| CM-34 表编辑事务范围 | 暂存/预览/回滚/提交、失败两条同回滚 | E2E `table-edit.ts`（`DE-006~DE-008` 暂存/预览/回滚/提交）；组件 `TableView.test.tsx`（显式事务开关） | F 已覆盖 |
| CM-35 旧结果正确归属 | 可写映射提交原目标、任意 SQL 只读 | 组件 `ResultTableView.test.tsx`（verified 结果在 binding 刷新前只读）；单测 `executionRecoveryJourney.test.ts`（切库使绑定失效并再次执行）、`metadata.test.ts::isProvenanceCurrent`；E2E `query-artifact-export.ts`；**新增** `temporary-result-writeback-journey.ts`（`SELECT * FROM <单关系>` 的真实结果在替换前后都不可编辑） | F 已覆盖（只读方向；正向在首版不可达，见下） |
| CM-36 临时对象结果失效 | 关闭 S 后编辑旧结果即 `SessionLost`、结果仍可读 | 组件 `ResultTableView.test.tsx`（标注 CM-36/61：原绑定失效时关闭已打开编辑器且绝不提交过期编辑）；**新增** `temporary-result-writeback-journey.ts`（真实 UI：会话替换后结果仍可阅读、仍不可编辑、无错误面板） | 组件级（正向）+ 真实 UI（可读方向）；正向在首版不可达，见下 |
| CM-37 缓存竞态与权限隔离 | —（用例标 H） | 前端最接近证据：`metadataCache.test.ts`（乱序读被拒、失效、epoch）、`metadata.test.ts`（按 connection/config/target/dbSession 隔离） | 用例无 F；H 见左 |
| CM-38 配置修改与凭据轮换 | —（用例标 H/W） | **新增 D**：`packages/drivers/postgres/e2e/connection-pool-rotation.ts`（真实 PG：CAS 轮换后 `get_session` 与 `execute_in_session` 报 `configRevisionMismatch`，`close_session` 仍 `confirmed`，新会话可用且物理后端归零） | 用例无 F；D 本轮补齐 |
| CM-39 配置禁用/删除 | —（用例标 H/W） | H：`resource_rotation_and_disable.rs::disabling_a_connection_drops_queued_requests_and_records_the_actual_cancellation`；W 属 P7 | 用例无 F |
| CM-55 事件重复/乱序/超大 revision | 旧事件不覆盖、不重复、缺口重读 | 单测 `sessionJourney.test.ts`（`rebuilds and clears the cursor for stream reset …`、`deduplicates large sequences, retries unread blocks, recovers gaps…`、`rejects a mismatched first chunk…`）、`executionRecoveryJourney.test.ts`、`runEditorQueryArtifacts.test.ts` | F 已覆盖（单测）；超大 Counter 的 W 属 P7 |
| CM-56 网络掉线与事务 TTL | 掉线不伪恢复、下一执行要求新 session | E2E `table-session-loss-recovery.ts`（杀掉后端→回执→显式新会话→再事务）；**新增** `query-fault-recovery-journey.ts`（流中杀后端→失败可见→替代 session→再执行，新 `backend_start` 严格更新）；TTL/心跳为 H（fake clock） | F 已覆盖（掉线）；TTL 属 H |
| CM-61 持久化白名单与临时结果恢复 | 普通表重授权可写、临时结果只读 | 组件 `ResultTableView.test.tsx`（CM-36/61 同条）、`metadata.test.ts`（provenance 过期判据）；**新增** `temporary-result-writeback-journey.ts`；白名单本体 H：`p4_usecase_journeys` artifact 旅程 | F 已覆盖（只读方向）；重授权后可写在首版不可达，见下 |
| CM-63 Attachment、TTL 与竞态 | —（桌面无第二 attach 方） | F 侧仅页面死亡：`pageUnload.test.ts`（页面死亡按活跃/aborted/unknown 事务请求回滚）；attach/TTL/竞态为 H（`directory_attachment_ttl.rs`）与 W1 | **桌面不适用（已定级）**：F 半在桌面没有可触发的第二 attach 方，attach/TTL/竞态移交 W1（P7） |
| CM-64 无消费者、截断与有界 drain | 视图消失不取消、返回可读、截断可见 | **新增** `result-view-loss-journey.ts`（切走标签页=视图消失：语句仍在跑→跑完→返回后 300 行恢复且 `data-execution-seq` 不变，证明未重投）；**新增** `query-fault-recovery-journey.ts`；截断文案 E2E `edge-cases.ts`（`query.resultTruncated`）；协议 barrier/配额为 H/D 夹具 | F 已覆盖（无消费者与恢复）；"有界"的量值属 H/D |
| CM-72 错误、控制回执与能力审计 | 回执/错误码一致、effectOutcome 不被覆盖、失效绑定不提交 | 单测 `packages/backend-client/__tests__/{client,errors}.test.ts`（拒绝旧 `accepted`、大小写与状态、异 executionId）、`ResultTableView.test.tsx`（失效绑定不提交）；**新增** `query-fault-recovery-journey.ts`（替换后按执行状态与产物读取，失败可见且不静默）＋ `temporary-result-writeback-journey.ts`（替换后只读、编辑入口关闭） | F 已覆盖（回执与失效绑定）；durable provenance 的**能力快照字段**（driverId/version、protocolVersion、capabilityRevision、confirmed evidence）由 H `p4_usecase_journeys` 断言——它们在 UI 上没有稳定的 DOM 面，不写一条读不到的快照断言 |

**首版能力边界（不是待办）**

- **查询结果写回的正向断言（CM-35/36/61）在首版对所有发布驱动不可达。** `WritableMapping::Verified` 只在 `context_before.confidence == Confirmed` 时产生（`src-tauri/src/platform/session_backend/execution.rs`），而 `Confirmed` 要求驱动声明 `ContextObservation::Full`（`packages/driver-api/src/capabilities.rs::confirms_context`）。当前没有任何内置驱动声明它：postgres / mysql / sqlite / redis 为 `Partial`，hbase / mongodb / vector 为 `Unsupported`。因此"可写结果在会话替换后失效"没有可触发的真实状态，用真实 UI 写它只会得到一条永远为真的空断言；该断言留在组件层（`ResultTableView.test.tsx`，标注 CM-36/61）。真实 UI 层断言的是可达的那一半（只读、替换后仍可阅读），见 `temporary-result-writeback-journey.ts`。只有某个驱动能确证上下文后，这条正向断言才重新具备 F 层可验证性。

**残余处置（已定级，不再作为待办）**

- **CM-16（真实 UI 的候选失败注入）——组件级定级。** "替换失败保留旧会话"由组件测试（`sessionReplacement.test.ts`、`queryPanelSessionSwitch.test.tsx`）与 H 层物理 journal（`p4_usecase_journeys::replacement_rollback_revision_commit_and_replay`）覆盖。真实 UI 注入需要在切库过程中让候选连接失败（例如先把端口改错再切回），与 `edit-delete-connection` 的配置突变夹具重叠，收益低于已完成的 CM-08/09 隔离旅程；**决定不做**，理由与覆盖范围如上。
- **CM-63（另一 attach 方）——桌面不适用。** 桌面没有第二个 attach 方，F 侧只剩页面死亡回滚（`pageUnload.test.ts`）；attach / TTL / 竞态属 W1，**移交 P7**。
- **CM-35/36/61 的正向断言（可写结果在会话替换后失效）——首版发布限制。** 产品决定：**首版不提供查询结果的原地写回**（能力门见上一节）。因此这条正向断言在真实 UI 上不可触发，留在组件层；它不是"未做"，而是"首版不提供该能力"的记录。某驱动确证上下文后需重新评估该决定与断言。

交付：


- QueryPanel 每编辑器独立懒 session，复制 tab 不复制 runtime ID。
- 删除执行前从 SQL 推测已生效上下文的行为，保留补全与浏览推测。
- 下拉切库支持确认、revision 冲突、两阶段替换和事务处理。
- TablePanel 使用明确对象 target；ChangeSet 保存时短事务，手工事务显式开关。
- 元数据批量与缓存按身份/配置/目标隔离；临时对象绑定原 runtime handle。
- 旧结果 provenance、读写映射与临时结果失效规则。
- 关闭 tab、断线、错误后的继续操作和用户提示；开发只新增/修改 en.ts。

退出门槛：CM-08～18、21、33～39、55～56、61～64、72 的 F 断言；Host 通用 WDIO + driver 专属旅程；桌面 Community 与 Pro consumer 兼容验证。

回退：切换以 consumer 为单位，启用前 session 全部重新建立；不能在活动事务中切 feature path。存在数据写入未知时先核验，回退 UI 不等于数据库回滚。

补充交付与门槛：按连接设计 §12.1 实现块去重、缺块恢复与旧事件隔离；执行→部分结果→取消/断线→恢复→再次执行的连续旅程须覆盖切库、替代 session 和关闭结果视图。旧 consumer 在对应迁移验证完成的 PR 中删除；P10 只做最终残留核验。

## 9. P5（已完成）：JobRuntime 与数据迁移三件套

详细实现：[迁移三件套与 JobRuntime](../architecture/platform/data-migration-jobs.md)。P5 于 2026-10-09 通过数据迁移三件套独立测试与冒烟测试，阶段关闭，P6 前置条件满足。

已落地事实：

- 桌面 `AppDb` 使用 SQLite v2 Job 仓储；共享 `JobHost` 提供 durable accept、幂等 receipt、状态/进度/结果、取消、恢复核验和后台 dispatch。
- Schema Diff、Data Sync、Data Transfer 的准备与应用均接入 JobRuntime。领域实现位于 `packages/schema-diff`、`packages/data-sync`、`packages/data-transfer`；Tauri IPC 与桌面适配保留在 `src-tauri/src/commands/`。
- Job 结果记录提交边界和 `effectOutcome`，区分未开始、已回滚、部分提交、未知和完成；apply `planId` 单次消费，worker 写入受 claim 代次校验。
- 任务窗口关闭后，后台 Job 继续运行；窗口重开可读取任务状态和结果。应用重启不会自动重放旧 worker：运行中的任务进入待核验，未派发任务标记为未执行。
- 迁移三件套独立测试和冒烟测试通过，P5 退出门槛已关闭。

本阶段边界：完整计划正文与部分审阅态仍为桌面进程内状态；应用重启后需要重新准备计划或执行只读恢复核验。服务端 ArtifactRepository、团队 Web、多 worker 自动接管仍属于后续阶段，不影响 P5 关闭。

## 10. P6：Workflow、AI、MCP、Wapp 与辅助任务

P5 退出门槛已关闭，P6 前置条件满足，可以启动。

2026-10-17 实施状态：Workflow/AI/MCP/Dashboard/metadata/export/backup 的共享领域代码与 settings 契约已抽入共享 packages；桌面保留 IPC、native/session identity、仓储和既有 P5 job adapter。表导出与数据库备份已接入桌面 Job/Artifact 交付；PostgreSQL/MySQL 备份执行已接专属 Runtime retained session / `backup_dump`，有界 chunk 同步背压写入 staging/gzip，保留 Job/Artifact 交付；其他驱动失败关闭，实库/桌面验收仍须独立核验，恢复已接单次消费的 apply Job，PostgreSQL/MySQL 的受限 SQL 格式走固定 Runtime session / ExecutionGateway。最终残余核对见 [feature-core](../architecture/platform/feature-core.md#final-residual-audit-2026-10-17) 与 [消费者边界 §8.1](../architecture/platform/consumer-adapters.md#81-当前共享实现与保留边界2026-10-17)。这不是 P6 关闭：Workflow v1 跨库 Runtime 路由、授权调度/native-tool 预算及真实驱动旅程验收仍有阶段工作；本轮提取已在隔离快照复验通过。

详细设计：[Workflow 资源模型](../architecture/platform/workflow-resource-model.md) 与 [消费者接入](../architecture/platform/consumer-adapters.md)。共享授权失效后拒绝绑定操作，MCP/Wapp 只释放自有资源，调度目标和服务授权显式保存，原生工具也计入完整预算。

**目的**：消除 GUI 外入口的隐式目标和共享状态。

交付：

- Workflow 保持 v1 独立 step 与既有模型/协议，底层连接派发使用共享 Runtime。session/transaction block 和 Workflow 专用 Job 不在本轮范围。
- 保持既有 connection/namespace、跨库步骤和 step output/template chaining 语义，验收底层 Runtime 替换不改变目标与结果。
- AI 默认独立操作，显式授权当前 editor session 时同队列。
- MCP client/session owner 与 TTL，Wapp 实例和任务归属。 MCP 的在途 open 由独立 owner task 与未交付回复 guard 持有，请求取消或断连后的晚到资源转入原 owner 清理；Host Runtime fixture 覆盖该故障路径，PG/MySQL 实库和传输验收仍独立进行。
- Dashboard/Monitor 独立资源；导出、备份、恢复归 Job/Artifact；原生工具由环境 port 管理。
- 服务身份/委托权限用于调度，所有入口走 ExecutionGateway。

退出门槛：CM-50～53；辅助任务共享预算、不污染编辑器；原 Workflow 的 query/command 默认目标契约仍通过。

回退：旧 Workflow 定义按版本解析，默认独立 step 语义；新 block 格式不交旧 executor。后台任务取消与运行状态通过 Job 查证，不用 GUI 关闭推断。

Workflow 验收按既有 v1 行为进行：跨连接/跨库逐步派发、前步输出供后步模板使用、错误/超时不重放已确认副作用；不以 block 目标设计扩展本轮协议。其他消费者继续独立验证 owner、授权、TTL 和 Gateway 拒绝路径。

Monitor occurrence 当前状态：周期执行已先受理 SQLite durable `monitorOccurrence` Job，严格投影保存 schedule、定义版本、计划时间、授权身份与只读目标范围；`scheduleId + definitionRevision + scheduledAt` 对应幂等 receipt。SQLite 重开后重复提交返回原 receipt，重复 tick 不再派发；启动恢复将未完成 Job 标记为待验证并要求人工复核，不恢复 handler 或重放查询。已有 codec 身份一致性、SQLite 跨重开 receipt 和变更重放拒绝测试；PG/MySQL 的共享策略/Runtime 查询层现另有 live 证据，但不表示多 worker 接管或完整桌面 Monitor 调度旅程已验收。

恢复 Apply Job 当前状态：已接受控 Artifact 输入、冻结 target/revision/input digest、单次 plan 消费和 JobRuntime 写预算；SQLite admission 已验证真实 typed payload、拒绝未知/不一致元数据且失败不消费 plan；PostgreSQL/MySQL 已接固定 Runtime Session / ExecutionGateway 的受限 SQL 恢复与提交证据；COPY/客户端指令不在此格式内，其他驱动至 P10 明确 Unsupported/NotStarted。PostgreSQL 已有 driver 专属 live 旅程，复用共享 production RuntimeRestoreGateway/Runtime Session/ExecutionGateway/JobRuntime 并独立读回，覆盖单事务提交、自有 ROLLBACK 与无残表、COPY/客户端格式及冻结输入/目标/revision 拒绝；MySQL 也已有 driver 专属 live 旅程，覆盖非事务 CREATE/INSERT 的逐句提交和独立读回、LOAD DATA/SOURCE 整份拒绝，以及 DDL 后 SQL 失败保留已确认边界并独立证明残表；失败语句效果缺少驱动确定证据时总体保留 Unknown，不宣称跨 DDL/DML 原子性。桌面 picker/Store/DesktopSessionBackend 接线不在这些旅程验收范围，具体边界见 [Job §2.5](../architecture/platform/data-migration-jobs.md#25-restore-apply-job已接线驱动执行失败关闭)。

桌面 Restore 入口边界补充：Host 组合旅程复用生产 staging/upload helper，并使用真实 Store/PlatformAdapter 注入的 ArtifactRepository、DesktopSessionBackend 与 SQLite JobHost，验证冻结文件输入、owner/组织隔离、实际 SQL 派发和提交边界、durable plan 重复消费拒绝，以及 revision/read-only/safe-mode 拒绝。provider 为宿主接线 fixture；原生 picker/Tauri 事件和真实数据库效果没有由该测试冒充覆盖，后者继续由 driver live 旅程负责。

## 11. P7：单实例团队 Web 服务

详细设计：[团队服务与认证](../architecture/platform/team-server-and-auth.md)、[持久化模型](../architecture/platform/persistence-model.md) 与 [部署、升级和恢复流程](team-service-operations.md)。运维验收使用发布镜像，备份恢复不得自动重放未知外部写入。

**目的**：先交付可部署、可授权的统一团队入口。

交付：

- server host，无 Tauri 依赖，初期同进程管理与执行。
- OIDC、membership、服务端登录会话、RBAC/资源 ACL、CSRF、同源 HTTPS 部署。
- ProfileRepository/JobRepository/Audit/ArtifactRepository 的服务端实现和 schema migration。
- secretRef/轮换、个人/团队执行身份、出站网络政策。
- 概要设计 v1 HTTP API，幂等提交、SSE 重订阅、分页/大小限制、受控上传下载。
- React Web shell：窗口替换为页面/面板，文件和剪贴板使用 Web 平台实现。
- 配额、任务中心、结果 TTL、配置版本 CAS、权限撤销传播。
- API schema/client 生成或契约一致性门禁，桌面 IPC 与 HTTP 同用例测试。

退出门槛：CM-05～07、38～40、53～56、59、60 单实例部分及 CM-63、68、70、72 的 W1 断言；浏览器无需数据库凭据；组织隔离、CSRF、产物与事件越权全部拒绝。

发布门槛：部署文档明确管理数据库、SecretProvider、网络可达性、备份、迁移、TLS 和资源额度；管理员能恢复服务元数据，活动数据库会话不声称可恢复。

回退：保留上一版本镜像和向后兼容 schema；数据库 schema 采用 expand/contract，不先删字段。drain 活任务后回退；发生外部数据库写入不能通过回退镜像撤销。

上线前冻结并验收：实际授权变更与 permissionVersion 同事务提交；普通 DDL 与历史登记同事务，事务外索引迁移可核验恢复；schema 支持范围允许旧镜像运行于下一版 expand schema；授权可见性先于会话存活错误；SSE 缺失 Origin 有明确校验；Web Wapp 使用独立真实 origin、可嵌入 CSP 与真实 iframe 握手。每项均有对应浏览器/管理库故障测试，不以 fake 替代。

## 12. P8：桌面团队客户端与首版发布

详细设计：[团队服务 §4.6](../architecture/platform/team-server-and-auth.md#46-p8-桌面团队认证与传输目标设计) 定义桌面登录及原生传输；[运维设计](team-service-operations.md) 定义兼容、升级和恢复演练。

**目的**：同一客户端可选择本地或团队 backend，避免句柄和凭据混用。

交付：

- backendId、连接来源、版本握手与 per-backend client。
- 桌面登录团队服务、HTTPS/SSE 生命周期；团队 session 不经本地 Driver 执行。
- 本地 profile/session 与团队 profile/session 独立索引；服务退出不删除本地配置。
- API major/minor 兼容提示；不兼容阻止写操作，不能默默降级。
- 第一版同 backend 迁移限制，选择跨 backend endpoint 明确拒绝。
- Community/Pro 桌面及团队发行 capability 清单；高级扩展仍经原有 EP 契约。
- 发布前 en.ts source key 检查与翻译补齐、功能使用和部署文档。

退出门槛：桌面离线本地可用；桌面/浏览器团队同用例一致；本地 ID 发往团队服务拒绝；团队秘密不存入本地普通 profile；CM-55/56 与 backend 隔离旅程通过。

回退：移除远程 backend 绑定前显式关闭/保留远程任务；本地状态不受影响。不能把 remote SQL 转为本地执行来“降级”。

认证和传输以团队服务 §4.6 为权威：系统浏览器 OIDC 与 PKCE 一次性交接、native 不透明会话凭据、钥匙串分区、HTTP/SSE 共用 native adapter；浏览器 cookie 路径仍独立。补交付 backendId/clientGeneration 的迟到事件隔离、认证过期/撤销与证书错误处理、版本/能力握手。发布门槛增加交接重放/恶意回调/认证中服务重启、双 backend 同名 ID、登出后 SSE 重连与未知写入不重投的连续旅程。

## 13. P9：多 worker、网络分区与全局预算

详细设计：[多 worker 协调协议](../architecture/platform/multi-worker-coordination.md)。首版目录为单权威内存服务，Job/数量型节点预算在管理库持久化；目录切换先隔离旧实例，全部旧 session 失效。该控制面可用性限制必须进入发布说明。

**目的**：扩容不破坏会话连续性和总连接上限。

交付：

- RuntimeRouter、SessionDirectory、owner worker epoch、认证内部调用。
- worker 能力/版本/网络区域匹配，固定 session 路由，替代 session 新 ID。
- 总预算节点分配/许可机制，idle/控制/cluster 资源统计与分区保守回收。
- Job claim/renew/fencing、失租停止派发、接管前目标核验。
- worker drain、滚动升级、任务恢复和元数据/权限事件失效。
- 压力、故障注入、协调器失联、worker 暂停后恢复测试。

退出门槛：CM-57～60 的 WN 部分、CM-68/71 多 worker 断言；关闭 sticky 后仍正确；worker 故障不伪恢复事务；旧 worker 隔离未证明前不复用额度；提交未知不盲目接管。

回退：停止新任务/会话路由并 drain 至单 worker；不迁移 live session。多实例元数据可保留，但执行 owner 必须明确，不能把目录清空后继续使用旧 ID。

实施协议见 [多 worker 详细设计](../architecture/platform/multi-worker-coordination.md)，概要 §9.2 保留边界摘要：目录 owner/替换 CAS、仓储 claimGeneration、全局许可 generation 与保守核销、协调器失联、目录丢失和 drain 状态转换。P5 的管理库 fencing 与 P9 的全局预算许可分别验收，均不能自动撤回外部 SQL。退出门槛增加旧 worker 暂停超过租约后恢复、在途 commit、目录/协调器重启和许可未核销时禁止再分配。

## 14. P10：扩展性验收、旧路径删除与稳定化

详细验收按「冻结选型 → 逐驱动验证 → 证据与发布判定」三段执行：

- **冻结选型**：从 `drivers-registry.json` 与 `scripts/resolve-drivers.mjs` 的实际解析结果取得选定 driverId、path/git 来源、版本与 DB entries。Git ref 必须解析为精确 commit SHA；同时冻结 host commit、edition、Driver API 版本、driver crate/features、UI SDK 版本与构建摘要。可移动 branch/tag 不能作为唯一复现依据；驱动子仓库有未提交修改时不计为可复现发布构建。
- **逐驱动验证**：按 driver 实现记录，并为其暴露的不同 dbType 分别跑适用方言路径；复用同 crate 不代表各服务器产品的事务、取消与上下文行为完全相同。path/Git 来源不改变契约门槛。Git 驱动测试仍落在其独立仓库 `packages/drivers/<id>/` 的实际 crate/tests/ui/e2e 中；Host 不复制方言测试，也不假设所有外部 crate 命名为 `datazen-driver-<id>`。需要修改外部驱动时独立提交该仓库，再更新 registry 钉选版本；宿主提交不能代替外部提交。
- **证据与发布判定**：每格保存 driver/host 精确版本、服务器产品及版本、能力声明、适用 CM 编号与层、运行入口、测试计数、退出码及限制；结果分为 verified、unsupportedVerified、notRun、failed，能力声明与验证结果分列。unsupportedVerified 要证明调用明确拒绝且没有目标副作用；notRun 仅表示未执行，不能转成 unsupported；failed 阻塞该能力发布，notRun 的能力不得列为已验证支持，发布时禁用或明确收窄支持范围。W1/WN 不能由 H fake 推断，真实驱动恢复不能由单元序列化测试推断。至少选一个冻结 SHA 的 Git driver 跑完整适用路径，并运行每个实际随发布分发的 driver。
- **测试配置边界**：测试适配使用现有 `ci-private-drivers.md` 与独立驱动指南；程序可读取其合法测试配置，Agent 不读取或回显 `.env` 内容。没有仓库访问或 live 服务时记录未验证，不能编造支持事实。私有 repo URL、认证材料与数据内容不进入公开矩阵。

实际发布选型覆盖 path 与 Git driver，Git ref 冻结为 SHA，能力声明与逐层运行结果分别记录。

**目的**：验证架构目标，而不只验证代表驱动能工作。

交付：

- 选择一个实现既有能力的新增/适配 driver，限定改动在 driver 包、注册选型、schema/UI/文案和 driver 测试。
- 运行通用 contract × driver 矩阵，确认 unsupported 路径明确拒绝。
- **补齐 P2 第一轮未覆盖的 path/Git 驱动**：把其余驱动迁移到 ResourceProvider 与可选能力注册，或显式声明不支持对应能力；本项完成前 `--drivers=all` 不作为发布选型，只作为开发期验证入口。
- **迁移其余驱动的语句结果摘要**：MySQL/PostgreSQL 已由 ResourceProvider 的查询、流式查询和 execute ACK 路径直接发送 `StatementSummary`；其余 path/Git 驱动需将成功语句总计改为 typed summary，`ResultChunk` 只携带非空行批次。所有发布选型均迁移并通过对应 contract 后，删除 Host 对空旧 `ResultChunk` 的兼容归一化。
- 删除 consumer 的旧共享 session/reconnect/双执行路径，删除 SDK 重复实现。
- 更新 Driver 公共协议、独立驱动指南和已实现架构；平台计划及关联设计逐节转为事实/流程。
- 采集 fake runtime 基准、真实典型负载和资源计数，按实测调整默认预算，不承诺跨环境 SQL 延迟。

退出门槛：详细设计 CM-01～74 全部适用门槛，新增 driver 不改宿主数据库分支；未能执行的真实环境验证明确列入发布限制，不能称全部通过。**`--drivers=all` 下每个驱动的能力声明与 unsupported 拒绝路径逐个核对**，未迁移驱动必须以显式拒绝通过，不得以静默回退计入已通过。所有入选驱动的成功语句必须恰好发布一次 `StatementSummary`，不得用空 `ResultChunk` 表示摘要；该条件满足后才能移除 Host 兼容归一化。

回退：只回退新增驱动选型/包；已删除旧路径不为了兼容缺失能力重新引入。公共契约确有新语义需求时走版本化演进。

最终验收逐条列出 H/D/F/W1/WN 的适用性、实际执行结果与限制；unsupported 是显式拒绝成功验证，未运行是未验证，二者不得混同。性能使用 P3 的逐请求总开销 p95；真实驱动、浏览器、多 worker 各自产出证据，不能用 fake 或单实例结果替代。旧路径由 P4/P5/P6 分 consumer 删除，P10 检查注册、SDK、重连和资源释放路径无残留。

## 15. 验证命令与 CI 规划

### 15.1 当前已有命令

以下命令已核对 `package.json`/workspace；Agent 不读取受保护 env 内容进入上下文，测试程序运行时读取合法。执行前核对日志/失败输出不会回显凭据，长输出按 AGENTS.md 落系统临时文件。

```bash
pnpm typecheck
pnpm test:unit
pnpm test:unit:drivers
pnpm test:layers
pnpm test:boundaries
cargo test -p datazen --lib
cargo test -p datazen-driver-api
cargo test -p datazen-driver-postgres
pnpm tauri:build:webdriver
pnpm e2e:skip-build
pnpm e2e:contract:matrix
```

WDIO 必须使用已注入 driver 的构建；三件套可运行 app 已生成而仅 DMG 失败时继续功能验证。driver 专属 E2E 不放 Host。新 runtime/application/server crate 创建后，其 package 名和测试命令同步写入 CI；不把未创建命令列为现有脚本。

### 15.2 新增 CI 门禁

- application/runtime 无 Tauri/server/UI 依赖构建。
- Driver API 无实现类型泄漏，driver 不 import host。
- DTO 与 API schema 的生成漂移检查。
- runtime fake 故障/竞态/预算测试。
- IPC/HTTP 适配一致性测试。
- Web 身份/权限/CSRF/SSE/Artifact 集成。
- 真实 driver 契约矩阵（各 driver 内部测试）。
- 多 worker 分区/drain/接管测试。
- 本地 Community、Pro、Web 与桌面 remote 的核心旅程。

## 16. 工作拆分和交接

每阶段可按“公共契约 → runtime → driver → consumer → 验证”拆 PR；有依赖的实现基于已合并契约，不自行复制 DTO。公共能力变更由 API/runtime 负责人评审，driver 维护者评审真实协议，应用负责人评审用户下一步流程，团队权限由 server 安全负责人评审。

实习生先完成 fake/runtime 中不含凭据的类型、会计、actor 和状态投影任务，再在维护者指导下实现 driver 协议与真实数据写入路径。工作交接必须包含测试编号、执行命令、实际结果、未验证能力和回退边界；不需要新增交接文件，写入 PR 与正式代码文档。

不承诺未经估算的日期。P0/P1 完成后根据 driver 缺口、消费者数量和团队规模估算后续工期；任何阶段不能因赶进度跳过会话、权限或提交未知的验收门槛。

## 17. 补充契约的阶段归属

| 阶段 | 必须交付的补充契约 | 验收范围 |
| --- | --- | --- |
| P0/P1 | DTO、持久化白名单、目标规范化、错误/回执、能力快照、令牌 port | CM-61/62/72 的类型与 schema 断言 |
| P2 | namespaceShape、targetRequirements、规范化资源 key、driver 清理与协议 drain | CM-62 的 D 断言（CM-64/67 移交 P3，见本节末） |
| P3 | attachment、结果放弃、类别调度、逻辑配额、PoolKey/cache、替换发布、幂等过期、内存目录、事务失效回收与会话级句柄登记 | CM-61～74 的 H 断言，CM-60 基准 harness |
| P4 | 恢复结果只读/写回、TTL 与切库错误投影、关闭结果视图行为 | CM-61～64/72 的 F 断言 |
| P5/P6 | 能力快照计划核验、Job 多端预算、独立消费者政策 | CM-40～53 与 CM-65/67 的任务断言 |
| P7/P8 | 单实例认证重附着、HTTP 回执与持久化投影、签名令牌和 SSE | CM-63/68/70/72 的 W1 断言 |
| P7/P8 | 登出、权限撤销与 CSRF 跨站拒绝（撤销传播到本实例的订阅与排队执行） | CM-59 的 W1 断言 |
| P9 | 原子 owner 登记、碰撞/失效、跨节点替换发布、分区和全局预算 | CM-57/58/60/68/71 的 WN 断言 |

测试标签代表执行层，不代表整条用例只属于某阶段。P3/P4 必须完成 CM-54～56 的 runtime/前端部分；P7 完成单实例适配部分；只有 owner 路由、worker 分区及跨节点预算推迟到 P9。P10 逐层汇总全部 74 条适用用例，不能把未执行的 D/WN 断言计为已通过。

### 17.1 P2 验收范围的实际落地情况

本节记录 P2 结束时的移交状态；CM-64 / CM-67 后续进入 P3，其当前 runtime 实现见 §7。以下历史说明不代表这些 P3 类型或行为目前仍未实现。

P2 行原列的三条 D 断言中，**CM-62 的七步现已全部有断言覆盖**（①与⑦是本轮补齐的），**CM-64 / CM-67 移交 P3**：它们的实现对象是 P3 的 artifact 结果运行时与 pool / lease 管理器，P2 无法在不越过阶段边界的前提下交付。已落地的是它们各自的 **Host 侧测试夹具**，落在 `packages/runtime/src/connection/testing/`（`#[cfg(any(test, feature = "test-harness"))]`，`cargo test -p datazen-runtime --lib` → `139 passed` / 0 failed，EXIT=0（计数与退出码测于 `5b7c5b49c`）。逐条对照：

- **CM-64（无消费者、截断与有界 drain）**：`testing/barrier/` 下的协议 drain barrier 夹具已可用，`barrier/tests.rs` 内 19 个单测（计数测于 `5b7c5b49c`）覆盖"无消费者时写入阻塞 → FakeClock 越过 drain 期限 → 截断"（`write_without_consumer_blocks_then_truncates_after_the_drain_deadline`）、消费者恢复后不再截断、按执行字节上限独立截断、订阅额度与执行额度分别计数、sink 失败上报。**这是测试夹具而非生产实现**：artifact 配额、unsubscribe 不立即取消 SQL、协议 barrier 的 D 侧断言均未落地。
- **CM-67（PoolKey 版本和多 database）**：`packages/platform-api/src/ports/budget/pool.rs` 的 `PoolKey` 与 `datazen-runtime` 夹具的 `policy_isolation_key` 已能支撑"换键判据"（`pool_key_equality_covers_all_eight_components` 等单测）。P2 移交时的状态是"管理器只到数据与 port 层"：`ResourceManager`（`packages/runtime/src/resource/manager.rs`）与 `SessionRegistry`（`packages/runtime/src/registry/registry.rs`）已是生产类型，而 `LeaseManager` / `PoolManager` / `ConnectionPool` 尚无类型定义，只有 `LeaseRequest` / `LeaseState` / `LeaseRecord` / `QueuedLease`、`PoolAcquireRequest` / `PoolLease` / `DriverPoolBudgetPort` / `InMemoryDriverPoolBudget` 与 `BudgetCoordinator` **trait**。**该状态已被 P3 取代**：`LeaseManager` / `PoolManager` / `ConnectionPool` 定义在 `packages/runtime/src/resource/{lease_manager,pool}.rs`，桌面生产路径用的是 `ProductionLeaseManager`（`resource/production.rs`），由 `src-tauri/src/platform/session_backend.rs` 驱动开/执行/归池/关闭/空闲回收；空闲资源停发并关闭、旧缓存不回填、撤权即时生效、空池元数据受 LRU 约束四条的 H 层断言已落地并实测通过（逐条对应与 D 层限制见[连接管理 CM-67](../architecture/platform/connection-management.md#167-补充契约与边界用例)）。
- **CM-62（命名空间规范化矩阵）**：**「代码侧零覆盖」这个结论是错的，错在检索方法**。原结论用「按用例编号 `CM-62` 检索代码扩展名」得出命中 0；该检索**可复现但无效**——测试函数不按用例编号命名，一律按被断言的行为命名，所以按编号检索必然查不到任何断言。改按行为反查（`canonicalize` / `NamespaceShape` / `validate_requirements` / `TargetResolver` / `TargetConflict`）后，7 步中 **6 步有断言**，分属两个层面，**可达性不同**：

  - **driver-api 层：生产可达。** 实现是 `packages/driver-api/src/namespace.rs` 的 `NamespaceShape::canonicalize` 与 `validate_requirements`，单测在 `packages/driver-api/src/namespace_tests.rs`（经 `namespace.rs` 的 `#[path]` 挂为 `namespace::tests`）。②不存在层级非 null → `canonicalize_rejects_a_value_for_a_level_that_does_not_exist`（`NonexistentNamespaceLevel { kind: Schema }`）；③required null → `canonicalize_rejects_a_missing_required_level`（`MissingRequiredNamespaceLevel { kind: Catalog }`），操作级 required / forbidden 由 `requirements_reject_a_missing_required_level`、`requirements_reject_a_forbidden_level` 覆盖；④optional 按操作允许 → `requirements_reject_a_missing_required_level` 的 `is_ok()` 分支；⑤同义 path → `canonicalize_merges_aliases_to_a_fixed_point`（`canonical.path[0] == "sales_main"`）、`canonicalize_appends_driver_specific_path_segments`；⑥冲突 path → `canonicalize_rejects_an_alias_cycle_instead_of_picking_one`（`ResourceError::AliasConflict`，**错误类型不是 CM-62 写的 `TargetConflict`**）。

  - **①缺字段：已补齐。** 原先 `canonicalize` 只判 `None`，`Some("")` 照常进 `resolved`，产出一个空段。现按 application 层 `validate_namespace_target` 的同一判据在层级检查之前拒绝：`canonicalize_rejects_an_empty_field_value`（三层级各一条，错误信息含层级名）、`canonicalize_rejects_an_empty_path_segment`（含段序号）。`canonicalize_rejects_an_empty_field_before_the_level_check` 是这条修复的**非空洞守卫**：同一 shape 上 `schema: Some("public")` 报 `NonexistentNamespaceLevel`、`schema: Some("")` 报 `NamespaceTargetRejected`，若把新检查删掉该测试仍会失败，空串缺陷会无声回归。

  - **`canonicalize` 的生产调用点不止 redis / postgres。** 除 `packages/drivers/redis/src/resource/provider/contract.rs`、`packages/drivers/postgres/src/resource/provider/contract.rs` 与两者的 `provider.rs` 外，还有 `packages/driver-api/src/resource_adapter.rs`（`LegacyResourceAdapter`）以及 sqlite / mysql / mongodb / hbase / vector 各自的 `resource.rs` 或 `resource/provider.rs`。这直接决定 ⑦ 的改造半径：把 `CaseFolding::Unknown` 改成硬拒绝会同时打断这 6 个驱动的生产路径。

  - **application 层：已实现、已断言，且已接入生产调用链。** 实现是 `packages/application/src/target.rs` 的 `TargetResolver::compute` 六步流水线，12 个单测在同文件 `mod tests`（`:336`）。①`empty_field_values_are_rejected_before_anything_else` → `InvalidArgument`（第 1 步 `validate_namespace_target`，`target.rs:100` → `packages/application/src/dto/requests.rs:40`）；②`the_six_steps_run_in_the_documented_order` → `TargetUnsupported` 且信息含 `catalog`，`unknown_operation_or_connection_fails_closed` 覆盖失败关闭；③`a_missing_required_layer_is_target_required_not_a_backfill` → `TargetRequired`；④`session_defaults_apply_only_when_the_command_allows_them`（`execute_in_session` 可补、`query` 不可补）与 `object_identity_is_gated_by_the_command_requirements`（schema 可省）；⑤`a_successful_computation_yields_canonical_identity_only`（别名 `main` 与真名 `app` 落到同一身份）、`the_canonical_target_round_trips_and_carries_no_display_name`（别名不出现在序列化结果里）；⑥`alias_in_the_wrong_slot_is_a_target_conflict` → `ApiErrorCode::TargetConflict`，即 CM-62 所指的那个 `TargetConflict`。**该"生产不可达"结论已被后续接线取代**：生产调用点共三处，全部在组装层——`src-tauri/src/platform/target_metadata.rs:51` 的 `resolver()` 构造 `TargetResolver`，`src-tauri/src/platform/adapter.rs:326` 用它在 `PlatformAdapter::inject` 里组装 `ApplicationServices` 并 `set` 出去（`adapter.rs:327-352`），`adapter.rs:567` 在 `resolve_execution_target` 内调用 `compute()`；生产入口是 `execute_at_target`（`src-tauri/src/platform/session_commands.rs:130-138` 先 `compute` 再把规范目标写回请求），另有 `src-tauri/src/commands/table_target.rs:79` 与 `src-tauri/src/workflow/host.rs:154` 两个调用方。**仍未经过 `compute()` 的是会话打开路径**（`open_session` / `execute_in_session`）：它们的必填层校验由 runtime 自己做（`TargetRequired` / `TargetConflict`，`packages/runtime/src/connection/types.rs`），不是遗漏。

  - **⑦大小写别名：已实现，只覆盖已声明的驱动。** `canonicalize` 现在经内部的 `canonical_identifier` 折叠每个进入规范路径的标识符。引号按驱动既有的记录方式识别——引号是字符串的一部分（`"Sales"`），与 `datazen_driver_postgres::resource::observation` 一致——因此**无需改 DTO、无需 `PROTOCOL_VERSION` 升版**；剥离后规范路径仍是裸标识符，由驱动在拼 SQL 时重新加引号。折叠发生在别名查找**之前**、别名结果**之后**，所以 `Sales` / `SALES` 会命中同一条 alias 项。别名解析本身不动（`resolve_alias` 仍是 public）。

    断言：`an_unquoted_identifier_is_folded_by_the_drivers_case_rule` 断言的是一个**被改变**的值（`Sales` → `sales`），若折叠缺失它会照原值通过；`spellings_differing_only_in_case_reach_one_canonical_identity` 覆盖 CM-62 的"同义 ID 规范化一致"；`a_quoted_identifier_keeps_its_case_and_loses_its_quotes` 同时断言"引号内的规则生效"与"规范路径是裸的"，并断言 `"Sales"` 与 `Sales` 不合并；`a_doubled_quote_inside_an_identifier_is_unescaped`；`an_alias_is_reached_through_a_differently_cased_spelling`。

    **各驱动的声明现状**：只有 postgres 是 `unquoted: Lowercases, quoted: Preserved`，redis 是全 `Preserved`，mysql / sqlite / mongodb 显式填 `Unknown`，hbase / vector 走 `Default::default()` 也是 `Unknown`，sqlserver 未声明 `case_rules` 同为 `Unknown`。`Unknown` 的语义定为**不折叠**（`an_unknown_case_rule_folds_nothing_rather_than_guessing`），理由是它表示驱动尚未声明规则，替他折叠等于凭空造一个身份。这不等于这些驱动的大小写行为已经正确：sqlite 的标识符实际是大小写不敏感的，不折叠会让 `Users` 与 `users` 产生两份缓存身份，正是 CM-62 尾部断言要防的事。**该定论需要逐驱动查证并改声明，属 P2 遗留**，硬拒绝 `Unknown` 会同时打断上述 6 个驱动的生产路径，不能顺手做。

  - **尾部断言（缓存 / 权限 / 驱动共用同一份 `CanonicalTarget`）未落地**，因为没有消费方。`packages/application/src/target.rs` 全文无 `PathBuf` / `std::fs` / `Path::new`，这只说明该模块不碰文件系统，不构成「驱动拿到的就是这份 `CanonicalTarget`」的断言。

按 §3 的完成定义，P2 自身承担的 CM-62 D 断言已实现并被断言覆盖；**尾部断言与 `Unknown` 驱动的声明仍未落地**，两者都依赖 P3 的消费方，不能在 P2 内结清。CM-64 / CM-67 的 D 断言按上表移交 P3，随 P3 的 pool / lease 与结果运行时一并交付。

## 18. 阶段详细设计入口

| 阶段 | 权威设计与实施落点 |
| --- | --- |
| P0 | [假资源夹具](../architecture/platform/fake-runtime-fixtures.md) |
| P1 | [共享边界与端口](../architecture/platform/shared-boundaries-and-ports.md)、[持久化白名单](../architecture/platform/persistence-model.md) |
| P2 | [驱动层](../architecture/backend/drivers.md)（协议版本闸门见 §2.5） |
| P3/P4 | [连接管理](../architecture/platform/connection-management.md)：actor/lease/budget、Gateway 与 §12.1 结果消费 |
| P5 | [迁移任务](../architecture/platform/data-migration-jobs.md)、持久化 §3.6、共享端口 §4.3 |
| P6 | [Workflow](../architecture/platform/workflow-resource-model.md)、[消费者接入](../architecture/platform/consumer-adapters.md) |
| P7/P8 | [团队服务与认证](../architecture/platform/team-server-and-auth.md)、[运维流程](team-service-operations.md)、持久化 §5/§7 |
| P9 | [多 worker 协议](../architecture/platform/multi-worker-coordination.md)、持久化 §3.11 |
| P10 | 本计划 §14（P10）的通用 contract × driver 矩阵；各消费者退役条件与本计划逐层门槛 |

本表是设计入口，不是完成进度。目标设计、现有实现和实际运行证据分别标明；阶段完成后同步将对应内容改写为事实。


### P6 Workflow v1 范围

Workflow 在 P6 保持既有 v1 协议与执行语义，只替换底层连接管理；保留跨数据库逐步目标、step output 和模板串联。生产路径为 `feature-core command_runtime → feature-core resource_execution → Runtime`。不接入 v2 session/transaction block、专用 Workflow Job owner/dispatch、Workflow Job IPC 或 PostgreSQL/MySQL transaction proof；模型/YAML/既有 IPC 形状不变。

#### 桌面 Monitor 定时授权（已实现，2026-10-17）

Monitor 周期轮询与手动刷新现已使用不同入口。手动刷新沿用当前桌面用户上下文；周期 tick 必须取得后端保存的 widget grant，不能因 `connection_id=None`、工作流可运行或桌面用户仍在线而获得授权。用户在报表编辑器中保存 interval 配置后，显式授权一小时或撤销；授权 IPC 仅接受 dashboard/widget ID 与 60～86400 秒有效期，不接受 principal、delegation token 或用户提供的 grant。

后端从当前桌面 RequestContext 取得批准者，并生成独立 service principal / delegation ID。AppDb schema v3 的 `monitor_schedule_grants` 仅保存批准者、服务身份、dashboard/widget/workflow ID、工作流 YAML SHA-256 revision、固定 canonical connection/database/schema 与配置版本、connect/query 动作及命令范围、readOnly、到期时间、撤销状态与版本。不保存 SQL 副本、session、attachment、bearer token、秘密或凭据。新授权取代同 widget 的旧授权；变更 workflow 定义或 profile 配置必须重新授权。

当前授权范围限 PostgreSQL/MySQL 的静态独立 `query` 步骤，要求保存的 workflow 明确声明 connection/database；不接受变量、循环、条件、AI 或资源块。Monitor 使用独立的只读查询 AST 校验（单查询，拒绝 writable CTE、SELECT INTO、行锁及非查询；函数仅允许明确的内置聚合/数值/文本/空值/时钟函数白名单，未知、带 schema、引号名称及表值函数失败关闭，因此序列推进/状态、命名锁获取/状态/释放和会话配置函数不能借 SELECT 派发），不使用会禁止所有 SQL 工具的 MCP ReadOnly 模式。其他驱动和写动作均失败关闭。

共享 MonitorEngine 在入队前及 semaphore / workflow lock 之后重载 dashboard/widget enabled、pause 与 grant，冻结并比较 workflow revision 和目标范围。桌面 scheduled adapter 使用同一冻结定义与 service RequestContext；Runtime 从 PolicyService 后端解析完整 grant、核对主体/客户端/有效期/动作，在 admission 与物理 dispatch 前重新核对 grant、schedule、workflow hash、目标、配置版本、enabled 和只读 SQL。delegation 不能降级为本地用户；撤销后仍允许 owner 关闭其自有资源，不能因撤权遗留短会话。原同 widget in-flight 合并保持。

撤销/禁用阻止后续派发，不伪造已在途执行的取消或回滚。每个 grant 版本、dashboard/widget、workflow revision 与 refresh slot 对应一个带稳定幂等键的 `monitorOccurrence` Job；Job 受理后才派发 scheduled workflow，WidgetRun 与有界 JobDomainResult 记录实际结果。启动恢复按持久化 service principal 扫描 occurrence：未派发 Job 标为未启动，运行中 Job 转为待核验，不重放查询。撤权后无法继续读取 receipt 时保留 SqlUnconfirmed/Unknown；完整桌面 scheduler / occurrence / 真实驱动链路仍须独立验收，P6 未整体关闭。

Monitor 真实驱动查询层补充：PostgreSQL/MySQL `tests/monitor_scheduled_query.rs` 运行生产只读策略、grant scope、Workflow resource execution 和 Runtime / 真实 Provider，并用独立连接核对结果；它不覆盖 AppState、grant 持久化、SQLite occurrence 或重复 tick receipt。live 旅程发现 SQL 成功但 delegated Artifact 发布失败的 P6 阻塞，已通过只针对 admitted execution 的内部结果 scope 修复；普通 Artifact API 的委托拒绝不变，受限结果不跨重启恢复，隔离/撤权/任意 Artifact/TTL 回归已补齐。详见 [消费者边界 §6](../architecture/platform/consumer-adapters.md#6-dashboardmonitor-与调度身份)。



MySQL Workflow transaction proof 不属于当前 P6 退出条件：2026-10-19 用户已明确 Workflow 保持 v1，只迁移底层连接管理，不改协议，也不增加额外事务语义。因此 MySQL `transactionBlock` 保持 fail-closed；现有关于 session-effective 目录可见性的研究仅作未来驱动能力背景，不要求在 P6 补齐或启用。若未来需要该能力，应另行按驱动范围立项和验收。
