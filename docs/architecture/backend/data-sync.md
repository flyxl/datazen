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

## 6. 连接、会话与取消

Data Sync **不接受任何运行时会话 ID**：源端与目标端一律从任务落盘的 `connectionId` 现场解析，会话由后端在每次端点解析时决定，用完释放或保持共享有明确分支（`commands/sync/tasks.rs` 的 `resolve_task_endpoint`）：

| 任务保存的目标库 | 会话来源 | 是否释放 |
|---|---|---|
| **指定了**目标库（哪怕它正好等于连接配置的默认库） | `connect_dedicated(connectionId, 该库)`，始终新建物理连接 | **是**——只释放 dedicated 会话 |
| 未指定 / 只有空白字符 | `resolve_session_for_connection(connectionId)`，复用 GUI 可能已开着的同一条会话 | **否**——共享路径不释放引用 |

判据是**目标库字符串是否非空**：`commands/sync/tasks.rs:77` 写的是 `database.filter(|value| !value.trim().is_empty())`，代码里**从不**把它与连接配置的默认库作比较，所以「目标库恰好等于默认库」照样开专用会话。这么判的理由是宿主不做切库：共享会话当前挂在哪个库是可变的，复用它无法保证任务落在任务自己指定的那个库上（`tasks.rs:79-82` 的注释）。dedicated 会话在后续任何一步失败时也会回滚释放。

与默认库的**比较**确实存在，但它发生在开完会话**之后**，而且是**配置漂移检查**而非会话选择判据（`tasks.rs:125-139`）：持久化任务存的 database 与**连接配置当前的 `config.database` 字段**比对，不一致说明任务 outlive 了一次配置编辑（该处注释原文：`A persisted task may outlive an edited connection config`），随即释放已开的 dedicated 会话并报冲突。注意它比的是配置字段，不是会话实际挂载的库。

三条随之而来的约束：

- **落盘的 `connectionId` 是唯一恢复依据**。任务文件里曾经存在过的运行时 `dbSessionId` 被刻意忽略——它是进程内的，重启后可能已经指向别的库。目标库 / schema 作为**独立的字符串字段**随任务保存；每次解析出会话后会拿**目标库**与该连接的实际库复核一次，不一致直接报冲突并提示重开任务，而不是静默统计到另一个库。schema 则相反：它在限定被检对象时显式应用，允许与连接的可变默认 search path 不同，因此不参与这次复核。
- **`connectionId` 缺失是配置错误**，不是降级路径。任务必须重新打开并选择连接。
- **取消走 job 标志**。`jobId` 是取消的唯一句柄，进程内有效、跨进程失效；不按连接或会话归类。取消早于执行到达时同样有效。

Data Sync 与另两个成员的边界：**Data Synchronization 只做同族、且结构与主键完全一致的复制**；结构差异与 DDL 生成属于 Schema Diff，异构数据搬迁属于 Data Transfer。写任务时不要把三者混成一种「迁移」。

## 7. Frontend

`DataSyncWindow.tsx` 使用 6 步流程：

```text
Endpoints → Setup → Objects → Compare → Preview → Result
```

Compare / Preview / Execute 的状态都在当前窗口流程中维护；取消通过 `jobId` 发送到 backend。

## 8. Tests

- Rust：`src-tauri/src/data_sync/**`、`src-tauri/src/commands/sync/tests.rs`
- Frontend：`src/windows/data-sync/__tests__/`
- E2E：`e2e/specs/data-sync-*.ts`

Driver 方言/类型规则属于对应 Driver，不在 Host 重复实现。
