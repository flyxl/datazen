# DataZen W2 独立测试报告（九域 IPC 线上契约切换）

- **被测提交**：`b962b4cc`（`refactor(ids)!: W2 switch IPC contracts to connectionId/dbSessionId`，2026-08-25 08:39 +0800）
- **分支 / 工作树**：`feature/db-session-id-rename` @ `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/db-session-id-rename`
- **基线**：`main`。注意：分支自 `d4f1aa4f` 分叉后，main 已前移 2 个提交（`4cb8ca99`、`e45aae4b`），本报告区分「vs 分叉基点（merge-base）」与「vs main tip」两种口径
- **测试执行人**：独立测试 agent（全新会话，只测试、不修复；除本报告外未改动任何文件）
- **日期**：2026-08-25
- **环境限制**：① lib 测试中 2 个 `resolve_log_settings_*` 用例因 sandbox 拒绝写用户目录失败（既有）；② `pnpm run build` 包装器触发 install 被 EPERM 拒绝，改跑实质步骤 `npx tsc --noEmit -p tsconfig.json` + `npx vite build`；③ e2e tsconfig 71 处既有类型噪音未复核（基线一致口径）
- **工作树既有状态说明**：测试开始前工作树即存在一处未提交修改（`ID_RENAME_PROGRESS.md` 进度表格行，1 行），非本测试所为、本测试未触碰

---

## 执行摘要

| 项 | 结果 |
|---|---|
| T1 独立全量执行 | ✅ 通过（Rust lib 1115/1118 可通过，3 失败均归为非 W2 缺陷，见 T1/T5；前端 1886 全绿；drivers 84 全绿；tsc/vite 零错） |
| T2 合规审计 | ⚠️ 规模/禁区/MCP/history_db 迁移链全部合规；但 **13 个命令参数语义装反**（ai 域 7 + backup 域 6），违反本次审计硬性规则 |
| T3 覆盖率 | ⚠️ 核心模块 4/5 ≥80%；**commands/connection.rs 行覆盖 65.37%（区域 68.59%）不达标** |
| T4 E2E 视角用例 | 12 条全给出状态；可在 Rust 层执行的均已实际执行（29 条定向单测 + 三起点迁移 SQL 等价验证）；GUI/E2E 级标注未执行并建议收尾回归 |
| T5 缺陷 | 缺陷 1 项（D1，契约一致性/中）；观察项 5 项；覆盖缺口 2 项 |

## 结论：**不通过**

无任何功能回归证据（四套件在排除既有环境因素后全绿），但两项验收硬指标未达成：

1. **T2 语义方向规则被违反 ×13**：`commands/ai.rs` 7 个命令与 `commands/backup.rs` 6 个命令的参数名仍为 `connection_id`（配置语义命名），实现却按运行时会话语义使用（直调 `get_session` / `get_session_config`，无双模兜底）——正是本次审计重点防范的"换名但语义装反"。当前端到端因前端把 dbSessionId 值装在 `connectionId` 键里传递而恰好可用，属"带病一致"，随 W3 改造极易断裂。
2. **T3 覆盖率门槛部分未达**：`commands/connection.rs`（connect/disconnect 所在地，W2 改动核心）65.37% < 80%。

复测条件：① D1 修复（13 个命令改名 `db_session_id` 或统一走 `resolve_session` 双模 + 守护测试）；② connection.rs 补测至 ≥80%；③ 收尾回归 `pnpm e2e:minimal` 通过。

---

## T1 独立全量执行（不采信开发方数字）

| # | 命令 | 结果 | 判定 |
|---|---|---|---|
| 1 | `cargo test -p datazen --lib` | **1114 passed / 3 failed / 2 ignored** | 见下方失败归因 |
| 1b | 同上（llvm-cov 插桩运行，`--skip` 两既有环境失败后） | **1115 passed / 0 failed / 2 ignored** | 与开发方宣称的 1115 一致 ✓ |
| 2 | `npx vitest run` | **239 文件 / 1886 用例全绿**（43.57s） | ✅ 与开发方数字一致 |
| 3 | `npx vitest run --config vitest.drivers.config.ts` | **14 文件 / 84 用例全绿**（含 D3 触碰的 redisInvoke/redisWorkbench 等） | ✅ |
| 4a | `npx tsc --noEmit -p tsconfig.json` | exit 0，零错误 | ✅ |
| 4b | `npx vite build` | 成功（4.70s，仅 chunk 体积警告） | ✅ |

**基线对照（main @ e45aae4b，主仓根目录、独立 target 目录实测）**：1112 passed / 2 failed——仅 `tests::resolve_log_settings_defaults_without_settings_file` 与 `tests::resolve_log_settings_reads_custom_level_and_path`（sandbox 拒写用户目录，已知既有）。

**分支上的第 3 个失败归因**（不在已知清单，独立查证）：
- `store::tests::concurrent_save_connection_all_succeed`（断言 15 ≠ 16）
- 隔离重跑 **5/5 通过**；llvm-cov 全量插桩运行亦通过；
- 该用例本身及 `save_connection` 实现路径相对 merge-base **零改动**（store 域仅 history/models/tests 三个文件的样例数据改名）；
- 失败仅出现在「分支全量套件 + vitest 并行抢 CPU」的高负载场景一次。
- 归因：负载型偶发疑似竞态，非 W2 引入 → 记观察项 O1，不计缺陷。

## T2 合规审计（对照 main）

### 2.1 规模核对 ✅

| 口径 | 数字 |
|---|---|
| **W2 提交本体**（`git show b962b4cc --stat`） | **109 files changed, +1162/−1066 —— 与改动摘要完全一致** ✅ |
| 分支累计 vs 分叉基点（W1+W2，`git diff d4f1aa4f...HEAD --stat`） | 119 files, +1801/−1283 |
| 两点 diff vs main tip（`git diff main --stat`，main 已前移 2 提交） | 147 files, +3442/−1592（含上游 2 提交的逆向差，非本工作产物） |

W2 提交分布：src-tauri/src 38、src/windows 27、packages 16、src/commands 9、e2e 9、src/lib 6、types/test/stores 各 1、进度文件 1。

### 2.2 后端 `#[tauri::command]` 参数语义方向审计（审计重点）

程序化提取全部命令签名（`commands/` 目录 **184 条** + `lib.rs::rebuild_menu` 无连接类参数）。结果：**`config_id` 参数 0 条** ✅；会话语义（db_session_id）37 条 ✅；配置语义（connection_id）7 条 ✅；**语义装反 13 条 ❌**。

判定方法：逐命令核对其内部解析路径（`get_session`/`get_session_config`=严格会话键；`resolve_session`/`resolve_connection`=双模；`store.get_connection`=持久化配置）。完整清单如下（连接/会话类参数；workflow_id/task_id 等非连接标识不计；`delete_connection(id)` 的裸 `id` 参数经核实亦为配置域删除键，✓）：

| 文件 | 命令 | 参数 | 语义判定 |
|---|---|---|---|
| adb.rs | adb_list_packages | () | — |
| adb.rs | adb_list_databases | package | — |
| adb.rs | adb_pull_database_with_dialog | app, package, db_path | — |
| adb.rs | adb_pull_database | package, db_path, local_path | — |
| ai.rs | ai_get_providers | state | — |
| ai.rs | ai_fetch_remote_models | protocol, endpoint, api_key | — |
| ai.rs | ai_validate_config | state, config | — |
| ai.rs | ai_save_config | handle, state, config | — |
| ai.rs | ai_get_config | state | — |
| ai.rs | ai_delete_config | handle, state | — |
| ai.rs | ai_generate_sql | state, window, connection_id, database, natural_language, request_id, current_table, recent_queries, context_files, context_tables | **装反(运行时)✗** |
| ai.rs | ai_diagnose_error | state, connection_id, database, sql, error_message | **装反(运行时)✗** |
| ai.rs | ai_analyze_explain | state, connection_id, explain_output, original_sql | **装反(运行时)✗** |
| ai.rs | ai_parse_filter | state, connection_id, database, table, natural_language | **装反(运行时)✗** |
| ai.rs | ai_chat | state, window, connection_id, database, messages, request_id, include_schema, scenario, context_files, context_tables | **装反(运行时)✗** |
| ai.rs | workflow_list | state | — |
| ai.rs | workflow_execute | state, workflow_id, variables, connection_id | 配置 ✓（executor 内 resolve_session 双模，工作流默认连接本就是持久化 id） |
| ai.rs | workflow_save | state, workflow | — |
| ai.rs | workflow_save_yaml | state, yaml | — |
| ai.rs | workflow_get_yaml | state, workflow_id | — |
| ai.rs | workflow_delete | state, workflow_id | — |
| ai.rs | workflow_reload | state | — |
| ai.rs | workflow_get_dir | state | — |
| ai.rs | workflow_get | state, workflow_id | — |
| ai.rs | workflow_history_list | state, workflow_id | — |
| ai.rs | workflow_history_get | state, history_id | — |
| ai.rs | workflow_history_clear | state, workflow_id | — |
| ai.rs | ai_generate_schema_doc | state, connection_id, database | **装反(运行时)✗** |
| ai.rs | ai_diagnose_connection | state, connection_id, error_message | 配置 ✓（store.get_connection 查持久化配置） |
| ai.rs | ai_analyze_queries | state, connection_id | **装反(运行时)✗**（源码注释自证："The IPC connection_id carries the runtime dbSessionId"，经 owner_connection_id 反查后按配置键查历史） |
| ai.rs | prompt_list / prompt_set_override / prompt_remove_override | driver_type 等 | — |
| backup.rs | backup_database | state, connection_id, database, output_path, options, compress | **装反(运行时)✗** |
| backup.rs | backup_database_with_dialog | app, state, connection_id, … | **装反(运行时)✗** |
| backup.rs | restore_database | state, connection_id, input_path, options, database | **装反(运行时)✗** |
| backup.rs | restore_database_with_dialog | app, state, connection_id, … | **装反(运行时)✗** |
| backup.rs | execute_sql_file | state, connection_id, input_path, options, database | **装反(运行时)✗** |
| backup.rs | execute_sql_file_with_dialog | app, state, connection_id, … | **装反(运行时)✗** |
| config.rs | get_groups / save_groups / get_settings / save_settings / get_log_path / open_log_dir / open_workflows_dir / open_context_dir / open_path / export_connections(_with_dialog) / import_connections_* / detect_connection_import_path / pick_connection_import_path_with_dialog / export_app_data(_with_dialog) / import_app_data(_with_dialog) / open_path / restart_app / get_system_ui_language / save_encryption_key_with_dialog | — | — |
| connection.rs | connect | state, connection_id | 配置 ✓（返回值即 dbSessionId） |
| connection.rs | disconnect / get_connection_info / ping_connection / release_connection | db_session_id | 会话 ✓ |
| connection.rs | delete_connection | id | 配置 ✓（裸名，删除持久化连接） |
| connection.rs | reorder_connections | ordered_ids | 配置 ✓ |
| connection.rs | save_connection / test_connection / get_connections / get_available_drivers | config 等 | — |
| context.rs | context_* | query/paths | — |
| dashboard.rs | create_widget_from_sql / create_widget_from_workflow / update_hidden_widget_sql | params.connection_id | 配置 ✓（原样存入隐藏工作流 `connection:`，由 executor 双模解析） |
| dashboard.rs | 其余 CRUD/runs/export/import | dashboard_id/widget_id/run_id/id | — |
| data.rs | commit_row_deletes / commit_row_updates | db_session_id | 会话 ✓ |
| data_transfer/mod.rs | inspect_data_transfer | source/target_db_session_id | 会话 ✓ |
| data_transfer/mod.rs | execute_data_transfer / preview_data_transfer / cancel_data_transfer / classify_transfer_pair | job/job 对 | — |
| driver_command.rs | execute_driver_command / execute_driver_command_stream | request.db_session_id | 会话 ✓（双模解析见下） |
| driver_command.rs | get_connection_commands | connection_id | 配置 ✓（resolve_session 双模，主用途按连接枚举命令） |
| driver_command.rs | get_driver_commands | driver_type | — |
| export.rs | export_tables_stream | request | — |
| file.rs | 全部 | path/token 等 | — |
| history.rs | purge_history | scope/retain_days | — |
| mcp.rs | mcp_client_* / mcp_* | server_id/config | — |
| plugins.rs | 全部 | plugin_id/id/path/key | — |
| query.rs | add_favorite_query / get_favorite_queries / get_query_history | connection_id | 配置 ✓（历史/收藏按持久化连接分组） |
| query.rs | execute_query / execute_query_stream / cancel_query / get_explain / begin/commit/rollback_session_transaction / session_transaction_status | db_session_id | 会话 ✓ |
| query.rs | clear_query_history / delete_favorite_query | — | — |
| schema.rs | get_columns/get_database_objects/get_databases/get_er_data/get_object_ddl/get_privileges/get_table_data/get_table_schema/get_tables/use_database | db_session_id | 会话 ✓ |
| schema_diff.rs | prepare_schema_diff_plan / compare_table_data / compare_table_schemas | source/target_db_session_id | 会话 ✓ |
| schema_diff.rs | execute_schema_diff_deploy | target_db_session_id | 会话 ✓ |
| structure.rs | get_structure_capabilities / plan_table_structure_changes | db_session_id | 会话 ✓ |
| sync/mod.rs | inspect/compare/generate/revalidate_data_sync、apply_data_sync | source/target_db_session_id | 会话 ✓ |
| sync/mod.rs | execute_data_sync | target_db_session_id | 会话 ✓ |
| sync/mod.rs | check/delete_sync_task / get_sync_tasks / save_sync_task_direct / classify_sync_pair / cancel_data_sync | task_id 等 | — |
| theme.rs / window.rs | 全部 | — | — |
| lib.rs | rebuild_menu | language | — |

### 2.3 MCP 审计 ✅

`grep -rn "config_id\|configId" src-tauri/src/mcp/` 仅命中 **3 处**，全部位于守护测试字符串内：

```
mcp/server.rs:925:  fn query_input_rejects_removed_config_id_field()
mcp/server.rs:926:    let json = r#"{"config_id":"c1","sql":"SELECT 1"}"#;
mcp/server.rs:930:      "removed config_id field must not deserialize"
```

生产代码所有输入结构体（server.rs 11+ 个 Input 类型）字段均为 `connection_id`（持久化配置语义，注释明确 "Persistent connection id (from list_connections)"）。旧键拒绝机制核实：`QueryInput.connection_id` 为必填非 Option 字段，传 `config_id` 时 serde 报 missing-field 错误——**是显式报错而非静默忽略**，守护测试在全量套件中实际执行通过 ✅。

### 2.4 禁区核查 ✅

vs 分叉基点（三点 diff）与 vs main tip（两点 diff）双重核查：

| 路径 | 要求 | 实际 |
|---|---|---|
| packages/plugin-sdk、packages/extensions、e2e/fixtures | 零改动 | **零改动**（两种口径均为空）✅ |
| packages/drivers | 仅 redis/ui（D3 许可） | W2 提交 packages 下 16 文件全部位于 `packages/drivers/redis/ui/`（含 __tests__）✅ |
| src/test/setup.ts | D3 记录的既有缺陷修复 | 仅此一处改动，内容为 Vitest4 下 jest-dom 匹配器经 `/matchers` + `expect.extend` 注册，附解释注释 ✅ |

### 2.5 history_db 迁移链审查 ✅（附覆盖缺口记录）

代码走读（`store/history_db.rs`，v4 迁移环 L141–L250）：

- **open 流程**：`init_schema`（新库直达最终态列名 connection_id）→ `run_migrations`（版本环）→ `migrate_legacy_json`（JSON v1 迁入，此时物理列已是最终态）。
- **v1→v2（历史步）**：探测 `connection_id` 列存在才执行 `DELETE FROM query_history`（设计即清数据，注释/tracing 均明示 "cleared old data"）+ RENAME 为 `config_id`；老库列名已是 `config_id` 时守卫跳过，不会误删。
- **v2→v3**：补 `schema` 列 + `(config_id,database)` 索引。
- **v3→v4（本次新增）**：对 `query_history`、`favorite_queries` 两表 guarded RENAME `config_id`→`connection_id`，DROP 4 个旧索引、重建 3 个新索引（`idx_query_history_connection_id` / `idx_query_history_connection_db` / `idx_favorite_queries_connection_id`），写入 version=4。
- 各起点推演的最终态均正确：全新库（经环回旋后落 v4）、存量 v3 库、无版本行老库（version 读作 0，守卫防误清）。

**等价执行验证**（受"不改产品代码/不加仓库文件"约束，以 Python+sqlite3 按 history_db.rs 源码逐条复现迁移 SQL，构造三种起点库实测）：

| 起点 | 构造 | 结果 |
|---|---|---|
| ① 存量 v3 库（config_id 列 + version=3 + 数据行/收藏行） | 手工建表灌数 | **PASS**：两表列更名 connection_id、数据完整保留（2 历史+1 收藏）、索引重建正确（旧 config_db 索引已删）、version=4 |
| ② v2 时代老库且无 schema_version 行 | version 读作 COALESCE=0 | **PASS**：守卫正确跳过 v1→v2 清数步，schema 列补齐，数据保留，终态 v4 |
| ③ 全新库（init_schema 直达最终态）走完整环 | 含空表 DELETE+RENAME 回旋 | **PASS**：终态列名/索引/version=4 正确 |

**测试覆盖缺口（记录，不修）**：仓库内 Rust 测试仅覆盖「全新库隐式走完整环」（每个 `HistoryDb::open` 测试均经过）与「JSON v1 迁入」两类起点；**缺失**：① 存量 v3 库→v4（真实用户升级必经路径！）的数据保留/列更名断言；② 无版本行老库起点；③ v1 SQLite 有数据行被设计性清空行为的显式回归锚。本报告的 SQL 等价执行可作一次性佐证，但不能防止未来退化。

### 2.6 其余抽查 ✅

- `driver_command` 请求字段 `dbSessionId`（camelCase 映射 `db_session_id`）✓；`extensionBridge.ts` 以 `dbSessionId: configId` 双模透传并注明 W3 计划 ✓（与摘要 D 决策一致）。
- monitor 注册表键 `monitor:{connection_id}`（`monitor/connections.rs`，含格式守护测试）✓。
- sync 任务持久化结构 `SyncTask`：`source/target_db_session_id`（运行时，注释明示 captured at create/resume）+ `source/target_connection_id`（持久化归属）——直改保语义 ✓（另见观察项 O5）。
- schema-diff 配置 JSON：导出固定 `version: 2`，导入门禁 `cfg.version !== 2 || !cfg.sourceConnectionId || !cfg.targetConnectionId` → 显式报 `schemaDiff.invalidConfig`，v1（configId 版）被拒 ✓。
- 前端 wire keys 抽查：`commands/connection.ts`（connect{connectionId}→返回 dbSessionId，disconnect/ping/release{dbSessionId}）、`commands/query.ts`（会话类 dbSessionId、历史/收藏 connectionId，含中文注释）方向全部正确 ✅。

## T3 覆盖率（llvm-cov，--summary-only）

环境：跳过 2 个 sandbox 既有失败用例后可出表（其余 1115 用例全过）。区域/行覆盖率：

| 模块 | Regions | Lines | ≥80% 判定 |
|---|---|---|---|
| store/history_db.rs | 87.30% | **91.14%** | ✅ |
| mcp/server.rs | 87.65% | **88.11%** | ✅ |
| commands/driver_command.rs | 82.45% | **81.95%** | ✅ |
| commands/query.rs | 83.33% | **80.62%** | ✅ |
| **commands/connection.rs** | 68.59% | **65.37%** | ❌ **不达标** |
| TOTAL（仅参考记录） | 77.14% | 77.04% | — |

说明：connection.rs 大量为 IPC 薄包装 + `*_with_dialog`（需 AppHandle/对话框）分支，属可测但未测；作为 connect/disconnect 契约所在地仍应按本次门槛补齐。跳过的两个用例位于 lib.rs 日志设置，对本表模块影响可忽略。

## T4 E2E 视角用例清单（12 条）

图例：【执行】= 本测试实际执行通过；【代码】= 源码级核对；【单测】= 由全量/定向 Rust 单测覆盖并实际执行；【未执行】= 需 GUI/WebdriverIO/MCP stdio 实进程，建议纳入收尾回归 `pnpm e2e:minimal`。

| # | 用例（前置/步骤/预期） | 实际状态 |
|---|---|---|
| ① | connect{connectionId}→返回 dbSessionId，可供 query/use_database/cancel 使用。前置：已有持久化连接 | 【单测】`connect_registers_session_and_returns_db_session_id`、`get_or_connect_session_reuses_existing_session` 等 ✅；【代码】query/use_database/cancel 后端签名均收 db_session_id、前端 wrapper 键名匹配 ✅；【未执行】真实驱动 GUI 全链路（原因：需 webdriver 构建） |
| ② | connect 未知 id → ConnectionConfigNotFound 可辨识文案。步骤：传入不存在的配置 id | 【执行】`connect_errors_when_connection_config_missing` ✅；【代码】文案 = "Connection config '…' not found (connectionId refers to a persisted connection configuration; no such configuration is stored)" ✅ |
| ③ | 对伪造 db_session_id 执行 ping/disconnect → DbSessionNotFound 且文案含"是否把 connectionId 当成 dbSessionId"提示 | 【代码】文案 = "DB session '…' not found (a dbSessionId is a runtime session id; maybe you passed a connectionId where a dbSessionId was expected)" ✅；【执行】error.rs 序列化/display 测试 + disconnect_removes_session/get_session_config 路径 ✅；【未执行】宿主命令级伪造 id 往返（需运行时 AppHandle） |
| ④ | 历史写入后列为 connection_id，按 connection_id 过滤生效 | 【执行】`query_history_connection_id_filter`、`query_history_database_filter`、`query_history_schema_roundtrip_and_filter`、dedup×3 ✅ |
| ⑤ | 收藏同上（值为 connection_id、过滤生效） | 【执行】`favorite_queries_crud`（cfg-a/cfg-b 过滤断言）✅ |
| ⑥ | 存量 v3 库（手工构造 config_id 列 SQLite）启动迁移后数据完整、列已更名 | 【执行·SQL 等价】三起点复现验证全 PASS（见 T2.5）；【缺口】仓库内无该起点的 Rust 回归测试（记 G1） |
| ⑦ | execute_driver_command{dbSessionId} 正常；传配置风格 id 经 resolve_session 双模仍可用（bridge 兼容路径） | 【执行】resolve_session 四专项（passthrough/reuses/prefers/falls_back）+ `evicted_session_auto_reconnects_preserving_db_session_id` 共 5 条 ✅；【代码】request.dbSessionId 字段、bridge `dbSessionId: configId` 透传注释 ✅；【未执行】真实驱动执行往返 |
| ⑧ | MCP list_databases/query 用 connection_id 成功；发 config_id → 明确报错（非静默忽略） | 【执行】`query_input_accepts_connection_id`、`query_input_rejects_removed_config_id_field`、`test_list_databases_input_deserialization` ✅；【代码】生产路径经 `resolve_connection` 双模（接受配置 id）✅；【未执行】MCP stdio 实进程工具调用 |
| ⑨ | create_widget_from_sql{connectionId} 成功且 workflow.connection 为该 id | 【代码】impl 将 connection_id 原样写入隐藏工作流 `connection:`（create.rs L67/L165），executor 以 resolve_session 双模解析 ✅；【执行】executor 相关单测（step_connection_override_executes_command 等）在全量套件绿 ✅；【未执行】GUI 落盘文件核验 |
| ⑩ | inspect/execute_data_sync 用 source/target_db_session_id 走通校验分支 | 【执行】`data_sync::session::tests` 12 条（revalidate 校验分支等）+ 签名核对 ✅；【未执行】真实同族双库同步 |
| ⑪ | schema-diff v1 JSON 拒绝报错、v2 接受 | 【代码】导入门禁 `version!==2 → invalidConfig` 显式错误，v1 必缺 sourceConnectionId 双重拦截 ✅；【执行】schemaDiffConfirm.test.ts 绿；【缺口】无专门"v1 样例被拒"单测（记 G2）；【未执行】子窗口剪贴板导入 UI 流 |
| ⑫ | monitor 注册表键隔离（不同 connection_id 不串会话） | 【执行】`monitor_registry_key_prefixes_connection_id`、`get_or_connect_reuses_existing_handle_pool_id`、`disconnect_monitor_removes_entry` ✅；【代码】键结构 `monitor:{connection_id}` 天然按配置隔离 ✅ |

定向执行汇总：`cargo test --lib` 过滤运行上述相关测试 **29 条全部通过**（0.30s）。

**未执行项汇总及理由**：WebdriverIO Host E2E（需 `tauri build --debug --features webdriver`，超出本轮沙箱时间/权限预算）、MCP stdio 实进程往返、真实驱动备份/恢复/同步链路。以上均不涉及纯契约改名以外的逻辑，风险集中在 D1 所述两域，建议收尾回归时优先覆盖 backup/AI 面板路径。

## T5 缺陷记录与观察项

### 缺陷

**D1（中｜契约一致性）：ai 域 7 个 + backup 域 6 个命令参数名 `connection_id` 实为运行时会话语义——"换名但语义装反"**
- 位置：`src-tauri/src/commands/ai.rs`（ai_chat / ai_generate_sql / ai_diagnose_error / ai_parse_filter / ai_analyze_explain / ai_generate_schema_doc 直调 `connection_manager.get_session()`；ai_analyze_queries 注释自证参数携带的是 dbSessionId）；`src-tauri/src/commands/backup.rs`（backup_database[_with_dialog] / restore_database[_with_dialog] / execute_sql_file[_with_dialog]，两条 impl 路径直调 `get_session` + `get_session_config`）。
- 重现：`invoke('ai_chat', { connectionId: '<真实持久化配置id>', … })` → 返回 "DB session '…' not found (…maybe you passed a connectionId where a dbSessionId was expected)"。对照 `get_connection_commands` / MCP tools / workflow executor 的 `resolve_session` 双模路径，同名参数行为不一致。
- 影响：① 违反 W2 自身契约约定（会话类参数应为 db_session_id）与本次审计规则；② 当前仅因前端把 dbSessionId 值装入 `connectionId` 键传递而碰巧可用（activeConnectionStore 将 connect() 返回值存为 `connectionId` 并透传；BackupWindow 同）；③ W3 若按新契约将前端改为传真正的配置 id，这 13 个命令将立即功能性断裂；④ 外部/插件侧按文档语义传配置 id 即失败。
- 建议：随 W3 一并将 13 个命令参数改名 `db_session_id`（前端键同步），或短期先统一改走 `resolve_session` 双模；补充守护测试（如 grep 式 lint 断言 command 参数不含运行时语义 connection_id），并把 BackupWindow/AI 面板纳入 e2e 回归。

### 观察项（不计缺陷）

- **O1** `store::tests::concurrent_save_connection_all_succeed` 高负载下偶发失败（15≠16）：隔离 5/5 过、代码路径 vs main 零改动、main 基线全量过 → 非 W2 引入；建议后续排查 Store 写并发竞态或降低该测试对 CPU 调度的敏感度。
- **O2** `resolve_log_settings_*` 2 例 sandbox 拒写用户目录：已知既有环境限制。
- **O3** `src/test/setup.ts` 变更为 D3 记录的 Vitest4 匹配器注册修复，合规纳入。
- **O4** e2e tsconfig 71 处既有类型噪音：基线一致口径，未复核。
- **O5** `SyncTask` 为持久化结构却含 `source/target_db_session_id`（运行时 id，重启后必然失效）；代码注释已声明其为"创建/续跑时捕获的快照"，续跑应以 `*_connection_id` 为准。建议后续明确序列化时的失效标记策略。

### 覆盖缺口（记录，不修）

- **G1** history_db 迁移环缺少「存量 v3 库」「无版本行老库」起点的 Rust 回归测试（本报告已做一次性 SQL 等价验证，见 T2.5/T4⑥）。
- **G2** schema-diff 缺少「v1 样例 JSON 被拒」的前端单测锚点。

---

## 附：证据索引

- 全量套件输出：T1 表格所载计数（cargo lib 两次运行 / vitest / drivers vitest / tsc / vite）。
- 定向测试：connection_manager::tests ×10、monitor::connections::tests ×3、mcp::server::tests ×3、error::tests ×3、data_sync::session::tests ×12（合计 29 passed）。
- 迁移等价执行脚本与三种起点库：`/tmp/w2test/{v3,v0,fresh}.sqlite`（临时目录，不入仓）。
- main 基线对照运行：主仓根目录（main @ e45aae4b，干净树）+ 独立 target 目录 `.worktrees/w2-main-target`。
