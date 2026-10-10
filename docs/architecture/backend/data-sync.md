# Data Sync 架构

> P5 桌面 JobRuntime 接入已完成；迁移三件套独立测试与冒烟测试通过。Source of truth: `packages/data-sync/`、`src-tauri/src/commands/sync/` 和 `src/windows/data-sync/`。

共享边界值规范化、比较与端点配对算法位于 `packages/migration-common/`；SQL 名称处理工具位于 `packages/driver-api/src/sql_identifiers.rs`。Data Sync 与 Data Transfer 直接依赖这些公共实现，Data Transfer 不依赖 Data Sync 领域包。Job 生命周期继续复用 `packages/runtime/src/job/`。

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

比较由 `dataSyncPrepare` Job 执行：Job adapter 以用户当前授权的 source/target session 校验端点，为任务创建专用会话，写入稳定端点引用并 dispatch 比较 handler。兼容的 `compare_data_sync` IPC 返回比较预览；Job 状态、详情和预览可按 `jobId` 查询。

相关代码：

- `packages/data-sync/src/keyset.rs`
- `src-tauri/src/commands/sync/keyset_source.rs`
- `packages/data-sync/src/compare.rs`

`jobId` 对应 AppDb 中的 Job 记录与取消意图，可取消长时间 compare。窗口关闭不会取消已接受的 Job；完整比较计划和行级审阅投影仍保存在当前进程。

## 3. Review / ChangeSet / Preview

用户在 Compare 阶段选择需要应用的行和 Insert/Update/Delete 选项。

`changeset.rs` 只保留：

- 用户勾选的变更；
- 当前 SyncOptions 允许的变更。

DELETE 默认不选。

Preview 生成 SQL 展示文本；真正 Execute 使用参数化 SQL，不把 Preview literal SQL 当作执行 SQL。

## 4. Execute

```text
reviewed ChangeSet + selectionRevision
  ↓
accept dataSyncApply + idempotent receipt + consume planId
  ↓
DesktopJobHost dispatches handler on job-owned sessions
  ↓
batch transaction / checkpoints / commit boundary
```

apply handler 重新核验计划版本、审阅选择、选项与端点身份。失败或取消回滚当前批次；先前已提交批次保留并记录在 Job 的 `effectOutcome`、进度、检查点和恢复证据中。Data Sync 使用专用执行通道，不经过普通 `execute_query` 的 Safe Mode 路径，避免正常的 UPDATE/DELETE 安全检查误伤已验证的 ChangeSet。

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
- `start_data_sync_prepare_job` / `start_data_sync_apply_job`
- `get_data_sync_job` / `get_data_sync_job_details` / `list_data_sync_jobs`
- `verify_data_sync_recovery`

## 6. 连接、会话与取消

P5 Job 请求以调用方当前授权的 source/target `dbSessionId` 校验所选端点；接受后运行时 session ID 不写入 Job。adapter 为每个 Job 建立专用 source/target 会话，并通过物理 endpoint identity 复核其与用户授权端点一致。任务完成、失败或派发失败后释放 Job-owned sessions；应用重启后不会复用旧 session 或重放 handler。

桌面 Job 持久化 connectionId、数据库/Schema 范围及受限的恢复目标标识；不持久化 `dbSessionId`、ChangeSet 行值或完整审阅计划。重启后用户可读取 Job 状态并对应用 Job 做显式只读恢复核验；Data Sync 的副作用需要重新比较，核验不会续跑或重放任务。取消通过 `cancel_data_sync` 写入共享 host 的持久 cancel intent，接受前到达的取消意图也会传递到 Job record。

Data Sync 与另两个成员的边界：**Data Synchronization 只做同族、且结构与主键完全一致的复制**；结构差异与 DDL 生成属于 Schema Diff，异构数据搬迁属于 Data Transfer。写任务时不要把三者混成一种「迁移」。

## 7. Frontend

`DataSyncWindow.tsx` 使用 6 步流程：

```text
Endpoints → Setup → Objects → Compare → Preview → Result
```

窗口继续承载端点选择、Compare / Preview / Execute 与行级审阅交互。Job 状态可通过 `jobId` 查询并在重新打开窗口后重附着；计划正文、行值和审阅选择仍为当前进程内状态，重启后需要重新比较。

## 8. Tests

- Rust：`packages/data-sync/**`、`src-tauri/src/commands/sync/tests.rs`
- Frontend：`src/windows/data-sync/__tests__/`
- E2E：`e2e/specs/data-sync-*.ts`

Driver 方言/类型规则属于对应 Driver，不在 Host 重复实现。
