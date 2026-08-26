# IPC 重构进度管理

> 依据 [dev-workflow.txt](./dev-workflow.txt) 与 [ipc-refactor-plan.md](./ipc-refactor-plan.md)。
> 分支：`feature/ipc-refactor`（worktree：`../datazen-ipc-refactor`）。
> 每个功能：编码子代理开发+单测 → commit → **全新**测试子代理测试（E2E 用例+结果、bug 记录、覆盖率≥80%）→ commit → bug 流转循环。

## 功能总览

| # | 功能 | 对应决策 | 状态 | 编码 commit | 测试 commit |
|---|------|---------|------|------------|------------|
| F1 | 废弃 `use_database`，query/stream/explain 显式传参 | 决策 1 | 已完成 | 34a28420 | 3d23cfd1 · 8b85cd49 · 本提交 |
| F2 | ADB 命令迁移 SQLite 驱动（DriverCommandDefinition） | 决策 2 | 编码完成 | 本提交 | — |
| F3 | backup/restore 合并 + `restore_sql_file` 四合一（override_path 模式） | 决策 3+6 | 未开始 | — | — |
| F4 | connections / app-data 导入导出 override_path 合并 | 决策 3 | 未开始 | — | — |
| F5 | 删除纯文件读写 IPC（write_file/write_file_base64/read_file），E2E 改 Node fs | 决策 4 | 已完成 | 本提交 | 本提交 |
| F6 | 删除冗余命令（monitor_paused×2 / compare_table_data / classify_sync_pair） | 决策 5 | 已完成 | d4e33801 | 本提交 |
| F7 | 驱动级 SQL 定位重写（限定名内联、无会话切换；PG 系含 database+schema 双维度） | 用户新指令 2026-08-26 | 未开始 | — | — |
| B5 | ConnectionNavigatorTree 刷新丢失已展开分类修复（=F1-BUG-005） | 既有缺陷 | 未开始 | — | — |
| R | 回归测试 + 文档更新（架构文档/AGENTS.md）+ 合并 main | 步骤 6 | 未开始 | — | — |

状态机：`未开始 → 编码中 → 编码完成 → 测试中 → 已完成`；bug 流转见下方 Bug 台账。

## Bug 台账

> 2026-08-26 协调者：F1 测试轮不通过，BUG-001~004 置「验证不通过」，转入修复轮（流程第 4 步）。
> 2026-08-26 复验代理（全新实例）：BUG-001~004 逐项闭环验证通过，置「已修复」；新发现既有缺陷 F1-BUG-005（连接刷新丢失已展开分类内容）登记为「待验证」，是否并入 F1 由协调者裁决。
> 2026-08-26 协调者裁决：F1-BUG-005 为既有缺陷、非 F1 引入，**不并入已关闭的 F1**；立为独立修复循环 B5（见功能总览），排期 F7 之后、R 之前，届时按标准循环派修复代理+全新测试代理。

| Bug ID | 所属功能 | 描述 | 状态 | 记录时间 | 验证记录 |
|--------|---------|------|------|---------|---------|
| F1-BUG-001 | F1 | 【高】SQL 编辑器库下拉切换不再作用于后端会话：`switchDatabase` 改纯本地状态后，编辑器执行链路（`panelStore.executeQuery` → `queryExecActions.runStreamingQuery/runBoundQuery` → `driverCommands.execute/executeStream`，command=`query`/`query_stream`）不携带 database，且 driver_command 输入 schema（`packages/driver-api/src/command.rs` query/query_stream 定义）无 `database` 字段 → 未限定 SQL 仍打到旧活动库。编码说明「其余查询路径经 driver_command 的会话已被惰性切换」在主链路不成立（仅 Explain 触发切库）。重现与文件见下方「F1 缺陷详情」 | 已修复 | 2026-08-26 | 修复轮（commit：`fix(ipc): f1 bugs - db scoping on driver_command path, table data/ddl targeting, coverage`）：driver_command 请求信封新增可选 `database`，宿主统一前置切库；前端编辑器全链路携带面板目标库。**复验通过**（2026-08-26 全新实例，commit 8b85cd49）：信封字段 `#[serde(default)]` 向后兼容；stream/bound 两入口均先 `ensure_session_database`（unbound driverType 显式忽略）；`handle.id` 即运行时 dbSessionId；前端 panelStore(`panelTargetDatabase`)→queryExecActions→queryCommands→driverCommands 全部调用点带参；Rust 单测断言切库+session 记录更新+零调用分支、前端单测断言真实透传值，非空转 |
| F1-BUG-002 | F1 | 【中】TableView 打开非活动库的表取数错位：挂载期 useDatabase 预切被删后，`get_table_data` 无 database 参数，后端以 session `config.database` 为限定符（`schema.rs get_table_data_impl`）→ 报表不存在或静默返回同名异库数据。重现见「F1 缺陷详情」 | 已修复 | 2026-08-26 | 修复轮（同上提交）：`get_table_data` 新增可选 `database` 并复用 `ensure_session_database`；TableView 打开表时携带面板目标库。**复验通过**（同上）：impl 在解析 session 前置切库；TableView 挂载+两处重试按钮均传 `database`；tableDataStore 按 per-connection 记忆 `activeDatabase` 保证翻页/过滤不漂移且有单测；Rust `get_table_data_pins_session_to_target_database` 断言取数落目标库 |
| F1-BUG-003 | F1 | 【中】结构编辑器 DDL 无库定位：`TableStructureEditor` 移除 ensureDatabase 后，`plan_table_structure_changes` 仅收 dbSessionId → 跨库建表/改表可能作用于 session 活动库而非面板目标库。重现见「F1 缺陷详情」 | 已修复 | 2026-08-26 | 修复轮（同上提交）：`plan_table_structure_changes` 以同一机制处理，编辑器传入目标库。**复验通过**（同上）：wrapper+impl 均收 `database` 且走同一 `ensure_session_database`，无第二套切库语义；preview/execute 两路径透传组件 prop；DDL 语句执行不带参依赖 plan 阶段持久化 pin（设计决策 3），成立；Rust 双单测（pin 切库 / 无 pin 保持）+ 前端断言第三参 `'db_b'`/`null` |
| F1-BUG-004 | F1 | 【低】改动 TS 文件覆盖率不达标：ConnectionNavigatorTree.tsx 行覆盖 53.13%、TableStructureEditor.tsx 37.64%（要求 ≥80%）；其余数字见「覆盖率」小节 | 已修复 | 2026-08-26 | 修复轮（同上提交）：两文件 vitest 用例扩展达 ≥80% 行覆盖（新数字见「覆盖率」表）；并补 Rust stream 路径独立切库单测。**复验通过**（同上）：全新实例重跑全量 1963 用例 + `--coverage.include` 过滤实测 ConnectionNavigatorTree.tsx 行覆盖 **96.74%**、TableStructureEditor.tsx **97.75%**，与修复轮声称数字逐位一致；ConnectionNavigatorTree.test.tsx 实测 64 用例独立运行全绿 |
| F1-BUG-005 | F1 | 【中】【既有行为，非本轮引入】连接刷新后已展开对象分类内容丢失且不自动恢复：单库树（如 SQLite）展开 procedure 分类出现条目后，执行连接级或库级刷新，分类行仍呈展开态但条目消失、计数归零，观察窗 3s 内无任何重载。根因指向 `useExpandedDbCacheRefresh` 在 schemaEpoch 变化时 `clearCaches` 清空该会话全部 `dbObjectsMap` 后仅重载展开库的**表缓存**、不重载对象分类缓存，与 `refreshConnection.reloadExpandedObjectCategories` 的重载竞态失败。多库树表节点不受影响（走 `reloadDbTables` 恢复）。临时探针在 4ba4831a（F1 前）/046acf7a（修复轮前）/8b85cd49 三时点症状一致，判定为既有缺陷。是否纳入 F1 范围由协调者裁决；重现步骤见「F1 缺陷详情」 | 待验证 | 2026-08-26 | 复验轮新登记（2026-08-26 复验 commit 8b85cd49 时发现） |
| F5-BUG-001 | F5 | 【低】【文档漂移，非代码缺陷】`docs/development/e2e-testing.md` L154 与 `docs/development/e2e-coverage.md` L106 仍表述「webdriver 构建保留 `write_file` / `export_app_data(path)` 等路径 API 供 E2E 使用」，与决策 4（三路径 IPC 已删、E2E 改 Node fs）相悖，会误导后续 E2E 编写者。重现：`grep -rn "write_file" docs/development/e2e-testing.md docs/development/e2e-coverage.md`。commands.md 已同步、两文件不在本决策承诺范围；归属 R 阶段文档收口统一修正，不阻塞 F5 判定 | 待验证 | 2026-08-26 | F5 复验轮新登记（commit 8f9e4a9c 复验时发现） |

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
| F1-E2E-001 | 编辑器切库端到端生效（回归 BUG-001） | webdriver 构建；MySQL 实例含 db_a（连接默认库）与 db_b；已保存连接 | 连接 → 打开 SQL 编辑器 → 库下拉选 db_b → 执行 `SELECT DATABASE()` → 再查询 db_b 独有表 | `SELECT DATABASE()` 返回 db_b；独有表查询成功不报错 | 【留待 R 阶段回归】需真实 MySQL 多库；~~当前代码推演预期失败~~ **[复验补注]** 修复后主链路已闭环：bound(`query`) 与 stream(`query_stream`) 两路径均经 driver_command 信封 `database` 字段由宿主 `ensure_session_database` 前置切库，本用例应补验流式长结果路径（点击执行即 stream），并观察日志出现一次 `session active database switched` |
| F1-E2E-002 | pin≠当前库惰性切库 + 会话记录更新 | 同上（PG 或 MySQL），会话活动库=db_a | 面板切到 db_b 后点 Explain → 再执行不带限定的 SQL | Explain 针对 db_b 成功返回计划；后续未限定 SQL 落在 db_b（`SELECT DATABASE()`/`current_schema()` 验证）；日志出现 `session active database switched` | 【留待 R 阶段回归】需真实多库实例与后端日志观测 |
| F1-E2E-003 | None/相同库零切库开销 | 会话活动库=面板当前库=db_a | 连续执行两次普通查询 + 一次流式长结果查询 | 全部成功；日志无 switched 记录（零次 driver.use_database） | 【留待 R 阶段回归】需日志观测 + 真实驱动 |
| F1-E2E-004 | 多库树打开非活动库表（回归 BUG-002） | MySQL 多库，db_a/db_b 各含 users 表且数据行数可区分 | 导航树展开 db_b → 点击其 users 表 | TableView 显示 db_b.users 数据（行数与 db_a 可区分），无报表不存在错误 | 【留待 R 阶段回归】需真实实例；当前推演预期失败 |
| F1-E2E-005 | 结构编辑器跨库 DDL 落库正确（回归 BUG-003） | MySQL 多库，活动库=db_a | 在 db_b 节点新建表 t_f1（CreateTable 面板）→ 填列执行 → 分别 `SHOW TABLES` 于 db_a/db_b | t_f1 仅出现在 db_b；db_a 无该表 | 【留待 R 阶段回归】需真实实例；当前推演存在落错库风险 |
| F1-E2E-006 | drop database 后 fallback 迁移 | ≥3 个库的真实实例；活动库=db_b 且具备 DROP 权限 | 右键删除 db_b 并确认 | 本地活动库迁移至 fallback（连接默认库方向）；树刷新无残留节点；后续未限定查询落在 fallback 库 | 【留待 R 阶段回归】需 DROP 权限真实实例 |
| F1-E2E-007 | restore / SQL 文件执行目标库正确 | PG 或 MySQL + 含建表语句的 .sql fixture；备份窗口或 SQL 文件入口 | 对指定库执行 restore_database_with_dialog / execute_sql_file_with_dialog | 对象落在命令自带的 database 参数库（不经 session 状态）；进度事件正常 | 【留待 R 阶段回归】需原生 dialog 流程 + 真实实例 |
| F1-E2E-008 | admin spec 显式参数回归 | TEST_MYSQL_*/TEST_PG_* 环境变量可用 | 运行 `e2e/specs/bugfix-admin-commands.ts` 全量 | 全部通过；spec 内无 use_database 调用（get_tables 显式 database 生效） | 【留待 R 阶段回归】需真实 MySQL/PG |
| F1-E2E-009 | 截图链路探活替代 | demo PG 实例 + 完整构建 | 运行 zz-screenshots 中 pinDemoPgDatabase 相关用例 | get_tables({database}) 探活成功；不再出现 use_database 报错 | 【留待 R 阶段回归】需 demo 数据与完整构建 |
| F1-E2E-010 | explain 面板显式传 currentDatabase | 任一支持 explain 的多库连接 | 切库 → 点击 Explain | ExplainResult 正常渲染且针对所选库（对不存在于旧库的对象给出预期计划/报错行为一致） | 【留待 R 阶段回归】UI 半场已被单测覆盖，端到端语义需真库 |
| F1-E2E-011 | 连接刷新保留已展开对象分类（回归 F1-BUG-005，若纳入修复范围则必测） | 单库树驱动（SQLite）+ 含 procedure/function 对象的实例；多库树对照 | 展开某对象分类出现条目 → 右键连接「刷新」（及库节点「刷新」）→ 观察分类内容；再手动收起重展验证可恢复 | 【修复后期望】刷新后条目仍在或自动重载恢复；【当前实测】内容清空且计数归零，需手动收起重展（既有缺陷，见 F1-BUG-005） | 【留待 R 阶段回归】归属待协调者裁决 |

### 测试结果

**修复轮复验（2026-08-26，全新测试实例，独立重跑 commit 8b85cd49，不信前序数字）：**

| 套件 | 复验实测 | 与修复轮声称对比 |
|------|---------|----------------|
| `cargo test -p datazen --lib`（共享主检出 target） | **1138 passed / 0 failed / 2 ignored** | 一致 ✅ |
| `npx vitest run` | **240 文件 / 1963 用例全过**（ConnectionNavigatorTree.test.tsx 单独运行 64/64 全绿） | 一致 ✅ |
| `npx tsc --noEmit` | **0 错误**（exit 0） | 一致 ✅ |

覆盖率独立重测（全量套件 + `--coverage.include` 两文件过滤，v8 provider）：ConnectionNavigatorTree.tsx 行覆盖 **96.74%**、TableStructureEditor.tsx **97.75%**，与修复轮声称逐位一致。

回归面复查：✅ 无 `use_database` IPC 回潮 —— 生产前端零命中；宿主侧剩余命中仅为 `ensure_session_database` 内部对 driver trait 的调用、mock/测试录制器、以及明确不在 F1 范围的 sync/transfer/workflow 内层调用；e2e 仅注释提及。修复提交未触碰任何 codegen/untracked 文件及 `packages/`（driver-api 输入 schema 未动，无需 PROTOCOL_VERSION 变更）。

**结论：BUG-001~004 全部闭环，F1 判定通过关闭；新登记既有缺陷 F1-BUG-005 待协调者裁决归属。**

---

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

度量方式：全量 vitest 套件 + `--coverage.include` 过滤本次改动 TS 文件（v8 provider）。任务给定的"定向子集"命令（只跑 4 个测试路径）会显著低估（如 schemaStore 仅 46.05%），已按预案改用全量套件度量。**[修复轮更新]** 全量套件现为 240 文件 / 1963 用例；BUG-004 两文件以 `--coverage.include=ConnectionNavigatorTree.tsx --coverage.include=TableStructureEditor.tsx` 过滤全量度量（两文件不在项目 Option C include 名单，数字取自 v8 coverage-final.json）。

| 文件 | Statements | Branch | Funcs | **Lines** | ≥80% 行覆盖 |
|-----|-----------|--------|-------|-----------|------------|
| `src/lib/ensureNamespace.ts` | 95.07% (135/142) | 83.15% | 100% | **100%** (120/120) | ✅ |
| `src/stores/schemaStore.ts` | 87.86% (239/272) | 74.86% | 93.22% | **90.45%** (218/241) | ✅ |
| `src/windows/connection/ConnectionNavigatorTree.tsx` | 92.64% (932/1006) | 84.82% | 94.14% | **96.74%** (832/860) | ✅（修复轮：53.13% → 96.74%） |
| `src/windows/connection/TableStructureEditor.tsx` | 92.38% (194/210) | 80% | 97.83% | **97.75%** (174/178) | ✅（修复轮：37.64% → 97.75%） |

补充说明：
- 其余改动 TS 文件（QueryPanel / TableView / PanelContentRenderer / DocumentConnectionView / UnifiedSchemaTree / sqlFileExecution / commands/*）不在项目 Option C coverage include 配置范围内，v8 默认不采集，无法给出数字。
- Rust 无 llvm-cov 工具链，以单测清单 + 被测分支枚举佐证：`ensure_session_database` 四分支中"pin≠当前→切库并更新 session 记录""相同库/空白/None 零调用""explain 路径切库"有直接单测；**[修复轮更新]** stream 路径独立切库单测已补（`stream_pins_session_database_before_query_stream`），另有 driver_command / get_table_data / plan_table_structure_changes 三入口的 pin 生效与零调用单测（见上方「修复轮 Rust 单测补充」）。
- ~~判定：❌ 2/4 文件未达 ≥80%，登记 F1-BUG-004。~~ **[修复轮判定]** ✅ BUG-004 两文件行覆盖修复后分别为 96.74% / 97.75%（全量 1963 用例套件下以 `--coverage.include` 过滤度量，数字来自 v8 coverage-final.json 汇总）。**[复验判定]** ✅ 全新实例独立重跑同口径度量，两文件数字逐位复现（96.74% / 97.75%），BUG-004 关闭。

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
- **[复验结论]** 已修复：扩展用例真实有效（非空转断言），覆盖率数字独立重跑逐位一致。

**F1-BUG-005（中，既有行为非本轮引入）连接刷新后已展开对象分类内容丢失且不自动恢复**
- 重现（jsdom 探针实证，GUI 实证留待 E2E F1-E2E-011）：
  1. 单库树驱动（SQLite，`/data/app.db`）已连接；
  2. 展开对象分类（如 procedure）→ 条目渲染正常、计数 1；
  3. 右键连接行「刷新」（或库节点「刷新」）。
- 预期：条目保留或自动重载。实际（探针实测）：分类行仍呈展开态（chevron 向下、`expandedCats` 未清），但条目消失、计数归零；3s 观察窗内零次 `getDatabaseObjects` 重载成功落位——重载请求即使发出（菜单路径实测发出 1 次）也被随后的缓存清除覆盖。
- 根因定位（源码级）：`loadForConnection → loadTables` 递增 `schemaEpoch` 触发 `useExpandedDbCacheRefresh` 指纹 effect → `clearCaches` 清空该会话全部 `dbObjectsMap` 后仅按 `expandedDbs` 重载表缓存，不重载对象分类缓存；`refreshConnection` 末尾的 `reloadExpandedObjectCategories` 与该 effect 竞态失败。多库树表节点因走 `reloadDbTables` 不受影响（探针对照通过）。
- 非本轮引入的实证：同一探针在 4ba4831a（F1 编码前基线）、046acf7a（修复轮前）、8b85cd49（HEAD）三个时点症状逐项一致。
- 相关文件：`src/windows/connection/schema-tree/useExpandedDbCacheRefresh.ts`、`src/windows/connection/ConnectionNavigatorTree.tsx`（refreshConnection / reloadExpandedObjectCategories / clearCaches 回调）
- 处置建议：修复方向为 hook 清缓存后同步重载展开的对象分类（或将 clearCaches 收窄为仅表缓存、分类缓存单独失效+重载）；是否纳入 F1 或另立后续功能，由协调者裁决。

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

**设计决策（修复轮）**

1. **单一切库机制**：BUG-001~003 宿主侧统一复用 `ensure_session_database`（`commands/query.rs`），在 `execute_driver_command(_stream)`、`get_table_data`、`plan_table_structure_changes` 执行前前置调用；不引入第二套切库语义。pin 为 `None` / 空白 / 与 session 当前库相同 → 零次 `driver.use_database`，保持原有 not-connected 错误语义（不提前解析 session）。
2. **PROTOCOL_VERSION 兼容性评估**：`database` pin 只加在**宿主 IPC 信封**（`src-tauri/src/commands/driver_command.rs` 的 `ExecuteDriverCommandRequest` / `ExecuteDriverCommandStreamRequest` 及相关 Tauri 命令参数），**不改** driver-api Command 输入 schema（`packages/driver-api/src/command.rs` 的 query/query_stream 定义未动）。理由：库限定是宿主会话层关注点——切库由宿主持有的 `driver.use_database` + session `config.database` 承载，驱动对"当前活动库"无感知；若把字段下沉进每个 Command 的 input schema，会把宿主会话语义泄漏进驱动协议，并要求所有插件同步升 `PROTOCOL_VERSION`。故本次**无需 PROTOCOL_VERSION 变更、无需插件联动**；旧前端/插件请求不带该字段时行为完全不变（serde `#[serde(default)]` 反序列化为 `None`）。
3. **pin 的持久化语义**：pin 命中时先 `driver.use_database(&handle, db)` 再 `set_active_database` 更新 session 记录，因此后续不带 pin 的未限定命令（`commit_row_updates/deletes`、结构 DDL 执行等）自然落在目标库；入口只在打开面板 / 执行前 pin 一次即可。
4. **前端取值口径**：SQL 编辑器链路以 `schemaStore` 中该 panel 会话的 `currentDatabase` 为目标库（`panelTargetDatabase`）；TableView/tableDataStore 显式携带并在 per-connection 状态记忆 `activeDatabase`，保证翻页 / 过滤 / 重试等 store 驱动的刷新不漂移；TableStructureEditor 直接透传组件 `database` prop。

**修复轮 Rust 单测补充**
- `commands/driver_command.rs`：`execute_driver_command_pins_session_database_before_execution`（BUG-001 切库生效 + session 记录更新）、`execute_driver_command_skips_switch_when_pin_missing_or_same`（None/相同/空白零调用）、`stream_pins_session_database_before_query_stream`（**stream 路径独立切库单测**，BUG-004 附带项）、unbound `driverType` 请求显式忽略 database
- `commands/schema.rs`：`get_table_data_pins_session_to_target_database`（BUG-002 跨库取数）
- `commands/structure.rs`：`plan_switches_session_database_when_pinned_differs` / `plan_without_pin_keeps_active_database`（BUG-003）
- `testing/mock_driver.rs`：MockDriver 补 `plan_structure_changes` no-op 覆写 + `use_database_calls()` 录制供断言

**验证三件套结果**
- 编码轮：`cargo test -p datazen --lib` **1132 passed / 0 failed / 2 ignored**；`npx vitest run` 240 文件 / 1882 用例全过；`npx tsc --noEmit` 0 错误
- **修复轮（本提交前复跑）**：`cargo test -p datazen --lib` **1138 passed / 0 failed / 2 ignored**（净增 6 个 F1 宿主单测）；`npx vitest run` **240 文件 / 1963 用例全过**（含 ConnectionNavigatorTree.test.tsx 修复轮扩至 64 用例全绿）；`npx tsc --noEmit` **0 错误**

**遗留注意**
1. 非 query 族命令（`get_table_data` / `commit_row_updates` / structure DDL / 通用 `execute_driver_command` 等）尚无 `database` 参数：原先依赖「先切库再操作」的入口（TableView 打开非活动库的表、建表/结构编辑等）在 F1 后不再预切 session，需等后续功能给这些命令补显式 `database` 参数；过渡期跨库操作由任一带 `database` 的 query/stream/explain 惰性触发切库。
   **[修复轮更新]** `execute_driver_command(_stream)` / `get_table_data` / `plan_table_structure_changes` 已补可选 `database` 参数（统一走 `ensure_session_database`）。因该机制会持久切换 session 活动库，`commit_row_updates/deletes`、结构 DDL 执行等后续未限定命令自然落在目标库，无需各自加参；仍不带 pin 的命令（如 `get_table_schema`）语义为"作用于 session 当前活动库"，由入口先 pin 保证正确性。
2. `src-tauri/capabilities/default.json` 为 codegen：本 worktree 曾被新版 resolve-drivers 写入 `redis:default`，导致裸 `cargo test -p datazen --lib`（default features=[]，redis 插件未编译）构建失败；现已对齐主检出权限集（无 redis）。若后续以 `--drivers=all` 等选型重新生成，需用带 feature 的构建验证。
3. 截图脚本 `zz-screenshots.ts` 的「钉住 demo 库」改为 `get_tables({database})` 探活；会话级钉住语义现完全依赖查询显式携带 `database`。

---

## F2 ADB 迁移 SQLite 驱动

### 范围
- **sqlite 驱动 crate**（`packages/drivers/sqlite/src/adb.rs` 新增）：三个 DriverCommandDefinition（`adb_list_packages` / `adb_list_databases` / `adb_pull_database`，均 `requiresConnection = false`、`hide_from_workflow`）+ `execute_command` 分派（无连接会话，忽略 handle）；解析/校验纯函数与单测自 Host `commands/adb.rs` 迁入；原 webdriver-gated 直连路径变体不迁移
- **driver-api**：`DriverCommandMetadata` 新增可选 `save_dialog: Option<DriverSaveDialogSpec>`（字段 `fileNameField` / `dataBase64Field` / `filterName` / `extensions` / `resultPathField`）。serde default + `skip_serializing_if`，向后兼容，不 bump PROTOCOL_VERSION
- **Host**：删除 `src-tauri/src/commands/adb.rs` 及 lib.rs 四条注册、`mod.rs` 导出；`execute_driver_command` IPC 增加 `AppHandle` 参数，新增通用薄壳 `finish_save_dialog`；内部复用路径 `execute_driver_command_impl` 不携带 AppHandle
- **前端**：`src/commands/adb.ts` 改走 `driverCommands.execute({ driverType: "sqlite", … })`，导出函数名与 TS 类型保持不变（`FileConnectionFields.tsx` 零改动）；`types/index.ts` 补 `DriverSaveDialogSpec`
- **守护测试改写**：`pathIpcWiring.test.ts` 与 `e2e/specs/path-ipc-hardening.ts` PIH-006 改断言新形态（execute_driver_command 路径 + Host 无 adb 注册残留）

### Dialog 方案选择（决策记录）
选「命令元数据声明需要保存对话框」机制，叠加「命令返回建议文件名 + 字节数据流」形态：
1. 宿主对 `requiresConnection = false` 命令的执行路径（`resolve_command_driver` unbound 分支）原本没有 AppHandle/dialog 回调，驱动层无法弹原生框——排除了"驱动直接弹框"
2. UI 需要拿回 savedPath 回填连接表单（原 `form.setDatabase(saved)` 语义）；现有通用 `save_base64_with_dialog` 只返回 bool 且扩展白名单不含 db/sqlite/sqlite3，无法等价替换，故不复用该 IPC
3. 形态：驱动命令返回 `{ fileName, dataBase64 }`（内存占用与原实现一致——原实现同样是先整读字节再弹框）；宿主 `finish_save_dialog` 弹原生保存框 → 按声明扩展名校验 → 落盘 → 结果替换为 `{ savedPath: string | null }`，取消返回 null（与原 `*_with_dialog` 语义一致）
4. 通用性红线满足：流程完全由命令元数据驱动，任何驱动/插件声明 spec 即可复用同一宿主薄壳，Host 无任何 `pluginId === 'sqlite'` 分支；非交互调用面（MCP / workflow / 内部复用）在执行前即拒绝（workflow 侧另被 `hide_from_workflow` 双重挡住），不会出现无头阻塞弹框
5. E2E 说明：真实拉取需 Android 设备，本就无法自动化；PIH-006 改为源码断言 + ADB 面板 UI 探活。若后续决策 3 的 override_path 模式推广到驱动命令，可再补测试注入通道

### 行为等价性说明
- 列表命令输出 JSON 键保持原样（`package_name`、`path`/`name`），前端类型零改动
- adb 二进制缺失：spawn NotFound 映射为 `DriverError::InvalidConfig`，文案与原 `CommandError::NotConfigured` 一致（"adb command not found. Please install Android SDK Platform Tools…"）；经 `From<DriverError>` 展示时多出 "Invalid configuration: " 前缀（轻微展示差异，语义与安装指引一致）
- 输入校验（包名字符白名单、dbPath 穿越/空字节拒绝）在 spawn 之前执行，与原实现同序

### 单测清单（编码轮）
- driver-api：`save_dialog_spec_round_trips_and_defaults_to_none`（serde 往返 + 缺省省略）
- datazen-driver-sqlite `adb::tests`（13 个）：解析×2、包名合法/非法×2、dbPath 穿越、default_pull_file_name、定义完备性（ids 精确匹配）、unbound/hide-from-workflow/save-dialog 元数据断言、pull input schema required、unknown command → Unsupported、缺参 → InvalidConfig、pull 先校验后 spawn、缺失二进制 → 安装指引文案
- Host `driver_command::tests`：`save_dialog_commands_rejected_without_interactive_handle`

### 测试结果（编码轮）
| 套件 | 结果 |
|------|------|
| `cargo test -p datazen-driver-sqlite` | lib 29 passed（含 adb 13）/ tests 2+3 passed，全绿 |
| `cargo test -p datazen-driver-api` | 83 passed，全绿 |
| `cargo test -p datazen --lib` | 1130 passed / 0 failed / 2 ignored（较重构前 -9 = 删除的 Host adb 测试，+1 = 新增守卫测试） |
| `npx vitest run` | 240 files / 1963 passed，全绿（含改写后的 pathIpcWiring） |
| `npx tsc --noEmit` | 0 错误 |

### 遗留注意
1. `execute_driver_command_stream` 仅支持 query_stream，天然不受 save_dialog 影响；未来若开放更多流式命令需同步考虑对话框语义
2. 第三方插件驱动若声明 save_dialog，宿主当前不做额外权限校验（信任插件声明的过滤器/扩展名，路径仍由用户在 OS 对话框中选定）；插件权限模型完善时可收口

## F3 backup/restore 合并 + restore_sql_file
（占位）

## F4 connections/app-data 导入导出合并
（占位）

## F5 删除纯文件读写 IPC

### 范围
- **Host**：删除 `src-tauri/src/commands/file.rs` 的 `write_file` / `write_file_base64` / `read_file` 三个 `#[tauri::command]` 及内部实现 `write_file_impl` / `read_file_impl`、webdriver 门控辅助 `deny_path_ipc`、仅为其服务的 `validate_file_path`（含专属单测 ×4）；删除门控单测 ×3（`path_ipc_{write_file,read_file,write_file_base64}_gated_without_webdriver`）；`lib.rs` 删除三条注册。dialog 系列（save/open/begin/append/finish/abort/export_tables_stream）全部保留，`ALLOWED_EXTENSIONS` / `validate_extension` 仍被对话框过滤器使用故保留
- **前端**：删除 `src/commands/file.ts` 的 `writeFile` / `writeFileBase64` / `readFile` 三包装；新增 `src/commands/__tests__/file.test.ts`（8 用例）覆盖全部现存封装的参数透传与 `onExportProgress` 订阅
- **E2E**：`e2e/specs/ai-context.ts`（2 处）与 `e2e/specs/ai-context-tables.ts`（1 处）fixture 准备由 `invokeBackend('write_file')` 改为 Node.js `fs.writeFileSync()`；目标路径逻辑不变（仍写入 `context_get_dir` 返回的应用上下文目录），仅换写入手段（E2E 进程即 Node）
- **文档**：`docs/architecture/backend/commands.md`「文件」行更新为对话框系列清单并注明纯路径读写 IPC 已删

### themePackApply 甄别结论
`src/lib/themePackApply.ts` L164 的 `readFile(relPath)` 是本地函数参数（`rewriteCssUrls(css, readFile: PackFileReader)` 的回调形参），非 `commands/file.ts` 的 IPC 封装，与本任务无关，未改动。全仓 Grep 确认三包装在 src/ 内无其他业务调用方。

### 守护测试说明
`src/commands/__tests__/pathIpcWiring.test.ts` 以当前分支内容核实：不含三包装相关断言（F2 改写后仅覆盖 ADB/dialog/open_* 路径），无需更新。另以全仓 grep 验证 `'write_file'` / `'write_file_base64'` / `'read_file'` / `writeFileBase64` / `fileCommands.writeFile` / `fileCommands.readFile` 在 src、src-tauri/src、e2e、packages/extensions 清零。

### 测试结果（编码轮）

| 套件 | 结果 |
|------|------|
| `cargo test -p datazen --lib`（共享主检出 target） | 1123 passed / 0 failed / 2 ignored（较 F2 后 1130 −7 = 删除的 validate_file_path×4 + 门控×3 专属单测） |
| `npx vitest run` | 241 files / 1971 passed，全绿（+1 file/+8 tests = 新增 file.test.ts） |
| `npx tsc --noEmit` | 0 错误 |
| 覆盖率（改动 TS：`src/commands/file.ts`，vitest --coverage.include 过滤实测） | 行/语句/分支/函数 **100%**（≥80% 达标；e2e 两 spec 不在 vitest 覆盖域） |

### 环境注意
本 worktree 的 codegen `src-tauri/capabilities/default.json` 曾含 `redis:default`（redis 插件未编译时 tauri-build 权限校验失败，同 F1 遗留注意 2 的坑）；已对齐主检出权限集（28 权限、无 redis）。该文件为 gitignore codegen，不入库。

### 复验（2026-08-26 全新测试代理实例，commit 8f9e4a9c）→ 通过

#### 三项审查
1. **删除完整性 ✅**：三 IPC 在 src / src-tauri/src / e2e / packages/extensions 全仓清零（残留 `write_file` 命中均为各模块 `#[cfg(test)]` 内同名本地测试辅助与 `store::write_file_atomic`、`read_file` 命中均为 `context_read_files`，非 IPC 残留）；lib.rs 三条注册已删；前端无 camelCase 包装、无解构导入、无字符串 invoke 残留（themePackApply.ts 的 `readFile` 确为本地 `PackFileReader` 形参，编码轮甄别属实）。**甄别复核**：`ALLOWED_EXTENSIONS` 由 `ext_refs()`（file.rs L53）消费且 `ext_refs` 被全部 5 个 dialog 命令调用；`validate_extension` 被 5 个 dialog 命令 + `driver_command.rs` L617 调用——「仍被使用故保留」属实，**非死代码**
2. **迁移等价性 ✅**：ai-context.ts ×2 / ai-context-tables.ts ×1 目标目录来源（均 `context_get_dir`）、文件名（`${contextDir}/schema.sql`、`${contextDir}/relations.md`）、内容字符串逐字一致；旧 write_file 为 Rust `String::as_bytes()`（UTF-8）、新 Node `fs.writeFileSync` 默认 utf8 且内容纯 ASCII → 字节级等价；`context_get_dir` 返回宿主视角绝对路径（`dir.display()`，store.data_dir()/contexts 或用户自定义 context_dir），E2E Node 进程与本机应用同机同用户，直写后端可读成立——且 CTX-002/003/004 经 `context_list_files`/`context_read_files` 读回断言闭环验证该前提。旧 `validate_file_path` 白名单是针对 webview 输入的防线，测试 fixture 直写不受其删除影响，无安全回归
3. **安全面 ✅**：`write_file_impl` / `read_file_impl` / `deny_path_ipc` 全仓零引用残留；共享门控 `require_webdriver_path_ipc`（error.rs）未被波及，config.rs ×4（连接导出/导入/app-data 导出/导入）与 backup.rs ×3（backup/restore/restore_sql_file）继续使用；webdriver 构建下无其他合法路径依赖被删三命令

#### 独立重跑结果

| 套件 | 结果 |
|------|------|
| `cargo test -p datazen --lib`（CARGO_TARGET_DIR=主检出 target） | **1123 passed / 0 failed / 2 ignored**（=1130−7，与声称一致） |
| `npx vitest run` | **241 files / 1971 passed**，全绿 |
| `npx tsc --noEmit` | **0 错误**（exit 0） |
| 覆盖率 `src/commands/file.ts`（vitest --coverage.include 过滤实测） | 行/语句/分支/函数 **100%/100%/100%/100%**；file.test.ts 单独运行 8/8 通过 |

#### E2E 用例登记（执行归属：R 阶段，需 `pnpm tauri build --debug --features webdriver` + wdio；本测试轮未执行）
- **F5-E2E-001** `e2e/specs/ai-context.ts`（CTX-001~006）：Node fs 种子写入 → 后端 `context_list_files`/`context_read_files` 读回断言（迁移核心闭环）
- **F5-E2E-002** `e2e/specs/ai-context-tables.ts`（CTX-T01~T06）：`fs.writeFileSync` 种子 schema.sql 后 AI @ 引用面板 Files 类目全链路
- **F5-E2E-003** 负向：生产/无头构建下 `invoke('write_file'|'read_file'|'write_file_base64')` 应报命令不存在（静态已证 lib.rs 注册删除；运行时断言随 R 阶段 E2E 执行）

#### 复验发现
- F5-BUG-001【低·文档漂移】登记 Bug 台账：e2e-testing.md L154 / e2e-coverage.md L106 仍称 webdriver 构建保留 write_file 路径 API；归属 R 阶段文档收口，不阻塞判定

## F6 删除冗余命令

> 状态：**已完成**（2026-08-26，轨道 B 第二棒编码代理；同日全新测试代理复验通过，见下方复验小节）

### 范围

- **Host**：删除 4 条 `#[tauri::command]` 及注册——`commands/dashboard.rs` 的 `get_monitor_paused` / `set_monitor_paused`（lib.rs 注册 ×2）、`commands/schema_diff.rs` 的 `compare_table_data` IPC 包装、`commands/sync/mod.rs` 的 `classify_sync_pair`
- **Host 连带清理**（孤儿代码，F5 先例）：`commands/sync/compare.rs` 删除仅被 `compare_table_data_impl` 消费的 8 个助手（`resolve_pk_columns` / `fetch_sample_rows` / `row_key` / `value_key_part` / `rows_to_key_map` / `row_to_json_map` / `values_equal` / `rows_equal`）及随之失活的 `use`（`TableSchema, Value`、`Hash, Hasher`、`DATA_COMPARE_SAMPLE_LIMIT`）；`commands/sync/types.rs` 删除双常量 `DATA_COMPARE_SAMPLE_LIMIT` / `DATA_COMPARE_MISMATCH_LIMIT`。`count_rows`（tasks.rs/inspect.rs 仍用）、`fetch_full_column_types` / `diff_table_schemas_ir` / `format_ir_type`（schema_diff 与既有测试仍用）、`value_as_u64` / `maybe_use_database` 均保留
- **前端**：`src/commands/dashboard.ts` 删除 `getMonitorPaused` / `setMonitorPaused` 包装；`src/commands/schemaDiff.ts` 删除 `compareTableData` 包装及 `TableDataCompare` import；`src/types/index.ts` 删除孤儿类型 `TableDataCompare` / `RowMismatch` / `RowMismatchKind`。`DashboardPanel.tsx` 的同名 `monitorPaused` 为本地 useState（UI 暂停态），与 IPC 无关，未改动
- **E2E**：`e2e/specs/data-sync-real.ts` SYNC-REAL-020 / SYNC-BATCH-002 由「调用 classify_sync_pair 断言 ir 拒绝」改为 `expectCommandNotFound` 负断言（命令已不存在），与本 spec 既有 SYNC-REAL-021 / SYNC-BATCH-001/003 的「legacy IPC 已移除」守护模式一致
- **文档**：`docs/architecture/backend/commands.md` 同步行更新（同步行去 classify_sync_pair、Schema Diff 行补 compare_table_schemas 并注明 compare_table_data 移除）；`docs/architecture/backend/data-sync.md` IPC 表删 classify_sync_pair 行并归入 Legacy 说明

### compare_table_data_impl 去留裁决

**裁决：impl 一并删除（连同 sync/tests.rs 中其专属消费）。** 判断依据：全仓 grep 证实 `compare_table_data_impl` 在生产代码中仅有两个引用点——schema_diff.rs 的 IPC 包装（本轮删除对象）自身，以及 `sync/tests.rs::compare_table_schemas_and_data_impl` 的测试调用；`prepare_schema_diff_plan` / `compare_table_schemas_impl` 等 schema diff 功能域现存代码均不经过它（行级比对引擎走 data_sync 的 compare_data_sync 链路，非此函数）。即删除包装后 impl 仅剩测试使用，符合任务书「连 impl+测试一并删」分支。处置：impl 整体删除；原测试拆留 schema 半场改名为 `compare_table_schemas_impl_returns_diff_for_table`（数据半场随 impl 消失）；由此失活的 8 助手+2 常量连带删除（见范围），其 11 个专属单测同轮删除——`cargo test` 净减 12（11 助手单测 + classify_sync_pair 单测）与逐项清单精确吻合。

### 守护测试说明

- Host：`sync/tests.rs::legacy_transfer_ir_compare_ipc_removed` 追加断言 `!mod.rs.contains("classify_sync_pair")`（源码级防回潮，沿用该测试既有的 include_str! 模式）
- E2E：SYNC-REAL-020 / SYNC-BATCH-002 改为运行时负断言（比源码级更强：验证构建产物中命令确已注销）；classify 的业务语义（PG→MySQL 应判 ir/不支持）由前端 `src/lib/syncPairing.ts` 镜像逻辑承载，后端真源测试保留在 `data_sync/pairing.rs` 单测（mysql/mariadb 直通、PG→MySQL ir、sqlite、redis 四分支未动）
- 前端：新增 `src/commands/__tests__/dashboard.test.ts`（14 用例）与 `src/commands/__tests__/schemaDiff.test.ts`（7 用例）覆盖两文件全部现存封装的参数透传 + schemaDiff 纯函数（F5 file.test.ts 同模式），同时把两个改动文件的覆盖率从 6.66%/20% 提至 100%

### 测试结果（编码轮）

| 套件 | 结果 |
|------|------|
| `cargo test -p datazen --lib`（CARGO_TARGET_DIR=主检出 target） | **1111 passed / 0 failed / 2 ignored**（较 F5 后 1123 净减 12，逐项见裁决小节） |
| `npx vitest run` | **243 files / 1992 passed** 全绿（+2 files/+21 tests = 新增 dashboard.test.ts + schemaDiff.test.ts） |
| `npx tsc --noEmit` | **0 错误** |
| 警告面 | cargo check 警告与基线（HEAD=31 条）逐条一致，零新增（孤儿助手若漏删会以 dead_code 警告暴露，已全部清理） |

### 覆盖率（改动 TS 文件）

全量 vitest 套件 + `--coverage.include` 过滤（v8 provider）：

| 文件 | Stmts | Branch | Funcs | Lines | ≥80% |
|-----|-------|--------|-------|-------|------|
| `src/commands/dashboard.ts` | 100% | 100% | 100% | **100%** | ✅ |
| `src/commands/schemaDiff.ts` | 100% | 100% | 100% | **100%** | ✅ |
| `src/types/index.ts` | — | — | — | N/A | ✅（纯类型导出，本轮仅删类型声明，v8 无可执行行可采） |

`e2e/specs/data-sync-real.ts` 不在 vitest 覆盖域（webdriver E2E，归属 R 阶段回归）。

### 编码说明（F6）

**四命令删除路径**：
1. `get_monitor_paused` / `set_monitor_paused`：dashboard.rs 两 `#[tauri::command]` + lib.rs 注册 ×2 + dashboard.ts 两包装。前者本为读第一个 dashboard 的 refresh_paused 的 legacy 兼容读口，后者纯 warn no-op；替代命令 `set_dashboard_refresh_paused`（含 id 维度）及其前端包装 `setDashboardRefreshPaused` 原样保留。
2. `compare_table_data`：schema_diff.rs IPC 包装 + 无主 impl（裁决见上）+ 孤儿助手/常量 + schemaDiff.ts 包装 + index.ts 类型三件套。
3. `classify_sync_pair`：sync/mod.rs 命令（serde_json 薄包装，真源 `data_sync::classify_data_sync_pair` 未动）+ lib.rs 注册 + tests.rs 单测（其断言的 JSON 形状属 IPC 信封而非分类逻辑，分类逻辑已有 pairing.rs 直测覆盖，故删而不迁）+ e2e 两处负断言化。

**设计决策**：
1. E2E 处置选「expectCommandNotFound 负断言」而非纯删除或源码级断言：任务书给出两选项，但本 spec 已存在更强的第三模式（对已移除 legacy IPC 的运行时注销验证），保持文件内一致性且守护力最强；两条用例编号保留不删。
2. `compare_table_schemas_and_data_impl` 改名 `compare_table_schemas_impl_returns_diff_for_table` 并顺带去掉仅为数据采样服务的 `MockDriverOptions` 定制（query_rows/count_total），改 `TestAppState::new()`。
3. 文档同步仅触及 tracked 架构文档两处表格；`ipc-refactor-plan.md`（untracked 决策文档）与历史报告 `test-reports/W2-test-report.md` 按纪律不改。

### E2E 用例登记（执行归属：R 阶段，需 webdriver 构建 + PG/MySQL 实例；本轮未执行）

- **F6-E2E-001** `data-sync-real.ts` SYNC-REAL-020：`invokeBackend('classify_sync_pair')` 应报 command not found（改造后负断言）
- **F6-E2E-002** `data-sync-real.ts` SYNC-BATCH-002：同上（批量语境重复守护）

### 复验（2026-08-26 全新测试代理实例，commit d4e33801）→ 通过

#### 三项审查

1. **删除完整性 ✅**：四命令在 src / src-tauri/src / e2e / packages 全仓清零（残留命中逐一甄别：`classify_sync_pair` 仅存 docs 历史记录（ipc-refactor-plan / commands.md / data-sync.md Legacy 行 / W2 报告）、进度文件自身、`sync/tests.rs::legacy_transfer_ir_compare_ipc_removed` 防回潮负断言字符串、e2e 两处 `expectCommandNotFound` 负断言参数；`compare_table_data` 仅存 commands.md 移除注记与计划文档；monitor_paused 仅存计划/进度文档）；lib.rs 四条注册删除经 diff 核实（L882 区 classify_sync_pair / compare_table_data + L963 区 get/set_monitor_paused）；前端 camelCase 包装零残留
2. **孤儿清理正确性 ✅**：8 助手 + 2 常量删除后全仓零消费者——现存同名 `values_equal` / `rows_equal` 属 `data_sync/model.rs` 既有函数（签名 `(&Value, &Value)`，被 data_sync 内部消费并经 mod.rs 再导出），与所删 `commands/sync/compare.rs` 版本（`Option<&Option<Value>>` 参数）不同源；tests.rs / sql.rs 的 `row_key:` 为结构体字段非函数引用。本轮 commit **零新增** `#[allow(dead_code)]`（types.rs:104 既有注解掩盖的是 `TableMappingInput` 结构体，与本次清理无关）；保留函数消费链逐一核实：`count_rows` → tasks.rs + data_transfer/inspect.rs、`fetch_full_column_types` / `diff_table_schemas_ir` → schema_diff.rs + data_transfer、`format_ir_type` → compare.rs 自用 + tests、`value_as_u64` → compare.rs 自用 + tests。`DashboardPanel.tsx` L89/174/345 本地 `monitorPaused` useState 完好未动
3. **impl 裁决 ✅**：d4e33801~1 全仓 grep 证实 `compare_table_data_impl` 生产引用仅定义自身（schema_diff.rs:305）与 IPC 包装调用（:448），另有 tests.rs 导入 + 调用一处；`prepare_schema_diff_plan` / `compare_table_schemas_impl` 从未引用它；现存行级比对真源为 `compare_data_sync` → `compare_data_sync_impl`（sync/mod.rs:100，lib.rs:890 注册在案）——「删包装后 impl 仅剩测试使用」裁决成立

#### E2E 负断言审查 ✅

`expectCommandNotFound`（dda1dfb6 引入，先于 F6 的成熟模式）直连 `__TAURI_INTERNALS__.invoke` 原生通道、绕过任何前端包装：命令若回潮注册则 invoke 成功 → message 为空串 → `toMatch(/command .+ not found|unknown command/i)` 必失败；命令存在但报其他错同样 fail-loud。唯一通过路径即「命令确不存在且 Tauri 报 not found」，防回潮成立；SYNC-REAL-020 / SYNC-BATCH-002 与同 spec SYNC-REAL-021/022、SYNC-BATCH-001/003 模式一致。

#### 独立重跑结果

| 套件 | 结果 |
|------|------|
| `cargo test -p datazen --lib`（CARGO_TARGET_DIR=主检出 target） | **1111 passed / 0 failed / 2 ignored**（与声称一致） |
| `npx vitest run` | **243 files / 1992 passed**，全绿（较 F5 后 241/1971 = +2 files/+21 tests，与新增两套件 14+7 吻合） |
| `npx tsc --noEmit` | **0 错误**（exit 0） |
| 覆盖率 dashboard.ts / schemaDiff.ts（--coverage.include 过滤实测） | 两文件均行/语句/分支/函数 **100%/100%/100%/100%** |

#### 复验发现

无 bug。cargo 净减 12 与逐项清单吻合（tests.rs 删 13 个测试 fn：11 助手单测 + classify_sync_pair 单测 + 被改名瘦身的 `compare_table_schemas_and_data_impl`，新增改名版 `compare_table_schemas_impl_returns_diff_for_table`）。E2E 归属不变：F6-E2E-001/002 执行归 R 阶段（需 webdriver 构建 + PG/MySQL 实例），本测试轮未执行。

## F7 驱动级 SQL 定位重写

### 需求（用户新指令 2026-08-26，两轮精化）
每条 SQL 命令携带定位信息直达驱动层，**驱动按方言把未限定表引用重写为限定名**（如 `select * from users` → `select * from \`mydb\`.users`）；不用 `USE`、不切会话，纯无状态。

### 设计基线
- **定位信息**：MySQL 系 = database；PG 系 = database + schema。信封在修复轮已有可选 `database`，本功能补 `schema`
- **方言形态**：mysql/mariadb `` `db`.`t` ``（真跨库内联）；postgres `"schema"."t"`（PG 引擎限制跨库不可内联 → database 维度沿用连接池切换=现有机制，schema 维度内联重写）；sqlite `alias.t`（仅 ATTACH 别名场景，通常 no-op）；sqlserver `db.schema.t`；clickhouse `db.t`；duckdb 同 PG 形态
- **覆盖面**：全部 SQL 型驱动（含各 path 驱动）；redis / mongodb 非 SQL 执行模型，N/A
- **解析与改写**：各驱动 crate 内用 `sqlparser` crate 按方言 AST 改写；仅定位语境（FROM/JOIN/INSERT INTO/UPDATE/DELETE FROM/TRUNCATE/CREATE|DROP|ALTER TABLE/CREATE INDEX ON）；跳过 CTE 名、子查询别名、字符串字面量、已限定引用；幂等
- **兜底**：解析失败 → 原样放行 + 日志 + 现有宿主 `ensure_session_database` 兜底（git 旧 pin 驱动无重写能力时的安全网）
- **协议**：driver-api 输入 schema 加可选字段，向后兼容不强制 bump PROTOCOL_VERSION；git 驱动更新 pin 后才获得重写能力（顺序依赖登记为风险）
- **前端**：database 链路已穿透（BUG-001 修复成果）；需补 PG currentSchema 传递（先侦察 schemaStore 是否已有该状态）
- **测试落点**：重写单测在各驱动 crate（简单 SELECT/JOIN/CTE/子查询/已限定/引号标识符/INSERT|UPDATE|DELETE/DDL 各方言矩阵）；Host 只测信封透传 + 兜底路径

## R 回归与收尾
（占位）
