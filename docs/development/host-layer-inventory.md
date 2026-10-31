# Host 层目录职责全景（apps/desktop/src-tauri）

> **状态**：静态盘点快照。2026-10-31，分支 `review/p2-p3-p5-2026-10`，基线 `22113a321`。
> **用途**：回答「host 层下各个目录的代码分别做什么」，作为 command 薄壳化余留两刀、
> P2 驱动注入静态化、P3 feature 图修剪、P5 目录语义化的**施工地图**。
> **判据来源**：`host-boundary-followups.md` §3（三层结构与接线判据）。
> **统计口径**：行数 = `src/**/*.rs`；「测试」= 路径或文件名含 `test` 的文件；
> IPC 数 = `bootstrap/run.rs` 的 `generate_handler!` 注册表（254 条，其中 4 条带
> `cfg(feature = "webdriver")` 门控，生产构建不编入）。

---

## 1. host 层是什么，判据是什么

host 层 = `apps/desktop/src-tauri`（crate 名 `datazen_desktop`），Tauri 2 桌面壳。它只做
**形态专有**的事：进程入口、IPC 接线、窗口/对话框/托盘/钥匙串、桌面文件存储、把端口实例
组装起来递给 modules。业务编排与数据访问一律在 `datazen` crate（`modules/`）的
feature `api` / application / domain 层。

```text
datazen_desktop（host，本目录）
  传输胶水（Tauri command / 窗口 / 对话框 / 钥匙串 / 托盘 / 桌面存储）
        │  只调用
        ▼
datazen crate 内各 feature 的 api 模块
  入参 DTO、RequestContext、调用 application、返回 ApiError
        │
        ▼
该 feature 的 application / domain
```

**接线判据**（§3 固化的操作定义）：command 层允许的 `state.X` 用法只有三类——

1. 把端口 / 服务实例递给 api 用例（`.as_ref()` / `.clone()` 后立即传参）；
2. 取作用域与身份（`connection_scope` / `organization_scope` / `platform_require`）；
3. 宿主专有钩子（事件发射、对话框、托盘、会话退休）。

一旦出现「读字段 → 判断 → 再读 → 组装」，那就是编排，必须下沉 modules。

**两种合法形态**：

- **薄壳命令**：`#[tauri::command]` → `*_impl` → `datazen::<feature>::api::用例(端口, 请求)`，
  宿主只留错误映射表（每个 feature 恰好一份）与事件发射。样板：`commands/ai/config.rs`
  （AI 配置刀，2026-10-31）、`commands/connection.rs`（F4a 第一刀）。
- **宿主专有命令**：系统对话框、文件读写、剪贴板、窗口、托盘、`datazen://`——这些没有
  api 孪生，`modules` 永远不该知道 Tauri 的存在。

**组织模式**：除 commands/ 外的域目录（`mcp/`、`workflow/`、`dashboard/`、`monitor/`、
`tunnel/`、`wapps/`、`services/`、`store/`、`platform/`）与 `commands/` 里的同名命令文件
成对出现——`commands/X.rs` 是 IPC 壳，根模块 `X/` 是该域的宿主侧服务 / 端口实现 / 存储。
`lib.rs` 还有一组 re-export 把 modules 的词汇挂到 crate 路径上
（`crate::transfer` / `crate::schema_diff` / `crate::data_sync` 实际指向
`datazen::migration::*`），宿主代码可以少写一层路径，但**本体仍在 modules**。

## 2. 全景数字

`src/` 共 **104,270 行**：生产 68,653（241 个文件）+ 测试 35,617。`#[tauri::command]`
标注 263 处（含少量测试/夹具），`generate_handler!` 注册 **254 条 IPC**。

| 目录 | 生产 | 测试 | 一句话职责 |
|---|---:|---:|---|
| `commands/` | 33,595 | 18,275 | IPC 传输面：254 条命令的接线与错误映射 |
| `store/` | 10,844 | 7,795 | 桌面存储实现（`DesktopRepository` + SQLite×2 + JSON 文件族 + 密钥栈） |
| `platform/` | 9,122 | 3,154 | Runtime 适配层：身份、作用域、会话后端、端口实现、两个常驻闸门 |
| `mcp/` | 4,204 | 1,025 | MCP server（暴露数据库能力）+ client（连外部 MCP）+ stdio 鉴权 |
| `workflow/` | 1,393 | 294 | YAML 工作流的桌面绑定（仓储/调度/执行宿主端口） |
| `dashboard/` | 1,367 | 247 | 仪表盘仓储绑定（规则在 modules） |
| `monitor/` | 1,296 | 491 | 后台仪表盘监控：调度引擎、授权、通知渠道 |
| `bootstrap/` | 1,072 | 525 | 组装根：日志、AppState 装配、GUI/MCP 入口、命令注册表 |
| `tunnel/` | 834 | 0 | 隧道物理装配（SSH/代理/WS 的网络提供方） |
| `services/` | 773 | 1,022 | 共享服务的桌面接线（AI 元数据源、只读命令源、会话身份绑定） |
| `wapps/` | 561 | 1,330 | `datazen://` 协议：UI 扩展资产服务 + 深链 |
| `theme/` | 244 | 0 | webview 启动背景缓存（防白屏） |
| `testing/` | 0 | 1,384 | 测试装置（`cfg(test / test-harness)` 门控，不进生产二进制） |
| src 根文件 | 3,348 | 75 | 见 §4 |

## 3. src-tauri/ 顶层资产（src 之外）

| 条目 | 内容 |
|---|---|
| `Cargo.toml` | 声明的 feature 只有 `default=[]`、`webdriver`、`test-harness`；17 个 `driver-*` feature 由构建链生成，但 `unexpected_cfgs` 的 `check-cfg` **静态声明了全部 17 个**（闭集，P2 的现成前提）。`test-harness` 的集成测试带 `required-features` |
| `build.rs` | tauri 构建脚本（含 `tauri-build` 上下文生成） |
| `tauri.conf.json` + `.macos` / `.windows` 两份覆盖 | 窗口**不在** conf 里定义——主窗/引导窗/子窗全部由 `commands/window.rs` 在 setup 期按 onboarding 状态创建 |
| `capabilities/default.json.host` | Tauri capability 声明：窗口集合（main/onboarding/`data-sync-*`/`data-transfer-*`/`schema-diff-*`/`backup-*`）与权限（core/dialog/notification/clipboard/os/updater/process）。`.host` 后缀属驱动注入构建链的文件管理约定（P2 手术面的一部分） |
| `resources/` | `builtin-workflows/`（内置工作流 YAML）+ `menu-labels.json`（菜单 i18n 文案） |
| `icons/` | 应用图标（含 Windows Store 各尺寸） |
| `tests/` | 纯 IPC E2E（`test-harness` feature 门控）：`ai_e2e.rs`、`data_sync_public_api.rs`、`driver_command_ipc.rs`、`mcp_stdio_auth.rs`、`workflow_tests.rs`——复用 `commands::test_harness` 直接调 `*_impl`，不起 Tauri 事件循环 |
| `src/driver_init.rs` | **构建期生成物，不在 git 里**：由 `resolve-drivers.mjs` 按当前驱动选择注入（`register_drivers(builder)` 就是它提供的）。这是 P2 要消灭的构建脆弱性根源——本地 checkout 若没跑过驱动解析脚本，桌面 crate 直接编不过 |

## 4. src/ 根文件（11 个生产文件，3,348 行）

| 文件 | 行 | 职责 |
|---|---:|---|
| `main.rs` | 16 | 进程入口：先选进程级 rustls backend，再分流 `--mcp-stdio` / GUI |
| `lib.rs` | 54 | 模块声明 + re-export（含把 `datazen::migration::*` 挂成 `crate::transfer` 等路径的技巧） |
| `app_data_archive.rs` | 1,261 | `{appData}` 目录 ZIP 导出/导入的**纯函数**集（被 `commands/app_archive.rs` 接线） |
| `app_menu.rs` | 731 | macOS 原生菜单：标签、动作映射、树构建、事件分发（`rebuild_menu` 也在 IPC 表里） |
| `tls.rs` | 594 | rustls 双 provider 冲突调停：reqwest 要 aws-lc-rs、updater 要 ring，进程级一次性选择 |
| `tray.rs` | 283 | 系统托盘（后台仪表盘监控的常驻入口，关窗进托盘的判定也在这） |
| `e2e_quiet.rs` | 259 | webdriver E2E 静默模式：拦下 macOS 四条会把进程顶到前台的路径 |
| `i18n_locale.rs` | 112 | OS locale 串 → 支持的 UI 语言码 |
| `redis_flush_gate.rs` | 32 | 启动期把 AppSettings 同步进各驱动 crate 的 `set_settings_*`（键由驱动自报，宿主不硬编码） |
| `product_features.rs` | 6 | 产品级 feature 门（与前端 `productFeatures.ts` 人工同步） |
| `driver_registry_tests.rs` | 75(测) | 驱动注册表测试 |

## 5. 目录详解

### 5.1 bootstrap/ — 组装根（1,072 生产 / 525 测试）

| 文件 | 行 | 职责 |
|---|---:|---|
| `run.rs` | 666 | `run_mcp_stdio`（headless MCP 进程）+ `run`（GUI 全链）：日志初始化 → e2e_quiet → 主题插件 → 官方插件族 → `driver_init::register_drivers` → `datazen://` 协议注册 → setup 钩子（事件 sink、`build_gui_app_state`、monitor/workflow 调度器启动、托盘、onboarding 判定建窗、MCP 自启/自连、macOS 菜单）→ **`generate_handler!` 254 条命令注册** → 窗口事件（关窗进托盘、全屏通知）→ 退出清理（MCP 断连、空闲回收器停车） |
| `app_state.rs` | 274 | `AppState` 装配：GUI 版（DesktopRepository → 驱动预载）与 headless 版共用 `finish_app_state`（23 个组件的 DI：registry/store/cache/引擎/管理器/端口实例/平台 inject/组织作用域）+ `build_desktop_job_host` + 启动期 Job 恢复扫描 |
| `helpers.rs` | 105 | 纯函数：日志/路径解析、驱动类型扫描、MCP 自启判定、全屏判定 |
| `tests.rs` | 525(测) | helpers 单测 + **IPC 注册契约测试**（`include_str!("run.rs")` 对着注册表断言） |

边界判定：装配期直接 `state.store.get_settings()` 判 onboarding / MCP 自启 / 菜单语言
（run.rs:174/203/212/247）——这是**组装根行为**（决定建哪个窗、起不起服务），不是命令层
编排，§3 判据不拦；但若未来设置读取长出判断分支，应走 `datazen::settings` 用例。

### 5.2 commands/ — IPC 传输面（33,595 生产 / 18,275 测试，149 文件）

`mod.rs`（437 行）是这层的地基：`AppState` 定义（23 个 pub 字段的 DI 容器）、
`QueryExecutionRegistry`（查询执行句柄的 owner 绑定，防跨会话取消）、
`SyncRepositoryPorts`（Data Sync 存储端口的桌面实例）、`test_harness` 导出。
共享基建还有：`error.rs`（629 行，`CommandError` 结构化错误 + `CmdExt::cmd_err` 脱敏日志）、
`port_call.rs`（`PortError → CommandError` 统一收口）、`ipc_contract_guards.rs`
（D1 回归锚：钉死 `db_session_id` 线参名）、`result_mapping.rs` / `table_target.rs`
（F4b 端口与保守结果映射）、`tunnel_probe_fixture.rs`（测试夹具）。

**命令族全景**（254 条 IPC 在 commands/ 侧的分布）：

| 族 | 文件 | IPC | 算法归宿 | 状态（§3 判据） |
|---|---|---:|---|---|
| 连接目录 | `connection.rs`(1281) | 15 | `connection::api`（F4a） | ✅ 薄壳样板；「保存后退休旧会话」钩子按方案留宿主 |
| 表浏览/行编辑 | `data.rs`、`data/`、`table_target.rs` | 5 | `table_browsing::api` + `data_browsing`（F4b） | ✅ 剩传输胶水 + `TargetCommandRunner` 端口实现 |
| 结果网格写回 | `result_origin.rs`、`result_write.rs` | 2 | `services::result_edit`（D 系） | ✅ |
| FK 导航 | `fk_navigation.rs` | 2 | `services::fk_navigation` | ✅ 壳只借会话+验封+映射 |
| 值搜索 | `value_search.rs` | 1 | `services::value_search` | ✅ 扫描循环在前端，逐表一命令 |
| Schema 元数据 | `schema.rs`、`schema/` | 2 | `QueryExecutor`（modules） | ✅ |
| 查询/事务 | `query.rs` | 10 | driver_command 信封转发 | ✅ 本就无算法 |
| 收藏 | `query_favorites.rs` | 5 | `store::favorites`（文件式存储） | ✅ |
| 历史 | `history.rs` | 3 | `history` 用例 + `migration_profile_reader` | ✅ D6 |
| 命名视图 | `named_views.rs` | 3 | `named_views` 聚合 | ✅ P3 形状测试钉死 |
| 驱动命令网关 | `driver_command/`(8 文件) | 4 | 驱动 crate 自带 | ✅ 发现/校验/执行/流式全是传输 |
| 隧道实体 | `tunnel.rs` | 7 | `tunnel`（modules）+ 宿主存储 | ✅ |
| 设置/分组/路径 | `config.rs` | 10 | `settings` 仓储边界（D1 服务化） | ✅ |
| 连接导入导出 | `connection_import/` | 6 | `connection_import`（modules 解析器） | ✅ 宿主只留对话框与合并 |
| 应用数据归档 | `app_archive.rs` | 2 | `app_data_archive.rs`（宿主纯函数） | ✅ 宿主专有（ZIP 本地目录） |
| 加密密钥 | `encryption_key.rs` | 1 | `store::key_store` + 平台 vault | ✅ 宿主专有（钥匙串/DPAPI） |
| 导出 | `export.rs`、`export/` | 1 | `export`（modules） | ✅ D6 item 5 |
| 备份/恢复 | `backup.rs`、`restore_job/`、`restore_gateway.rs`、`file_jobs.rs` | 3 | `backup`（modules）+ 共享恢复编排器 | ✅ 宿主留对话框/Job 受理/原生投递 |
| 文件对话框 | `file.rs`、`dialog.rs` | 11 | — | ✅ 宿主专有（含 E2E 注入队列） |
| 剪贴板 | `clipboard.rs` | 3 | — | ✅ 宿主专有 |
| 窗口/引导 | `window.rs`、`sample.rs`、`theme.rs` | 5 | — | ✅ 宿主专有（753 行窗口机器） |
| AI 配置 | `ai/config.rs` | 9 | `ai::api`（AI 配置刀） | ✅ 2026-10-31 薄壳化 |
| AI 生成/诊断 | `ai/generate.rs` | 7 | 部分下沉 | ⚠️ 请求装配仍在宿主（余留刀①） |
| AI 对话 | `ai/chat.rs` | 1 | 部分下沉 | ⚠️ `ai_chat_impl_with_claim` ~160 行编排（余留刀①） |
| AI 授权/取消 | `ai/grant.rs`、`ai/mod.rs`、`ai/util.rs` | 4 | 宿主自有服务面 | ✅ 授权是宿主安全语义 |
| Prompt 模板/上下文 | `ai/prompts.rs`、`context.rs` | 6 | `PromptResolver`（modules） | ✅ |
| MCP 管理 | `mcp.rs` | 10 | `mcp/`（宿主自有服务面） | ✅；`mcp_tool_definitions` 的 settings 过滤待随 MCP 刀处理（余留刀②） |
| 工作流 | `workflow.rs` | 12 | `workflow/` + `datazen::workflow` | ✅ 薄委托 |
| 仪表盘 | `dashboard.rs` | 18 | `dashboard/` + `datazen::dashboard` | ✅ P3 形状测试（8 聚合/6 宿主残留） |
| UI 扩展 | `wapps/` | 13 | `workspace_apps`（modules 内核） | ✅ 宿主留注册表接线与会话桥 |
| 数据同步 | `sync/`(19 文件) | 24 | `data_sync::api`（F4c 三刀+收口） | ✅ 剩 LiveExecutor 端口/租约/JobRuntime/合同测试 |
| 数据传输 | `data_transfer/`(26 文件) | 17 | `data_transfer`（F4c 预览刀） | ✅ 剩 Job 生命周期/claimed-plan 执行/检查点 |
| 结构比对 | `schema_diff/`(11 文件) | 11 | `schema_diff::api`（F4c-3 复核刀） | ✅ 剩 JobRuntime 接线+历史记账+两个端口实现 |

两个「测试只有」的目录：`data/`（129 生产/3,141 测试）、`schema/`（0/432）——生产面已薄到
只剩 re-export 与适配器，测试密度恰恰是薄壳化的证据。

### 5.3 platform/ — Runtime 适配层（9,122 生产 / 3,154 测试，35 文件）

宿主里最厚的一层，但厚得**有理由**：它是 §6.2「窄适配组装根」的落点，把 modules 的
Runtime / ApplicationServices 接到桌面现实（OS 身份、SQLite、物理连接）。**21 条 IPC 也
注册在这里**（`session_commands` 20 条 + `artifact_export` 1 条），因为它们是 Runtime 自有词汇的
传输面，不是业务命令。

| 文件 | 行 | 职责 |
|---|---:|---|
| `adapter.rs` | 972 | `PlatformAdapter` / `PlatformEntry`：§6.2 第 8 步组装根，`inject` 注入存储/驱动注册表/隧道端口；一个空壳都不写——组装失败就显式报错 |
| `session_backend.rs` + `session_backend/{opening,execution,release}.rs` | 1,731 | Runtime 会话后端：物理资源的开/执/收（`resource_id` 台账、`by_session` 反查） |
| `session_commands.rs` | 488 | Runtime IPC 面（~20 条）：`open/get/close/attach/detach_session`、`execute_in_session/at_target`、事件订阅、制品读取、连接 CRUD（Runtime 侧）、身份与可观测快照 |
| `identity.rs` | 513 | 桌面身份组装：`LOCAL_ORGANIZATION_ID`（安装级常量）、principal（OS 用户，进程内解析一次）、clientInstanceId/requestId 派发 |
| `scope.rs` | 34 | 单租户组织作用域；越组织一律按「不存在」处理（CM-05，不泄漏「存在但无权」） |
| `ipc.rs` | 444 | `begin_ipc_call`（硬失败）/ `observe_ipc_call`（只记日志不改控制流）/ `require_connection_service` 等服务取用入口 |
| `execution_identity.rs` | 470 | `IdentityResolver` 端口实现：HMAC-SHA256 确定性身份键（组织+连接+隔离键+盐；凭据轮换只推进 revision） |
| `connections.rs` / `settings.rs` / `history.rs` / `repositories.rs` | 735 | 模块仓储端口的桌面适配（D2/D6：存储/缓存/锁仍归 `DesktopRepository`，只有端口实现挪上来） |
| `live_source.rs` | 157 | `LiveConnectionSource` 出向端口：把 runtime 台账里的会话映射到已开的物理资源 |
| `feature_ports.rs` | 84 | `ArtifactChunkReader` / `BackupOutputBinder` 薄壳（转调 Runtime 门面） |
| `event_sink.rs` | 66 | 驱动事件 → Tauri 事件的出口（驱动只认形态中立的 `DriverEventSink`，Redis Pub/Sub 因此不再桌面独占） |
| `known_hosts.rs` | 172 | SSH known-hosts 的文件 I/O（规则在 `tunnel`，形态的事留宿主） |
| `resource_budget.rs` | 326 | 四类运行时资源预算账本适配 |
| `handles.rs` | 422 | owner 作用域会话句柄登记（不持物理资源，物理账归 session actor） |
| `artifact_export.rs` | 124 | 制品的原生对话框导出（1 条 IPC） |
| `policy.rs` / `target_metadata.rs` / `diagnostics.rs` / `resource_mapping.rs` | 426 | 策略绑定、执行目标元数据、脱敏诊断、兼容 re-export |
| `scope_gate.rs` / `org_literal_gate.rs` | 1,620(测) | **两个常驻闸门**（G1：生产代码不得直读组织常量；G-ORG-1：不得内联组织 id 字面量），`--lib` 跑即进 CI，自带 kill test |

### 5.4 store/ — 桌面存储实现（10,844 生产 / 7,795 测试，76 文件）

规则在 modules、**存储事实**在宿主：一份 app data 一份存储，一份数据只有一个写者。

| 模块 | 内容 |
|---|---|
| `mod.rs`(527) | `DesktopRepository`：连接/设置/历史/各 profile 的门面 + 组织作用域纪律（单租户收窄） |
| `connections.rs`(274) | `connections.json`：AES-256-GCM 加密落盘 + 内存缓存 + 写锁 |
| `settings.rs`(373) | `settings.json`（原子写） |
| `key_store.rs`(518) + `key_backend.rs`(245) + `key_codec.rs`(26) + `platform_vault.rs`(287) | 主密钥栈：backend 是**启动期事实**（`DATAZEN_KEYRING` 只读一次）、macOS Keychain / Windows DPAPI / 文件回落、旧 `.key` 透明迁移 |
| `app_db.rs`(404) + `app_db/` | `datazen.sqlite`：dashboards/widgets/runs/workflows/monitor grants + **`jobs/`**（P5 Job 仓储：admission/codec/runtime_ops/owner/results/writes，~2.4k 行） |
| `history_db.rs`(247) + `history_db/` | `history.sqlite`：查询历史（明文，文档已声明风险）、workflow 历史、迁移运行历史、v1→v4 迁移环、退役的 legacy favorites 表 |
| `favorites/`（4 文件，1,470） | 文件式收藏：`--` frontmatter 的自描述 `.sql`、一次性迁移、ULID 文件名 |
| `tunnels.rs`(318) | `tunnels.json`：隧道定义（`TunnelRepository` 端口的桌面实现，敏感字段同密钥加密） |
| `named_views.rs` / `sync_tasks.rs` / `sync_profiles.rs` / `transfer_profiles.rs` / `schema_diff_profiles.rs` / `platform_profiles.rs` / `ai_config.rs` / `models.rs` | 其余 JSON 存储：named views、同步任务/双 profile 族、传输 profile、diff profile、平台 profile、`ai_config.enc` |
| `migration_profile_reader.rs`(179) | 三桌面存储 → 迁移 profile 窄读投影（历史命令不知道具体文件） |

### 5.5 services/ — 共享服务接线（773 生产 / 1,022 测试）

| 文件 | 行 | 职责 |
|---|---:|---|
| `schema_context_source.rs` | 59 | AI 元数据源接线：实现住在 `datazen::services::schema_context`，宿主只递 Runtime 门面与执行宿主 |
| `runtime_command_source.rs` | 220 | 只读命令的 Runtime 共享适配（表浏览/AI 分析共用） |
| `migration_endpoint.rs` | 326 | 迁移受理的会话身份绑定 + `retire_sessions_for_profile`（保存连接后退休旧会话，PR #116 语义） |
| `schema_metadata.rs` | 41 | 便携元数据读的兼容适配 |
| `feature_ports.rs` / `job_registry.rs` | 107 | 消费者端口实现 / 进程内 Job 取消标志 |

### 5.6 mcp/ — MCP 服务与客户端（4,204 生产 / 1,025 测试）

暴露面（server）与消费面（client）都在这：`server.rs`(347) 编排入口、`handler.rs`(103)
ServerHandler、`tools.rs`(714) 工具、`resources.rs`(257)、`prompts.rs`(147)、`types.rs`(157)
输入类型、`auth.rs`(143) stdio 本地令牌、`client.rs`(553) 外部 server 连接与工具调用、
`session.rs`(424) 有状态工具的会话协议（不透明 ref，映射只在适配层内存）、
`session_open.rs`(265) 取消下的获取所有权、`session_cleanup.rs`(441) 每客户端清理工、
`runtime_schema.rs`(92) 经 Runtime 边界的元数据、`tool_help.rs`(229) 错误附加帮助、
`contract.rs`(246) 外部契约快照回归。规则与工具定义大量来自 `datazen::services`，这层是
MCP 协议胶水 + 会话生命周期。

### 5.7 workflow/ — 工作流桌面绑定（1,393 生产 / 294 测试）

`registry.rs`(420) 基于 `AppDb` 的持久化（规则在 `datazen::workflow::registry`）、
`history.rs`(203) 执行历史、`host.rs`(316) 共享执行宿主端口（`complete_ai` 已委托
`ai::api::resolve_active_provider`）、`scheduler.rs`(88) 调度适配（时钟在 modules）、
`command_runtime.rs`(144) 命令步骤 → 驱动命令的桥、`live_acceptance.rs`(191) live 契约。
引擎/模型/规则全在 `datazen::workflow`。

### 5.8 dashboard/ — 仪表盘仓储绑定（1,367 生产 / 247 测试）

`repository.rs`(145) / `runs.rs`(183) 双仓储绑定（规则在 modules，桌面单租户收窄）、
`create.rs`(423) 便携创建用例的仓储绑定、`execute.rs`(289) 经 WorkflowExecutor 的
widget 执行、`export.rs`(319) 单文件导入导出。

### 5.9 monitor/ — 后台监控（1,296 生产 / 491 测试）

`engine.rs`(295) 共享监控调度器的桌面适配（启动期 `start(handle)`）、`scheduled.rs`(619)
冻结 Monitor 执行（后端签发的服务上下文）、`grants.rs`(238) 本地调度授权（桌面用户签发、
后端元数据持有）、`channels.rs`(138) 通知渠道（应用内/webhook/邮件 stub）。

### 5.10 tunnel/ — 隧道物理装配（834 生产）

`mod.rs`(435) 本地端口统一隧道（驱动永远连 `127.0.0.1:<local_port>`；定义走
`TunnelRepository` 端口引用而非拷贝）、`network_provider.rs`(399) `DesktopTunnelHost`：
把 Runtime 的路由/引用契约接到物理 SSH/代理/WS 实现（共享账在 modules runtime）。

### 5.11 wapps/ — UI 扩展协议（561 生产 / 1,330 测试）

`protocol.rs`(536) `datazen://` 自定义协议：path 形式服务包内资产（固定 Content-Type
表），command 形式发深链事件给前端；Windows 的 `http://datazen./` 兼容。`mod.rs`(25)：
内核（manifest 规则/安装/KV 存储/注册表/URI 语法）在 `modules::workspace_apps`，这层只是
协议处理。

### 5.12 theme/ — 启动背景缓存（244 生产）

`surface_bg.rs`：最后一次解析的 `--c-surface` 持久化到 `{appData}/surface-bg.json`（不用
localStorage），原生窗口底色 + webview `initialization_script` 注入，防启动白屏闪烁。

### 5.13 testing/ — 测试装置（1,384 行，全测试）

`cfg(any(test, feature = "test-harness"))` 门控：`app_state.rs`（`TestAppState`）、
`resource_provider.rs`(527)、`mock_ai_provider.rs`、`ai_wiremock.rs`。生产二进制零编入。

## 6. 达标状态与余留施工面

对 §3 接线判据的达标结论：**254 条 IPC 中约 246 条已达标**（薄壳或宿主专有），
余留集中在 AI 族两刀（§3 余留清单已有名字）：

1. **AI chat / generate 请求装配下沉**：`ai_chat_impl_with_claim`（chat.rs，~160 行：
   schema 管道、context 文件、MCP 工具装配、取消注册、工具环调用）与 generate.rs 的
   请求装配。需要 `AiChatHost` 级端口（runtime 会话借用、context 目录读取、MCP 工具
   枚举/执行；现有 `AiToolLoopHost` 已覆盖工具环执行，缺请求装配侧）。
2. **`mcp_tool_definitions` 的 settings 过滤**（chat.rs）：`settings.mcp_client_servers`
   读取，随 MCP 域刀处理。

与 P 系列的交叉：

- **P2**：`src/driver_init.rs` 生成物 + `capabilities/*.host` 命名 + `resolve-drivers.mjs`
  改写 Cargo.toml——本盘点确认 17 个 driver feature 闭集已在 `unexpected_cfgs` 静态声明，
  optional-dep 化前提成立。
- **P3**：`store/` 与 `platform/` 的适配器（connections/settings/history/repositories）
  是 `application` vs `services` 判据书写的最好素材——同是端口适配，一个挂 platform、
  一个挂 store，判据落地时应说明白。
- **P5**：本文件的结构即为迁移基线（`modules/` → `crates/datazen/` 时，本文路径全量替换）。

维护约定：本文是快照不是契约，行数会漂；结构漂了（新目录/目录消失/职责换家）才必须更新，
随对应 PR 一并改。
