# 数据库驱动层

> Source of truth: `packages/driver-api/` 与 `packages/drivers/*`。

DataZen 的 Driver 是**编译期集成**的数据库实现，不通过运行时 Rust 动态库 ABI 加载。

## 1. Driver API

`packages/driver-api/src/` 提供 Host 与 Driver 之间的公共契约。

核心接口：

- `DatabaseDriver`
- `DatabaseDriverFactory`
- `DriverCommandDefinition`
- `MigrationRenderer`
- `MigrationCapabilities`
- `TypeNormalizer`
- `SyncSourceAdapter` / `SyncTargetAdapter`

公共 API 不暴露 sqlx、mongodb、redis 等数据库实现类型；具体连接池、Row、事务实现由 Driver 自己管理。

当前 API：

- `PROTOCOL_VERSION = 3`
- `MIN_PROTOCOL_VERSION = 1`

## 2. DatabaseDriver

Driver 基础能力包括：

- connect / test_connection / disconnect
- get_databases / get_tables / get_table_schema
- query / query_multi / query_stream
- query_with_params / execute
- transaction
- EXPLAIN
- Driver Commands

查询流支持 opaque `QueryExecutionId` 生命周期：

```text
prepare_query_execution
       ↓
query_stream_with_execution
       ↓
cancel_query_with_execution
       ↓
cleanup_query_execution
```

只有实际声明精确取消能力的 Driver 才会被 Host 当作 cancellable；兼容默认实现不会自动获得取消能力。

## 3. Driver Commands

Driver 可以通过 `command_definitions()` 声明 Command，通过 `execute_command()` 执行。

Host / Workflow 只依赖 Command Definition 和 JSON input/output，不按 database type 复制 Driver-specific dispatch。

标准 SQL Driver 默认提供 query / execute，以及 schema catalog commands；Driver 可以扩展管理、KV、NoSQL 等 Command。

## 4. Schema Migration

Schema Diff 不直接在 Host 拼接数据库方言。

```text
schema_diff
  ↓
MigrationOperation
  ↓
MigrationRenderer
  ↓
MigrationStatement
```

Driver 同时可提供：

- `MigrationCapabilities`
- `TypeNormalizer`

因此 PostgreSQL/MySQL/SQLite 等差异位于 Driver 层，而不是 UI 或 Schema Diff domain 中。

## 5. Sync / Transfer Adapter

`SyncSourceAdapter` / `SyncTargetAdapter` 服务于异构 Data Transfer：

```text
source schema/value
      ↓
     IR
      ↓
target native type/value
      ↓
target SQL
```

Data Sync 的同族结构门闸和 Data Transfer 的 IR pairing 不应混为一个执行路径。

## 6. Driver Registry

`src-tauri/src/db/registry.rs` 负责 Host 侧 Driver Registry；`src-tauri/src/db/mod.rs` re-export Driver API。

Driver 通过 inventory 在编译期注册。构建时由 registry / build tooling 决定哪些 Driver 被编译进 DataZen。

## 7. Driver 开发

独立 Driver 推荐使用独立 Git repository，通过 DataZen registry 的 `source: "path"` 在本地宿主中调试，发布后可使用固定 commit 的 Git dependency。

详见 [独立驱动开发指南](../../development/independent-driver-development.zh-CN.md)。
