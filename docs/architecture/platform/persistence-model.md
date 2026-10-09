# DataZen 持久化模型、可落盘白名单与 Schema 迁移详细设计

> 状态：本文 §3/§5 面向 P7 团队服务 PostgreSQL 管理库，仍属目标设计；§4.1 描述桌面文件与 SQLite 的当前事实，其中 `{appData}/datazen.sqlite` Job 表已在 P5 Core 升为 schema v2。本文其他服务端 schema/迁移内容不代表已经实现。
> 读者：负责《分阶段开发计划》P1（抽取共享应用边界与持久化白名单）和 P7（服务端 ProfileRepository / JobRepository / Audit / ArtifactStore 与 schema migration）的实现者。本文定义**存什么、存在哪、谁来写、怎么迁**；DTO 形状、会话状态机、错误码取值以[连接管理详细设计](connection-management.md)为权威。
> 配套：[系统概要](system-overview.md)、[连接管理详细设计](connection-management.md)、[共享应用边界与端口详细设计](shared-boundaries-and-ports.md)、[团队 Web 服务详细设计](team-server-and-auth.md)、[分阶段开发计划](../../development/platform-development-plan.md)。
> 现状盘点来源：[持久化存储（现状）](../backend/store.md)、[`src-tauri/src/store/`](../../../src-tauri/src/store/mod.rs)。这些是**已实现事实**；本文其余章节为目标设计。

## 1. 文档定位与状态

| 本文负责 | 权威文档（不重复定义） |
| --- | --- |
| 可落盘白名单与禁持久化清单 | [连接 §4.4 可落盘来源与运行时绑定](connection-management.md#44-可落盘来源与运行时绑定) 给出原则，本文给出可执行的字段级清单与落库断言 |
| `DurableExecutionRecord` 的字段集合 | [连接 §4 DTO](connection-management.md#4-dto-与字段定义) 的 `ResultProvenance` / `StatementResultSource` / `ExecutionView` 形状，本文只定义其持久化投影 |
| 仓储 port 签名 | [共享边界 §4.3 持久化与授权端口](shared-boundaries-and-ports.md#43-持久化与授权端口) |
| 服务端进程形态、中间件、认证与 CSRF | [团队服务 §3–§9](team-server-and-auth.md#3-请求中间件链) |
| 登录会话行与授权数据的**表结构** | 字段语义见 [团队服务 §4.3](team-server-and-auth.md#43-服务端登录会话)、[§6.1](team-server-and-auth.md#61-角色与资源-acl)；本文 §3.10 给 DDL / CHECK / 索引 / 保留策略 |
| 迁移的**运行方式**（何时执行、失败是否启动） | [团队服务 §13 Schema migration 运行方式](team-server-and-auth.md#13-schema-migration-运行方式)；本文定义**版本策略、文件格式、expand/contract 判据与回滚边界** |
| 会话、lease、预算的行为契约 | [连接 §6 状态机与并发](connection-management.md#6-状态机与并发)、[§9 连接池、预算与清理](connection-management.md#9-连接池预算与清理) |
| 阶段交付顺序与门槛 | [开发计划 §5 P1](../../development/platform-development-plan.md#5-p1抽取共享应用边界和前端传输契约)、[§11 P7](../../development/platform-development-plan.md#11-p7单实例团队-web-服务) |

三条本文反复使用的前提：

1. `connectionId` 是可落盘的配置标识，`dbSessionId` 是进程内标识。落盘模型里只有前者。
2. 物理资源（socket、事务、游标、服务端预处理对象）随进程终止而消失。任何"从旧库恢复会话"的说法都违反[连接 §12](connection-management.md#12-web多实例权限与结果)。
3. 落盘的是**执行结果的事实与来源**，不是**继续执行的资格**。恢复出来的东西一律要重新授权、重新解析、重新验证。

## 2. 持久化白名单与禁持久化清单

### 2.1 白名单：允许落盘

| # | 可落盘对象 | 落盘载体 | 关键字段 | 依据 |
| --- | --- | --- | --- | --- |
| W1 | 执行来源 `ResultProvenance` | `durable_executions.provenance` | `organizationId`、`principalId`、`connectionId`、`configRevision`、`contextBefore`、`contextAfter`、`requestedTarget`、`capabilitySnapshot`、`executedAt` | [连接 §4.4](connection-management.md#44-可落盘来源与运行时绑定) 明示"结果来源可落盘" |
| W2 | 语句级来源 `StatementResultSource[]` | `durable_executions.statement_sources` | `executionId`、`statementIndex`、`context`、`relation`、`writableMapping` | 同上；`writableMapping` 落盘后用于写回前的映射判定 |
| W3 | 执行产物标识 `artifactIds[]` | `durable_executions.artifact_ids` + `artifact_metadata` | `artifactId` 列表、字节数、内容摘要、TTL | [概要 §5 统一领域对象](system-overview.md#5-统一领域对象) 中 Artifact 持久化 |
| W4 | 能力快照 `CapabilitySnapshot` | `durable_executions.provenance.capabilitySnapshot` | `driverId`、`driverVersion`、`protocolVersion`、`capabilityRevision`、`confirmed` | 审计需要"当时的能力"，不含凭据 |
| W5 | 任务计划 | `jobs.plan` / `jobs.plan_fingerprint` | 冻结的目标、`configRevision` / `credentialRevision` / 能力版本、对象结构指纹、映射、事务要求 | [连接 §10.1](connection-management.md#101-公共处理) 计划冻结 |
| W6 | 任务检查点 | `job_checkpoints` | 稳定键、已确认提交边界、源一致性证据、映射指纹、恢复策略 | [连接 §10.4](connection-management.md#104-data-transfer)；不能只存 offset |
| W7 | 任务状态与阶段 | `jobs` / `durable_executions` | `state`、`effectOutcome`、`stage`、`stateVersion`、时间戳 | [概要 §6.2](system-overview.md#62-backendclient-与领域客户端) `JobState` / `JobView` |
| W8 | 连接配置（非敏感部分） | `connections` | `connectionId`、`driverId`、`configRevision`、`credentialRevision`、`initialNamespace`、`publicOptions`、时间戳 | [连接 §4.1](connection-management.md#41-服务接口与补充响应) `ProfileView` |
| W9 | 幂等请求摘要 | `idempotency_records.request_digest` | `operation`、摘要、算法、签发与过期时间、`state` | [连接 §13.1](connection-management.md#131-幂等键期限与响应分类) |
| W10 | 幂等回执的**持久化投影** | `idempotency_records.receipt_projection` | `executionId` / `jobId`、稳定目标、owner 标识、终态 | [连接 §13](connection-management.md#13-错误重试和事件) 幂等记录原子保存 |
| W11 | 结果完整性 | `durable_executions.result_completeness` / `truncation_reason` | `pending` \| `complete` \| `truncated`、截断原因 | [连接 §7.7](connection-management.md#77-结果订阅与放弃消费) |
| W12 | 额度计数 | `quota_counters` | scope、已用、已预留、限额、窗口 | [概要 §6.4](system-overview.md#64-repositories-与环境-ports) `BudgetCoordinator` |
| W13 | 审计事件的白名单投影 | `audit_events` | 见 §3.7 | [连接 §16.7 CM-72](connection-management.md#167-补充契约与边界用例) 审计含非敏感能力版本，不含 live handle 与凭据 |
| W14 | 服务端登录会话行（服务端行，**不是**自包含令牌） | `login_sessions` | `session_id`、`organization_id`/`principal_id`、`auth_time`、`issued_at`/`absolute_expires_at`/`idle_expires_at`、`reauth_required`、`permission_version`、`client_binding`、`token_epoch`、`revoked_at`/`revoke_reason` | 字段语义见 [团队服务 §4.3](team-server-and-auth.md#43-服务端登录会话)；DDL 见 §3.10；**不保存** IdP 令牌副本（F14） |
| W15 | 授权数据：成员关系 / 固定角色 / 资源 ACL / 组织权限版本 | `memberships` / `acl_entries` / `permission_versions` | `principalId → roleId` 基线、6 个固定角色（以 CHECK 表达，不建表）、逐资源逐 action 的 grant/deny、单调 `permission_version`、IdP `sub` 映射 | [团队服务 §6.1](team-server-and-auth.md#61-角色与资源-acl)、[§6.2](team-server-and-auth.md#62-权限版本与-policyisolationkey)；DDL 见 §3.10 |

### 2.2 禁持久化清单

| # | 禁止落盘 | 出现过的载体 | 处置 |
| --- | --- | --- | --- |
| F1 | `dbSessionId` | `SyncTask.source_db_session_id` / `target_db_session_id`（[`store/models.rs`](../../../src-tauri/src/store/models.rs)） | 读取旧 JSON 时可读，落库前由 `normalize_legacy_state()` 清空，序列化时 `skip_serializing` |
| F2 | `SessionHandle{dbSessionId, runtimeEpoch}` | `SessionView.handle`、`OpenSessionReceipt` | 只在 runtime 内存；不得进入任何 repository 入参 |
| F3 | `RuntimeResultBinding{handle, resourceBindingId, executionId}` | `ExecutionView.runtimeBinding` | 仓储使用独立的 `DurableExecutionRecord`，**不序列化 `ExecutionView`** |
| F4 | `EventEnvelope.sessionHandle` | 事件流、SSE、审计 | 归档只写显式白名单投影（§2.4） |
| F5 | 取消句柄 `cancelHandle` | `ExecutionRecord` 内部 | runtime 内存；终态即失效 |
| F6 | `ResourceLease` / `leaseId` / 归池元数据 | pool 内部结构 | 内存；worker 终止后预算从零重启 |
| F7 | 游标、事务、`SessionHandleRef`（[连接 §6.5](connection-management.md#65-会话级资源句柄登记)） | 会话 actor 登记表 | 内存；恢复流程不得重建句柄 |
| F8 | `attachmentToken` 及其哈希 | `OpenSessionReceipt.attachmentToken` | 哈希只在 owner 内存做去重，**明文与哈希都不落盘** |
| F9 | 隧道/网络路由句柄 | `NetworkProvider` 内部 | 内存；`networkRouteRef` 与 `networkRouteRevision` 是配置标识，可落盘 |
| F10 | 秘密材料本身（密码、token、TLS 私钥、可解密值） | `connections.driverOptions` 的敏感部分 | 只落 `secretRef`；`ProfileView.publicOptions` 不返回秘密 |
| F11 | 用户绝对路径 | 文件选择、备份工具路径 | 改为 artifact/配置引用（[概要 §10](system-overview.md#10-安全和环境差异)） |
| F12 | `runtimeEpoch` | `SessionView`、内部记录 | **见下方说明** |
| F13 | `schemaCache` 内容与 `cacheRevision` | [`cache/schema_cache.rs`](../../../src-tauri/src/cache/schema_cache.rs) | 纯内存缓存，不落盘 |
| F14 | IdP `access_token` / `refresh_token` / 原始 `id_token` 及其可重放副本 | 登录会话行 | 只落服务端生成的会话行与 IdP `sub` 映射（§3.10）；需要调用 IdP 后端接口时按需重新交换（[团队服务 §4.3](team-server-and-auth.md#43-服务端登录会话)） |

关于 F12：`runtimeEpoch` 是"某次 worker 启动的代号"，既不含秘密也确实是数字，但落盘后极易被误读为"可以接续原绑定"的证明。[连接管理 §4.4](connection-management.md#44-可落盘来源与运行时绑定)（不是本文 §4.4「主密钥与团队秘密」）已明确 `configRevision` / `contextRevision` 不能证明物理资源连续性，`runtimeEpoch` 同理。它只用于 owner 校验（[连接 §13.1](connection-management.md#131-幂等键期限与响应分类)），因此不进入任何表列。

关于 F8 的理由：不可逆哈希降低了泄露后果，但没有改变"它是可重放证明"这一性质。令牌一旦落盘，恢复出来的进程可能用旧哈希匹配新请求，绕过"令牌必须绑定 owner `runtimeEpoch`"的约束。

### 2.3 两层 DTO 与投影规则

```text
ExecutionView（对外实时视图，含 runtimeBinding）
        │  ① 仓储只接受 DurableExecutionRecord（类型层面拒绝 View）
        ▼
DurableExecutionRecord（落盘投影）
   executionId, organizationId, principalId, connectionId, configRevision, contextRevision,
   requestedTarget, state, effectOutcome, provenance, statementSources, artifactIds, resultCompleteness, truncationReason, errorCode, startedAt, finishedAt
        │  ② 反序列化后重建成 ExecutionView 时 runtimeBinding = null
        ▼
读回路径：ExecutionView { ..., runtimeBinding: null }   // 客户端据此知道「需要重新建立」
```

- 规则一：写入路径的类型是 `DurableExecutionRecord`，不是 `ExecutionView`。序列化器即使被绕过也拿不到 `runtimeBinding`，因为字段根本不在类型里。
- 规则二：读回路径产出的 `ExecutionView.runtimeBinding` 恒为 `null`（或显式的 `bindingLost`），客户端不得用 `null` 推断"还没执行"。
- 规则三：`statementSources[].context` 与 `relation` 必须先按 [连接 §4.3](connection-management.md#43-命名空间规范化契约) 的 `CanonicalTarget` 归一化后再落库，**不得**使用文件系统路径别名形式（CM-62）。
- 规则四：`statement_sources` 单条执行最多保留 1024 条（**本文建议值，来自部署配置**；[连接 §4.4](connection-management.md#44-可落盘来源与运行时绑定) 与 [§9](connection-management.md#9-连接池预算与清理) 都没有给出该上限，因此落地前不得当作权威值引用）；超出时截断并写审计事件（这是投影裁剪，**不是**结果截断，不改写 `result_completeness`）。

### 2.4 事件归档的白名单投影

事件归档**永远不序列化 `EventEnvelope`**。归档写入 `audit_events` 的映射是逐字段固定的：

| `EventEnvelope` / payload 字段 | 归档列 | 规则 |
| --- | --- | --- |
| `streamId`、`sequence` | `audit_events.request_id`（附 `payload.streamRef`） | `sequence` 用 `BIGINT`；传输层不得转成 JS `Number`（[连接 §13](connection-management.md#13-错误重试和事件)） |
| `executionId` / `jobId` | `subject_id` | 二选一，另一列为 NULL |
| `contextRevision` | `payload.contextRevision` | 审计字段，不参与绑定判断 |
| `payload` 自由内容 | **不归档** | 只允许映射到下表已列字段 |
| `sessionHandle` | **禁止** | F4 |
| 能力版本（driverId/version/protocolVersion/capabilityRevision） | `payload.capability` | CM-72 要求"审计包含非敏感能力版本" |
| 目标投影（`connectionId` + 归一化 namespace，不含路径） | `target_projection` | |
| 凭据、SQL 原文、参数、原始数据库错误 | 默认**不归档** | 是否保留由审计策略控制；默认不打印密码、令牌、原始数据库错误与用户绝对路径（[概要 §10](system-overview.md#10-安全和环境差异)） |
| 终态与错误码 | `outcome` / `error_code` | `errorCode` 取 [连接 §4](connection-management.md#4-dto-与字段定义) 的 `ExecutionErrorCode` 枚举 |

`audit_events.payload` 使用 JSONB 承载上述**已枚举**的白名单字段，键名固定在代码常量中；写入前由一个 `AuditProjection` 函数构造，构造函数的单测对键集合做相等断言——新增键必须显式改测试，而不是靠"对象序列化"顺带落库。

### 2.5 落库断言（CM-61 的可执行形式）

CM-61（[连接 §16.7](connection-management.md#167-补充契约与边界用例)）要求"落盘结构无 `dbSessionId` / `SessionHandle` / `resourceBindingId` / lease / cancel 句柄"。落地为三条测试：

1. **列名断言**：对每张表读取 `information_schema.columns`，断言列名不包含 `db_session_id`、`session_handle`、`resource_binding_id`、`lease_id`、`cancel_handle`、`attachment_token`、`credential`、`password`。仓库已有同形先例：[`store/history_db/migration_run.rs`](../../../src-tauri/src/store/history_db/migration_run.rs) 的测试遍历 `PRAGMA table_info(migration_run_history)`，断言不含 `sql`、`payload`、`credentials`、`db_session_id`、`file_token`。
2. **序列化断言**：写入一条完整 `ExecutionView`（含 `runtimeBinding`）后回读 `DurableExecutionRecord`，断言结果 JSON 不含上述键名；同形先例是 [`store/tests/migration_tests.rs`](../../../src-tauri/src/store/tests/migration_tests.rs) 的 `!persisted.contains("dbSessionId")`。
3. **JSONB 键断言**：对 `provenance`、`statement_sources`、`receipt_projection` 展开后逐层检查键名，命中禁持久化清单中任一项即失败。

## 3. 服务端元数据库表结构

### 3.1 选型与边界

- 服务端管理库使用 **PostgreSQL**（[概要 §6.4](system-overview.md#64-repositories-与环境-ports) 推荐"第一版用 PostgreSQL 管理组织、配置版本、任务、审计和额度"），**不与用户业务数据库混用**，迁移脚本也绝不触碰用户业务库。
- 桌面本地持久化**不是**这套 schema 的另一个实例。本地仍是文件 + SQLite（§4.1），两者的字段同构关系见 §4.2，但存储引擎、加密方式和一致性语义不同。
- 所有表都带 `organization_id`，且**复合主键一律以 `organization_id` 打头**：越组织访问在存储层就是查不到，而不是靠应用层记得加过滤。
- 时间列统一 `TIMESTAMPTZ`（UTC）。所有 TTL 的**起点**用服务端单调时钟保存（[连接 §6.4](connection-management.md#64-attachment-与超期处理)），`expires_at` 只是最早适用期限的 UTC 投影。
- v1 **不建** `organizations` / `principals` 主数据表：`organization_id` / `principal_id` 由认证适配器按 `RequestContext` 提供，`memberships` 是主体（principal）在管理库中的唯一登记点；组织开通走部署配置导入，不在运行期自助创建。因此 §3 的 `organization_id` 列没有可指向的组织表外键，越组织隔离由「主键以 `organization_id` 打头」保证，而非外键。
- 本节只定义**表结构**。`ApiError.code` 的取值全集**不在本文**：[连接 §13](connection-management.md#13-错误重试和事件) 是权威表；[团队服务 §9.2](team-server-and-auth.md#92-apierror--http-映射表) 列出的 `Unauthenticated` / `NotFound` / `PayloadTooLarge` / `QuotaExceeded` / `RateLimited` / `ServiceUnavailable` / `ConfigRevisionMismatch` 这 7 个 code **已由 §13 正式登记**，团队服务 §9.2 只是它们在 HTTP 层的落地映射，不新增也不改写 code 的语义。

### 3.2 表清单

目标表分两批：P7 的 13 张管理表（§3.3–§3.10），以及 P9 增加的 `budget_allocations`（§3.11）。顺序遵循外键依赖；它们是目标 schema，不表示当前已建表。

| 表 | 对应 port / 用途 | 主键 |
| --- | --- | --- |
| `connections` | `ProfileRepository` | `(organization_id, connection_id)` |
| `durable_executions` | 执行记录（`DurableExecutionRecord` 投影） | `(organization_id, execution_id)` |
| `idempotency_records` | 幂等请求摘要与回执投影 | `(organization_id, operation, idempotency_key)` |
| `jobs` | `JobRepository` 主记录 | `(organization_id, job_id)` |
| `job_checkpoints` | `JobRepository.save_checkpoint` | `(organization_id, job_id, checkpoint_version)` |
| `audit_events` | append-only 审计 | `(organization_id, event_id)` |
| `artifact_metadata` | `ArtifactStore` 元数据（字节在对象存储/受控目录） | `(organization_id, artifact_id)` |
| `quota_counters` | `BudgetCoordinator` 的持久额度部分 | `(organization_id, scope, scope_key, resource_kind)` |
| `schema_migration_history` | 迁移运行器自身状态 | `(version)` |
| `memberships` | 主体 → 角色基线（§3.10，W15） | `(organization_id, principal_id)` |
| `login_sessions` | 服务端登录会话行（§3.10，W14） | `(organization_id, session_id)` |
| `acl_entries` | 逐资源逐 action 的 grant/deny（§3.10，W15） | `(organization_id, resource_type, resource_id, subject_type, subject_id, action)` |
| `permission_versions` | 组织级单调权限版本（§3.10，W15） | `(organization_id)` |
| `budget_allocations`（P9） | 跨节点配额分配与保守核销，不是物理资源账 | `(organization_id, allocation_id)` |

**不存在的表**（显式声明，防止后续实现"顺手加一个"）：

| 不存在 | 原因 |
| --- | --- |
| `sessions` / `session_directory` | 会话目录禁止磁盘持久化、快照和 append-only 日志（[连接 §12](connection-management.md#12-web多实例权限与结果)）；丢失目录只会使会话明确失效，不据此恢复物理会话 |
| `session_tombstone` | `closeSession` 的 tombstone 只在内存保留默认 24 小时（[连接 §13.1](connection-management.md#131-幂等键期限与响应分类)） |
| `attachment_tokens` | attachment 令牌及其哈希只在 owner 内存；服务端不保存可重放的会话凭据 |
| `idp_tokens` / `refresh_tokens` | 登录会话行不是 IdP 令牌副本（F14）；需要后端调用时按需重新交换 |
| `resource_leases` / `pool_state` / `budget_runtime` | 物理占用与运行时额度随进程终止；进程重启后额度从零重新开始是单实例形态下的自洽前提（[团队服务 §10.3](team-server-and-auth.md#103-sessiondirectory-与-budgetcoordinator单实例)） |
| `cursors` / `transactions` | 会话级句柄只存在于内存 actor |
| `credentials` | 只存 `secret_ref`；`SecretProvider` 在执行节点解析 |
| `schema_cache` | 元数据缓存是进程内缓存，且旧缓存不得回填（CM-67） |

### 3.3 `connections`

```sql
CREATE TABLE connections (
    organization_id      TEXT        NOT NULL,
    connection_id        TEXT        NOT NULL,
    name                 TEXT        NOT NULL,
    driver_id            TEXT        NOT NULL,
    config_revision      BIGINT      NOT NULL DEFAULT 1,
    credential_revision  BIGINT      NOT NULL DEFAULT 1,
    initial_namespace    JSONB       NOT NULL,
    public_options       JSONB       NOT NULL DEFAULT '{}'::jsonb,
    driver_options       JSONB       NOT NULL DEFAULT '{}'::jsonb,
    secret_ref           TEXT,
    read_only            BOOLEAN     NOT NULL DEFAULT FALSE,
    enabled              BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, connection_id),
    CONSTRAINT ck_connections_config_revision    CHECK (config_revision     >= 1),
    CONSTRAINT ck_connections_credential_revision CHECK (credential_revision >= 1),
    CONSTRAINT ck_connections_options_object      CHECK (jsonb_typeof(driver_options) = 'object')
);

CREATE INDEX idx_connections_list
    ON connections (organization_id, enabled, updated_at DESC, connection_id);
```

| 字段 | 规则 |
| --- | --- |
| `connection_id` | 服务端分配；不是 `dbSessionId`，可安全落盘 |
| `config_revision` | 每次 `compare_and_set` 成功 +1；参与 PoolKey（[连接 §9.6](connection-management.md#96-poolkey版本与缓存的生产者)） |
| `credential_revision` | 只由 `advance_credential_revision` 递增；不使用密码散列作为版本 |
| `driver_options` | 按 driver schema 分离敏感字段；**敏感部分不落本表**，只写 `secret_ref` |
| `initial_namespace` | 已按 `CanonicalTarget` 归一化 |
| `name` | **不建唯一约束**：同组织允许同名 profile，列表按 `(updated_at DESC, connection_id)` 稳定排序，UI 靠 id 区分 |
| 缺失列 | 服务端另有 `policy_ref`、`policy_ref_revision`、`network_route_ref`、`network_route_revision`（见 §4.3；`policy_ref_revision` 是 [团队服务 §6.2](team-server-and-auth.md#62-权限版本与-policyisolationkey) 的 `policyIsolationKey` 输入之一），P7 首个迁移版本加入 |

### 3.4 `durable_executions`

```sql
CREATE TABLE durable_executions (
    organization_id       TEXT        NOT NULL,
    execution_id          TEXT        NOT NULL,
    connection_id         TEXT        NOT NULL,
    config_revision       BIGINT      NOT NULL,
    context_revision      BIGINT      NOT NULL,
    state                 TEXT        NOT NULL,
    effect_outcome        TEXT        NOT NULL,
    requested_target      JSONB       NOT NULL,
    provenance            JSONB       NOT NULL,
    statement_sources     JSONB       NOT NULL DEFAULT '[]'::jsonb,
    artifact_ids          JSONB       NOT NULL DEFAULT '[]'::jsonb,
    result_completeness   TEXT        NOT NULL DEFAULT 'pending',
    truncation_reason     TEXT,
    error_code            TEXT,
    owner_principal_id    TEXT        NOT NULL,
    client_instance_id    TEXT,
    started_at            TIMESTAMPTZ NOT NULL,
    finished_at           TIMESTAMPTZ,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, execution_id),
    FOREIGN KEY (organization_id, connection_id)
        REFERENCES connections (organization_id, connection_id),
    CONSTRAINT ck_exec_state     CHECK (state    IN ('queued','running','cancelRequested',
                                                    'succeeded','failed','cancelled')),
    CONSTRAINT ck_exec_outcome   CHECK (effect_outcome IN ('notStarted','completed','rolledBack',
                                                           'partiallyApplied','unknown')),
    CONSTRAINT ck_exec_complete  CHECK (result_completeness IN ('pending','complete','truncated')),
    CONSTRAINT ck_exec_trunc     CHECK ((result_completeness = 'truncated'
                                         AND truncation_reason IS NOT NULL)
                                     OR (result_completeness <> 'truncated'
                                         AND truncation_reason IS NULL)),
    CONSTRAINT ck_exec_finished  CHECK ((state IN ('succeeded','failed','cancelled'))
                                        = (finished_at IS NOT NULL))
);

CREATE INDEX idx_executions_job_scope
    ON durable_executions (organization_id, connection_id, started_at DESC);
CREATE INDEX idx_executions_open
    ON durable_executions (organization_id, state)
    WHERE state IN ('queued', 'running', 'cancelRequested');
```

枚举取值全部来自[连接 §4](connection-management.md#4-dto-与字段定义) 的 `ExecutionState` / `EffectOutcome` / `resultCompleteness`，本文不新增取值。`ck_exec_trunc` 与 `ck_exec_finished` 是两条强不变量：截断必须有原因，终态必须有结束时间。两条 CHECK 都写成等价式（`<A> = <B>`），所以方向也是双向的——`truncation_reason` 非空就只能是 `truncated`，`finished_at` 非空就只能是终态，不存在"有原因但完整"或"有结束时间但仍在 running"的行。注意两条 CHECK 的绑定对象不同：`ck_exec_trunc` 绑 `result_completeness`，`ck_exec_finished` 只绑 `state`（终态行 `finished_at` 非空、非终态行为 NULL），**不绑** `result_completeness`，因此 `succeeded` + `result_completeness = 'pending'` 不会被它拒绝，这一组合由应用层写入路径保证不出现（见 §8.3）。

**没有的列**（对应 §2.2）：`db_session_id`、`session_handle`、`resource_binding_id`、`runtime_epoch`、`cancel_handle`、`lease_id`。

`effect_outcome` **不由"请求成功"覆盖**（CM-72）：`state='failed'` 且用户主动取消时，`effect_outcome` 仍可能是 `completed`。

### 3.5 `idempotency_records`

```sql
CREATE TABLE idempotency_records (
    organization_id   TEXT        NOT NULL,
    operation          TEXT        NOT NULL,
    idempotency_key    TEXT        NOT NULL,
    request_digest     TEXT        NOT NULL,
    digest_algorithm   TEXT        NOT NULL DEFAULT 'sha256',
    key_version        BIGINT,
    issued_at          TIMESTAMPTZ NOT NULL,
    expires_at         TIMESTAMPTZ NOT NULL,
    retained_until     TIMESTAMPTZ NOT NULL,
    state              TEXT        NOT NULL,
    subject_kind       TEXT        NOT NULL,
    subject_id         TEXT        NOT NULL,
    receipt_projection JSONB       NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, operation, idempotency_key),
    CONSTRAINT ck_idem_operation CHECK (operation IN ('createProfile','openSession',
        'executeInSession','executeAtTarget','setSessionContext','startJob')),
    CONSTRAINT ck_idem_state     CHECK (state IN ('accepted','dispatched','completed',
                                                  'failed','cancelled','outcomeUnknown')),
    CONSTRAINT ck_idem_subject   CHECK (subject_kind IN ('profile','execution','job')),
    CONSTRAINT ck_idem_digest    CHECK (digest_algorithm = 'sha256')
);

CREATE INDEX idx_idempotency_expiry ON idempotency_records (retained_until);
CREATE UNIQUE INDEX uq_idempotency_live_subject
    ON idempotency_records (organization_id, subject_id)
    WHERE subject_kind = 'execution' AND state IN ('accepted', 'dispatched', 'outcomeUnknown');
```

| 规则 | 内容 |
| --- | --- |
| 落库时点 | 接受记录必须在 SQL / Job 派发**之前**提交（[连接 §13](connection-management.md#13-错误重试和事件)） |
| 保留期 | 完整记录至少保留到 `expires_at + 24 小时`，即 `retained_until`；之后可按审计策略删除 |
| 删除后 | 过期签名令牌仍被拒绝；过期和删除后都**不再执行**（CM-70） |
| 冲突 | 同 key 同摘要 → 返回同 receipt；同 key 不同摘要 → `IdempotencyConflict` |
| 不落盘 | `openSession` / `setSessionContext` 的完整指纹、`SessionView`、`attachmentToken`、会话回执**只在 owner 内存**。本表只保存请求摘要与 `executionId`/`jobId` 等稳定投影 |
| `key_version` | 令牌签发用的密钥版本；未知 `keyVersion` 一律拒绝，签名密钥保留期 ≥ 已发令牌全部过期时间 |
| `uq_idempotency_live_subject` | 保证一次执行至多被一个未终结幂等记录引用，避免两个不同 key 的记录指向同一次执行 |

### 3.6 `jobs` 与 `job_checkpoints`

以下 PostgreSQL DDL 仍是服务端管理库目标 schema。桌面公共 Job Core 使用既有 `{appData}/datazen.sqlite`，其 SQLite v2 表与约束由 [`store/app_db.rs`](../../../src-tauri/src/store/app_db.rs) 定义，不直接复用本节 PostgreSQL DDL。

```sql
CREATE TABLE jobs (
    organization_id   TEXT        NOT NULL,
    job_id            TEXT        NOT NULL,
    kind              TEXT        NOT NULL,
    state             TEXT        NOT NULL,
    state_version     BIGINT      NOT NULL DEFAULT 1,
    stage             TEXT,
    connection_id     TEXT        NOT NULL,
    config_revision   BIGINT      NOT NULL,
    plan_fingerprint  TEXT        NOT NULL,
    plan              JSONB       NOT NULL,
    owner_principal_id TEXT       NOT NULL,
    delegation_id     TEXT,
    worker_id         TEXT,
    claim_expires_at  TIMESTAMPTZ,
    claim_generation  BIGINT      NOT NULL DEFAULT 0,
    cancel_requested_at TIMESTAMPTZ,
    execution_ids     JSONB       NOT NULL DEFAULT '[]'::jsonb,
    artifact_ids      JSONB       NOT NULL DEFAULT '[]'::jsonb,
    error_code        TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at       TIMESTAMPTZ,
    PRIMARY KEY (organization_id, job_id),
    FOREIGN KEY (organization_id, connection_id)
        REFERENCES connections (organization_id, connection_id),
    CONSTRAINT ck_jobs_state CHECK (state IN ('queued','running','succeeded','failed','cancelled')),
    CONSTRAINT ck_jobs_generation CHECK (claim_generation >= 0),
    CONSTRAINT ck_jobs_claim CHECK ((worker_id IS NULL) = (claim_expires_at IS NULL)),
    CONSTRAINT ck_jobs_claim_live CHECK (state <> 'running' OR worker_id IS NOT NULL)
);

CREATE INDEX idx_jobs_list     ON jobs (organization_id, created_at DESC, job_id);
-- P5 apply 计划消费：同一计划只能接受一个应用 Job；失败也不释放消费资格。
CREATE UNIQUE INDEX uq_jobs_apply_plan ON jobs
    (organization_id, (plan ->> 'consumedPlanId'))
    WHERE kind IN ('schemaDiffApply','dataSyncApply','dataTransferApply');
ALTER TABLE jobs ADD CONSTRAINT ck_jobs_apply_plan CHECK (
    kind NOT IN ('schemaDiffApply','dataSyncApply','dataTransferApply')
    OR COALESCE(length(plan ->> 'consumedPlanId'), 0) > 0
);

CREATE INDEX idx_jobs_claim    ON jobs (claim_expires_at) WHERE state = 'running';
CREATE INDEX idx_jobs_recover  ON jobs (organization_id, updated_at)
    WHERE state IN ('queued', 'running', 'cancelled');

CREATE TABLE job_checkpoints (
    organization_id        TEXT        NOT NULL,
    job_id                 TEXT        NOT NULL,
    checkpoint_version     BIGINT      NOT NULL,
    stage                  TEXT        NOT NULL,
    stable_keys            JSONB       NOT NULL,
    committed_boundary     JSONB       NOT NULL,
    verification_evidence  JSONB       NOT NULL,
    mapping_fingerprint    TEXT        NOT NULL,
    recovery_policy        TEXT        NOT NULL,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, job_id, checkpoint_version),
    FOREIGN KEY (organization_id, job_id) REFERENCES jobs (organization_id, job_id),
    CONSTRAINT ck_ckpt_policy CHECK (recovery_policy IN ('resumeAfterVerify',
                                                         'requireManualReview', 'abort'))
);
```

- P5 `jobs.plan.consumedPlanId` 保存 apply Job 消费的 planId，连同 planVersion、handlerVersion、checkpointVersion、selectionRevision、计划摘要与不可变 Artifact 引用一起冻结。上述唯一索引与非空 CHECK 在接受 Job/幂等记录的同一事务生效；不能仅在应用层查重。准备 Job 不写 consumedPlanId。输入参数不能覆盖权威计划。详情见 [迁移任务设计](data-migration-jobs.md)。
- `jobs.plan` 冻结计划（[连接 §10.1](connection-management.md#101-公共处理)）：目标、`configRevision` / `credentialRevision` / 能力版本、对象结构指纹、映射、事务要求。计划冻结后任何一项变化 → `PlanStale`。
- `jobs.worker_id` / `claim_expires_at` 是**调度租约**，不是 `ResourceLease`：它只记录"哪个 worker 负责推进这个 Job"和到期时间，**不含任何物理资源标识**，因此不违反 §2.2 的 F6。终态时必须置 NULL（由 `ck_jobs_claim` 保证成对）。`renew` 失败即视为失联，Job 停止新增资源并转人工核验，**不重新派发副作用阶段**。
- `job_checkpoints.stable_keys` 是稳定对象键（`connectionId` + 归一化对象标识 + 版本），**不是**字节 offset；`committed_boundary` 的 P5 目标元素包含 stageId、operationId/batchId、stableTarget、payloadDigest、真实提交 evidence 与 verifiedAt；不能只存目标结构指纹。它记录已确认提交的边界对象集合与核验时间；`verification_evidence` 记录源一致性证据（[连接 §10.4](connection-management.md#104-data-transfer)）。只存 offset 的检查点在设计上不合格。
- 检查点**不保存** live session、lease、cursor 或句柄；恢复流程不得重建会话级句柄，只允许以新 `dbSessionId` 显式重建（[连接 §6.5](connection-management.md#65-会话级资源句柄登记)）。任务生命周期独立于窗口：关闭编辑器只取消订阅，不取消 Job（INV-12，[连接 §3](connection-management.md#3-不可破坏的不变量)）。

### 3.7 `audit_events`

```sql
CREATE TABLE audit_events (
    organization_id   TEXT        NOT NULL,
    event_id          BIGINT      GENERATED ALWAYS AS IDENTITY,
    occurred_at       TIMESTAMPTZ NOT NULL,
    principal_id      TEXT,
    principal_kind    TEXT        NOT NULL,
    delegation_id     TEXT,
    client_instance_id TEXT,
    action            TEXT        NOT NULL,
    connection_id     TEXT,
    config_revision   BIGINT,
    target_projection JSONB,
    outcome           TEXT        NOT NULL,
    error_code        TEXT,
    request_id        TEXT,
    payload           JSONB       NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (organization_id, event_id),
    CONSTRAINT ck_audit_principal CHECK (principal_kind IN ('user','service','delegation')),
    CONSTRAINT ck_audit_outcome   CHECK (outcome IN ('succeeded','failed','denied','unknown'))
);

CREATE INDEX idx_audit_time  ON audit_events (organization_id, occurred_at DESC);
CREATE INDEX idx_audit_target ON audit_events (organization_id, connection_id, occurred_at DESC);
```

- **append-only 由权限保证**：应用角色只授予 `INSERT` 与 `SELECT`，不授予 `UPDATE` / `DELETE`（[团队服务 §10.1](team-server-and-auth.md#101-实现矩阵)）。保留期到期后由运维按时间分区整段卸载，不做逐行删除。
- `event_id` 是 `GENERATED ALWAYS AS IDENTITY` 的全局单调序列，仅用于排序与分页；**它不是** `EventEnvelope.sequence`。每条执行事件的 `streamId + sequence` 由 runtime 维护并在 SSE 重放时使用，两者不互相替代。`payload` 只接受 §2.4 列出的白名单键。

### 3.8 `artifact_metadata` 与 `quota_counters`

```sql
CREATE TABLE artifact_metadata (
    organization_id     TEXT        NOT NULL,
    artifact_id         TEXT        NOT NULL,
    execution_id        TEXT,
    job_id              TEXT,
    connection_id       TEXT        NOT NULL,
    byte_size           BIGINT      NOT NULL DEFAULT 0,
    chunk_count         BIGINT      NOT NULL DEFAULT 0,
    content_digest      TEXT,
    result_completeness TEXT        NOT NULL,
    truncation_reason   TEXT,
    visibility          TEXT        NOT NULL DEFAULT 'private',
    owner_principal_id  TEXT        NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL,
    expires_at          TIMESTAMPTZ,
    finalized_at        TIMESTAMPTZ,
    revoked_at          TIMESTAMPTZ,
    deleted_at          TIMESTAMPTZ,
    PRIMARY KEY (organization_id, artifact_id),
    FOREIGN KEY (organization_id, connection_id)
        REFERENCES connections (organization_id, connection_id),
    CONSTRAINT ck_artifact_size    CHECK (byte_size  >= 0),
    CONSTRAINT ck_artifact_chunks  CHECK (chunk_count >= 0),
    CONSTRAINT ck_artifact_visible CHECK (visibility IN ('private','organization')),
    CONSTRAINT ck_artifact_complete CHECK (result_completeness IN ('pending','complete','truncated')),
    CONSTRAINT ck_artifact_final CHECK ((result_completeness = 'pending') = (finalized_at IS NULL)),
    CONSTRAINT ck_artifact_digest CHECK ((result_completeness = 'pending') = (content_digest IS NULL)),
    CONSTRAINT ck_artifact_trunc CHECK ((result_completeness = 'truncated') = (truncation_reason IS NOT NULL))
);

CREATE INDEX idx_artifact_exec   ON artifact_metadata (organization_id, execution_id);
CREATE INDEX idx_artifact_expiry ON artifact_metadata (expires_at) WHERE deleted_at IS NULL;

CREATE TABLE quota_counters (
    organization_id TEXT        NOT NULL,
    scope           TEXT        NOT NULL,
    scope_key       TEXT        NOT NULL,
    resource_kind   TEXT        NOT NULL,
    used            BIGINT      NOT NULL DEFAULT 0,
    reserved        BIGINT      NOT NULL DEFAULT 0,
    limit_value     BIGINT,
    window_started_at TIMESTAMPTZ,
    window_ends_at   TIMESTAMPTZ,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, scope, scope_key, resource_kind),
    CONSTRAINT ck_quota_scope  CHECK (scope IN ('organization','principal','database')),
    CONSTRAINT ck_quota_counts CHECK (used >= 0 AND reserved >= 0)
);
```

- `quota_counters` 只存**跨进程/跨实例需要**的额度（组织、用户、数据库三级与产物总量）。单进程内的 control/interactive/metadata/job 保留与队列属于 `BudgetCoordinator` 内存，**不落盘**。
- 额度会计规则沿用[连接 §9.3](connection-management.md#93-预算会计)：失败必须核销，不能把额度重复发放。`reserved` 在承诺时增加、`used` 在实际消耗时转移，两者都不得为负。
- `visibility` 默认 `private`：共享业务结果默认私有，共享 SQL 文件不授予结果读取权（[概要 §10](system-overview.md#10-安全和环境差异)）。

P3/P7 流式元数据：create 时 pending、byte_size/chunk_count 为 0、digest/finalized_at 为 NULL；每次发布块原子增加已发布连续前缀的字节与块数。finalize/abort 固化 count、已发布字节摘要和完整性，零块空产物合法；truncated 必须有原因。读元数据先检查 deleted_at/expires_at/revoked_at，再返回 writing/complete/truncated；writing 的 totalChunks 为 null，publishedChunkCount/ByteSize 映射 chunk_count/byte_size。字节存储先完成写入再提交可读元数据，失败的未引用字节由清理器回收。P3 本地 adapter 与 P7 管理库均验收写字节/发布元数据之间的故障窗口。

### 3.9 `schema_migration_history`

```sql
CREATE TABLE schema_migration_history (
    version     BIGINT      PRIMARY KEY,
    name        TEXT        NOT NULL UNIQUE,
    checksum    TEXT        NOT NULL,
    applied_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    duration_ms INTEGER     NOT NULL DEFAULT 0
);
```

只在校验和执行**都成功**后插入；失败不写行，迁移整体回滚（§5.5）。

仓库现有两个本地先例，命名不一致，本文不做统一改造（避免制造与本文无关的改动），只说明差异：

| 位置 | 表 | 版本常量 | 记录形态 |
| --- | --- | --- | --- |
| [`store/app_db.rs`](../../../src-tauri/src/store/app_db.rs) | `schema_migrations` | `SCHEMA_VERSION: i32 = 1` | `(version INTEGER PRIMARY KEY, applied_at TEXT)`，只记版本 |
| [`store/history_db/schema.rs`](../../../src-tauri/src/store/history_db/schema.rs) | `schema_version` | 环形 v1→v4 | `CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)`，只记版本 |
| 服务端管理库（本文） | `schema_migration_history` | 内嵌于二进制 | 版本 + 文件名 + 校验和 + 耗时 |

服务端额外记 `name` 与 `checksum` 的原因见 §5.2。

### 3.10 认证与授权表

`login_sessions` / `memberships` / `acl_entries` / `permission_versions` 是 [团队服务 §4.3](team-server-and-auth.md#43-服务端登录会话) 与 [§6.1](team-server-and-auth.md#61-角色与资源-acl) 的落库登记；**本节是它们表结构的权威**（字段语义仍以团队服务为准），DDL、CHECK、索引与保留策略一次给全。四张表都带 `organization_id` 且以它打头（§3.1）。本节 DDL 按**可执行顺序**给出：`memberships` 必须先于 `login_sessions` 建立，因为 `login_sessions` 的复合外键引用 `memberships (organization_id, principal_id)`；PostgreSQL 在建表时即解析外键目标，同一迁移事务内**后建的表不能作为先建表的外键目标**，前置引用会直接报 `relation "memberships" does not exist`，而 §5.5 是 fail-closed，服务因此起不来。§3.3–§3.9 的字面顺序同样已核验：`acl_entries` 与 `permission_versions` 不声明任何外键，其余外键的目标表都出现在引用它的表之前。因此首个迁移文件按 §3.3→§3.10 的文件内顺序自上而下执行即可满足全部外键依赖（§5.1）。

```sql
CREATE TABLE memberships (
    organization_id TEXT NOT NULL, principal_id TEXT NOT NULL, role_id TEXT NOT NULL,
    external_subject TEXT NOT NULL, enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(), updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, principal_id),
    CONSTRAINT ck_membership_role CHECK (role_id IN ('orgAdmin','orgMember','connectionOwner','connectionViewer','jobOperator','auditor')),
    CONSTRAINT ck_membership_subject CHECK (external_subject <> '')
);
CREATE INDEX idx_memberships_role ON memberships (organization_id, role_id) WHERE enabled;

CREATE TABLE login_sessions (
    organization_id TEXT NOT NULL, session_id TEXT NOT NULL, principal_id TEXT NOT NULL,
    auth_time TIMESTAMPTZ NOT NULL, issued_at TIMESTAMPTZ NOT NULL,
    absolute_expires_at TIMESTAMPTZ NOT NULL, idle_expires_at TIMESTAMPTZ NOT NULL,
    reauth_required BOOLEAN NOT NULL DEFAULT FALSE, permission_version BIGINT NOT NULL,
    client_binding TEXT NOT NULL, token_epoch BIGINT NOT NULL DEFAULT 1,
    auth_mode TEXT NOT NULL DEFAULT 'cookie', native_secret_hash TEXT,
    revoked_at TIMESTAMPTZ, revoke_reason TEXT,
    PRIMARY KEY (organization_id, session_id),
    FOREIGN KEY (organization_id, principal_id) REFERENCES memberships (organization_id, principal_id),
    CONSTRAINT ck_login_mode CHECK (auth_mode IN ('cookie','native')),
    CONSTRAINT ck_login_native CHECK ((auth_mode = 'native') = (native_secret_hash IS NOT NULL)),
    CONSTRAINT ck_login_window CHECK (idle_expires_at <= absolute_expires_at),
    CONSTRAINT ck_login_revoke CHECK ((revoked_at IS NULL) = (revoke_reason IS NULL)),
    CONSTRAINT ck_login_reason CHECK (revoke_reason IS NULL OR revoke_reason IN ('logout','permission_revoked','admin','credential_rotated')),
    CONSTRAINT ck_login_epoch  CHECK (token_epoch >= 1)
);
CREATE INDEX idx_login_sessions_owner ON login_sessions (organization_id, principal_id) WHERE revoked_at IS NULL;
CREATE INDEX idx_login_sessions_sweep ON login_sessions (idle_expires_at) WHERE revoked_at IS NULL;

CREATE TABLE acl_entries (
    organization_id TEXT NOT NULL, resource_type TEXT NOT NULL, resource_id TEXT NOT NULL DEFAULT '',
    subject_type TEXT NOT NULL, subject_id TEXT NOT NULL, action TEXT NOT NULL,
    deny BOOLEAN NOT NULL DEFAULT FALSE, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, resource_type, resource_id, subject_type, subject_id, action),
    CONSTRAINT ck_acl_resource CHECK (resource_type IN ('connection','session','execution','job','artifact','stream')),
    CONSTRAINT ck_acl_subject  CHECK (subject_type IN ('user','serviceAccount','delegated')),
    CONSTRAINT ck_acl_action   CHECK (action IN ('read','execute','write','cancel','download','subscribe','admin'))
);

CREATE TABLE permission_versions (
    organization_id TEXT NOT NULL, permission_version BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id),
    CONSTRAINT ck_permission_version CHECK (permission_version >= 1)
);
```

规则与保留策略（`roles` 的处理见下条；字段语义与默认时长一律以[团队服务 §4.3](team-server-and-auth.md#43-服务端登录会话) 为准）：

- **没有 `roles` 表**：首版角色是固定集合（[团队服务 §6.1](team-server-and-auth.md#61-角色与资源-acl) 明确不可由管理员自定义），因此用 `ck_membership_role` 的 CHECK 表达；P9 引入自定义角色时再补 `roles` 表与外键。
- P8 以 expand 迁移增加 `auth_mode/native_secret_hash`；cookie 会话的 hash 为 NULL，native 会话仅存随机 secret 的 HMAC 摘要，不存原值或 IdP token。`native_secret_hash` 编码包含 HMAC keyVersion 与摘要；HMAC 密钥由 SecretProvider 管理，轮换保留至相关会话到期。一次性交接事务和 code 只存短期内存，重启即失效；流程序列见 [团队服务 §4.6](team-server-and-auth.md#46-p8-桌面团队认证与传输目标设计)。
- `session_id` 是服务端生成的不透明随机值，不是 `dbSessionId`（§2.1 W14）；`login_sessions` 里**没有**任何 IdP 令牌列（F14），需要调用 IdP 后端接口时按需重新交换。`revoke_reason` 四个取值与 [团队服务 §4.5](team-server-and-auth.md#45-登出与会话撤销) 逐字一致；`token_epoch` 只增，用于令已签发的短期令牌立即失效。
- `absolute_expires_at` / `idle_expires_at` 的默认时长取团队服务（**首版建议值，来自部署配置**），本文不写死；`idle_expires_at` 只由**入站请求**顺延，登录心跳与 SSE `: keepalive` 都不刷新它（[共享边界 §4.5](shared-boundaries-and-ports.md#45-会话目录预算与令牌端口)、[团队服务 §8.3](team-server-and-auth.md#83-心跳与超时)）。
- `memberships.external_subject` 是 IdP `sub` 在管理库中的**唯一**落库处，只用于登录映射，不进任何 API 响应；成员关系只置 `enabled = FALSE`，不物理删除（撤销要可追溯）。`acl_entries.resource_id` 用空串 `''` 表示**组织级**条目（主键列不能为 NULL），`session` / `execution` / `stream` 的条目只在需要显式 deny 时登记，默认按 owner 继承（[团队服务 §6.1](team-server-and-auth.md#61-角色与资源-acl)）。
- 撤销必须先移除 grant、写入 deny 或禁用 membership，并在**同一事务**内递增 `permission_versions.permission_version`；版本变化只负责使缓存分区键与 PoolKey 失效，不能代替实际授权变更（[团队服务 §6.3](team-server-and-auth.md#63-撤销传播)）。`permission_versions` 每组织一行、只增不减、**永不删除**；`login_sessions` 的撤销与过期行保留至 `absolute_expires_at` 之后按审计保留期清理，因为删除会话行等价于强制重新登录，**删除永远是安全方向**。

### 3.11 `budget_allocations`（P9 目标增量）

P7 单实例不创建该表；P9 expand 迁移增加它，并保留兼容的旧 schema 支持区间。分配账持久化的是节点可用数量，不是 socket/ResourceLease；workerId 与 Job 调度字段一样标识本次启动的执行者，runtimeEpoch 不落库。SessionDirectory 仍禁止任何磁盘记录。

```sql
CREATE TABLE budget_allocations (
    organization_id TEXT NOT NULL,
    allocation_id TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    worker_id TEXT NOT NULL,
    generation BIGINT NOT NULL DEFAULT 1 CHECK (generation > 0),
    dimensions JSONB NOT NULL,
    amount BIGINT NOT NULL CHECK (amount > 0),
    returned_amount BIGINT NOT NULL DEFAULT 0,
    state TEXT NOT NULL CHECK (state IN ('issued','expiredHeld','reclaimed')),
    expires_at TIMESTAMPTZ NOT NULL,
    closure_evidence JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, allocation_id),
    UNIQUE (organization_id, operation_id),
    CHECK (returned_amount BETWEEN 0 AND amount),
    CHECK (state <> 'reclaimed' OR
           (returned_amount = amount AND closure_evidence IS NOT NULL))
);
CREATE INDEX idx_budget_allocations_held
    ON budget_allocations (organization_id, worker_id, expires_at)
    WHERE state <> 'reclaimed';
```

`dimensions` 是冻结的组织/用户/物理服务 quota key、资源种类和类别，不含资源标识。allocation 表与各 `quota_counters.reserved` 增减同事务，按稳定 key 顺序锁行；部分归还只减 returnedAmount 的增量。多端申请在一个事务中创建全部记录或全部失败；每笔 operationId 从同一 requestId 与维度序号稳定派生，重传同摘要返回原 allocationId。

expiredHeld 的 `amount - returned_amount` 继续计入 reserved；TTL 到期、目录重启或 worker 心跳丢失都不能清零。核销 CAS 同时检查 workerId/generation，必须有连接已关闭/旧节点隔离加目标核验的证据。证据只是脱敏证明摘要，不含数据库连接句柄或秘密。预算限额降低到已占数量以下时拒绝新分配，不能修改旧记录伪造释放。详细协议见 [多 worker 设计 §5](multi-worker-coordination.md#5-全局预算分配协议)。

## 4. 本地 Store 与服务端 DB 映射

### 4.1 桌面本地持久化现状（已实现）

| 载体 | 内容 | 加密 | 打开位置 |
| --- | --- | --- | --- |
| `{appData}/connections.json` | `ConnectionConfig` 数组（明文 JSON，仅 `password`、`ssh_tunnel.password`、`ssh_tunnel.passphrase` 逐字段 AES-256-GCM） | 字段级 | [`store/connections.rs`](../../../src-tauri/src/store/connections.rs) |
| `{appData}/settings.json` / `tunnels.json` / `groups.json` | 应用设置、隧道、分组 | 同上（仅凭据字段） | [`store/mod.rs`](../../../src-tauri/src/store/mod.rs) `load_all()` |
| `{appData}/ai_config.enc` | AI Provider 配置 | 整体密文 | [`store/ai_config.rs`](../../../src-tauri/src/store/ai_config.rs) |
| `{appData}/history.sqlite` | `query_history`、`workflow_history`、`migration_run_history`（WAL，`PRAGMA foreign_keys = ON`） | **明文**（SQL 文本、错误信息按明文存） | [`store/history_db.rs`](../../../src-tauri/src/store/history_db.rs) |
| `{appData}/datazen.sqlite` | `schema_migrations`、`workflows`、`dashboards`、`widgets`、`widget_runs`、`widget_latest_run`；v2 增加 `jobs`、`job_idempotency_receipts`、`job_stages`、`job_commit_boundaries`、`job_checkpoints`、`job_result_details`、`job_domain_results`、`job_artifact_refs` | 明文；Job plan/result 是受限 DTO 投影 | [`store/app_db.rs`](../../../src-tauri/src/store/app_db.rs)、[`store/app_db/jobs/`](../../../src-tauri/src/store/app_db/jobs/) |
| `{appData}/favorites/**/*.sql` | 收藏 SQL（含 front-matter） | 无 | [`store/favorites/`](../../../src-tauri/src/store/mod.rs) |
| `{appData}/.key` | AES-256 主密钥（仅 file 后端） | base64 明文文件 | [`store/key_store.rs`](../../../src-tauri/src/store/key_store.rs) |

桌面 AppDb 当前 `SCHEMA_VERSION` 为 2。打开既有 v1 数据库时，保留原 workflows/dashboard 数据，并在一个 SQLite `Immediate` 事务内创建全部 Job v2 表及索引、登记 migration version 2；任一 v2 DDL/登记失败会回滚该迁移事务。Job accept 在一个事务内写 Job、幂等 receipt、consumed plan ID 与初始结果行。SQLite job rows 保存白名单化 plan（最大 1 MiB）、安全进度、状态/error code、commit boundaries、checkpoints、bounded domain results 与 recovery verdict；不保存 `dbSessionId`、session/lease/cursor、worker closure、token、password、任意 SQL 或 driver 自由格式错误文本。运行时 worker/claim 字段属于本机 Job 调度 fencing，不会重建外部资源句柄。

`job_artifact_refs` 只保存 Artifact ID 与创建/过期时间，不存产物字节；引用 TTL 当前固定为 30 天。Artifact 内容由独立存储管理，AppDb 的引用 TTL 不延长或保证内容留存；调用者读取产物时仍须检查产物是否存在并重新授权。结果 DTO 可保留已过期 Artifact ID 供审计/详情展示，但不能据此宣称字节可下载。

三点现状结论，直接影响 P1/P7：

1. `connections.json` 里**没有** `configRevision`、`credentialRevision`、`organizationId`。这三个字段必须由新增的映射层补出，映射层不是"读现有文件"而是"把旧结构映射到新结构"。
2. `history.sqlite` 明确以明文存 SQL 与错误信息（见[持久化存储 §1.3](../backend/store.md#13-查询历史明文)），因此备份 app data 目录必须视同敏感审计日志。本文的 §7 以此为前提。
3. 旧实现把运行时会话 id 写进过持久化模型。`SyncTask` 的处理方式就是目标模板：字段保留在 Rust 结构里以便读旧 JSON，但标 `#[serde(default, skip_serializing)]` 并在归一化时清空（[`store/models.rs`](../../../src-tauri/src/store/models.rs) `normalize_legacy_state()`）。**"能读旧数据、绝不再写出"** 是本仓已有的、已测试的先例。

### 4.2 字段同构（本地 `ConnectionConfig` → 服务端 `connections`）

| 服务端列 | 本地来源 | 迁移动作 |
| --- | --- | --- |
| `organization_id` | 无 | 本地模式取固定本地组织 id；团队模式由认证与 membership 决定（[概要 §6.1](system-overview.md#61-应用上下文)） |
| `connection_id` | `ConnectionConfig.id` | 直接映射；已是持久化标识 |
| `name` / `driver_id` | `name` / `type` | 直接映射 |
| `config_revision` | 无 | 新建为 `1`；此后只由服务端 `compare_and_set` 递增 |
| `credential_revision` | 无 | 新建为 `1`；`advance_credential_revision` 递增 |
| `initial_namespace` | `database` 等初始目标字段 | 按 `CanonicalTarget` 归一化后写入 |
| `public_options` | 非敏感连接选项 | 直接映射 |
| `driver_options` | 驱动选项（含敏感部分） | 敏感部分**不迁移**，改为 `secret_ref`（§4.4） |
| `secret_ref` | 无 | 团队模式下由 `SecretProvider` 在服务端创建；本地模式下不适用 |
| `read_only` / `enabled` | 无 | 新建为 `FALSE` / `TRUE`，之后由服务端策略维护 |
| `created_at` / `updated_at` | 无 | 新建为导入时间；**不伪造**历史时间 |

反向（服务端 → 本地）只允许携带 `connection_id`、`name`、`driver_id`、`initial_namespace`、`public_options` 与 `credentialConfigured` 布尔位，形状等于 [`ProfileView`](connection-management.md#41-服务接口与补充响应)；不返回密码、token 或 TLS 私钥。

### 4.3 只存在于服务端的字段

| 字段 | 归属 | 为什么不能落在本地 profile |
| --- | --- | --- |
| `organizationId` | `RequestContext` | 桌面本地模式是单组织；团队模式下身份由认证决定，不是配置属性 |
| `policyRef` / `policyIsolationKey` | `PolicyService` | 权限撤销必须让 PoolKey 立即变化，配置本地副本会延迟生效（CM-67） |
| `networkRouteRef` / `networkRouteRevision` | `NetworkProvider` | 出站路由是执行节点策略，客户端自报无效 |
| `executionIdentityKey` | `IdentityResolver` | 必须由服务端解析，不能用客户端传入的用户名 |
| `driverResourceKey` | `describeResource` | 由 driver 对 `CanonicalTarget` 的规范化结果决定 |
| `runtimeEpoch`、会话状态、attachment 状态 | SessionDirectory（内存） | 见 §2.2 |
| 额度、预算会计 | `BudgetCoordinator` | 组织/用户/数据库额度是服务端权威 |
| 审计事件 | `Audit` | 桌面只写本地日志；服务端审计是安全资产 |
| 登录会话行、membership（含 `role_id`）/ ACL、组织 `permissionVersion`、IdP `sub` 映射 | 认证适配器 + `PolicyService`（§3.10） | 桌面本地模式是单组织且没有登录会话行；团队模式下的身份与授权数据由服务端落库，本地 profile 只存连接配置，`external_subject` 不回传 |

### 4.4 主密钥与团队秘密

| 形态 | 主密钥来源 | 连接秘密来源 |
| --- | --- | --- |
| 桌面本地模式 | `key_store::key_backend()`：默认 OS 钥匙串（`keyring::v1::Entry::new(APP_IDENTIFIER, "app-encryption-key")`）；`DATAZEN_KEYRING=file` 或 macOS 未签名/adhoc 构建时用 `{appData}/.key`；旧 `.key` 会透明迁入钥匙串后删除 | 本地 `connections.json` 逐字段 AES-256-GCM（nonce 12 字节前置，base64 STANDARD） |
| 桌面团队模式 | 同上（本地设置仍需加密） | **不下载团队数据库凭据**（[概要 §11 验收 2](system-overview.md#11-系统级验收)）；执行身份由服务端 `SecretProvider` 解析 |
| 服务端 | 不使用用户钥匙串（无用户钥匙串可访问） | KMS / 外部密钥服务；管理库只存 `secret_ref` |

因此「团队秘密不存入本地普通 profile」有三重保证：客户端不接收秘密（只拿 `credentialConfigured` 布尔位）；管理库不存秘密（只存 `secret_ref`）；本地文件即使被读到也只会命中本地模式自己的凭据。`settings.json` 里的 `favoritesRoot` 这类"指向任意目录"的设置是本地偏好，不属于连接 profile，不得用于指代服务端路径。

## 5. Schema 版本与迁移运行器

### 5.1 版本策略

- 版本号是**单调递增整数**，语义与 [`store/app_db.rs`](../../../src-tauri/src/store/app_db.rs) 的 `SCHEMA_VERSION` 一致，但服务端用 `BIGINT` 并多存文件名与校验和。
- 二进制内嵌 `migrationHead` 与 `supportedSchemaMin/Max`；expand 发布必须让上一版的已声明兼容范围覆盖下一版 expand 版本，contract 必须超出已退役版本范围。运行器启动时读取 `SELECT COALESCE(MAX(version), 0) FROM schema_migration_history` 后比较：

| 比较结果 | 行为 |
| --- | --- |
| 已应用 < migrationHead，且起点受支持 | 顺序执行缺失版本；**不启动服务** |
| 已应用 = migrationHead，且受支持 | 不做写操作，直接启动 |
| 已应用 > migrationHead，且在 supportedSchemaMin/Max 内 | 不执行迁移或降级写，按兼容 schema 启动；供旧镜像回退与滚动部署使用 |
| 已应用不在 supportedSchemaMin/Max 内 | **fail-closed**：拒绝启动；升级路径仅允许二进制声明支持的迁移起点 |
| 同版本但 `checksum` 不匹配 | **fail-closed**：拒绝启动（有人改过已合入的迁移文件） |

- 首批迁移版本一次性建立 §3.2 的 13 张表（含 §3.10 的认证与授权表）；这是纯 expand（只加表），不得与任何 contract 混版。文件内的建表顺序就是 §3.2 的列出顺序与 §3.3–§3.10 的 DDL 顺序，已核验满足全部外键依赖（被引用表先建，见 §3.10）。

### 5.2 迁移文件

- 位置：`server/migrations/`，随二进制内嵌（路径与加载方式见[团队服务 §13](team-server-and-auth.md#13-schema-migration-运行方式)）。
- 命名：`NNNN_snake_case_name.sql`，四位零填充序号 + 单下划线 + 语义名。序号即执行顺序，`VARCHAR` 字典序与数值序一致（例如 `0001_create_connections.sql`、`0002_add_job_claim_columns.sql`）。
- 不可变性：**已合入的迁移文件永不修改**。写错了（例如先删字段后发现有问题）就发新版本向前修，不回头改旧文件——否则 `checksum` 校验会拒绝启动。
- `checksum` = 规范化后文件内容的 SHA-256（统一 LF、去掉 BOM、去掉行尾空白）。`name` 列必须与文件名去掉扩展名后一致，由加载器断言。
- 幂等：迁移文件不写 `IF NOT EXISTS`。表已存在说明环境不对，应由 `checksum` 与版本表报错，而不是静默跳过。

### 5.3 expand / contract 判据

| 类别 | 操作 |
| --- | --- |
| expand（旧代码可继续跑） | 加表、加可空列、加带默认值的列、加索引、放宽 CHECK、加新枚举值 |
| contract（必须等旧镜像下线） | 删列、删表、类型收窄、加 `NOT NULL`、删旧枚举值、收紧 CHECK、改主键 |

规则：

1. **禁止在同一个版本里对同一列做加与删**。开发计划的发布门槛原文是"数据库 schema 采用 expand/contract，不先删字段"（[开发计划 §11 P7](../../development/platform-development-plan.md#11-p7单实例团队-web-服务)）。同一版本 add + drop 会让"跑旧镜像"和"跑新镜像"两种状态在同一个部署里同时不存在或同时存在，回退时无路可走。
2. contract 分两次发布：N 版 expand（双写：新旧列都写），N+1 或更晚的 N+k 版 contract。间隔至少跨过一次完整发布周期，且期间旧镜像已下线。
3. 删列前必须先把该列的数据**搬到新列**（expand 阶段完成），contract 阶段只做删除。
4. 索引创建使用 PostgreSQL 的 `CREATE INDEX CONCURRENTLY` 时必须**单独**成一个迁移文件，且不能包在事务里；这类迁移在文件名后缀标记（如 `0007_add_exec_idx_concurrently.sql`），运行器按后缀切换"事务外执行"模式。
5. 用户业务数据库**永远不参与**本迁移体系。

### 5.4 触发方式

- `serve` 启动前自动执行（[团队服务 §13](team-server-and-auth.md#13-schema-migration-运行方式)），迁移完成前不接流量，因此不存在"运行中代码遇到缺失列"的中间态。
- 另提供显式 `migrate`（执行）与只读 `check-schema`（只比较版本与校验和，不写库），供运维在部署流水线里先跑一次。
- 桌面本地 SQLite 侧维持现状：`init_schema()` 用 `CREATE TABLE IF NOT EXISTS` 建表（[`store/app_db.rs`](../../../src-tauri/src/store/app_db.rs)），`run_migrations()` 按 `has_column` 探测逐版本升级（[`store/history_db/schema.rs`](../../../src-tauri/src/store/history_db/schema.rs)）。**本地不需要 expand/contract**，因为本地库与应用同版本发布、且可以整体替换；本文的 expand/contract 规则只约束服务端管理库。
- 多实例并发启动时用 PostgreSQL advisory lock 串行化迁移；拿不到锁就等待，不并行执行。

### 5.5 失败与回滚

| 项 | 规则 |
| --- | --- |
| 单文件事务性 | 普通迁移的 DDL 与历史登记在**同一个事务**内执行；失败整体回滚；§5.3 的 concurrently 文件走事务外恢复协议 |
| 失败策略 | **fail-closed**：不进入"部分应用"状态，服务拒绝启动；错误写入服务日志与健康检查输出 |
| 成功记录 | 普通迁移在 DDL 同一事务内插入 `schema_migration_history`，提交后一起可见；不存在 DDL 已提交但历史未登记的窗口 |
| 回滚方式 | **不写反向 SQL 文件**。回退靠"回退镜像 + 向后兼容 schema"（[开发计划 §11 P7](../../development/platform-development-plan.md#11-p7单实例团队-web-服务)）；已经发生的外部数据库写入**不能**通过回退镜像撤销 |
| 数据损坏处理 | 由运维用备份恢复管理库；恢复后按 §7 重新核验 |

事务外索引迁移持有同一 advisory lock：启动前检查目标索引定义、`indisvalid` 与预期摘要；已存在且有效、定义匹配时只补历史；无效的同名索引先删除再重建；定义不匹配则拒绝并要求运维修复。索引成功后登记历史，崩溃重启走上述核验，不能直接重发裸 CREATE。已知版本校验内嵌 checksum；未知更高版本只有在支持范围和兼容发布约定内才允许读取，不伪称已核验未知文件 checksum。发布 CI 验证 N-1 镜像在 N 的 expand schema 上读写与启动，以及超范围/contract 后明确拒绝。

P5/P9 claim 存储：`claim_generation` 在首次认领/接管时原子自增，renew 不改变它。所有 worker 的阶段、提交边界、checkpoint 与状态写入同时校验当前 generation、worker 和未过期租约，状态版本 CAS 是附加条件。`cancel_requested_at` 是持久化取消意图，不是 cancelled 终态；它可在 queued/running 时记录，不能撤销已提交边界。worker 异常后保留边界并待核验，不用清空 owner 的方式让旧 worker 重新获得写入权。

## 6. 迁移脚本要求

### 6.1 `dbSessionId` 不是迁移数据

迁移脚本**禁止**把任何会话标识、lease、cursor、`runtimeEpoch` 搬进新列。理由不是"这些字段敏感"，而是它们没有物理含义：物理资源随进程终止而消失，会话目录禁止磁盘持久化、快照与 append-only 日志（[连接 §12](connection-management.md#12-web多实例权限与结果)）。把旧 `dbSessionId` 复制到新表，得到的是一串指向不存在资源的字符串。

落地要求：迁移脚本的单测在执行后对全表做 §2.5 的列名与 JSON 键断言；迁移脚本中出现 `db_session_id` 字面量即视为测试失败。

### 6.2 旧共享会话持久化数据标记为不可迁移

旧实现把连接句柄按 `connectionId` 复用（`get_session` 通过同一 id 重连，而事务 map 由命令层单独管理，这正是 CM-73 记录的基线缺陷，[连接 §16.7](connection-management.md#167-补充契约与边界用例)）。因此**旧实现写下的任何"会话/句柄"字段都不是迁移数据**。三类旧数据分别这样处理：

| 旧数据 | 处理 | 依据 |
| --- | --- | --- |
| `SyncTask.source_db_session_id` / `target_db_session_id` | 保留字段用于读旧 JSON，落库前清空（`normalize_legacy_state()`），序列化 `skip_serializing` | [`store/models.rs`](../../../src-tauri/src/store/models.rs) |
| `SyncTask.current_table_offset > 0` 或 `strategy = "continue"` | offset 归零、`strategy` 置 `unknown`、`status` 不得是 `running`/`paused` | 同上（`had_unsafe_checkpoint` 分支） |
| 服务端导入的旧 execution / job 记录 | **不自动再执行**；核验态只能落在合法取值上：`durable_executions` 记 `effect_outcome='unknown'`（`state` 仍留在 `ck_exec_state` 允许的 6 个取值内），`state='outcomeUnknown'` 只属于 `idempotency_records`（`ck_idem_state`，见 §3.5）；`jobs` 没有 `effect_outcome` 列，其核验结论由同键幂等记录（`subject_kind='job'`）承担 | [连接 §13.1](connection-management.md#131-幂等键期限与响应分类) |

"进程重启后非终态进入核验/OutcomeUnknown，不自动再执行"对本地与服务端是同一条规则。本地已有同形实现：打开 `history.sqlite` 时把 `status='running'` 的 `migration_run_history` 行更新为 `status='interrupted'`、`outcome='unknown'`、`phase='interrupted'`，并在 `rollback_outcome` 仍为默认值 `'notRequired'` 时置 `'unknown'`（[`store/history_db/schema.rs`](../../../src-tauri/src/store/history_db/schema.rs) `init_schema` 的收尾 `UPDATE`）。

### 6.3 `configRevision` 变化后的行为

| 场景 | 行为 |
| --- | --- |
| profile 被 `compare_and_set` 更新 | `config_revision + 1`；PoolKey 变化，旧 idle 资源停发并关闭（[连接 §9.6](connection-management.md#96-poolkey版本与缓存的生产者)） |
| 历史执行记录 | **不重写** `durable_executions.config_revision`。它记录的是"当时执行用的版本"，属于审计事实 |
| 结果写回普通表 | 按 §8.3 的写回规则 1：重新授权完整对象目标 + 验证结构/可写映射/PK/version 后，用**新短租约**写回 |
| 结果写回临时对象 | 只读。临时对象不承诺跨会话存活 |
| 任务计划 | `config_revision`、`credential_revision`、能力版本或映射指纹任一变化 → `PlanStale`，不得静默继续 |
| 凭据轮换 | 只递增 `credential_revision`；已建立会话不被悄悄改配置（CM-38） |

**禁止**用"配置版本相同"或"上下文指纹相同"去寻找"等价 session"：`configRevision` / `contextRevision` 只能证明配置与上下文声明一致，不能证明物理资源连续性（[连接 §4.4](connection-management.md#44-可落盘来源与运行时绑定)）。丢失绑定时正确结果是新 `dbSessionId` + 重新授权，或 `SessionLost`。

### 6.4 一次性导入的护栏

| 场景 | 护栏 | 仓库先例 |
| --- | --- | --- |
| 本地 JSON → SQLite | 导入成功后**重命名**源表/源文件而不是删除；中途失败则源数据原样保留，下次启动重跑（幂等由"重命名"保证，不是标记文件） | `history_db::migrate_legacy_json` 的 `rename_aside`；收藏迁移把 `favorite_queries` 改名为 `favorite_queries_legacy_v1`（[持久化存储 §1.4.1](../backend/store.md#141-从-favorite_queries-表迁移)） |
| 已归档表复活（备份恢复、版本回滚） | 迁移前先查归档表，命中即返回"无事可做"，不读任何行 | 同上 |
| 确定性 id | 导入 id 由旧 id 确定性派生（收藏用 `Sha256(legacy_id)[0..10]`），保证重跑不产生副本 | 同上 |
| 本地 profile → 服务端管理库 | **显式用户动作**，不由应用自动上传；导入是"复制配置"，不是"同步凭据" | §4.4 |

### 6.5 迁移脚本的可测试性

每个迁移文件配一个测试夹具：旧版本 schema 的最小快照 → 执行迁移 → 断言（a）`schema_migration_history` 新增一行且 `checksum` 匹配；（b）§2.5 的三条禁持久化断言全绿；（c）该版本声明的 expand/contract 性质成立（例如 expand 版本执行后，**旧版代码路径**仍能读写该表——用前一个版本的仓储实现跑一遍最小读写）。

## 7. 备份与恢复边界

### 7.1 备份范围

| 类别 | 是否备份 | 说明 |
| --- | --- | --- |
| 服务端管理库（13 张表） | 是 | 组织、配置版本、执行记录、任务、审计、额度、迁移历史、登录会话与授权数据（`login_sessions` / `memberships` / `acl_entries` / `permission_versions`） |
| `ArtifactStore` 字节 | 按 TTL 与额度策略，可选 | 备份必须与 `artifact_metadata` 一起做，否则元数据会指向不存在的字节 |
| 令牌签名密钥（KMS / 配置） | 是，独立流程 | 保留期 ≥ 已发令牌全部过期时间；恢复后未知 `keyVersion` 仍必须拒绝 |
| 审计归档（超出在线保留期后卸载的部分） | 是 | append-only 卸载后仍需可追溯 |
| 桌面 `{appData}` 目录 | 按用户选择 | 包含 `.key` 时的处理见下；`history.sqlite` 明文含 SQL 与错误信息，备份视同敏感审计日志 |
| `{appData}/.key` | **不随应用数据 ZIP 打包** | 跨机恢复密文需另行备份主密钥 |

### 7.2 恢复后必须显式失效的东西

| 对象 | 恢复后状态 | 原因 |
| --- | --- | --- |
| 所有数据库会话 | 全部失效，客户端重连得到 `SessionLost`，**不是**新句柄 | 物理连接随进程终止；目录不落盘 |
| 未终结 Job | 转为待核验，按 `job_checkpoints` 核验已提交边界，**不自动重放副作用阶段** | 提交未知不自动重试（[概要 §11 验收 9](system-overview.md#11-系统级验收)） |
| 未终结幂等记录 | 保持记录，状态转 `outcomeUnknown`；同指纹同摘要仍返回同 receipt | CM-70 |
| attachment 令牌 | 全部失效；同 principal 无令牌重附着拒绝（[连接 §16.7 CM-63](connection-management.md#167-补充契约与边界用例)） | 仅 `openSession` / 上下文替换的完整指纹与 `attachmentToken` 绑定 owner `runtimeEpoch`：owner 丢失时即使令牌未过期也返回 `SessionLost`，**不在新 owner 当作首次请求重建**（[团队服务 §10.4](team-server-and-auth.md#104-幂等回执存储)）。其余 attachment 路径的 TTL、detach 与竞态按 [连接 §6.4](connection-management.md#64-attachment-与超期处理) 判定，不受本行的 `runtimeEpoch` 绑定要求约束 |
| 会话目录、pool 元数据、运行时预算 | 从零开始 | 单实例自洽前提（[团队服务 §10.3](team-server-and-auth.md#103-sessiondirectory-与-budgetcoordinator单实例)） |
| `close` tombstone | 丢失；恢复后对已关闭会话返回 `SessionLost` 而非重建 | tombstone 只在内存保留 |

恢复后必须跑一次**对账**：把 `artifact_metadata` 的 `content_digest` 与对象存储/受控目录中的实际字节比对。校验不通过的产物标记为不可用，**不返回半份数据**；`deleted_at` 已置但字节仍在的，只做标记不删除（删除由 §8.2 的清理任务负责）。

### 7.3 不能声称的事

不能声称"恢复备份后活动数据库会话可恢复"——开发计划的发布门槛原文即"管理员能恢复服务元数据，活动数据库会话不声称可恢复"（[开发计划 §11 P7](../../development/platform-development-plan.md#11-p7单实例团队-web-服务)）；不能声称 schema 回退能撤销已经发生的外部数据库写入（回退只回退服务端镜像）；不能声称恢复过程会"继续"任何未完成任务（恢复后进入核验态）；不能声称查询历史是隐私安全的——`history.sqlite` 明文存 SQL 与错误信息是既定事实（[持久化存储 §1.3](../backend/store.md#13-查询历史明文)），备份必须按敏感审计日志对待。

本文也不规定 RPO / RTO 数值，这属于部署文档（[团队服务 §14](team-server-and-auth.md#14-部署形态)）与组织自身策略。

## 8. 事件与产物保留策略

### 8.1 事件序号与归档

| 机制 | 规则 |
| --- | --- |
| 事件序号 | 每条事件带 `streamId` + `sequence`；`sequence` 是 `BIGINT`，序列化为十进制字符串，**不得**经 JS `Number`（[连接 §13](connection-management.md#13-错误重试和事件)） |
| 重放 | 断线重连带 `Last-Event-ID` 从**内存环形缓冲**重放（[团队服务 §8.2](team-server-and-auth.md#82-last-event-id-回放)）；缓冲过期则客户端取快照 |
| 缺口与重复 | 缺口触发重读，重复事件忽略；`streamResetRequired` 后不再积压 |
| 队列上限 | 每订阅 256 条 / 1 MiB，先触及任一上限即停止订阅并发 `streamResetRequired`（[连接 §7.7](connection-management.md#77-结果订阅与放弃消费)） |
| 归档 | 事件**不整体落库**。只按 §2.4 把白名单投影写入 `audit_events`；SSE 终态可从 repository 重建 |
| 事件体 | 不持久化 `EventEnvelope`，因此不持久化 `sessionHandle`、结果数据、SQL 原文 |

### 8.2 产物额度与 TTL

P5 Job 引用的计划/输入 Artifact 另有保留锁：接受 Job 与建立引用原子提交，终态及恢复保留窗口结束前禁止普通 TTL 清理；删除 adapter 检查有效引用并 CAS 标记删除。过期计划不能新建 apply，但已接受 Job 的引用不能中途消失。准备失败的未引用字节仍按孤儿规则清理。

| 项 | 首版要求 | 取值来源 |
| --- | --- | --- |
| 每 execution 产物字节上限 | **必填**，不得为空 | 桌面默认 256 MiB，团队默认 1 GiB（[连接 §7.7](connection-management.md#77-结果订阅与放弃消费)） |
| 组织总产物额度 | 必填，仍受组织额度约束 | 部署配置（`quota_counters`） |
| 未消费缓冲上限 | 8 MiB | [连接 §7.7](connection-management.md#77-结果订阅与放弃消费) |
| 无消费者等待 | 30 秒 | 同上 |
| 放弃后协议 drain deadline | 10 秒 | 同上 |
| 产物 TTL | 必填，按删除策略配置 | 部署配置；**本文不写死时长**（权威文档未给默认值），但要求：TTL 与额度是两套独立配置；`expires_at` 落库；清理任务先置 `deleted_at`（软删）再异步硬删；硬删前完成 §7.2 的对账 |
| 导出 | 导出前重做 ACL 校验；共享 SQL 文件不授予结果读取权 | [概要 §10](system-overview.md#10-安全和环境差异) |
| 服务端路径 | 不接受任意服务器绝对路径，只接受 artifact/配置引用 | 同上 |

### 8.3 结果完整性与截断

| 状态 | 含义 | 落库 |
| --- | --- | --- |
| `pending` | 仍在执行；未定完整性 | 终态行的 `finished_at` 由 `ck_exec_finished` 保证非空，但该 CHECK 只绑 `state`、**不拒绝** `pending` 与终态并存（§3.4），所以这一组合由**应用层写入路径**保证不出现：写入终态行时必须显式落 `complete` 或 `truncated`，不得依赖列默认值 `'pending'`；完整性确实无法判定时落 `truncated` 并给出原因，不把不可判定伪装成 `pending`（CM-64 要求截断可见且原因落库）。本文**不**为此补 CHECK，理由见下方说明。 |
| `complete` | 完整产生 | `truncation_reason` 必须为 NULL |
| `truncated` | 有界截断 | `truncation_reason` 必须非空 |

**B 的择一结论：不补数据库 CHECK，改由应用层写入路径保证。** 理由：`resultCompleteness` 与 `state` 的耦合不在 [连接 §4](connection-management.md#4-dto-与字段定义) 与 [§7.7](connection-management.md#77-结果订阅与放弃消费) 的权威定义内（连接只声明 `resultCompleteness: 'pending' | 'complete' | 'truncated'`，未声明终态必须非 `pending`），而 `pending` 的含义本身就含"未定完整性"；在 §3.4 里新加一条只存在于本表的不变量，等于把本文的落地猜测抬成数据库约束，迫使仓储、恢复与导入路径反向服从，超出 §1「不重复定义 DTO 取值」的边界。排除该组合的责任因此落在写入路径：`DurableExecutionRecord.resultCompleteness` 是必填字段，写入终态行时必须显式赋值，并由 §6.5 的测试夹具断言"终态行 `result_completeness <> 'pending'`"。若连接后续收录"终态必须非 `pending`"的权威结论，再按 §5.3 补 CHECK——加/收紧 CHECK 属 contract，必须等旧镜像下线后单独发版。`truncation_reason` 建议取值如下——**本文建议值，待 [连接 §7.7](connection-management.md#77-结果订阅与放弃消费) 收录后方为权威**；[连接 §4](connection-management.md#4-dto-与字段定义) 只声明 `truncationReason: string | null`，故本节不是对既有 DTO 字段的重复定义或收窄，下表只是本文侧代码常量与 §3.4 的 `ck_exec_trunc` 配合的建议集合，**不是**权威枚举，取值以连接 §7.7 收录后的定义为准，届时本文只做引用；在此之前读取方（客户端、审计投影）必须按 `string` 处理 `truncationReason`，只对下表列出的值做展示映射，遇到未列出的值既不得报错也不得丢弃：

| 建议值 | 触发条件 |
| --- | --- |
| `subscriptionQueueOverflow` | 每订阅 256 条 / 1 MiB 上限 |
| `unconsumedBufferOverflow` | 每 execution 8 MiB 未消费缓冲 |
| `noConsumerTimeout` | 无消费者等待 30 秒到期 |
| `artifactQuotaExhausted` | 产物额度或每 execution 字节上限 |
| `drainDeadlineExceeded` | 放弃后 10 秒未能 drain 完 |
| `cancelRequested` | 精确取消导致结果提前结束 |
| `producerError` | 协议/驱动侧错误导致结果不完整 |

写回规则（[连接 §4.4](connection-management.md#44-可落盘来源与运行时绑定)）：

1. 普通表结果：**重新授权完整对象目标 + 验证结构 / 可写映射 / PK / version**，通过后用新短租约写回。`writableMapping` 来自落库的 `statement_sources`（W2），但**必须重新验证**，不能直接信任历史值。
2. 临时对象结果：只读。
3. 截断产物不得被宣称为完整导出；客户端展示与导出入口都要读 `resultCompleteness`。
4. 同一 `executionId` 不能被新 `dbSessionId` 接替原绑定（CM-61 断言"相同配置新 session 不能接替原绑定"）。

## 9. 验收映射

用例编号与标题逐字取自 [连接 §16.7](connection-management.md#167-补充契约与边界用例)（含括号里的层标记，**不改写标题**）；「首次落地阶段」取自[开发计划 §17：补充契约的阶段归属](../../development/platform-development-plan.md#17-补充契约的阶段归属)，该表未单列的用例按计划中首次列出它的阶段门槛标注。本表只列**本文负责的那一层**：标 P7/P8 的行是本文为 W1 断言提供的落点，标更早阶段的行是本文为那些阶段提供的 schema / 列语义落点；两者都不得据此把整条用例计入 P7/P8 门槛——P7/P8 承接的补充契约以[开发计划 §17](../../development/platform-development-plan.md#17-补充契约的阶段归属) 的 P7/P8 行为准（当前为 CM-59、CM-63、CM-68、CM-70、CM-72 的 W1 断言），P7 的其余门槛见[开发计划 §11 P7](../../development/platform-development-plan.md#11-p7单实例团队-web-服务)，服务端侧的逐条落点在[团队服务 §16](team-server-and-auth.md#16-验收映射)。

| ①用例（[连接 §16.7](connection-management.md#167-补充契约与边界用例) 原标题） | ②首次落地阶段 | ③本文负责的层与断言落点 |
| --- | --- | --- |
| CM-05 跨用户/组织资源访问（H/W） | P1（[开发计划 §5 P1](../../development/platform-development-plan.md#5-p1抽取共享应用边界和前端传输契约) 门槛含 CM-01、03～07）；本文的 schema 断言随 P7 首批迁移落地 | **schema 层**：§3.1、§3.2 —— 全部主键以 `organization_id` 打头；跨组织查询在存储层查不到 |
| CM-06 前端伪造 owner（H/W） | P1（同上）；schema 断言随 P7 | **schema 层**：§3.3、§3.4、§4.3 —— `owner_principal_id` / `client_instance_id` 由服务端按 `RequestContext` 填充，不接受客户端自报；`organizationId` 不是连接配置属性 |
| CM-07 目标缺失与冲突（H/D） | P0（[开发计划 §4 P0](../../development/platform-development-plan.md#4-p0现状基线与契约测试夹具) 门槛含 CM-01、04、07）与 P1 | **schema 层**：§2.3 规则三、§3.3、§4.2 —— `requested_target` / `initial_namespace` 落库前归一化；不存在层级非 null、required null 在执行前拒绝 |
| CM-59 登出/权限撤销/CSRF（W） | P7/P8（§17：CM-59 的 W1 断言；[开发计划 §11 P7](../../development/platform-development-plan.md#11-p7单实例团队-web-服务) 门槛中的组织隔离与单实例认证部分） | **schema 层**：§2.1 W14/W15、§3.10、§4.3 —— 登录会话行、成员关系、固定角色与 ACL 可落库（字段语义见 [团队服务 §4.3](team-server-and-auth.md#43-服务端登录会话)、[§6.1](team-server-and-auth.md#61-角色与资源-acl)、[§6.3](team-server-and-auth.md#63-撤销传播)，`external_subject` 是 IdP `sub` 唯一落库处）；IdP 令牌副本与 attachment 令牌不落库；撤权将实际授权变更与 `permission_version` 递增同事务提交，提交后使缓存与池键失效；删除会话行等价于强制重新登录。CSRF 拒绝与订阅/排队的时机本身不在本文 |
| CM-61 持久化白名单与临时结果恢复（H/F） | P0/P1（§17：类型与 schema 断言）；P3 的 H、P4 的 F 不在本文 | **schema / 落库断言层**：§2.1、§2.2、§2.5、§3.4、§6.1、§6.2、§8.3 —— 落盘结构无 `dbSessionId`/`SessionHandle`/`resourceBindingId`/lease/cancel 句柄（§2.5 三条断言）；普通表可重新授权后写、临时结果只读、同配置新 session 不能接替原绑定 |
| CM-62 命名空间规范化矩阵（H/D） | P0/P1（§17：类型与 schema 断言）；P2 的 D 断言不在本文 | **schema 层**：§2.3 规则三、§3.3、§4.2 —— 缓存、权限、driver 与落库使用同一 `CanonicalTarget`；无文件路径误用 |
| CM-63 Attachment、TTL 与竞态（H/F/W1） | P3（§17：CM-61～74 的 H 断言）；**P7/P8（§17：W1 断言）** | **W1 层**：§2.2 F8、§3.2「不存在的表」、§3.5 不落盘行、§7.2 attachment 行 —— `attachmentToken` 与哈希不落盘；恢复后全部失效、同 principal 无令牌重附着拒绝；仅 `openSession` / 上下文替换的指纹与令牌绑定 owner `runtimeEpoch`（[团队服务 §10.4](team-server-and-auth.md#104-幂等回执存储)），TTL 与竞态取值以 [连接 §6.4](connection-management.md#64-attachment-与超期处理) 为准 |
| CM-64 无消费者、截断与有界 drain（H/D/F） | P2（§17：D 断言）；P3 的 H、P4 的 F 不在本文 | **schema 层**：§8.2、§8.3 —— 缓冲与产物有界、截断可见且原因落库 |
| CM-67 PoolKey 版本和多 database（H/D） | P2（§17：D 断言）；P3 的 H、P5/P6 的任务断言不在本文 | **schema 层**：§2.2 F13、§4.3、§7.2、§8.1 —— 旧缓存不回填、撤权即时生效（缓存与 `policyIsolationKey` 不落盘）；恢复后会话显式失效 |
| CM-68 候选替换发布与响应丢失（H/W1/WN） | P3（§17：CM-61～74 的 H 断言）；**P7/P8（§17：W1 断言）**；P9 的 WN 断言不在本文 | **W1 层（持久化投影侧）**：§3.5 落库时点与 `uq_idempotency_live_subject`、§6.3 profile CAS、§7.2 未终结幂等记录行 —— 接受记录在 SQL / Job 派发前提交；同 key 同摘要返回同 receipt；一次执行至多被一个未终结记录引用。候选 `prepared` 不可路由与提交原子性属 [团队服务 §10.3](team-server-and-auth.md#103-sessiondirectory-与-budgetcoordinator单实例)，不在本文 |
| CM-70 过期幂等键与记录删除（H/W1） | P3（§17：H 断言）；**P7/P8（§17：W1 断言）** | **W1 层**：§3.5、§6.2、§6.3、§7.2 —— 过期和删除后均不再执行；同 receipt、不同输入冲突；owner 重启后旧令牌 `SessionLost`；运行时 receipt / token 不落盘 |
| CM-72 错误、控制回执与能力审计（H/F/W1） | P0/P1（§17：类型与 schema 断言）；P4 的 F 不在本文；**P7/P8（§17：W1 断言）** | **schema / W1 层**：§2.4、§3.4、§3.7 —— 审计包含非敏感能力版本（W4），不含 live handle 与凭据（F2/F10）；`effect_outcome` 与 `state` / `result_completeness` 是各自独立的列，不被"请求成功"覆盖 |

## 10. 与既有设计的关系与本文边界

- **不重复定义 DTO 与错误码取值**。`DurableExecutionRecord` 的语义来自 [连接 §4](connection-management.md#4-dto-与字段定义) 与 [§4.4](connection-management.md#44-可落盘来源与运行时绑定)，错误码取值全集见 [连接 §4](connection-management.md#4-dto-与字段定义) 与 [§13](connection-management.md#13-错误重试和事件)；本文只定义持久化投影、列映射与表结构。
- **不重复定义端口**。`ProfileRepository` / `JobRepository` / `SecretProvider` / `ArtifactStore` / `BudgetCoordinator` 的方法签名以 [共享边界 §4.3](shared-boundaries-and-ports.md#43-持久化与授权端口) 与 [§4.6](shared-boundaries-and-ports.md#46-结果与事件端口) 为准，本文只给服务端实现落到哪张表。
- **不重复定义迁移运行方式**。何时执行、失败是否启动、是否与回退镜像配合由 [团队服务 §13](team-server-and-auth.md#13-schema-migration-运行方式) 定义；本文给版本策略、文件格式、expand/contract 判据与回滚边界，二者必须一致读。
- **沿用仓库已有先例**：`schema_migrations` / `schema_version` 版本表、`has_column` 探测式升级、legacy JSON 的 rename-aside 导入、`SyncTask` 的 `skip_serializing` 会话 id、收藏迁移的归档表护栏、`migration_run_history` 的禁列名断言。本文是把这些做法推广到服务端管理库，不是发明新机制。
- **扩展 [持久化存储（现状）](../backend/store.md)**：那份文档描述已实现的桌面本地存储，本地加密文件格式与主密钥后端细节也在那里与 [`store/key_store.rs`](../../../src-tauri/src/store/key_store.rs)；本文描述目标的服务端管理库与两者的映射，两者对同一主题的不同层次不构成冲突。
- **扩写 [概要 §5 统一领域对象](system-overview.md#5-统一领域对象)** 的"持久化：是/否"列：本文把"是"展开成具体表与列，把"否"展开成禁持久化清单与断言；并补齐 [开发计划](../../development/platform-development-plan.md) P1 与 P7 的交付项——P1 的"所有持久化路径排除运行时句柄"由 §2.2 + §2.5 落地，P7 的"ProfileRepository / JobRepository / Audit / ArtifactStore 的服务端实现和 schema migration"由 §3、§5、§6 落地。
- **本文不覆盖**：会话、lease、预算的**行为算法**（[连接 §6](connection-management.md#6-状态机与并发) 与 [§9](connection-management.md#9-连接池预算与清理)）；服务端进程形态、中间件链、OIDC、CSRF、404 不可见策略与 SSE 实现细节（[团队服务 §3–§9](team-server-and-auth.md#3-请求中间件链)）；Artifact 字节存储与出站网络策略的具体实现（[团队服务 §11](team-server-and-auth.md#11-artifactstore-服务端实现)、[§12](team-server-and-auth.md#12-出站网络策略)）；包边界、依赖护栏与 CI 门禁（[共享边界 §2](shared-boundaries-and-ports.md#2-包边界与依赖矩阵)、[§8](shared-boundaries-and-ports.md#8-依赖护栏与-ci-门禁)）；fake resource、故障注入与基准 harness（[P0 夹具详细设计](fake-runtime-fixtures.md)）。
- **产物 TTL 的具体时长**、RPO / RTO 数值、组织级留存策略：权威文档未给出默认值，本文显式不写死常量，交由部署配置与运维策略决定。
- 任何把 `dbSessionId` 恢复到磁盘、或让恢复出来的记录继续代表物理资源的方案。本文对此的立场是不可协商：落盘的是事实与来源，不是继续执行的资格。
