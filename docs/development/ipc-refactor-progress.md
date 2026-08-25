# IPC 重构进度管理

> 依据 [dev-workflow.txt](./dev-workflow.txt) 与 [ipc-refactor-plan.md](./ipc-refactor-plan.md)。
> 分支：`feature/ipc-refactor`（worktree：`../datazen-ipc-refactor`）。
> 每个功能：编码子代理开发+单测 → commit → **全新**测试子代理测试（E2E 用例+结果、bug 记录、覆盖率≥80%）→ commit → bug 流转循环。

## 功能总览

| # | 功能 | 对应决策 | 状态 | 编码 commit | 测试 commit |
|---|------|---------|------|------------|------------|
| F1 | 废弃 `use_database`，query/stream/explain 显式传参 | 决策 1 | 编码完成 | 本提交 | — |
| F2 | ADB 命令迁移 SQLite 驱动（DriverCommandDefinition） | 决策 2 | 未开始 | — | — |
| F3 | backup/restore 合并 + `restore_sql_file` 四合一（override_path 模式） | 决策 3+6 | 未开始 | — | — |
| F4 | connections / app-data 导入导出 override_path 合并 | 决策 3 | 未开始 | — | — |
| F5 | 删除纯文件读写 IPC（write_file/write_file_base64/read_file），E2E 改 Node fs | 决策 4 | 未开始 | — | — |
| F6 | 删除冗余命令（monitor_paused×2 / compare_table_data / classify_sync_pair） | 决策 5 | 未开始 | — | — |
| R | 回归测试 + 文档更新（架构文档/AGENTS.md）+ 合并 main | 步骤 6 | 未开始 | — | — |

状态机：`未开始 → 编码中 → 编码完成 → 测试中 → 已完成`；bug 流转见下方 Bug 台账。

## Bug 台账

| Bug ID | 所属功能 | 描述 | 状态 | 记录时间 | 验证记录 |
|--------|---------|------|------|---------|---------|
| （暂无） | | | | | |

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
（待测试代理填写）

### 测试结果
（待测试代理填写）

### 覆盖率
（待测试代理填写）

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
