# DDD 模块化迁移 · 协调进度（hub）

> 依据 `.agents/skills/subagent-coordinator/SKILL.md`：本文件登记跨轨/跨会话进度，提交进 git，
> 随会话中断可恢复；整个 initiative 验收合并后删除，长期结论进代码/测试/正式文档。
> 集成分支：`feat/ddd-modules-migration`。权威方案：`docs/development/ddd-refactor-plan.md`（总纲）
> + `docs/development/ddd-refactor/`（细节，原 §2–§14）。

## 全局状态

- 当前阶段：**D3 基本完成**（词汇/端口/机制内核/连接资源运行时机制/编排归位 + wait_for_execution 第一步）；D2 完成 + D6 item 1 完成；下一步 **D4：Schema、表读写与 SQL 消费者**（kernel schema_cache → modules::schema；feature-core data_browsing/query_executor → table_browsing/table_editing；参考 06-first-slice-table-browsing.md）。
- 既有 flake 备注：`p4_usecase_journeys` 两用例在弱 CPU 沙箱偶发超时（D1 基线可复现、单跑通过、CI 绿），随 runtime 机制切片处理等待语义时一并解决。
- **CI 待办（PR #133 打通验证链后新暴露，非迁移引入）**：`platform::native_tool::tests::native_desktop_forged_resolution_and_unverified_version_fail_closed` 在 ubuntu-latest 上失败（`tools.resolve(&selection)` 第二次 resolve 报 `BackendUnavailable("native tool unavailable or unverified")`，tests.rs:164）。该测试 8bf6eb5 后未改动、本沙箱通过——疑似 runner 环境敏感（首次真正在 CI 执行；此前每轮 run 都在更早步骤失败）。**这是 fail-closed 安全语义测试，修前先弄清 resolve 二次调用的失败机理，不许为绿放松校验**。另外 `p4_usecase_journeys`/`p4_usecase` 相关 9 个 ignored 与 CM-60 基准照旧。
- 阶段与 PR 粒度：计划按 **D0–D8 九个阶段**推进（无 "wave" 概念，wave 仅是 coordinator 的
  并行批次术语）；每阶段可拆多个小 PR，每 PR 必须可构建、有真实消费者和测试。预估全程
  ~15–25 个 PR（D5 三件套各一个、D6 每功能族单独交付）。
- 环境限制（本沙箱）：**cargo 可用**（`~/.cargo/bin`，注意默认 PATH 不含，需
  `export PATH="$HOME/.cargo/bin:$PATH"`；磁盘紧张，构建用 `CARGO_INCREMENTAL=0`，
  定期清理 `target/debug/incremental`）。**src-tauri（桌面宿主）无法本地编译**：
  缺 GTK/WebKit 系统库且无 sudo；桌面侧改动靠 grep 审计 + 定向 cargo check（依赖
  crates）+ PR CI 验证。Node 侧检查（typecheck/vitest/scripts 检查）可用。

## 架构原则更新（用户 2026-10-26 指令，覆盖计划书旧表述）

1. **AI crate 收拢**：`ai-api` 并入 `ai-providers`；两者合并后**整体并入 modules**
   （`modules/src/ai/`），不再保留独立 AI crate。
2. **platform-infra 改名 infra**：`packages/platform-infra` → `packages/infra`
   （crate 名 `datazen-infra`），定位为 **drivers / modules / web / desktop 四类消费者
   中任意两者共用**的基础设施工具的家，提高代码复用率。
3. **Repository trait 放 domain 层**：trait 定义在各功能模块的 `domain`，
   具体实现由 web/desktop 宿主的 infrastructure 提供（与 01-target-architecture.md
   §4.1 第 5 条一致，后续所有模块迁移照此执行）。

## 已完成

| 日期 | 提交 | 内容 |
|---|---|---|
| 2026-10-25 | `d9d6a868b` | D1 settings 切片：`datazen-modules::settings`（domain/application）、桌面仓储适配 `src-tauri/src/platform/settings.rs`、旧包过渡再导出、门禁脚本扩展（详见总纲 §3 / 07-stages.md D1 小节） |
| 2026-10-26 | `eafaf87e1` | 计划书拆分：总纲（原路径）+ `ddd-refactor/` 10 个细节文件（01–10 对应原 §2–§14，保留编号）；AGENTS.md、docs/README.md 同步；验证脚本逐行比对零丢失 |
| 2026-10-26 | `b9ce44cd0` | D1 收尾清理：删除 `datazen_platform_api::settings::*`、`platform_api::ports::SettingsRepository`、`datazen_application::settings` 门面与 `ApplicationServices` 设置字段；`SettingsService` 改为持有 `Arc<dyn SettingsRepository>`；宿主 adapter 直接持有模块设置服务（`require_settings_service`），命令经 `platform::ipc` 取用例；feature-core/platform-api 直接依赖 `datazen-modules`（settings） |
| 2026-10-26 | 评审修复系列（3 个提交） | 独立 review 发现并修复：① `delete_connection`/`reorder_connections` 补齐 store 级 `write_lock` 并改用 `persist_connections_locked`（与 `save_connection` 锁口径对齐，消除并发旧快照覆写窗口），删除孤儿化的非 locked `persist_connections`；② connection_import 两处平台条件编译警告用 `cfg_attr(not(macos))` 精确放行（macOS 行为不变）；③ 修复 D3 第三切片 a 的 platform-api 自检测试编译破损：budget 垫片 glob 改命名再导出（inventory 文本抽取需要）、`SOURCE_FILES` 收敛 7 个已迁走的 budget 子文件条目、`lease_manager.rs` 与 runtime 测试共 3 处深层路径 `ports::budget::coordinator::` 改顶层、`job_kernel.rs` 清理 D3 迁移遗留的 3 组 unused imports。验证：platform-api / modules(454) / application / runtime lib(517) / job_kernel(19) / budget_cm65(14) 全绿；三道门禁 PASS；feature-core 警告清零 |

## 下一步（按计划顺序）

1. **D5 后续切片**：migration_common crate → modules::migration_common；随后 Schema Diff → Data Sync → Data Transfer 三 crate 逐个归位 modules（07-stages.md D5 工作项 2/5）；runtime 的 native_tool 模块勘察归位。
2. **D6/D7/D8** 按 07-stages.md 顺序推进；每阶段开工前先读 08-pr-playbook.md §11.1。

### D1.5 已完成（2026-10-26）

| 提交 | 内容 |
|---|---|
| `46dc15ba6` | ai-api 并入 ai-providers（第一步） |
| `e02d60d2c` | ai-providers 整体并入 modules/src/ai（第二步）；F-09 退役、F-14 豁免 reqwest |
| `a6d4b38fe` | platform-infra 改名 infra（第三步）+ 四类消费者共享审计结论 |
| `a15b228ba` | D2 第一切片：`datazen-modules::connection`（domain/application + 7 用例测试）；桌面 `DesktopConnectionStore`；删除 platform-api `ConnectionCatalog` 与 feature-core 重复 trait；`ApplicationServices` 去 connections；连接/分组/导入导出/schema diff/AI/MCP 消费者全部收口到模块服务 |
| `731c3b2d1` | D2 第二切片前半：platform-api 的 id/context/error（含全部单测）迁入 `modules::shared`（常驻，无 feature 门）；platform-api 原路径改再导出垫片 |
| `d740a3624` | D2 第二切片后半：Profile/namespace 词汇归位 `modules::connection`（domain::profile 双端口 + domain::target + application::profile_view 投影）；platform-api ports/dto/target 垫片化；PORT_TRAITS 口径扩展（声明或再导出）；4 个 to_view 调用点改 from_record；connection 模块转目录结构（domain/{catalog,profile,target}、application/{service,profile_view}） |
| `3d1da4df7` | D3 第一切片：`modules::execution` 建立（feature 依赖 connection）。domain 四文件（session/execution/target/artifact：句柄、状态机含 resolve_cancel 终态规则、能力快照、来源、ExecutionTarget+ObjectTarget、产物家族）；application 四文件（views/requests/events/idempotency：视图、回执、请求、事件信封、提交令牌）。platform-api dto 六文件 + target.rs 执行侧垫片化；测试随迁（modules 112 / platform-api 195） |
| `7eb386c1b` | D3 第二切片：七个执行端口归位——domain：SessionDirectory、ArtifactRepository（含 ChunkPayload/ByteRange/导出/吊销/配额与指纹函数）+ CanonicalTarget 家族；application：ExecutionSettingsReader/ExecutionTargetResolver/IdentityResolver/SubmissionTokenIssuer/EventSink。platform-api ports 七垫片；include_str 自检测试改指新文件名；modules 167 / platform-api 145 / runtime 517 全绿 |
| `317a1be3b` | D3 第三切片 a：机制内核先行——budget 端口家族→execution::application::budget、账本/调度→execution::runtime::budget、driver_registry→execution::runtime、sql_guard→execution::domain、hash→shared；kernel 再导出保旧路径、platform-api ports/budget 垫片；test-harness feature 链贯通；形态中立闸门扩到 modules（豁免 src/ai） |
| `667b9ff30` | D3 第三切片 b：connection/registry/gateway/resource/directory/tunnel/artifact/latency + RuntimeConnectionUseCases 编排整体迁入 execution::runtime（application/ 过渡期整块）；network/policy/secret 端口、ConnectionUseCases 签名、check_owner、TargetNamespaceShape 家族随迁；platform-api target.rs 全垫片化；runtime 只余 native_tool（D5）+ bin + tests + 再导出 facade；路径敏感自检测试全部重定位；modules 872 / runtime 444+9 / platform-api 46 / kernel 34 / feature-core 765 全绿 |
| `028d9d0b0` | D3 第三切片 c：编排归位 execution::application::orchestration；§6.4 等待能力第一步 wait_for_execution + WaitOutcome（超时≠取消，零 wire 变化）+ 三态行为测试（p4）；桌面关停装配显式延后至 D7/D8 宿主装配 |

### D4 已完成（2026-10-26，四切片）

| 提交 | 内容 |
|---|---|
| `ea4e38670` | fmt 收敛（D4 前两切片 rustfmt 漂移 7 文件；前一会话推送、主检出未含，随 rebase 收编） |
| `dda9cc051` | fix(commands)：SchemaError From 路径补齐——728e33e 后 src-tauri 仍引用不存在的顶层 schema::SchemaError，改指 application::SchemaError（前一会话推送） |
| `1844ee8fb` | D4 第一切片：schema 元数据缓存归位 `modules::schema`（kernel schema_cache + LegacyCatalogReader 分层词汇 → schema::application 的 metadata/physical_context/scope/ports + CachedColumns/SchemaCache）；platform-api 对应路径垫片化；缓存 key 含组织/连接/目标/revision 隔离维度 |
| `728e33e5e` | D4 第二切片：schema 应用服务归位 modules::schema::application；feature-core services 按 §5.5 再导出垫片 |
| `730ce27e4` | D4 第三切片：表浏览读链路归位 modules::table_browsing（query_executor 全套 + browsing + paging；keyset/count/筛选排序）；feature-core 再导出垫片；读链路无 FeatureError 依赖 |
| `a8ca8cc39` | D4 第四切片：表编辑写链路归位 modules::table_editing（data_browsing 全套：ChangeSet/insert_plan/row_identity/write_exec/tests）；新 error.rs（TableEditingError，逐字兼容）+ ports.rs（ArtifactChunkReader 消费者端口）；feature-core data_browsing 改再导出垫片 + From<TableEditingError>；宿主 CommandError From 接线；顺手收敛 D3 遗留 unused import ×3 + fmt 漂移 6 文件；modules 1160 / feature-core 518+14×3 全绿；六道门禁 PASS |
| `4f5f62da9` | D4 第五切片：结果网格写回规则归位 table_editing::result_edit（DB-14 全套判定 + WriteTarget）；宿主依赖改指模块词汇（schema/connection/execution）；FeatureError→TableEditingError 仅 Validation 族；feature-core 垫片再导出，桌面 result_origin/result_write 与集成测试零改动；modules 1178（+18）/ feature-core 518 / reasons 4 / locating 3 全绿；D4 工作项 1-4 模块侧完成，工作项 5（方言测试留 driver、UI 行为 Host E2E）归 CI/E2E 验收 |
| `3507e0c08` | fix(table_columns)：宿主适配器随 D4 契约迁移——TableColumnSource 签名改回 TableEditingError（前一会话推送、主检出未含，随 rebase 收编） |

### D5 已完成（2026-10-26，收官）

| 提交 | 内容 |
|---|---|
| `aee5cafc0` | D5 第一切片：jobs 三分归位 modules::jobs——kernel::job 全家（17 文件）+ platform-api Job 词汇（ports/job、dto/job）+ 时基端口（→shared::clock，常驻）+ native_tool 词汇（→jobs::domain::native_tool）；kernel/platform-api 垫片化；kernel/src/schema_cache 孤儿目录删除；jobs/mod.rs 镜像旧扁平名与深路径；form_neutrality 闸门清单随迁。验证：modules 1221 / runtime 9+32 / platform-api 25 / feature-core 500 / schema-diff 179 / data-sync 12 / data-transfer 253 全绿；五道门禁 PASS |
| `d16e0a9c2` | D5 第二切片：migration-common 共享纯算法归位 modules::migration_common（07-stages D5 工作项 5 后半） |
| `46db92507` | D5 第三切片：Schema Diff 引擎归位 modules::schema_diff（三件套之一） |
| `2fd65d74f` | D5 第四切片：Data Sync 引擎归位 modules::data_sync（三件套之二；41 文件）。modules 新增 data_sync feature（rusqlite bundled 随 SQLite 侧实现）；垫片保留集成测试（cm43–cm46 等持有 driver dev-deps）；modules 内联单测的家族归一化经 dev-deps + `#[cfg(test)] extern crate` force-link 三驱动（cargo 允许 dev-dep 成环）；shared::migration（EndpointIdentity）随迁自 platform-api。验证：modules 242 / 垫片 25 全绿 |
| `3694cfd94` | D5 第五切片：Data Transfer 引擎归位 modules::data_transfer（三件套之三；56 文件）。flate2 随 sql_file gzip、inventory 随 transfer 适配器自注册进入 feature；引用收口：kernel::job→jobs、kernel::budget→execution::runtime::budget、platform-api id/context/ports/dto→shared/jobs/execution 各归位点、migration_common 指模块内；垫片保留 filter_operator_refusal 集成测试（补 driver-api dev-dep）。验证：modules 250 / 垫片 3 全绿 / workspace check 通过。**D5 收官**（jobs 三分 + migration_common + 三件套 + native_tool 全部归位；退出准则中的真实驱动/UI 旅程证据归 CI/E2E 验收） |
| `b248e15` + `b53702f` + `beba3d6` | CI 随迁修复（评审 agent，PR #133 首次 CI run 40274319982/40282921088 两层失败定位）：① `shared/scope.rs` doctest 仍指 platform-api 旧路径（E0433），改 crate 内自洽路径恢复编译校验；② chrono 转 modules 常驻依赖——shared::clock 无 feature 门，settings/connection 等单 feature 矩阵腿与默认构建在 clock.rs 编译破损（先例：shared 常驻 ⇒ serde/thiserror 非可选）；③ modules 补声明 webdriver feature + data-transfer 垫片恢复转发（src-tauri E2E 传导链 + 8 条 unexpected_cfgs 清零）；④ datazen-migration-common 垫片零消费方且 F-08 违规（→datazen-modules/infra），提前退役（原定 D8），DDD 门禁 DOMAIN_PACKAGES 同步；⑤ cm06 负例 doctest 随 D3-3b 归位改 modules 内路径（compile_fail 负例被 E0433 歪打正着放行，可编译对照如实转红拦截）。验证：modules doctest（默认/CI 同款 features/单 feature 腿/all-features）全绿、三门禁 exit 0、其余核心 crate doctest 面扫描为空 |

## 会话恢复要点

- 分支 `feat/ddd-modules-migration`，与远端同步；主检出即工作区（本轮文档工作未用 worktree）。
- 代码级开发（D2 起）按 coordinator 技能走 worktree + Coder/Tester 派发；Rust 编译验证依赖 CI。
- 沙箱磁盘 9.9G 极易打满：`CARGO_INCREMENTAL=0` 必开；测试二进制（deps 下无扩展名大文件）
  跑完即删（`find target/debug/deps -maxdepth 1 -type f -size +30M ! -name "*.rlib" ! -name "*.rmeta" ! -name "*.d" -delete`）；
  `~/.cargo/registry/src` 可随时清（自动从 cache 重解压）。
- 简报落点参考：06-first-slice-table-browsing.md §9.4 的命令族台账记录了 P3 阶段命令族
  迁移事实（历史，勿混淆为 D 阶段）。

### D6 进行中（2026-10-26，续）

| 提交 | 内容 |
|---|---|
| `b248e153a` / `b53702f50` | （前一会话推送、本轮 rebase 收编）rust job 首次 CI 三处编译修复（shared/scope doctest 视角、chrono 转常驻依赖并收敛各 feature 清单、webdriver feature 补声明）；零消费方的 datazen-migration-common 垫片提前退役 |
| `81a38480c` | D6 第一切片（item 1 后半）：History 归位 `modules::history`——domain（query/migration_run/scope/repository 端口）+ application（HistoryService）；仓储 trait 归模块 domain（§7.1/§7.3），桌面 SQLite 实现经 `platform/history.rs DesktopHistoryRepositories` 适配同一 `HistoryDb`；`DesktopSessionBackend` 构造注入 `QueryHistoryRepository` 端口（执行路径写历史不再经 DesktopRepository 门面）；消费者全收口（commands history/query/driver_command/ai、MCP resources、AI 分析、全部相关测试）；`DesktopRepository` 历史门面方法删除（store/history.rs 只剩 favorites）；platform-api dto::migration 垫片化。验证：history 10 / 全 feature-lite 1234 / platform-api 21 全绿 |
| `8f3e97014` | fix：history feature 对齐 chrono 常驻化（81a3848 rebase 到 b248e153 后残留 `dep:chrono` 导致非可选依赖报错，收敛为 `dep:async-trait + dep:tracing`） |
| `0291f8e27` | D6 第一切片 a / item 3 前半：Workflow 领域词汇归位 `modules::workflow::domain`（model/error/executable：step/block/输出/错误策略/执行结果，纯 wire 词汇仅 serde）。执行器/注册表/调度器等应用层留待后续切片 b |
| `78e8f882d` | style：rustfmt 收敛 history 切片随迁文件格式漂移 |
| `7923998fd` | fix(history)：补齐 D6 第一切片宿主集成缺口——HistoryError 从模块平铺导出 + 宿主 CommandError From 转换 |
| `1e9b7e959` | D6 第二切片 / item 2：Dashboard 家族归位 `modules::dashboard`——词源 feature-core::dashboard 全家族（11 文件）逐字迁移，分层 domain（records/types 含 serde 形状钉单测/repository 端口/schedule_grant）+ application（create/runs/alert/archive/execute/monitor/notifications）；feature-core 11 文件垫片化；Monitor 授权元数据的 ExecutionTarget/sql_guard 改指模块内定义；形态中立闸门配套词级窄域豁免（ALLOWED_WIRE_VOCABULARY）+ 豁免自证测试。验证：dashboard 48 / feature-lite 1306 / feature-core 429 全绿 |
| `943de5540` / `4c0875a4d` | （前一会话推送、本轮 rebase 收编）rustfmt 收敛 dashboard 切片与 form_neutrality 格式漂移；fix(mcp)：history seed 提前到 server 构建之前（AppState 所有权先取服务后移交） |
| `21d940450` | **形态中立收口（用户 2026-10-26 指令：形态字面量若需改前端则一并改）**：`AlertChannel::Desktop → App`，wire 值 `"desktop" → "app"`；序列化只写新值，旧持久化字面量经自定义 Deserialize 拆开拼接收映射到 App（不破坏已存告警规则）；form_neutrality 的 ALLOWED_WIRE_VOCABULARY 清空（机制保留）；src-tauri 分发臂/测试样例、前端类型与默认渠道（仅两处消费，grep 审计无遗漏）、shared-boundaries-and-ports.md 文档同步。验证：dashboard 51 / form_neutrality 6 / feature-lite 1310 / feature-core 429 全绿；module-layers/ddd-layers/id-terminology 门禁通过；前端 typecheck/vitest 归 PR CI |
| `90dce3343` / `2d3a30c9e` | （前一会话推送、本轮 rebase 收编）fix(workflow)：文档行号守卫随 D6 dashboard 切片改指 modules 新位置；rustfmt 收敛 dashboard/domain/types.rs（21d9404 随迁漂移） |
| `42abe43e2` | D6 第三切片 / item 3 收官：Workflow 引擎与用例归位 `modules::workflow`——feature-core::workflow 剩余 14 文件逐字迁移合成三层（domain：command/history/plan_guard/target_plan/repository 端口；application：host/context/conditions/data_step/command_runtime/resource_execution 经 execution 端口派发保留 block 资源边界/registry serde_yaml 定义编解码/results；engine：executor 步骤归一 Driver Command 派发 + scheduler tick/在途守卫/取消）。同波次 item 4 切片二：MCP 命令准备策略 `feature-core::services::command_policy → modules::mcp::application::command_policy`（权限档位映射/命令定义解析含 schema 元数据命令合成/调用校验/SQL 参数绑定/信封目标注入；宿主 driver_command 适配器与 workflow resource_execution 共用）；mcp feature 补 schema 依赖。feature-core workflow 14 文件 + services/command_policy 垫片化（旧路径不变，src-tauri 零改动）；workflow feature 接线（serde_yaml/regex 新 optional + execution/schema/table-editing/mcp）。验证：modules feature-lite 含 jobs 1303（workflow 102 随迁）/ feature-core 407（lib 318 + 集成 89）/ module-layers + id-terminology 门禁 PASS / rustfmt 收敛；ddd-layers 全量 metadata 门禁与 src-tauri 编译归 PR CI（沙箱缺 brotli 等约 14 包，既有决策：不下载，PR CI 兜底） |

## 范围决策更新（用户 2026-10-26 指令，第二次）

**Web 功能移出本次重构范围**：本次不开发 Web 功能，Web 宿主（原 D7）延后到
后续版本另立阶段交付。已修订：07-stages.md（D7 整段标注移出 + D8 输入改桌面
单宿主）、10-checks-and-release.md（§13.3 web 命令延后注释、§14.2 验收项改
"桌面单宿主模块化版本"表述）、总纲阶段表（D7 移出 + D4/D5 状态同步）、
06-first-slice（Web 集成验证延后）、05-host-assembly §8.4。01-target-architecture
的 web/ 目标命名（待实现）保留为未来约定。模块 feature 中立性（无 Tauri 泄入
共享层）仍由门禁持续守护。D8 在桌面单宿主范围内进行。

## D6 下一步（按 07-stages 顺序；item 1/2/3 已完成，item 4 已完成两切片）

1. **AI / MCP 剩余**（item 4 收尾）：feature-core 中 Provider-neutral 业务
   （会话编排、工具循环、安全策略）与 MCP 业务分发残余；认证/stdio/
   HTTP 登录/事件传输在宿主。已归位：mcp 三文件业务规则（66c88bdaa）+
   command_policy（42abe43e2）。
2. **Monitor、Export、Backup/Restore、Wapps、Tunnel**（item 5）：按各自
   模型迁移，保留 grant、文件安全、恢复锁、沙箱桥与网络限制。注：
   Monitor 刷新引擎已随 dashboard 切片入模块（1e9b7e959），此处余项为
   宿主侧监控装配与残余词汇。
3. **connection_import / services 残余**（fk_navigation/referenced_by/
   value_search/db_tools/migration_endpoint 等）随 item 5 一并评估归位。
4. 收尾：移除 DesktopRepository 残余领域业务方法（favorites 尚在，属宿主
   文件存储还是迁模块随上述切片顺带评估）。

### 会话恢复备忘（本轮实测）

- 磁盘极紧（9.9G 总量，target 7G+）：feature-core 全量集成测试需**逐目标**
  编译-运行-删除（`--test <name>` 循环，每轮删 deps 下 >10M 无扩展名
  二进制）；一次性 `cargo test -p datazen-feature-core` 会打满磁盘致
  rust-lld 崩溃。
- 网络抖动：GitHub TLS 频繁中断，fetch/push 需重试与 `--depth=N` 渐进；
  origin remote 已内嵌 PAT。
- feature-lite 全量口径 = settings,history,workflow,dashboard,
  connection,named_views,execution,schema,migration_common,ai,mcp,
  webdriver,jobs（本轮 1303）。
- ddd-layers 门禁需全量 metadata（缺 brotli 等约 14 包）：本地跳过，
  PR CI 兜底（用户 2026-10-26 决策）。
