# Data Sync 架构

> Source of truth: `src-tauri/src/data_sync/`、`src-tauri/src/commands/sync/` 和 `src/windows/data-sync/`。

Data Sync 用于**同族数据库的行级差异同步**。它与 Schema Diff、Data Transfer 是三个独立执行模型。

| 能力 | 用途 |
|---|---|
| Schema Diff | 修改目标结构 |
| Data Sync | 同族数据库按行比较并同步 |
| Data Transfer | 异构/映射/结构+数据搬运 |

## 1. 安全门闸

当前 Data Sync 要求：

1. Source/Target 属于相同 normalized family。
2. V1 同步 family 为 MySQL/MariaDB 和 PostgreSQL。
3. 列名、类型、nullable 等结构满足 `types_eq`。
4. Primary key 集合及顺序一致。
5. 禁止同连接、同 database、同 schema 自同步。
6. Source/Target database 是请求的一部分；不是按 connection 下的所有 database 自动同步。

跨方言不进入 Data Sync，而进入 Data Transfer 的 IR 路径。

## 2. Compare

生产 compare 使用 keyset pagination：

```text
get tables/schema
   ↓
classify mappings
   ↓
keyset page source
   ↓
compare_table_pages()
   ↓
INSERT / UPDATE / DELETE / UNCHANGED
```

相关代码：

- `data_sync/keyset.rs`
- `commands/sync/keyset_source.rs`
- `data_sync/compare.rs`

`jobId` 对应取消标志，可中断长时间 compare。

## 3. Review / ChangeSet / Preview

用户在 Compare 阶段选择需要应用的行和 Insert/Update/Delete 选项。

`changeset.rs` 只保留：

- 用户勾选的变更；
- 当前 SyncOptions 允许的变更。

DELETE 默认不选。

Preview 生成 SQL 展示文本；真正 Execute 使用参数化 SQL，不把 Preview literal SQL 当作执行 SQL。

## 4. Execute

```text
ChangeSet
  ↓
generate_table_sql
  ↓
execute_data_sync(jobId)
  ↓
begin
  ↓
query_with_params
  ↓
commit
```

失败或取消会 rollback。Data Sync 使用专用执行通道，不经过普通 `execute_query` 的 Safe Mode 路径，避免正常的 UPDATE/DELETE 安全检查误伤已验证的 ChangeSet。

Execute 前支持 `revalidate_data_sync`，发现结构/PK 漂移时阻止执行。

## 5. IPC

`src-tauri/src/commands/sync/` 当前主要提供：

- `inspect_data_sync`
- `compare_data_sync`
- `generate_data_sync_sql`
- `revalidate_data_sync`
- `apply_data_sync`
- `execute_data_sync`
- `cancel_data_sync`

## 6. Frontend

`DataSyncWindow.tsx` 使用 6 步流程：

```text
Endpoints → Setup → Objects → Compare → Preview → Result
```

Compare / Preview / Execute 的状态都在当前窗口流程中维护；取消通过 `jobId` 发送到 backend。

## 7. Tests

- Rust：`src-tauri/src/data_sync/**`、`src-tauri/src/commands/sync/tests.rs`
- Frontend：`src/windows/data-sync/__tests__/`
- E2E：`e2e/specs/data-sync-*.ts`

Driver 方言/类型规则属于对应 Driver，不在 Host 重复实现。
