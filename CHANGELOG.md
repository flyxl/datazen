# 更新日志 / Changelog

本文件记录 DataZen 的显著变更，重点是影响外部契约的破坏性变更。

格式参照 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 简化版；
术语约定：**`connectionId` / `connection_id` = 配置连接 id（持久化）**，
**`dbSessionId` / `db_session_id` = 运行时数据库会话 id（内存态）**。

## [Unreleased]

### ⚠️ 破坏性变更（Breaking Changes）

本次重构统一了连接 ID 术语（持久化配置 id 与运行时会话 id 彻底分离），以下外部契约**均不保留兼容别名**：

- **MCP DB 工具入参改名**：所有数据库工具（`list_databases`、`list_tables`、`search_tables`、`query`、`get_schema`、`explain_query`、`describe_table` 等）的参数 `config_id` → `connection_id`，旧键名会被直接拒绝（deserialize 失败），无别名回退。
  - 迁移：将工具调用 payload 中的 `"config_id"` 键改名为 `"connection_id"`；取值不变（仍为 `list_connections` 返回的持久化连接 id）。

- **MCP 资源输出与模板改名**：
  - Schema 资源 URI 模板为 `datazen://schema/{connectionId}/{database}`；
  - `datazen://query-history` 条目 JSON 字段 `configId` → `connectionId`（serde camelCase 序列化）。
  - 迁移：解析这些资源输出的客户端按新键名读取；拼接 schema URI 时使用 `{connectionId}` 占位。

- **SQLite 历史库列名改名**：`history.sqlite` 中 `query_history.config_id` / `favorite_queries.config_id` → `connection_id`。应用启动时自动执行一次性迁移（schema v3 → v4），**数据完整保留**，无需手工干预。
  - 迁移：直接升级即可；外部直读该 SQLite 文件的脚本需按新列名查询。

- **Schema Diff 剪贴板/配置 JSON 升级到 v2**：导出格式键 `configId` → `sourceConnectionId` / `targetConnectionId`（`version: 2`）。导入时 **v1 格式（含 `configId` 键）会被明确拒绝**并提示无效配置。
  - 迁移：旧的 v1 配置文本无法直接导入；在界面上重新选择源/目标连接后重新导出。

- **数据同步任务持久化字段改名**：`SyncTask` 的 `sourceConfigId/targetConfigId` → `sourceDbSessionId/targetDbSessionId`（运行时会话）+ `sourceConnectionId/targetConnectionId`（归属配置连接）。旧字段名的持久化载荷将无法反序列化。
  - 迁移：升级前请完成进行中的同步任务；已中断的旧任务需重建后重跑。

- **插件桥协议键改名**：`command.invoke` 消息参数 `configId` → `connectionId`（无别名，缺键/错键返回错误提示 `{connectionId, command, args?}`）；plugin-sdk 类型定义已同步更新。
  - 迁移：插件代码中调用 `command.invoke` 时将参数对象中的 `configId` 键改为 `connectionId`，并升级到配套版本的 plugin-sdk。

### Changed

- 全端术语统一：持久化连接配置标识统一为 `connectionId`（前端 camelCase）/ `connection_id`（Rust snake_case）；运行时会话标识统一为 `dbSessionId` / `db_session_id`。GUI IPC（`connect` 除外）、Workflow 步骤解析等相应对齐。
- 文档对齐上述术语（架构文档、功能指南、官网文档页），历史演进说明保留但均标注现行为。
