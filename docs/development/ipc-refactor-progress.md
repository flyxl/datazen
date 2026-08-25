# IPC 重构进度管理

> 依据 [dev-workflow.txt](./dev-workflow.txt) 与 [ipc-refactor-plan.md](./ipc-refactor-plan.md)。
> 分支：`feature/ipc-refactor`（worktree：`../datazen-ipc-refactor`）。
> 每个功能：编码子代理开发+单测 → commit → **全新**测试子代理测试（E2E 用例+结果、bug 记录、覆盖率≥80%）→ commit → bug 流转循环。

## 功能总览

| # | 功能 | 对应决策 | 状态 | 编码 commit | 测试 commit |
|---|------|---------|------|------------|------------|
| F1 | 废弃 `use_database`，query/stream/explain 显式传参 | 决策 1 | 测试中 | 34a28420 | 本提交 |
| F2 | ADB 命令迁移 SQLite 驱动（DriverCommandDefinition） | 决策 2 | 未开始 | — | — |
| F3 | backup/restore 合并 + `restore_sql_file` 四合一（override_path 模式） | 决策 3+6 | 未开始 | — | — |
| F4 | connections / app-data 导入导出 override_path 合并 | 决策 3 | 未开始 | — | — |
| F5 | 删除纯文件读写 IPC（write_file/write_file_base64/read_file），E2E 改 Node fs | 决策 4 | 未开始 | — | — |
| F6 | 删除冗余命令（monitor_paused×2 / compare_table_data / classify_sync_pair） | 决策 5 | 未开始 | — | — |
| R | 回归测试 + 文档更新（架构文档/AGENTS.md）+ 合并 main | 步骤 6 | 未开始 | — | — |

状态机：`未开始 → 编码中 → 编码完成 → 测试中 → 已完成`；bug 流转见下方 Bug 台账。

## Bug 台账

> 2026-08-26 协调者：F1 测试轮不通过，BUG-001~004 置「验证不通过」，转入修复轮（流程第 4 步）。

| Bug ID | 所属功能 | 描述 | 状态 | 记录时间 | 验证记录 |
|--------|---------|------|------|---------|---------|
| F1-BUG-001 | F1 | 【高】SQL 编辑器库下拉切换不再作用于后端会话：`switchDatabase` 改纯本地状态后，编辑器执行链路（`panelStore.executeQuery` → `queryExecActions.runStreamingQuery/runBoundQuery` → `driverCommands.execute/executeStream`，command=`query`/`query_stream`）不携带 database，且 driver_command 输入 schema（`packages/driver-api/src/command.rs` query/query_stream 定义）无 `database` 字段 → 未限定 SQL 仍打到旧活动库。编码说明「其余查询路径经 driver_command 的会话已被惰性切换」在主链路不成立（仅 Explain 触发切库）。重现与文件见下方「F1 缺陷详情」 | 验证不通过 | 2026-08-26 | |
| F1-BUG-002 | F1 | 【中】TableView 打开非活动库的表取数错位：挂载期 useDatabase 预切被删后，`get_table_data` 无 database 参数，后端以 session `config.database` 为限定符（`schema.rs get_table_data_impl`）→ 报表不存在或静默返回同名异库数据。重现见「F1 缺陷详情」 | 验证不通过 | 2026-08-26 | |
| F1-BUG-003 | F1 | 【中】结构编辑器 DDL 无库定位：`TableStructureEditor` 移除 ensureDatabase 后，`plan_table_structure_changes` 仅收 dbSessionId → 跨库建表/改表可能作用于 session 活动库而非面板目标库。重现见「F1 缺陷详情」 | 验证不通过 | 2026-08-26 | |
| F1-BUG-004 | F1 | 【低】改动 TS 文件覆盖率不达标：ConnectionNavigatorTree.tsx 行覆盖 53.13%、TableStructureEditor.tsx 37.64%（要求 ≥80%）；其余数字见「覆盖率」小节 | 验证不通过 | 2026-08-26 | |

> BUG-001~003 与编码说明「遗留注意 1」同根因（非 query 族命令无 database 参数、入口不再预切库），但遗留说明给出的过渡缓解（"由任一带 database 的 query/stream/explain 惰性触发切库"）对编辑器主链路不生效，故按缺陷登记；由编码代理裁决在 F1 内修复（补参数/补预切）或明确降级为后续功能承接。

状态机：`待验证（新发现，编码代理未处理）/ 验证不通过 → 待验证（修复后等待复测）→ 已修复`

## 测试约定

- 单测：Host Rust（`cargo test -p datazen --lib`，共享主检出 target 缓存）、前端 vitest（受影响配置）
- 覆盖率：改动 TS 以 `vitest --coverage` 度量 ≥80%；Rust 无 llvm-cov 工具链时以新增单测清单佐证
- E2E 用例：每个功能在本文档登记用例清单与执行结果；真实 webdriver 全量回归统一在 R 阶段执行
- 测试代理只输出结果，不修复问题

## F1 废弃 use_database

### 范围
- 后端：`execute_query` / `execute_query_stream` / `get_explain` 增加 `database: Option<String>`；impl 内若与 session 当前库不同先 `driver.use_database()` 再执行；删除 `use_database` IPC 注册（`src-tauri/src/lib.rs` ~L819、`commands/schema.rs` 的 IPC 包装保留内部 impl 供内部调用则需评估）
- 前端：移除 `src/commands/database.ts` 的 useDatabase 封装及全部调用点（sqlFileExecution.ts 等）；schemaStore 仅维护前端 currentDatabase 状态
- E2E spec 同步：`bugfix-admin-commands.ts`、`zz-screenshots.ts` 改显式 database 参数
- 内部 `maybe_use_database`（sync/transfer/workflow 的 driver 层调用）不在本功能范围

### E2E 用例

> F1 的全部语义围绕"后端 session 活动库"这一运行时概念，端到端正确性必须依赖 webdriver 构建 + 真实多库实例（MySQL/PG）；本地状态迁移半场已由 vitest mock 层覆盖。故本轮无用例可标【本机可执行】，全部登记为【留待 R 阶段回归】。BUG-001/002/003 对应用例须在修复复测后执行。

| 编号 | 场景 | 前置 | 步骤 | 断言 | 标注 |
|------|------|------|------|------|------|
| F1-E2E-001 | 编辑器切库端到端生效（回归 BUG-001） | webdriver 构建；MySQL 实例含 db_a（连接默认库）与 db_b；已保存连接 | 连接 → 打开 SQL 编辑器 → 库下拉选 db_b → 执行 `SELECT DATABASE()` → 再查询 db_b 独有表 | `SELECT DATABASE()` 返回 db_b；独有表查询成功不报错 | 【留待 R 阶段回归】需真实 MySQL 多库；当前代码推演预期失败 |
| F1-E2E-002 | pin≠当前库惰性切库 + 会话记录更新 | 同上（PG 或 MySQL），会话活动库=db_a | 面板切到 db_b 后点 Explain → 再执行不带限定的 SQL | Explain 针对 db_b 成功返回计划；后续未限定 SQL 落在 db_b（`SELECT DATABASE()`/`current_schema()` 验证）；日志出现 `session active database switched` | 【留待 R 阶段回归】需真实多库实例与后端日志观测 |
| F1-E2E-003 | None/相同库零切库开销 | 会话活动库=面板当前库=db_a | 连续执行两次普通查询 + 一次流式长结果查询 | 全部成功；日志无 switched 记录（零次 driver.use_database） | 【留待 R 阶段回归】需日志观测 + 真实驱动 |
| F1-E2E-004 | 多库树打开非活动库表（回归 BUG-002） | MySQL 多库，db_a/db_b 各含 users 表且数据行数可区分 | 导航树展开 db_b → 点击其 users 表 | TableView 显示 db_b.users 数据（行数与 db_a 可区分），无报表不存在错误 | 【留待 R 阶段回归】需真实实例；当前推演预期失败 |
| F1-E2E-005 | 结构编辑器跨库 DDL 落库正确（回归 BUG-003） | MySQL 多库，活动库=db_a | 在 db_b 节点新建表 t_f1（CreateTable 面板）→ 填列执行 → 分别 `SHOW TABLES` 于 db_a/db_b | t_f1 仅出现在 db_b；db_a 无该表 | 【留待 R 阶段回归】需真实实例；当前推演存在落错库风险 |
| F1-E2E-006 | drop database 后 fallback 迁移 | ≥3 个库的真实实例；活动库=db_b 且具备 DROP 权限 | 右键删除 db_b 并确认 | 本地活动库迁移至 fallback（连接默认库方向）；树刷新无残留节点；后续未限定查询落在 fallback 库 | 【留待 R 阶段回归】需 DROP 权限真实实例 |
| F1-E2E-007 | restore / SQL 文件执行目标库正确 | PG 或 MySQL + 含建表语句的 .sql fixture；备份窗口或 SQL 文件入口 | 对指定库执行 restore_database_with_dialog / execute_sql_file_with_dialog | 对象落在命令自带的 database 参数库（不经 session 状态）；进度事件正常 | 【留待 R 阶段回归】需原生 dialog 流程 + 真实实例 |
| F1-E2E-008 | admin spec 显式参数回归 | TEST_MYSQL_*/TEST_PG_* 环境变量可用 | 运行 `e2e/specs/bugfix-admin-commands.ts` 全量 | 全部通过；spec 内无 use_database 调用（get_tables 显式 database 生效） | 【留待 R 阶段回归】需真实 MySQL/PG |
| F1-E2E-009 | 截图链路探活替代 | demo PG 实例 + 完整构建 | 运行 zz-screenshots 中 pinDemoPgDatabase 相关用例 | get_tables({database}) 探活成功；不再出现 use_database 报错 | 【留待 R 阶段回归】需 demo 数据与完整构建 |
| F1-E2E-010 | explain 面板显式传 currentDatabase | 任一支持 explain 的多库连接 | 切库 → 点击 Explain | ExplainResult 正常渲染且针对所选库（对不存在于旧库的对象给出预期计划/报错行为一致） | 【留待 R 阶段回归】UI 半场已被单测覆盖，端到端语义需真库 |

### 测试结果

测试代理独立复测（2026-08-26，commit 34a28420，worktree `feature/ipc-refactor`）：

| 套件 | 结果 | 与编码声明对比 |
|------|------|---------------|
| `cargo test -p datazen --lib`（共享主检出 target） | **1132 passed / 0 failed / 2 ignored** | 一致 ✅ |
| `npx vitest run` | **240 文件 / 1882 用例全过** | 一致 ✅ |
| `npx tsc --noEmit` | **0 错误**（exit 0） | 一致 ✅ |

新增 Rust 单测确认：`execute_query_switches_session_database_when_pinned_differs` / `execute_query_skips_switch_when_same_or_none` / `get_explain_switches_session_database_when_pinned_differs` 经 `cargo test -- --list` 确认存在且包含于上述通过数。

范围完整性审查（对照决策 1 五步）：✅ 无遗漏 ——
1. 三命令 impl + Tauri 包装均新增 `database: Option<String>`（query.rs diff 核对）；
2. 后端分发逻辑正确：pin trim 后非空且 ≠ session 当前库 → `driver.use_database` + `set_active_database`（与原 use_database_impl 行为等价）；None/空白/相同 → 短路且不提前解析 session（保留 not-connected 错误语义）；
3. IPC 注册（lib.rs）、schema.rs 包装与 use_database_impl、mock 测试调用全部删净；
4. 生产代码 `use_database|useDatabase` 调用点清零（Grep 工具全仓扫描；剩余命中仅为文档、驱动内部 trait 方法/maybe_use_database（明确不在 F1 范围）、注释与测试死 mock）；
5. E2E spec 同步完成：bugfix-admin-commands.ts 5 处 + zz-screenshots.ts 2 处，与编码说明一致。

逻辑正确性审查发现缺陷 4 项（F1-BUG-001~004，见台账），故 F1 保持「测试中」。

非缺陷观察项（不阻塞，供后续参考）：
- `ConnectionNavigatorTree.activateDatabase` 非缓存分支只更新 per-session entry、不刷新顶层扁平 `currentDatabase`；现调用点表格行渲染必来自 `dbTablesMap` 缓存（L1905），实际不可达，建议统一改走 `commitConnectionPatch` 消除双份状态的不对称。
- `ensure_session_database` 将"切库→执行"竞态窗口从一次 IPC 往返收窄到同一命令内 `use_database` 与 SQL 执行之间（无 per-session 互斥），并发异库 pin 理论上仍可交错，属架构固有权衡。
- 陈旧痕迹：QueryPanel.tsx ~L234 注释仍提 "racing the session useDatabase"；ConnectionNavigatorTree.test.tsx L122/L129、SchemaTree.test.tsx L95、TableView.test.tsx L20 残留死 mock。
- 全量 `--coverage` 下项目 Option C 门槛存在多处未达（DataTable statements 76.35%、ConnectionPage 52.8%、dashboard/** 等）——非 F1 引入，另行跟踪。
- F1 改造的 `execute_query` / `execute_query_stream` Tauri 命令当前在前端无调用点（编辑器实际走 driver_command 的 query/query_stream；仅 E2E invokeBackend 直调），新参数对 UI 仅 get_explain 生效——与 BUG-001 相关。

### 覆盖率

度量方式：全量 vitest 套件（240 文件 / 1882 用例）+ `--coverage.include` 过滤本次改动 TS 文件（v8 provider）。任务给定的"定向子集"命令（只跑 4 个测试路径）会显著低估（如 schemaStore 仅 46.05%），已按预案改用全量套件度量。

| 文件 | Statements | Branch | Funcs | **Lines** | ≥80% 行覆盖 |
|-----|-----------|--------|-------|-----------|------------|
| `src/lib/ensureNamespace.ts` | 95.07% (135/142) | 83.15% | 100% | **100%** (120/120) | ✅ |
| `src/stores/schemaStore.ts` | 87.86% (239/272) | 74.86% | 93.22% | **90.45%** (218/241) | ✅ |
| `src/windows/connection/ConnectionNavigatorTree.tsx` | 50% (503/1006) | 35.42% | 43.93% | **53.13%** (457/860) | ❌ → F1-BUG-004 |
| `src/windows/connection/TableStructureEditor.tsx` | 33.8% (71/210) | 27.2% | 23.91% | **37.64%** (67/178) | ❌ → F1-BUG-004 |

补充说明：
- 其余改动 TS 文件（QueryPanel / TableView / PanelContentRenderer / DocumentConnectionView / UnifiedSchemaTree / sqlFileExecution / commands/*）不在项目 Option C coverage include 配置范围内，v8 默认不采集，无法给出数字。
- Rust 无 llvm-cov 工具链，以单测清单 + 被测分支枚举佐证：`ensure_session_database` 四分支中"pin≠当前→切库并更新 session 记录""相同库/空白/None 零调用""explain 路径切库"有直接单测；stream 路径共享同一助手但无独立切库单测（小缺口，随 F1-BUG-004 一并补测建议）。
- 判定：❌ 2/4 文件未达 ≥80%，登记 F1-BUG-004。

#### F1 缺陷详情（重现步骤 / 相关文件）

**F1-BUG-001（高）SQL 编辑器库下拉切换不作用于后端会话**
- 重现（代码链路推演，GUI 实证留待 E2E F1-E2E-001）：
  1. 连接 MySQL 多库实例，连接默认库 db_a，另有 db_b；
  2. 打开 SQL 编辑器，库下拉由 db_a 切至 db_b（`schemaStore.switchDatabase` 现仅 `get_tables` + 本地状态）；
  3. 执行 `SELECT DATABASE()`。
- 预期：返回 db_b。实际（推演）：返回 db_a —— 执行走 `panelStore.executeQuery`(L343) → `queryExecActions.runStreamingQuery`(L90)/`runBoundQuery`(L118) → `driverCommands.execute/executeStream(command=query/query_stream)` → `execute_driver_command(_stream)`，全程无 database；driver-api 输入 schema（command.rs L190/L214）亦无该字段；后端按 session 活动库执行。
- 相关文件：`src/stores/schemaStore.ts`、`src/stores/queryExecActions.ts`、`src/commands/query.ts`、`packages/driver-api/src/command.rs`、`src-tauri/src/commands/driver_command.rs`、`src/windows/connection/QueryPanel.tsx`

**F1-BUG-002（中）TableView 打开非活动库的表取数错位**
- 重现（推演，留待 F1-E2E-004）：MySQL 多库 db_a/db_b 各有 users 表（数据可区分）→ 会话活动库 db_a → 导航树点击 db_b 的 users 表 → TableView 取数。
- 预期：显示 db_b.users。实际（推演）：`get_table_data` 无 database 参数，后端以 session `config.database`=db_a 解析（`schema.rs get_table_data_impl` L120-125）→ 报表不存在；若同名表存在则静默返回错误库数据。`commit_row_updates/deletes` 同路径受累。
- 相关文件：`src/windows/connection/TableView.tsx`、`src-tauri/src/commands/schema.rs`、`src/windows/connection/PanelContentRenderer.tsx`、`src/commands/database.ts`

**F1-BUG-003（中）结构编辑器跨库 DDL 无库定位**
- 重现（推演，留待 F1-E2E-005）：活动库 db_a → 在 db_b 节点 CreateTable → 填列执行。
- 预期：t_f1 建于 db_b。实际（推演）：`planTableStructureChanges(dbSessionId, request)` 不含 database（`src/commands/structure.ts` L12），DDL 作用于 session 活动库 db_a 或直接报错。
- 相关文件：`src/windows/connection/TableStructureEditor.tsx`、`src/commands/structure.ts`、`src/windows/connection/PanelContentRenderer.tsx`

**F1-BUG-004（低）改动 TS 文件覆盖率 <80%**
- 重现：见「覆盖率」小节度量命令与数字（ConnectionNavigatorTree.tsx 53.13%、TableStructureEditor.tsx 37.64%）。
- 建议：为两组件的关键交互分支补单测（树行渲染/右键动作、编辑器 DDL 构建/预览路径），并顺带补 stream 路径切库单测；两文件缺口大概率为既有欠账，但按本功能验收口径登记。
- 相关文件：`src/windows/connection/__tests__/ConnectionNavigatorTree.test.tsx`、`src/windows/connection/__tests__/TableStructureEditor.test.tsx`

### 编码说明（F1）

**改动清单**

后端（src-tauri/）：
- `commands/query.rs`：新增共享助手 `ensure_session_database(state, db_session_id, database: Option<&str>, op)`；`execute_query` / `execute_query_stream` / `get_explain` 的 impl 与 Tauri 包装均新增 `database: Option<String>`。impl 执行前：pin 非空且 ≠ session 当前库 → 先 `driver.use_database(&handle, db)` 再 `set_active_database` 更新 session 记录；`None` / 空白 / 相同 → 不动（不提前解析 session，保持原 not-connected 错误语义）
- `commands/schema.rs`：删除 IPC 包装 `pub async fn use_database` 与 `use_database_impl`；`schema_commands_with_connected_mock` 移除对应调用
- `lib.rs`：移除 `commands::use_database` 注册
- `testing/mock_driver.rs`：MockDriver 覆写 trait `use_database` 并按序记录到 `use_database_calls()`（供宿主单测断言）

前端（src/）：
- `commands/database.ts`：删除 `useDatabase` 封装
- `commands/query.ts`：`getExplain(dbSessionId, sql, database?)` 透传可选参数
- `stores/schemaStore.ts`：`loadTables` / `switchDatabase` 不再发切库 IPC，仅维护本地 currentDatabase；`ensureNamespacePath` deps 去掉 `useDatabase`
- `lib/ensureNamespace.ts`：`EnsureDeps` 移除 `useDatabase`，4 处调用点删除（`getTables` 本就显式带 database）
- 组件去 IPC 化：`QueryPanel.tsx`（explain 传 currentDatabase）、`TableView.tsx`、`TableStructureEditor.tsx`（ensureDatabase 体系整体移除）、`PanelContentRenderer.tsx`（CreateTable 门控改本地判断）、`DocumentConnectionView.tsx`、`ConnectionNavigatorTree.tsx`（activateDatabase / drop-database fallback 改为更新本地 currentDatabase，含顶层与 per-session 两份）、`schema-tree/UnifiedSchemaTree.tsx`
- `lib/sqlFileExecution.ts`：删除 `invoke('use_database', …)`

测试同步：
- 前端：`schemaStore.test.ts`、`ensureNamespace.test.ts`、`ConnectionNavigatorTree.test.tsx`、`TableStructureEditor.test.tsx`、`BackupWindow.test.tsx` 断言/mock 改为无 use_database 行为
- E2E spec 显式参数模式：`e2e/specs/bugfix-admin-commands.ts`（5 处）、`e2e/specs/zz-screenshots.ts`（pinDemoPgDatabase 等 2 处）→ `get_tables({ dbSessionId, database })`

**新签名**
- `execute_query(db_session_id, sql, database? Option<String>) -> MultiQueryResult`
- `execute_query_stream(…, sql, database?, on_event, apply_result_limit?, record_history?)`
- `get_explain(db_session_id, sql, database?) -> ExplainResult`
- 前端 `queryCommands.getExplain(dbSessionId, sql, database?)`；其余查询路径经 driver_command 的会话已被上述命令惰性切换

**新增 Rust 单测**（`commands/query.rs` tests，connected-mock 基建 TestAppState + MockDriver 录制）
1. `execute_query_switches_session_database_when_pinned_differs` — pin ≠ 当前库 → 触发切库后执行，且 session config.database 同步更新
2. `execute_query_skips_switch_when_same_or_none` — None / 相同库 / 空白 → 零次 driver.use_database
3. `get_explain_switches_session_database_when_pinned_differs` — explain 路径同样触发切库

**验证三件套结果**
- `cargo test -p datazen --lib`：**1132 passed / 0 failed / 2 ignored**
- `npx vitest run`：**240 文件 / 1882 用例全过**
- `npx tsc --noEmit`：**0 错误**

**遗留注意**
1. 非 query 族命令（`get_table_data` / `commit_row_updates` / structure DDL / 通用 `execute_driver_command` 等）尚无 `database` 参数：原先依赖「先切库再操作」的入口（TableView 打开非活动库的表、建表/结构编辑等）在 F1 后不再预切 session，需等后续功能给这些命令补显式 `database` 参数；过渡期跨库操作由任一带 `database` 的 query/stream/explain 惰性触发切库。
2. `src-tauri/capabilities/default.json` 为 codegen：本 worktree 曾被新版 resolve-drivers 写入 `redis:default`，导致裸 `cargo test -p datazen --lib`（default features=[]，redis 插件未编译）构建失败；现已对齐主检出权限集（无 redis）。若后续以 `--drivers=all` 等选型重新生成，需用带 feature 的构建验证。
3. 截图脚本 `zz-screenshots.ts` 的「钉住 demo 库」改为 `get_tables({database})` 探活；会话级钉住语义现完全依赖查询显式携带 `database`。

---

## F2 ADB 迁移 SQLite 驱动
（占位）

## F3 backup/restore 合并 + restore_sql_file
（占位）

## F4 connections/app-data 导入导出合并
（占位）

## F5 删除纯文件读写 IPC
（占位）

## F6 删除冗余命令
（占位）

## R 回归与收尾
（占位）
