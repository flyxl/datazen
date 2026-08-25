# IPC 命令重构计划

> [返回架构总览](../README.md) | 相关文档: [commands.md](./commands.md)

本文档记录 IPC 命令层的分析结论和重构方向。目标是减少命令数量、消除冗余、统一调用模式。

## 现状

当前共注册约 **185 个** Tauri IPC 命令。按资源类别分布如下:

| 资源域 | 数量 | 说明 |
|--------|------|------|
| Connection (CRUD + lifecycle) | 11 | 连接配置管理 + 运行时生命周期 |
| Connection Group | 2 | 分组 CRUD |
| Database / Schema browse | 9 | 数据库/表/列/对象浏览 |
| Query / Driver Command | 13 | 查询执行 + 统一 Driver Command 入口 |
| Table Data (read/edit) | 3 | DataTable 行级编辑 |
| Structure (DDL planning) | 2 | 结构编辑计划 |
| Query History / Favorites | 6 | 历史 + 收藏 |
| Backup / Restore | 6 | 备份恢复（含 path/dialog 配对）|
| Data Sync | 12 | 同族数据同步全流程 |
| Data Transfer | 5 | 异构迁移 |
| Schema Diff | 4 | 结构比较 + 部署 |
| AI / Chat | 14 | AI Provider CRUD + 功能 |
| Workflow | 12 | 工作流 CRUD + 执行 + 历史 |
| Prompt (AI template) | 3 | Prompt 模板管理 |
| MCP Server | 5 | MCP Server 控制 |
| MCP Client | 5 | MCP Client 连接 + 工具调用 |
| Dashboard / Widget / Monitor | 16 | Dashboard CRUD + Widget |
| Plugins | 11 | 插件 CRUD + 存储 |
| Theme | 5 | 主题包管理 |
| Settings / App Config | 9 | 设置 + 路径操作 |
| File IO | 11 | 文件读写（dialog + path 配对）|
| Window | 2 | 子窗口 + 菜单 |
| ADB (Android) | 4 | Android 数据库拉取 |
| Context (AI files) | 3 | AI 上下文文件 |
| Import/Export (connections) | 7 | 连接导入导出 |
| Import/Export (app data) | 5 | 应用数据归档 |

---

## 重构决策

### 决策 1: 废弃 `use_database` — 前端显式传参

**问题:**
- 前端先调 `use_database` 切换后端 session 状态，再发 query — 有状态设计存在竞态风险
- 多个 Tab 共享同一连接时，先后调用可能互相覆盖 session 的 active database

**方案:**
- 所有需要 database 上下文的 IPC 接受显式 `database: Option<String>` 参数
- 后端在执行 SQL 前自动 dispatch `USE db`（MySQL）或无操作（PostgreSQL），对前端透明
- `use_database` IPC 废弃

**迁移步骤:**
1. `execute_query` / `execute_query_stream` / `get_explain` 增加 `database` 参数
2. 后端 `execute_query_impl` 中: 若 `database` 与当前 session 不同，先调 `driver.use_database()` 再执行
3. 前端移除 `databaseCommands.useDatabase()` 调用; `schemaStore` 只更新前端 `currentDatabase` 状态
4. 保留 `get_tables(connectionId, database)` 已有的显式 database 参数模式（已是无状态）
5. 删除 `use_database` IPC 注册

### 决策 2: ADB 命令迁移到 SQLite 驱动

**问题:**
- `adb_list_packages` / `adb_list_databases` / `adb_pull_database` / `adb_pull_database_with_dialog` 写在 Host
- ADB 是 SQLite 驱动专属能力（从 Android 设备拉取 .db 文件），不应由 Host 承载

**方案:**
- 在 `packages/drivers/sqlite/` 实现为 `DriverCommandDefinition`（`requiresConnection = false`）
- 前端通过 `execute_driver_command(driverType: "sqlite", command: "adb_list_packages", ...)` 调用
- Host `commands/adb.rs` 标记 deprecated 后移除

### 决策 3: Path/Dialog 命令合并 — override_path 模式

**问题:**
- 每个文件操作有两个 IPC: `foo`（接收路径, E2E only）+ `foo_with_dialog`（弹 dialog, 生产）
- 约 10 对配对，导致命令膨胀

**方案: 统一为一个 IPC + optional `override_path`**

```rust
async fn backup_database_with_dialog(
    // ... 其他参数
    override_path: Option<String>,  // 仅 webdriver build 生效
) -> Result<bool, CommandError> {
    let path = if let Some(p) = override_path {
        #[cfg(not(feature = "webdriver"))]
        return Err(CommandError::Validation("path override disabled in production".into()));
        #[cfg(feature = "webdriver")]
        PathBuf::from(p)
    } else {
        // 弹出原生 dialog
        let picked = app.dialog().file().blocking_save_file();
        // ...
    };
    // 共享业务逻辑
    do_backup(&state, connection_id, database, path, options).await
}
```

**合并清单:**

| 当前 (2 IPC) | 合并后 (1 IPC) |
|-------------|---------------|
| `backup_database` + `backup_database_with_dialog` | `backup_database` |
| `restore_database` + `restore_database_with_dialog` + `execute_sql_file` + `execute_sql_file_with_dialog` | `restore_sql_file` |
| `export_connections` + `export_connections_with_dialog` | `export_connections` |
| `import_connections_preview` + `import_connections_with_dialog` | 保持两个（preview/import 语义不同），各自合并 path 参数 |
| `export_app_data` + `export_app_data_with_dialog` | `export_app_data` |
| `import_app_data` + `import_app_data_with_dialog` | `import_app_data` |
| `adb_pull_database` + `adb_pull_database_with_dialog` | 迁移到 SQLite driver（决策 2）|

**安全分析:** 原生 dialog 的核心安全保障是 **用户必须手动点击确认**（OS 级交互），与 dialog 由 Rust 还是 JS 侧调起无关。`override_path` 仅在 `cfg!(feature = "webdriver")` 构建中生效，生产构建不编译该分支。

### 决策 4: 删除纯文件读写 IPC — E2E 用 Node.js fs 替代

**问题:**
- `write_file` / `write_file_base64` / `read_file` 仅被 E2E 测试用于环境准备（写入 fixture 文件）
- E2E 测试本身是 Node.js 进程，可直接用 `fs.writeFileSync()` 操作文件系统

**方案:**
- 删除 `write_file` / `write_file_base64` / `read_file` 三个 IPC
- E2E 中所有文件准备改用 Node.js `fs` 模块
- `save_text_with_dialog` / `open_text_with_dialog` 等 dialog 系列保留（生产必需）

### 决策 5: 废弃已失效或冗余的命令

| 命令 | 废弃原因 |
|------|---------|
| `get_monitor_paused` | 已被 `set_dashboard_refresh_paused` 取代 |
| `set_monitor_paused` | 同上 |
| `compare_table_data` | 无前端调用，功能未上线 |
| `classify_sync_pair` | 前端 `syncPairing.ts` 已有相同逻辑（选择保留一端，删另一端） |

### 决策 6: `restore_database_with_dialog` 与 `execute_sql_file_with_dialog` 合并

**现状:** 两者 Rust 实现完全相同（都调用 `sql_file_with_dialog`）:

```rust
pub async fn restore_database_with_dialog(...) -> Result<bool, CommandError> {
    sql_file_with_dialog(&app, &state, connection_id, database, options).await
}

pub async fn execute_sql_file_with_dialog(...) -> Result<bool, CommandError> {
    sql_file_with_dialog(&app, &state, connection_id, database, options).await
}
```

**方案:** 合并为 `restore_sql_file`，前端使用统一入口。

---

## 关键概念辨析

### `release_connection` vs `disconnect`

| | `release_connection` | `disconnect` |
|--|---------------------|-------------|
| **语义** | 引用计数 -1; 计数到 0 才真正断开 | 强制断开，无视引用计数 |
| **使用场景** | 关闭 Tab / 窗口关闭（优雅释放） | 用户右键 "断开连接"（立即断开）|
| **返回** | `bool`（是否触发了实际断开） | `()` |
| **底层逻辑** | `ref_counts -= 1; if 0 → disconnect()` | `ref_counts.remove(); driver.disconnect()` |

两者语义不同，不建议合并。可选: 合为一个 IPC + `force: bool` 参数。

### `test_connection` vs `ping_connection`

| | `test_connection` | `ping_connection` |
|--|-------------------|-------------------|
| **输入** | `ConnectionConfig`（未保存的配置） | `connection_id`（活跃连接） |
| **功能** | 建立真连接 → 取 ServerInfo → 断开 | 刷新 `last_used` 时间戳（防 idle 回收） |
| **网络** | 真连接（TCP/TLS） | 无网络操作（纯内存更新） |
| **目的** | 连接表单 "测试连接" 按钮 | 心跳保活（前端每 5 分钟一次） |

完全不同的功能，不建议合并。

### 原生文件选择器的必要性

**为什么用原生 dialog:**
1. 最佳 UX — OS 级体验（收藏夹、最近文件、拖拽）
2. 安全 — 用户必须明确交互才能授权文件访问
3. 路径不暴露给 JS — 降低 XSS 利用面

**为什么不能用 Web 替代:**
- `<input type="file">` 只能 Open，不能 Save As
- Web File System Access API: macOS WKWebView 不支持
- 自建文件浏览器: UX 差、需完整 fs 权限、安全性降低

**E2E 测试的解决方案:**
- 不替换 native dialog（保持安全 + UX）
- 通过 `override_path: Option<String>` + `#[cfg(feature = "webdriver")]` 在测试构建中绕过
- 文件环境准备使用 Node.js `fs` 模块（不走 IPC）

---

## 重构后命令清单预估

| 维度 | 当前 | 重构后 |
|------|------|--------|
| 总 IPC 数 | ~185 | ~166 |
| Path-only IPC | ~10 | 0（合入 dialog 版） |
| 纯 E2E 文件 IPC | 3 | 0（改 Node.js fs）|
| ADB Host IPC | 4 | 0（迁到 SQLite driver）|
| 废弃冗余 | 5 | 0 |
| **净减少** | | **~19** |

长期（`execute_query` / schema 对象 IPC 迁入 `execute_driver_command`）可再减 5-8 个。

---

## 按资源分类的目标命令清单

### Connection (保留 11)
`get_connections` / `save_connection` / `delete_connection` / `reorder_connections` / `test_connection` / `connect` / `ping_connection` / `release_connection` / `disconnect` / `get_connection_info` / `get_available_drivers`

### Connection Group (保留 2)
`get_groups` / `save_groups`

### Database / Schema (8, 去掉 use_database)
`get_databases` / ~~`use_database`~~ / `get_tables` / `get_columns` / `get_table_schema` / `get_er_data` / `get_database_objects` / `get_object_ddl` / `get_privileges`

### Query / Driver Command (保留 13)
`execute_query` (+database) / `execute_query_stream` (+database) / `export_tables_stream` / `get_driver_commands` / `get_connection_commands` / `execute_driver_command` / `execute_driver_command_stream` / `get_explain` / `cancel_query` / `begin_session_transaction` / `commit_session_transaction` / `rollback_session_transaction` / `session_transaction_status`

### Table Data (保留 3)
`get_table_data` / `commit_row_updates` / `commit_row_deletes`

### Structure (保留 2)
`get_structure_capabilities` / `plan_table_structure_changes`

### Query History / Favorites (保留 6)
`get_query_history` / `clear_query_history` / `purge_history` / `get_favorite_queries` / `add_favorite_query` / `delete_favorite_query`

### Backup / Restore (3, 从 6 合并)
`backup_database` (含 override_path) / `restore_sql_file` (四合一) / `save_encryption_key_with_dialog`

### Data Sync (保留 12)
全部保留

### Data Transfer (保留 5)
全部保留

### Schema Diff (3, 去掉 compare_table_data)
`prepare_schema_diff_plan` / `execute_schema_diff_deploy` / `compare_table_schemas`

### AI / Chat (保留 14)
全部保留

### Workflow (保留 12)
全部保留

### Prompt (保留 3)
全部保留

### MCP Server (保留 5)
全部保留

### MCP Client (保留 5)
全部保留

### Dashboard / Widget (14, 去掉 2 deprecated)
去掉 `get_monitor_paused` / `set_monitor_paused`

### Plugins (保留 11)
全部保留

### Theme (保留 5)
全部保留

### Settings / App Config (保留 9)
全部保留

### File IO (8, 去掉 write_file / write_file_base64 / read_file)
`save_text_with_dialog` / `save_base64_with_dialog` / `begin_save_with_dialog` / `append_save_text` / `finish_save` / `abort_save` / `open_text_with_dialog` / `open_base64_with_dialog`

### Window (保留 2)
全部保留

### ADB (0, 迁移到 SQLite driver)
全部迁移

### Context (保留 3)
全部保留

### Import/Export Connections (5, 从 7 合并)
`export_connections` (含 override_path) / `import_connections_preview` (含 override_path) / `import_connections_with_dialog` (含 override_path) / `detect_connection_import_path` / `pick_connection_import_path_with_dialog` / `import_connections_from_app`

### Import/Export App Data (3, 从 5 合并)
`export_app_data` (含 override_path) / `import_app_data` (含 override_path) / `save_encryption_key_with_dialog`
