# Data Transfer 架构

> 当前实现说明；P5 本机持久化 Job Core 于 2026-10-08 接入。Source of truth：`packages/data-transfer/`、`src-tauri/src/commands/data_transfer/`、`packages/runtime/src/job/` 与 `src-tauri/src/store/app_db/jobs/`。Core 与尚未接入的领域目标见[迁移任务详细设计](../platform/data-migration-jobs.md)。

## 1. 职责与执行边界

Data Transfer 搬运结构和数据，支持异构数据库、表列映射，以及 SQL 文件目的地。它不承担 Schema Diff 的差异 DAG，也不使用 Data Sync 的同行 ChangeSet。三者可以共用驱动与类型模型，但执行入口、检查和恢复证据分别成立。

前端位于 `src/windows/data-transfer/`，IPC 封装位于 `src/commands/transfer.ts`。后端由三层组成：

| 层 | 目录 | 职责 |
| --- | --- | --- |
| IPC 与计划管理 | `commands/data_transfer/` | inspect、preview、执行、取消、计划认领与恢复请求核验 |
| 迁移执行 | `packages/data-transfer/src/` | 参数检查、映射、筛选、读取、分批写入、文件输出与 checkpoint |
| 方言与类型转换 | `packages/data-transfer/src/transfer/` + Driver API | adapter 注册、IR 类型、源值读取、目标类型与 DDL 渲染 |

`transfer/mod.rs` 复用 Driver API 的 `IRColumn`、`IRDefault`、`IRTable`、`IRType`、`SyncSourceAdapter`、`SyncTargetAdapter`；Host 编排迁移，不复制驱动专属 DDL。

## 2. 请求模型

`data_transfer/model.rs` 的 `TransferJob` 包含 source、可选 target、可选 sqlFileTarget、mode、writeMode、tables 与 options。当前端点是运行时 `dbSessionId` + database + 可选 schema；它不是可跨进程恢复的持久化端点。保存的迁移 profile 则使用 sourceConnectionId / targetConnectionId 与 database/schema，不保存运行时会话。

| 字段 | 当前约束 |
| --- | --- |
| mode | Structure、Data、StructureAndData；默认 Data |
| 表映射 | sourceTable / targetTable、enabled、createNew、columnMappings |
| DDL 覆盖 | ddlOverride 由预览与执行路径核验，不改变源数据读取范围 |
| 源范围 | sourceFilter 与结构化 recordset 边界；边界由类型化 tuple 值和 inclusive 标志表达 |
| batchSize | 默认 500，合法范围 1～500；执行时还受目标参数数量限制 |
| 破坏性确认 | confirmedDestructive 必须随执行请求显式提供 |
| SQL 文件 | 受控 fileToken，编码及压缩选项；目的数据库类型用于 SQL 渲染 |

SQL 文件支持 UTF-8、UTF-8 BOM、UTF-16 LE/BE，以及 none/gzip 压缩。目的地 database/schema 按单个标识符验证；客户端提供的任意服务器文件路径不是输出授权。

## 3. Inspect、Preview 与冻结计划

```text
端点与对象选择
  → inspect / classify
  → 表列映射与类型核验
  → preview / 冻结计划
  → 用户确认
  → claim plan
  → 执行 / Result
```

`commands/data_transfer/plans.rs` 的 `TransferPlanStore` 使用进程内 Mutex + HashMap 保存计划与恢复信息。计划冻结原始 job、驱动协议、结构/范围/映射指纹、预期 DDL 和目标只读状态，并带有效期。TransferPlanStore 本身不持久化。

peek 不消耗计划；claim 原子将 available 转为 executing 并建立活动执行占用。计划消费后不因失败重新变为 available，避免未知提交结果触发重复执行。执行请求引用 planId，selection 只允许缩小 sourceTables 集合；运行选项只开放破坏性确认，不允许客户端替换冻结映射与 SQL。

公共 Job 生命周期由 `AppState.desktop_job_host` 持有的 `DesktopJobHost` 与 AppDb SQLite 仓储管理。prepare 命令创建并执行 `dataTransferPrepare` Job，完成后返回终态及审阅数据；apply 命令创建新的 `dataTransferApply` Job，在 Job、幂等 receipt 和 consumed plan ID 原子提交后立即返回 queued jobId，并将 handler future 放入 Tauri 后台任务。`get_job`、`list_jobs`、`get_transfer_job_details` 可读取状态；`cancel_data_transfer` 记录 Job cancel intent 并触发 runtime watcher。幂等重放返回原 Job，不重复执行。

持久 Job 不会使计划变成持久：plan body、resume token 和 live handler/endpoints 仍在进程内。进程重启后不恢复旧 session/worker，也不自动 apply；Queued admission 转为 Failed/NotStarted + `notExecuted`，Running 转为 Failed/Unknown + `pendingVerification`。需要重新 prepare 或由新授权的只读 verifier 按 Job details/checkpoint 明确核验。

## 4. 数据库执行

`data_transfer/execute/dispatcher.rs` 解析固定源/目标驱动，先核验源结构、有效列映射、对象限定名与同端点冲突，再执行写入。读取范围来自冻结的结构化条件；标识符通过驱动 quote，参数 placeholder 与参数上限由目标能力决定。

目标参数限制会进一步缩小有效 batch：有效列越多，每批行数越小。列映射为空、结构漂移、破坏性操作未确认等情况在写入前拒绝。目标 DDL 与数据插入不是天然的一个事务；不能把数据批次 rollback 描述成撤销全部结构变更。

跨方言的数据流为：

```text
源对象 / 源类型
  → Source adapter
  → IR 类型与类型化值
  → 映射及转换
  → Target adapter
  → 目标 DDL / 参数化批量写入
```

结构类型、默认值与值转换相关实现分别位于 `transfer/full_types.rs`、`transfer/ddl.rs` 和 adapter。类型转换的驱动测试放在对应 `packages/drivers/<id>/`，Host 测试只验证编排和公共契约。

## 5. 分批恢复

`data_transfer/resume.rs` 的 `TransferResumeCheckpoint` 定义 renew、prepareTable、advanceTable、token、invalidate 等旧恢复操作。恢复证据包括稳定 key 列、批大小、源指纹、已提交批次和 cursor；单纯保存行数或 OFFSET 不足以恢复。共享 JobRuntime 另将运行中 handler 提交的 checkpoint 与 CommitBoundary 写入 AppDb，并由 `get_transfer_job_details` 原样读回。

旧 resume helper 的可恢复路径要求完整且非 NULL 的稳定主键、可核验的一致性条件与目标事务能力；源与目标运行时会话须分离。实现对已验证的 PostgreSQL/MySQL 路径作显式检查，不应据此声称所有已注册驱动都支持恢复。该 helper 不重建旧 Job 的运行时句柄，也不授权重放；检查点失效、结构/范围变化、无法证明源一致性时拒绝续跑。

持久化边界和 checkpoint 供 UI 在窗口/进程重开后显示，但 Core 不替 Transfer handler 验证目标数据库事实。异步恢复 verifier API 需要调用方提供新授权的只读资源；当前 Transfer 没有自动跨进程续跑能力。必须按代码中已保存的证据核验，不能仅凭“上一批没有 checkpoint”就认定未提交。通用 API 与约束见 [P5 详细设计](../platform/data-migration-jobs.md)。

## 6. SQL 文件与取消

`data_transfer/sql_file.rs` 负责 SQL 文件输出；SQL 文件目的地不建立目标数据库写会话，但仍需源读取与渲染能力。文件输出失败或取消不能标成完整产物；结果必须表达已输出部分和错误。

旧执行与兼容路径仍由 `commands/data_transfer/exec.rs` 等入口管理；P5 新路径在 `commands/data_transfer/job_api/`。新路径的 cancel intent 持久写入本机 Job repository，运行时通过 `CancelWatch` 将它传给当前 stage。该 SQLite repository 是单机桌面 host，不是团队服务跨实例协调器。Job 只存 Artifact ID 引用，不存文件字节；当前引用 TTL 为 30 天，且不保证仍可下载对应内容。

## 7. 相关文档

- [功能使用](../../features/data-transfer-guide.zh-CN.md)
- [Schema Diff](schema-diff.md) / [Data Sync](data-sync.md)
- [驱动架构](drivers.md)
- [迁移任务平台化目标设计](../platform/data-migration-jobs.md)
