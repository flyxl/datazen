# DataZen 连接管理详细设计

> 状态：待实现的开发契约。基线：2026-09-30，`8592b0fe1`。本次文档交付不代表已完成重构。
> 读者：第一次参与 DataZen 的开发者。按“类型 → fake driver → 资源 → 会话 → 网关 → UI → Job → Web”顺序实现，不从修改 UI 当前库开始。
> 配套：[系统概要](system-overview.md)、[开发计划](../../development/platform-development-plan.md)。

## 1. 开发前先理解的六个对象

| 对象 | 保存什么 | 谁拥有 | 是否落盘 |
| --- | --- | --- | --- |
| ConnectionProfile | 地址、驱动配置、初始目标、secretRef、版本 | 组织/本地用户 | 是 |
| ExecutionTarget | connectionId + 命名空间 + 可选对象 | 用例/任务 | 是 |
| DbSession | 连续上下文、事务状态、资源绑定 | 编辑器/显式会话块/客户端 | 否 |
| ResourceLease | 对物理资源的独占、预算与清理 | Session 或 Job 阶段或一次操作 | 否 |
| Execution | 一次执行、取消句柄、来源和结果 | 请求调用者/Job | 执行记录可落盘 |
| Job | 计划、阶段、提交边界、恢复条件 | 后端 JobRuntime | 是 |

`connectionId` 不能作为 session key。`dbSessionId` 不能写进 profile、Workflow 定义或恢复检查点。共享配置不共享事务。tab 只是视图，连接使用范围由操作语义决定。

## 2. 当前代码定位与替换规则

| 当前文件 | 阅读重点 | 新实现中的去向 |
| --- | --- | --- |
| [connection_manager.rs](../../../src-tauri/src/services/connection_manager.rs) | ActiveSession、owner map、refs | SessionRegistry + ResourceManager |
| [connections.rs](../../../src-tauri/src/services/connection_manager/connections.rs) | 配置、隧道、driver.connect | 版本化配置解析 + 受预算约束建连 |
| [sessions.rs](../../../src-tauri/src/services/connection_manager/sessions.rs) | 按配置复用、release、reconnect | 明确 owner 的 session API、显式失效 |
| [driver_command](../../../src-tauri/src/commands/driver_command/mod.rs) | Command 网关 | adapter 进入统一 ExecutionGateway |
| [transaction.rs](../../../src-tauri/src/services/transaction.rs) | DDL 原子性与事务句柄 | 在同一 Lease 上管理事务；回滚失败必须上报 |
| [driver traits](../../../packages/driver-api/src/traits.rs) | connect、query、Command、事务 | 保留领域能力，补 opaque 固定资源契约 |
| [activeConnectionStore](../../../src/stores/activeConnectionStore.ts) | connectionId 索引的连接状态 | 连接可达性与编辑器 session 状态分开 |
| [QuerySidebarSection](../../../src/windows/connection/query/QuerySidebarSection.tsx) | SQL 推测上下文 | 仅做补全/浏览推测，实际上下文由后端更新 |
| [dedicatedDbSession](../../../src/lib/dedicatedDbSession.ts) | 三件套窗口准备/释放 | 明确目标，执行 Lease 归 Job |
| [workflow command_runtime](../../../src-tauri/src/workflow/command_runtime.rs) | 默认目标与 Command | 按 step/block 范围申请资源 |

旧 `ConnectionHandle` 有 `id` 与 `pool_id`，不能从名字推断固定物理连接。先做 driver 契约测试，再将 consumer 移到新接口。旧 API 过渡适配禁止把未知连接假称为有状态 session。

## 3. 不可破坏的不变量

| 编号 | 必须成立 |
| --- | --- |
| INV-01 | 身份来自 adapter，组织与 owner 不由请求体授予 |
| INV-02 | 同一固定 Lease 的物理会话在整个范围内连续，driver 不内部换连接 |
| INV-03 | 同一 session 最多一个普通执行；取消使用独立控制路径 |
| INV-04 | 实际上下文由 driver 确认；SQL 文本推断不能修改 observedContext |
| INV-05 | target 操作不改变任何其他 session 的默认库或事务 |
| INV-06 | 未消费完结果、仍执行、回滚失败或状态未知的资源不归池 |
| INV-07 | runtimeEpoch/dbSessionId 不匹配的执行与取消被拒绝 |
| INV-08 | 物理会话失效后返回 SessionLost，禁止透明重建；显式新建会话必须使用新 ID |
| INV-09 | 所有实际连接占用预算，包含 idle、cluster 和 cancel 连接 |
| INV-10 | lease 清理/关闭幂等，预算只释放一次 |
| INV-11 | 结果操作使用执行时来源，不用当前 UI 数据库 |
| INV-12 | Job 独立于窗口；提交未知不自动重试 |

## 4. DTO 与字段定义

本节给出可直接转为 TypeScript 契约的定义；Rust 使用 serde 的 camelCase 映射。所有 string ID 在 Rust 内部使用不同 newtype，禁止互换。64 位计数在 JSON 中编码为十进制字符串，避免 JavaScript 精度丢失。

```typescript
type Id = string;
type Counter = string;
type Timestamp = string;
type Availability = 'supported' | 'unsupported' | 'unknown';

interface NamespaceTarget {
  database: string | null;
  catalog: string | null;
  schema: string | null;
  path: readonly string[];
}

interface ObjectTarget {
  kind: string;
  name: string;
  signature: string | null;
}

interface ExecutionTarget {
  connectionId: Id;
  namespace: NamespaceTarget;
  object: ObjectTarget | null;
}

type OwnerRef =
  | { kind: 'editor'; clientInstanceId: Id; editorSessionId: Id }
  | { kind: 'job'; jobId: Id; stageId: Id }
  | { kind: 'workflowBlock'; jobId: Id; blockId: Id }
  | { kind: 'clientSession'; clientInstanceId: Id; purpose: string };

interface SessionHandle {
  dbSessionId: Id;
  runtimeEpoch: Id;
}

type TransactionState =
  | 'none'
  | 'active'
  | 'aborted'
  | 'unknown'
  | 'unsupported';

interface SessionContext {
  namespace: NamespaceTarget;
  searchPath: readonly string[] | null;
  effectiveIdentity: string | null;
  transactionState: TransactionState;
  autocommit: boolean | null;
  confidence: 'confirmed' | 'partial' | 'unknown';
}

type AttachmentState = 'attached' | 'detached' | 'expired';

type SessionState =
  | 'new'
  | 'opening'
  | 'ready'
  | 'executing'
  | 'reconfiguring'
  | 'closing'
  | 'closed'
  | 'lost';

interface SessionView {
  handle: SessionHandle;
  connectionId: Id;
  configRevision: Counter;
  owner: OwnerRef;
  initialTarget: ExecutionTarget;
  observedContext: SessionContext;
  contextRevision: Counter;
  state: SessionState;
  attachmentState: AttachmentState;
  activeExecutionId: Id | null;
  expiresAt: Timestamp | null;
}

interface OpenSessionRequest {
  initialTarget: ExecutionTarget;
  owner: OwnerRef;
  idempotencyKey: string;
}

interface CommandCall {
  command: string;
  input: unknown;
}

interface ExecuteInSessionRequest {
  handle: SessionHandle;
  expectedContextRevision: Counter | null;
  call: CommandCall;
  idempotencyKey: string;
}

interface ExecuteAtTargetRequest {
  target: ExecutionTarget;
  expectedConfigRevision: Counter;
  call: CommandCall;
  idempotencyKey: string;
}

interface SetSessionContextRequest {
  handle: SessionHandle;
  expectedContextRevision: Counter;
  desired: NamespaceTarget;
  idempotencyKey: string;
}

type CloseMode = 'requireNoTransaction' | 'rollbackAndClose';

interface CloseSessionRequest {
  handle: SessionHandle;
  mode: CloseMode;
}

type EffectOutcome =
  | 'notStarted'
  | 'completed'
  | 'rolledBack'
  | 'partiallyApplied'
  | 'unknown';

type ExecutionState =
  | 'queued'
  | 'running'
  | 'cancelRequested'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

// 执行终态 errorCode。与 ApiError.code 是两个独立命名空间，不得互相塞入对方取值。
// 网关按此枚举脱敏映射 driver 原始错误；driver 的错误文本、SQL 片段与绝对路径只进日志。
type ExecutionErrorCode =
  // 数据库返回错误；方言差异由网关归一，宿主与 UI 只见本枚举
  | 'sqlError'
  // 编解码/协议层失败，语句可能已送达
  | 'protocolError'
  // 精确取消已送达驱动；数据库生效范围仍由 effectOutcome 表达，可能为 unknown
  | 'cancelled'
  // 执行超过 timeout 且无法判定数据库生效范围
  | 'timeout'
  // 执行中物理资源丢失
  | 'resourceLost'
  // 放弃消费或背压导致的截断中止
  | 'pipelineAborted'
  // 已建立执行记录后由宿主二次校验拒绝：权限撤销、额度、队列、sql_guard；尚未建立记录时直接返回 ApiError，不产生本枚举取值
  | 'hostRejected';

interface ExecutionReceipt {
  executionId: Id;
  streamId: Id;
  state: ExecutionState;
}

interface ResultProvenance {
  organizationId: Id;
  principalId: Id;
  connectionId: Id;
  configRevision: Counter;
  contextBefore: SessionContext;
  contextAfter: SessionContext;
  requestedTarget: ExecutionTarget;
  capabilitySnapshot: CapabilitySnapshot;
  executedAt: Timestamp;
}

interface CapabilitySnapshot {
  driverId: string;
  driverVersion: string;
  protocolVersion: number;
  capabilityRevision: Counter;
  confirmed: Readonly<Record<string, unknown>>;
}

interface RuntimeResultBinding {
  handle: SessionHandle;
  resourceBindingId: Id;
  executionId: Id;
}

interface ExecutionView {
  executionId: Id;
  state: ExecutionState;
  effectOutcome: EffectOutcome;
  provenance: ResultProvenance | null;
  artifactIds: readonly Id[];
  resultCompleteness: 'pending' | 'complete' | 'truncated';
  truncationReason: string | null;
  errorCode: ExecutionErrorCode | null;
  runtimeBinding: RuntimeResultBinding | null;
}

interface EventEnvelope<T> {
  streamId: Id;
  sequence: Counter;
  runtimeEpoch: Id;
  executionId: Id | null;
  jobId: Id | null;
  sessionHandle: SessionHandle | null;
  contextRevision: Counter | null;
  payload: T;
}
```

字段校验：所有 NamespaceTarget 字段必传，空字符串非法；层级含义与 `null` 规则统一见 4.3。对象操作必须传完整身份，不能把未指定 schema 猜成 public/dbo。`path` 是驱动命名空间 ID，不是文件路径；重复表达同一层级必须一致，否则 TargetConflict。

OwnerRef 是请求的归属意图。后端确认 editor 属于当前 client，job/block 属于已授权 Job；不能允许用户声称任意 job owner。组织和用户绑定存于服务端记录。

客户端另有 `{ backendId, handle }` 包装。后台数据库执行身份为服务端生成的不可伪造 `executionIdentityKey`；PoolKey 使用该 key，不使用客户端传入用户名。

### 4.1 服务接口与补充响应

以下声明与上节类型一起构成连接模块的客户端接口，不依赖 Tauri Channel。流传输适配为 AsyncIterable；客户端结束迭代只取消订阅，用户主动取消 SQL 必须调用 cancelExecution；runtime 可按 7.7 的明确放弃消费政策超期取消。

```typescript
interface ProfileView {
  connectionId: Id;
  name: string;
  driverId: string;
  configRevision: Counter;
  credentialRevision: Counter;
  initialNamespace: NamespaceTarget;
  publicOptions: Readonly<Record<string, unknown>>;
  credentialConfigured: boolean;
  enabled: boolean;
}

interface ContextChangeReceipt {
  session: SessionView;
  replacedSessionId: Id | null;
  attachmentToken: string | null;
}

interface CloseReceipt {
  dbSessionId: Id;
  state: 'closing' | 'closed' | 'lost';
  effectOutcome: EffectOutcome;
  resourceRelease: 'pending' | 'confirmed' | 'quarantined';
}

interface CancelReceipt {
  executionId: Id;
  disposition: 'requested' | 'unsupported' | 'alreadyFinished';
  state: ExecutionState;
}

interface StatementResultSource {
  executionId: Id;
  statementIndex: number;
  context: SessionContext;
  relation: ExecutionTarget | null;
  writableMapping: 'verified' | 'readOnly';
}

type ConnectionEvent =
  | { kind: 'sessionChanged'; session: SessionView }
  | { kind: 'executionChanged'; execution: ExecutionView }
  | { kind: 'resultChunk'; artifactId: Id; chunkIndex: Counter; source: StatementResultSource }
  | { kind: 'streamResetRequired'; reason: string };

// createProfile 属配置写入，不绑定会话；其余运行时会话操作绑定 owner runtimeEpoch。
type IdempotentOperation = 'createProfile' | 'openSession' | 'executeInSession' | 'executeAtTarget' | 'setSessionContext' | 'startJob';

interface SubmissionToken {
  idempotencyKey: string;
  expiresAt: Timestamp;
}

interface OpenSessionReceipt {
  session: SessionView;
  attachmentToken: string;
}

interface AttachmentRequest {
  handle: SessionHandle;
  attachmentToken: string;
}

interface ConnectionService {
  listConnections(): Promise<readonly ProfileView[]>;
  issueSubmissionToken(operation: IdempotentOperation, handle: SessionHandle | null): Promise<SubmissionToken>;
  openSession(request: OpenSessionRequest): Promise<OpenSessionReceipt>;
  attachSession(request: AttachmentRequest): Promise<SessionView>;
  detachSession(request: AttachmentRequest): Promise<SessionView>;
  getSession(handle: SessionHandle): Promise<SessionView>;
  executeInSession(request: ExecuteInSessionRequest): Promise<ExecutionReceipt>;
  executeAtTarget(request: ExecuteAtTargetRequest): Promise<ExecutionReceipt>;
  setSessionContext(request: SetSessionContextRequest): Promise<ContextChangeReceipt>;
  closeSession(request: CloseSessionRequest): Promise<CloseReceipt>;
  getExecution(executionId: Id): Promise<ExecutionView>;
  cancelExecution(executionId: Id): Promise<CancelReceipt>;
  subscribeEvents(streamId: Id, afterSequence: Counter | null): AsyncIterable<EventEnvelope<ConnectionEvent>>;
}
```

内部 ConnectionProfile 的完整字段为：organizationId、connectionId、name、driverId、configRevision、initialNamespace、driverOptions、secretRef、credentialRevision、networkRouteRef/revision、readOnly、policyRef、enabled、createdAt、updatedAt。driverOptions 按 schema 分离敏感字段，秘密内容存 SecretProvider。ProfileView.publicOptions 只含可见且非敏感字段；列表不返回 password、token、TLS private key 或可直接解密材料。

执行来源分两层：ExecutionView 的 provenance 记录整个调用前后状态；每个 StatementResultSource 记录该结果产生时的上下文。同一脚本 A→B 的两个结果不能都贴最终 B。driver 无法观察批次内准确上下文时返回 partial/unknown 且 readOnly，不猜测 relation 或提供可写映射。

### 4.2 内部记录

| 记录 | 完整必要字段 |
| --- | --- |
| SessionRecord | SessionView、organizationId、principalId、workerId、executionIdentityKey、credentialRevision、actor queue、leaseId、lastBusinessActivity、attachmentState、attachmentTokenHash、detachedAt、resourceBindingId、单调 deadline |
| LeaseRecord | leaseId、providerId、opaqueResource、owner、scope、state、budgetPermit、tunnelLease、baseline、cleanupDeadline |
| PoolKey | organizationId、connectionId、configRevision、credentialRevision、executionIdentityKey、policyIsolationKey、driverResourceKey、networkRouteRevision |
| ExecutionRecord | 请求指纹、身份、target、runtimeEpoch、resourceBindingId、状态、effectOutcome、cancelHandle、结果来源、时间、事件 sequence |
| JobCheckpoint | 稳定目标/版本/映射指纹、已提交边界、核验证据、恢复策略；不保存 live session/lease/cursor |

PoolKey 不含密码原文。`driverResourceKey` 由驱动描述 database-bound 资源及初始化基线；不同 database 是否可共享 pool 由驱动证明，不由宿主猜测。

### 4.3 命名空间规范化契约

驱动 ResourceDescriptor 注册 namespaceShape：声明 database/catalog/schema 是否存在、pathSegments 的顺序与语义、名称大小写及 canonical ID 规则、可接受的别名映射。Command definition 另注册 targetRequirements：每个层级 required/optional/forbidden、是否允许对象、是否允许使用该 session 已确认默认值。两者必须同时校验，不能用一个全驱动 required 列表代替操作需求。

处理顺序：校验 DTO 所有字段存在 → 按 namespaceShape 拒绝不存在层级的非 null 值 → 合并字段/path 别名并拒绝冲突 → driver 规范化 ID → 校验本操作 targetRequirements → 输出 CanonicalTarget。不存在的层级只能 null；存在但未指定的层级为 null，仅 optional 允许。executeAtTarget 不从 UI 或其他 session 补值；executeInSession 仅在 Command 明确允许时使用该 session 已确认值。无法确认仍返回 TargetRequired。

| 用途示例 | 规则 |
| --- | --- |
| PostgreSQL session 打开 | database required，schema 可 optional；观察 searchPath 独立于对象身份 |
| PostgreSQL 对象操作 | database/schema/object required，不猜 public |
| SQLite | 文件由 profile 配置解析；path 不承载文件系统路径 |
| Redis | db index 的映射、编码与范围由 driver schema 定义，不把它通用定义成 path |

path 默认只承载未被 database/catalog/schema 表达的扩展层级；若 driver 兼容 schema 明确声明别名，双方同时提交必须规范化后相等。ResourceManager、权限检查、缓存和执行都使用同一 CanonicalTarget；前端展示名称不参与身份比较。

### 4.4 可落盘来源与运行时绑定

ResultProvenance、StatementResultSource、结果数据/映射可以持久化；SessionHandle、RuntimeResultBinding、EventEnvelope.sessionHandle、取消句柄、lease/cursor **永不落盘**，也不能进入持久化日志、审计或 Job checkpoint。ExecutionView 是实时响应；仓储使用独立 DurableExecutionRecord DTO，不直接序列化它。事件归档只写显式白名单投影，不能直接序列化 EventEnvelope。

DurableExecutionRecord 保存 executionId、身份、state/effectOutcome、provenance、statementSources、artifactIds、resultCompleteness/truncationReason、errorCode 和时间；不包含 runtimeBinding。能力快照保存驱动/协议/能力版本及本次确认的非敏感能力，作为计划核验与审计证据，不保存凭据。RuntimeResultBinding 仅由 execution registry 在内存附加，资源丢失、关闭或替换立即失效。

恢复普通表结果可以重新授权完整对象目标，验证结构/可写映射和 PK/version 后短租约写回。临时对象结果可恢复数据供阅读，但恢复后不能写回；禁止通过连接配置或上下文指纹寻找“等价”session。configRevision/contextRevision 用于版本与上下文冲突，不能证明物理资源连续性。

## 5. 驱动资源契约

### 5.1 新增契约的落点

在 `packages/driver-api` 增加传输无关的 opaque resource、能力、错误和 trait；以下文件名为拟新增，不是现有文件：`resource.rs`、`session.rs`、`capabilities.rs`。Host runtime 的具体池、Tokio actor、channel 和 semaphore 不暴露到 Driver API。

| 操作 | 输入 | 返回 | 强制语义 |
| --- | --- | --- | --- |
| describeResource | 经验证配置、身份范围、目标、用途 | ResourceDescriptor | 是否固定/可复用、资源 key、所需能力 |
| acquireResource | 建连材料、目标、scope、BudgetPort | opaque ResourceHandle | 固定范围内底层会话不改变；所有建连经预算 |
| executeOnResource | ResourceHandle、executionId、CommandCall、ResultSink | ExecutionCompletion | 不自行再从随机 pool 取连接 |
| observeSession | ResourceHandle | SessionObservation | 在同一物理资源读取；不可观察明确返回 Unknown |
| changeContext | ResourceHandle、desired | Confirmed/RequiresReplacement/Unsupported | 不改宿主配置、不默默重连 |
| begin/commit/rollback | ResourceHandle、事务选项 | TransactionObservation | 必须作用于同一资源；提交不确定返回 Unknown |
| requestCancel | 精确 execution cancelHandle | Requested/Unsupported/AlreadyFinished | 不调用 session-wide cancel 作为隐式回退 |
| resetResource | ResourceHandle、baseline | Clean/Discard | 仅覆盖协议层与初始化基线，不含 §6.5 已登记句柄；成功必须满足全部清理条件 |
| closeResource | ResourceHandle | Closed/CloseUnconfirmed | 幂等；未证实关闭不能伪称预算已回收 |

资源句柄仅在对应 provider/worker 有效，不在客户端传递。能力接口使用标准库类型、API 自有类型和 serde；async-trait 方式遵守现有契约。

ResourceDescriptor 必须包含 providerId、resourceKey、sessionContinuity、reusePolicy、initializationRequirements、connectionCostPolicy 和 namespaceShape；Command definition 提供操作级 targetRequirements。ResourceHandle 是 API 定义的不可伪造 opaque ID，由 provider 校验所有权和 epoch。

ExecutionCompletion 必须包含 completionStatus、effectOutcome、statementResults、contextBefore/after、transactionObservation、sessionHandles、protocolDrained、resourceHealth。`sessionHandles` 是本次执行交出的会话级句柄（§6.5）：driver 必须如实返回，没有则为空数组，runtime 只对已登记项负责终态后的句柄生命周期，宿主不直接消费句柄。SessionObservation 的 unknown 字段不能填入 initialTarget 假充确认。ResultSink 提供带字节大小的异步写入/完成/失败方法，写入等待表示背压；不暴露 Tokio channel 类型。

`BudgetPort` 返回 opaque 许可，并支持按实际物理连接申请/释放。SQLx 驱动必须约束内部 pool 的真实建连；仅在 acquire 时计数会漏掉 idle 连接。第三方 SDK 无法统计实际 socket 时声明保守资源成本/硬上限，或在严格服务端预算模式拒绝该能力，不能报告精确值。

### 5.2 能力粒度

| 能力 | 值/范围 |
| --- | --- |
| statefulSession | supported/unsupported/unknown |
| namespaceSwitch | inPlace/requiresReplacement/unsupported/unknown |
| contextObservation | full/partial/unsupported |
| transactionObservation | full/partial/unsupported |
| sessionScopedHandles | supported/unsupported/unknown（是否返回事务、游标、服务端预处理句柄） |
| resetForReuse | verified/unsupported |
| preciseCancel | supported/unsupported/unknown |
| snapshots | perTable/perDatabase/coordinated/unsupported |
| transactions | 支持的隔离级别、savepoint、对象/引擎限制 |
| ddlAtomicity | 按计划操作确认，可包含非事务操作 |

描述阶段和实际连接握手都校验能力。Unknown 不开启需要保证的功能。遗留驱动可以支持独立受控操作，但不能因为“池大小为 1”就声明固定会话：底层重建和不同 consumer 仍会破坏语义。

## 6. 状态机与并发

### 6.1 Session 状态机

| 当前状态 | 输入/条件 | 下一状态 | 动作 |
| --- | --- | --- | --- |
| New | 首次执行/显式连接 | Opening | 申请预算、建立固定 Lease |
| Opening | 建立和初始化成功 | Ready | 绑定 resourceBindingId，发布确认状态 |
| Opening | 超时/取消/失败 | New | 清理部分资源、发布错误；未执行 SQL |
| Ready | 开始查询 | Executing | 原子登记 activeExecution |
| Executing | 完成且资源健康 | Ready | 观察状态、确认结果、释放 active 标记 |
| Ready | context change | Reconfiguring | 校验 revision、执行切换 |
| Reconfiguring | 原地成功/失败且旧状态确认 | Ready | 成功更新 revision；失败保留实际状态 |
| Ready/Executing/Reconfiguring | 协议损坏或连接丢失 | Lost | 禁止后续执行、终结相关执行 |
| New/Ready | 关闭允许 | Closing | 停止接受新请求、处理事务与清理 |
| Executing | 强制关闭 | Closing | 发取消；等待完成，不立即归池 |
| Closing | 资源确认释放 | Closed | 终态 tombstone、预算核销 |
| Lost | 清理关闭 | Closed | 幂等；恢复另建新 session |

Lost/Closed 不能直接转 Ready。用户“重新连接”调用 openSession 获得新 ID。需要重连的切库采用两阶段替换：先建立新 session，成功后 UI 原子替换绑定，旧 session 再关闭；不是复用旧 ID。

### 6.2 Lease 状态机

`Reserved → Opening → InUse → Cleaning → Returned/Closed`；未知或清理失败进入 `Quarantined → Closed`。Quarantined 不能再次 acquire。连接关闭未确认时保留预算占用或转为待核验占用，不能立即归零。

### 6.3 锁与队列

每个 session 一个 actor，普通消息 FIFO 串行。execution cancel registry 和 shutdown control 独立，避免取消被长查询堵在后面。运行中的 driver future 与控制信号由 actor 协调；执行任务不能绕开 actor 持有同一个资源。

注册表的读写锁只用于定位 actor 或插入记录，取 Arc 后释放锁，再 await 网络、预算或 driver。禁止持全局锁等待外部服务。多端预算先统一预留，或按稳定 endpoint key 排序且等待失败释放已持许可，避免 AB/BA 死锁。

客户端 dbSessionId 和 runtimeEpoch 必须精确匹配。下拉切库要求 expectedContextRevision；有新状态时返回 ContextConflict，重新读取后由用户操作重发。普通编辑器执行也默认携带 revision，阻止旧界面在新上下文执行；仅显式 session 脚本内部按队列自然延续。

### 6.4 Attachment 与超期处理

Attachment 是独立维度：attached/detached/expired，与 SessionState 组合；不新增混入 SessionState 的 Detached。attached 的 session 仍可能 Lost；detached 的 session 仍可能 Executing。Job 不使用交互 attachment TTL。

ConnectionService 的 attachSession/detachSession 接受 AttachmentRequest，返回 SessionView。openSession 返回 OpenSessionReceipt，通过独立敏感字段返回 attachmentToken，仅发给 owner，不能进入事件、URL、日志或磁盘。前端只保存在内存；令牌丢失不能仅凭同 principal 恢复，需显式关闭/新建会话。服务端保存内存哈希，认证组织/principal、原 clientInstanceId/editorSessionId 及令牌后才允许重附着；重复 detach 不重置 detachedAt，重复 attach 幂等。返回失败使用 PermissionDenied 的不可见资源策略。

| 条件 | 到期点/动作 |
| --- | --- |
| attached，无事务，非执行中 | lastBusinessActivity + 30 分钟，关闭 |
| attached，active/aborted/unknown 事务或游标（§6.5 已登记句柄），非执行中 | lastBusinessActivity + 5 分钟，在原资源上回滚/关闭句柄后再关闭 |
| detached | detachedAt + 60 秒；与已适用 idle deadline 取最早 |
| grace 内 attach | 取消掉线 deadline，不刷新 lastBusinessActivity/事务期限 |
| 执行中 | 暂停 idle 检查，不暂停掉线 grace；grace 到期请求取消并关闭 |
| 关闭/取消进行中 | 独立 cleanup deadline，超时隔离，记录真实 effectOutcome |

lastBusinessActivity 在已接受的实际业务操作开始和终结时更新；登录心跳、读取状态、订阅和 attach 不刷新。执行中没有 detached deadline 时由执行 timeout/管理员政策管理。每个 TTL 起点在服务端用单调时钟保存；expiresAt 是最早适用 deadline 的 UTC 投影，执行中无适用期限为 null。超期动作由同 actor 串行裁决，expired 不可重附着；竞态中已进入 Closing 就不能恢复 Ready。

### 6.5 会话级资源句柄登记

Driver Command 可能返回超出该次 execution 生命周期的会话级句柄：事务、游标、服务端预处理对象。句柄由 §5.1 的 `ExecutionCompletion.sessionHandles` 显式交出，形状为 `SessionHandleRef`：

```typescript
interface SessionHandleRef {
  handleId: Id;
  kind: 'transaction' | 'cursor' | 'serverPrepared';
  resourceId: Id;        // 所属物理资源；换资源后失效，不得在新 resource 上复用
  runtimeEpoch: Counter; // 归属 epoch，owner 更换后旧句柄一律拒绝
  closed: boolean;
}
```

句柄一经返回，必须在返回 execution 终态之前向 session actor 登记；未登记的句柄视为非法，runtime 拒绝把它交给宿主。

execution 终态只表示该次语句结束，不表示句柄失效。命令层自管的句柄映射不算登记。

actor 对已登记句柄承担与 Lease 相同的引用责任，并据此驱动：

- §6.4 的 idle 期限：存在未结束事务或游标时按 5 分钟规则，不进入 30 分钟关闭
- §7.5 closeSession 的回滚与终结顺序
- §9.4 归池前置中的"事务终结"：宿主侧判定的是已登记句柄为空
- 淘汰、替换或隔离时，先在原 resource 上回滚/关闭句柄并从 actor 注销，确认后才释放资源；不得在新 resource 上复用旧句柄

句柄登记只存在于内存，随 actor 终止而失效。恢复流程不得重建句柄，只允许以新 dbSessionId 显式重建。

## 7. 方法处理顺序

### 7.1 openSession

1. adapter 认证，构造 RequestContext。
2. 校验 owner 属于调用者，授权 connection/use + session/open。
3. 读取 profile 和版本，解析初始目标；必需 database 缺失返回 TargetRequired。
4. 校验驱动支持该会话语义和执行环境；配置 schema 校验通过。
5. 按 `(组织, principal, client, operation, idempotencyKey)` 查询去重。相同键不同输入返回 IdempotencyConflict。
6. 创建 New session，生成唯一 dbSessionId，contextRevision=0，observedContext=unknown。
7. 写注册表/目录和去重记录，再返回 OpenSessionReceipt；此时物理连接数量不变。

首次 execution 进入 actor 才建立固定 Lease，建连成功时生成内部 resourceBindingId；公开句柄只有 runtimeEpoch + dbSessionId，首版不设置恒为 1 的 generation。建连失败且未执行 SQL 可以在同一 New session 重试。已经建立的物理会话丢失则进入 Lost 并返回 SessionLost，禁止用原句柄透明替换资源；用户显式新建会话时分配新 ID。后续排队请求仍要重新校验 contextRevision。

### 7.2 executeInSession

1. 检查 authentication、organization、owner、runtimeEpoch/dbSessionId、当前权限。
2. 校验 Command definition、input schema、访问级别、预算/队列上限，规范化输入。
3. 原子登记幂等请求与 executionId。接受后返回 receipt，不等 SQL 完成才生成 ID。
4. 进入 actor；实际执行前再次检查权限、handle 和 expectedContextRevision。
5. New session 按 7.1 建立 Lease；Ready 使用已有 Lease；Lost/Closed 明确失败。
6. 保存 contextBefore，将精确取消句柄绑定 executionId + runtimeEpoch + resourceBindingId + opaque resource。
7. 在同一 Lease 调用 driver；任意用户 SQL 保留原始顺序，不预扫描最后 USE 后先切库。
8. driver 按方言正确处理批次/分隔符，返回语句或批次结果；Host 不用分号正则拆 SQL。
9. 消费结果/完成协议；即使 SQL 报错也尝试观察实际上下文和事务状态。
10. 更新 contextRevision（任何可见观察快照变化，包括 confirmed→unknown，都增加；确认无变化不增加）、保存结果来源和 effectOutcome。
11. 注销取消句柄，发布终态。确认资源健康后进入 Ready，否则 Lost/Quarantined。

若 observe 失败但连接健康，只把对应字段标记 Unknown，禁止依赖未知事务状态的切库、清理复用或保证原子性的操作；允许用户显式结束会话。不能把所有未知情况强行显示成无事务。

### 7.3 executeAtTarget

1. 验证 profile 版本、完整对象目标、权限和 Command schema。
2. 任意 SQL 在该操作范围内使用独立固定 Lease；它不获得跨请求会话连续性。
3. 受控对象 Command 可借共享干净资源，初始化目标或使用 driver 完整限定引用。
4. 不支持目标定位则 TargetUnsupported，不回退默认库。
5. 执行、观察、生成来源；未结束事务必须回滚。
6. 结果完全消费后 reset；只有 Clean 才归池，其余关闭。
7. 同一调用中的 SQL 状态变更仅影响该 Lease；下一个独立调用重新初始化目标。

### 7.4 setSessionContext

1. 验证 handle、owner、权限和 revision，将操作放入 actor。
2. New 且从未建立资源时，只更新经验证的 initialTarget、增加 contextRevision，observedContext 保持 Unknown，不建连；不存在事务，无需走替换流程。
3. Executing 时 UI 禁用切库；API 排队时仍在实际处理前检查 revision。
4. 已建立资源且 Unknown/Active/Aborted 事务时，首版 UI 切库返回 TransactionResolutionRequired；先显式提交/回滚。用户原生 SQL 是否允许 USE 由数据库语义决定，不改写 SQL。
5. inPlace：driver 切换并观察；成功后发布已确认上下文。
6. requiresReplacement：保留旧 session，建立仅内部可见的候选资源与候选记录。候选不登记到公共 SessionRegistry/SessionDirectory，不接受执行和订阅。确认候选成功后，以内存替换操作记录为提交依据，在旧 actor 内停止旧 session 发放新执行，通过 12 节的内存目录提交协议原子发布新 session 与 replacement receipt、将旧 session 标记 Closing，再把旧资源移交 cleanup。目录发布失败不得返回成功，保持提交屏障直到恢复发布；提交前失败销毁候选，提交后失败只能恢复同一 receipt，不能重新建立候选。返回新 session 后 UI 原子切换绑定。预算不足或建连失败，销毁候选并保留旧 session。
7. unsupported/unknown：明确错误，不创建假成功状态。

`setSessionContext` 返回 `ContextChangeReceipt { session: SessionView, replacedSessionId: string | null, attachmentToken: string | null }`，BackendClient 对应方法据此原子更新绑定。替换时返回新 session 的 attachmentToken，原地切换为 null；旧 token 不授予新 session 附着权，token 只进入调用者内存，不能进入事件。

切换操作按 idempotencyKey 登记：网络丢失后同键重试返回原 receipt，不再建立候选连接。旧 session 的授权状态读取可返回 replacement ID，帮助客户端恢复绑定；未授权调用者看不到该 ID。两阶段是“建立候选 → 服务端提交替换”，不等待浏览器确认才释放旧资源；客户端离线不会造成无限保留。

### 7.5 closeSession

1. 验证归属，幂等查 tombstone；关闭状态下不再接受新请求。
2. New 且从未建连可以直接关闭。已建连 requireNoTransaction 在 Active/Aborted/Unknown 时返回 TransactionResolutionRequired，不关闭。
3. rollbackAndClose 请求取消运行中 execution，等待停止/取消超时。
4. 能确认事务时显式 rollback；失败记录 RollbackFailed/Unknown，不能返回“已回滚”。
5. 任意用户 SQL session 首版直接关闭物理资源；不复用未知会话状态。
6. 关闭未确认进入隔离，稍后核验；释放隧道引用、核销预算只执行一次。
7. 记录 Closed/Lost 和实际效果；短期保留 tombstone 供重复请求查询。

Drop 仅做同步兜底标记并把资源移交 runtime cleanup queue；异步 rollback/close 必须被显式 await。cleanup queue 在应用退出时有界 drain，不能依赖被丢弃的 unawaited future。

### 7.6 cancelExecution

查 execution 与当前权限 → 验证内部取消绑定的 executionId/runtimeEpoch/resourceBindingId → 对活动执行调用 driver 独立控制路径 → 按请求结果返回 CancelReceipt。driver 确认 Requested 后标记 cancelRequested；unsupported 保留当前执行状态；已经终结返回 alreadyFinished。取消与完成竞态由 execution registry 原子处理；终态优先，不重新激活取消句柄。用户可以显式关闭自己的会话。

取消请求接受后返回 `{ executionId, disposition: 'requested', state: 'cancelRequested' }`；只有 terminal event/getExecution 的终态才能显示“已取消”。超时可以隔离/关闭资源，写入结果未知时仍为 Unknown，不能伪称 CancelledRolledBack。

### 7.7 结果订阅与放弃消费

订阅仅观察已接受执行。结束 AsyncIterable、关闭结果视图或网络断线都不自动 cancelExecution，也不立即释放固定编辑器资源。ResultSink 将数据写入有界内存/ArtifactStore，事件订阅慢不能把无界数据留在内存。每订阅事件队列最大 256 条/1 MiB，先触及任一上限即停止订阅并发 streamResetRequired（无法发送则断开）；客户端读快照，不继续积压。

首版每 execution 未消费缓冲上限 8 MiB、无消费者等待 30 秒、放弃后的协议 drain deadline 10 秒，均可配置。ArtifactStore 的每 execution 字节上限必填，桌面默认 256 MiB，团队默认 1 GiB，仍受组织总产物额度限制；达到上限停止写入并按下面流程处理，不能无限 spool。订阅端收到结果截断原因，ExecutionView 记录产物完整性，不把截断产物宣称完整导出。

处理流程：无订阅者时继续在额度内持久化 → 等待期限或产物额度耗尽后，普通查询切换到 bounded discard/drain → 能完成则保留 SQL 终态，结果标为 truncated → 不能在 drain deadline 内结束则请求精确取消 → 等协议终结、观察事务/健康 → 仍未知则关闭/隔离资源。bounded discard 按固定块读取并丢弃，不累计整个结果。流水线导出/迁移由 Job 的背压与任务策略控制，不适用普通结果放弃策略。

对固定 session：协议与事务确认健康时可回 Ready，取消不必然丢失临时状态；有活动事务保留明确状态等待用户处理。对短 Lease：还需 9.4 全部检查和 reset Clean 才归池。用户显式 closeSession 则始终结束该 session；关闭结果视图不能隐式触发它。unsubscribe 后仍可 getExecution/重新订阅；事件过期用快照，已截断行不能假装可重放。

## 8. multidb 与用户 SQL

命名空间枚举、原地切库、连接绑定数据库、跨库引用是四种独立能力。浏览树位置、补全推测位置和 observedContext 分开存储。

MySQL USE 改变当前会话默认数据库；完整限定对象名不等于切库。[MySQL USE](https://dev.mysql.com/doc/refman/8.0/en/use.html)。PostgreSQL 连接绑定一个数据库，切 database 需要新连接；search_path 属于库内上下文。[PostgreSQL 数据库边界](https://www.postgresql.org/docs/current/manage-ag-overview.html)。SQL Server/Azure SQL 差异由 driver 运行时能力确认，[USE 官方说明](https://learn.microsoft.com/en-us/sql/t-sql/language-elements/use-transact-sql?view=sql-server-ver17)。

用户执行 `A 查询 → USE B → B 查询失败` 时，默认停止后续语句，实际当前库保留 B。执行 `USE 无权限库` 失败时保留确认的旧库；切换成功但观察/网络状态无法证明时显示 Unknown，不推断成功或失败。

禁止为了目标定位通用改写任意 SQL 的所有关系名。受控对象 SQL 可以由 driver 渲染限定名；任意 SQL 用真实会话目标。变量、过程、动态 SQL 和用户直接 BEGIN/COMMIT 都要求实际状态观察。

## 9. 连接池、预算与清理

### 9.1 默认资源政策

| consumer | 默认策略 | 长持有条件 |
| --- | --- | --- |
| QueryPanel | 懒固定 session | 第一次执行至关闭/失效 |
| TablePanel | 短操作 Lease | 显式事务/快照/游标 |
| 表编辑 | 本地 ChangeSet，保存时短事务 | 手工事务模式才长期持有 |
| 结构/ER/补全 | 元数据资源 | driver 需要一致读取范围 |
| 仪表盘/监控 | 受限短租约 | 驱动声明专用连续资源 |
| 导出 | Job 阶段固定 | 流/游标/快照结束 |
| 三件套 | Job 声明多端阶段资源 | 事务、快照、session mode |
| Workflow | step/block 声明范围 | session/transaction block |
| AI/MCP/Wapp | 目标操作 | 显式受控 session |

### 9.2 可调初始值

这是首版配置起点，不是当前默认值或性能结论。所有值通过 settings/admin 配置，测试注入更小值。

| 项 | 初始值 | 行为 |
| --- | --- | --- |
| 短操作 pool | min=0、max=2/PoolKey | 还受服务级预算约束 |
| pool idle TTL | 60 秒 | 空 pool 元数据按 LRU 可删除；有资源的 pool 先关闭再删除 |
| 空 pool 元数据条目上限 | 32 个 | 每 pool min=0、max=2，仍受服务总预算约束 |
| 每用户已连接编辑器 | 5 | Opening 时预留名额；资源仍存活的 Ready/Executing/Closing/Lost 占用至确认释放 |
| 桌面总目标连接 | 16 | idle、任务和控制资源都计入 |
| 团队单数据库服务额度 | 20 | 管理员按目标 max connections 调整 |
| acquire timeout | 10 秒 | 返回 ResourceBusy，可取消等待 |
| session 普通队列 | 32 请求 | 超过拒绝，不无界积压 |
| 每用户逻辑 session / 每组织逻辑 session | 100 / 1000 | New 也计数；超限 SessionQuotaExceeded；Job 不冒充编辑器 session |
| client 掉线保留 | 60 秒 | 原 owner + attachment 凭据重附着，规则见 6.4 |
| 无事务会话业务空闲 | 30 分钟 | 明确过期关闭，不默默重建 |
| 空闲事务 | 5 分钟 | 请求回滚关闭，记录实际结果 |
| cancel/cleanup deadline | 10 秒 | 不能确认则隔离并核验 |
| 数据缓冲 | 每 pipeline 8 MiB | 按字节背压；超大单值走单独上限/分块 |

额度和 timeout 使用单调时钟判断持续时间；对外 expiresAt 使用 UTC。登录心跳不能刷新 lastBusinessActivity；执行期间不以 idle 判定回收。事务状态 Unknown 按保守事务上限处理。

### 9.3 预算会计

预算覆盖组织、用户、数据库服务、worker 和 Job。占用集合包含 Opening、InUse、Cleaning、idle pool、Quarantined 和 control sockets。归还到 idle pool不释放物理连接预算，只有实际 close 才释放。

cancel 所需控制连接预留在同一预算内，不能在满池时无限额外建连。多 profile 指向同一服务需 driver 服务身份与管理员资源组辅助聚合；无法可靠识别时使用显式配置组，不能声称自动精确识别。

多 worker 使用节点额度或全局 permit。失去协调器续约停止新建连接；旧额度在确认 worker 隔离/连接关闭之前不重新分配，以免网络分区超额。

### 9.4 归池前检查

必须全部成立：执行结束、结果协议完全消费、事务终结、锁释放、默认命名空间恢复、角色/会话变量/编码恢复、临时对象/预处理状态按 driver 契约处理、连接健康。

归池条件是宿主检查全部通过 AND driver 返回 Clean。宿主检查执行终态、无活跃消费者/取消句柄、已收到 protocolDrained、预算和 owner 合法、以及 §6.5 已登记句柄为空；driver 负责协议、初始化基线与健康检查。事务与游标由宿主按 §6.5 的登记记录判定，driver 不重复负责：driver 对已交出的句柄没有可见性，其 Clean 不构成事务终结的证据。任一失败都关闭，即使 driver 返回 Clean 也不能绕过宿主检查。reset 不支持、失败或超时直接关闭。不能把 ping 成功等同于 Clean。任意 SQL 编辑器首版关闭销毁，短操作只复用工具控制的可清理资源。

### 9.5 资源类别与调度

每个数据库服务预算分为 control、interactive、metadata、job；分类由服务端用例决定，客户端不能自报优先级。默认团队额度 20 中 control 保留 2、metadata 保留 1、interactive 保留 2，其余 15 为共享额度；桌面 16 中分别保留 1/1/2，共享 12。额度低于保留总和则拒绝配置；所有保留都在总额度内。首版保留不可被其他类借用，control 只能用于取消/健康恢复，不用于普通 SQL。

固定编辑器和 TablePanel 使用 interactive 类；元数据、补全使用 metadata；迁移/导出阶段使用 job。类内按 principal 轮转 FIFO，共享额度按 interactive:metadata:job=4:2:1 加权轮转，空队列跳过；非空队列不得无限饥饿。每类每服务排队上限 32、每用户 32；actor 队列另计，任一满返回 QueueFull。acquire 等待可取消并受 10 秒期限约束。

已连接固定 session、活动事务、游标不为优先请求抢占。idle pool 保留原类别与实际预算，可按 LRU 关闭并在确认后释放额度，再向其他类别发放；不能先记账转移仍存活的 socket。Job 的多端申请必须一次预留全部许可或失败释放，不持有半边无限等待。每用户 5 个编辑器名额不代替物理额度；Job 另受组织最大并发阶段数（默认 4）及全局连接额度约束。

### 9.6 PoolKey、版本与缓存的生产者

| 字段 | 权威生产者 | 变化处理 |
| --- | --- | --- |
| organizationId/connectionId/configRevision | RequestContext + ProfileRepository 的 CAS 版本 | 新 key；旧 idle 停发并关闭 |
| credentialRevision | SecretProvider 的不透明版本 | 新 key；不使用密码散列作为 key |
| executionIdentityKey | IdentityResolver：DB 登录身份、委托角色、有效权限范围 | 重新解析并隔离池与缓存 |
| policyIsolationKey | PolicyService：组织/用户/策略版本及只读限制 | 权限撤销立即停发，排队重新鉴权 |
| driverResourceKey | describeResource 对 CanonicalTarget/基线的规范化结果 | 数据库绑定资源按 database 分 key |
| networkRouteRevision | NetworkProvider 的路由/隧道/TLS 配置版本 | 新 key，旧路由不发放新租约 |

PoolKey 使用类型化结构等值比较，不能用随意字符串拼接。policyIsolationKey 在共享 DB 账号但权限不同的用户之间必须不同。已建立 session 保存建连时版本，不因配置更新改变底层身份；管理员按 drain/force 明确处理旧资源。effectiveIdentity 变化须先重新授权，不得在未匹配的池内直接切换角色。

SchemaCache key 为组织、executionIdentityKey、policyIsolationKey、connectionId、config/credential/capabilityRevision、CanonicalTarget、Command/参数指纹；临时元数据只进绑定 runtime handle 的内存缓存。缓存 TTL 默认 60 秒，配置/授权/DDL 事件即时失效；刷新使用独立 cacheRevision 防止慢结果回填，它不是 session 代次。不能仅靠 TTL 延迟权限撤销。

浏览多个 database 不必各保留连接，但 database-bound driver 可产生多个 pool key；每 pool min=0、max=2 仍受服务总预算，idle TTL 默认 60 秒、最多保留 32 个空 pool 元数据条目。空 pool LRU 可删除；有资源的 pool 先关闭再删除。跨数据库可安全复用只能由 driver 契约证明，不能为降低 pool 数量强行合并。

## 10. 三件套与 Job 实现细则

### 10.1 公共处理

准备阶段使用明确 target + 短 Lease；审阅等待释放快照。计划冻结 target、config/credential/capability 版本、对象结构指纹、映射和事务要求。执行前重新验证，变化则 PlanStale。

Job 接受时先持久化记录，再获取资源。Job 资源 owner 是 jobId/stageId，关闭窗口只取消订阅。完成/失败/取消均走 cleanup；应用进程关闭仍会停止任务，只有独立 worker 才能跨桌面进程继续。

源与目标角色独立。相同 connectionId 可以不同 database；不同 connectionId 可以同一真实对象。结合服务身份、命名空间、对象映射检查重叠；身份无法证明时提示并禁止危险自覆盖模式。

### 10.2 Schema Diff

compare 短租约读取结构 → 计划 → review → 重新校验目标 → 目标专用阶段资源 → 按依赖执行 DDL。原子性按计划确认，Unknown 不承诺全部回滚。MySQL 多种 DDL 会隐式提交，[官方说明](https://dev.mysql.com/doc/refman/8.0/en/implicit-commit.html)。失败记录已生效 DDL；反向 DDL 是补偿，不等于 rollback。

### 10.3 Data Sync

先过同族/结构/PK 门闸，比较时声明实时/单表/全库/协调快照范围。源目标各自快照不等于跨服务器同一时刻快照。review 默认释放快照；ChangeSet 保存目标旧值/版本证据。apply 再过门闸、结构检查和行冲突验证。

首版默认按批次事务；整表/整任务仅在计划支持时开放。session mode 开启、写入、关闭必须同一 Lease，恢复失败销毁。取消只回滚未提交范围，已提交批次保留。

### 10.4 Data Transfer

source reader → 有界缓冲 → IR 转换 → target writer。游标/快照固定读取资源，写入按提交范围固定资源。首版单 reader/writer；并行需稳定分片、快照协调及约束顺序证明。PostgreSQL 可导入导出快照，但不能假定所有 driver 支持，[官方说明](https://www.postgresql.org/docs/current/sql-set-transaction.html)。

checkpoint 记录稳定键、已确认提交、源一致性证据和映射指纹。目标 commit 与本地 checkpoint 之间存在崩溃窗口，恢复依赖目标事务内批次记录/幂等或重新核验，不能仅保存 offset。SQL 文件输出不建目标数据库连接。

## 11. Workflow / AI / MCP / Wapp

Workflow 目标优先级：step 显式目标 → block target → workflow 默认目标 → profile 初始目标。独立 step 每次初始化，USE 只在 step 内生效。session block 同一资源串行、跨 step 保留状态；transaction block runtime 控制 begin/commit/rollback，禁止步骤破坏提交边界。需重连的 database 变更不得进入现有 transaction block。

并行分支各自资源；同 session block 不并行。事务失败重试整个已证实回滚的块。任意 session block 默认不能从中间 checkpoint 恢复。跨端事务不承诺原子性，按幂等/补偿处理。

transaction block 执行前由 driver 校验整个计划是否能保证提交边界。首版优先受控 DML Command；任意 SQL 若无法证明不含手工 COMMIT、autocommit 修改、隐式提交或其他破坏边界的行为，则返回 UnsupportedPlan。不能只用关键词正则宣称安全。session block 可执行原生任意 SQL，但不提供 runtime 管理的整块原子性承诺。

AI 默认独立目标操作；用户显式授权当前编辑器 session 后，进入同队列并共享真实状态。MCP 显式 session 归客户端/身份、受 TTL 配额控制；Wapp session 归实例或 Job。后台调度用服务身份，不依赖 GUI 当前库或长期保留用户登录。

## 12. Web、多实例、权限与结果

SessionDirectory 保存组织/owner/worker/runtimeEpoch，不保存连接。dbSessionId 使用随机 128 位以上 ID，runtimeEpoch 每次 worker 启动重新生成；目录以 dbSessionId 唯一约束原子登记 owner/epoch，碰撞重生成，登记失败不返回成功。单进程实现同一目录 port；多 worker 用共享内存目录与 CAS，不需要中央 ID 分配器。替换提交必须对目录中的 old/new/operation 状态提供原子事务：prepared 候选不可路由；committed 指向新 owner/epoch，并把旧条目改为关闭路由。旧 actor 在提交期间不派发；提交成功后完成本地新记录发布再返回回执。目录提交失败且确认未提交则恢复旧 Ready；提交结果未知则保持 Reconfiguring 屏障，按 operation key 查询提交状态，不同时允许 old/new 执行。worker 崩溃使两者失效；不从内存目录恢复物理状态。目录禁止磁盘持久化、快照和 append-only 日志；仅保存带 TTL 的运行时路由，丢失目录则使相关会话明确失效，不据此恢复物理会话。任意 API 实例完成授权后转发到 owner worker。worker 故障返回 SessionLost，不能换机器保留 ID。sticky session 仅是优化。

浏览器网络断开不是数据库断开。允许 grace 内按 6.4 认证原 attachment 后重附着；前端恢复订阅/getExecution，不重新发送未知写入。超期关闭；有事务的实际回滚结果单独记录。租约到期的 Job 接管必须先验证旧执行与目标提交，fencing token 无法自动撤回外部数据库语句。

池隔离由执行身份而非显示用户名决定。共享账号的应用 ACL 不能代替数据库权限；任意 SQL 的限定名/过程/角色可能绕过当前库范围。结果/缓存/event 都按组织和有效权限范围检查。配置禁用/删除拒绝新操作，已有资源按显式 drain 或强制结束策略处理。

旧结果编辑：受控对象结果有完整 relation identity + PK/version，可重新授权并乐观写回原目标；任意 SQL 多表/表达式结果默认只读，driver 未证明可写映射时不猜测。临时表结果写回要求原 runtimeBinding 精确有效，否则 SessionLost；相同配置、指纹或用户不能代替原物理会话。

## 13. 错误、重试和事件

| code | 触发 | UI/调用者动作 |
| --- | --- | --- |
| InvalidArgument | DTO/Command schema 不合法 | 修正请求，不调用 driver |
| TargetRequired / TargetConflict / TargetUnsupported | 目标缺失/重复冲突/无法定位 | 修正请求，不默认切库 |
| SessionNotFound / SessionLost | 失效或不存在 | 读终态，显式建立新会话 |
| RuntimeEpochMismatch / ContextConflict | 旧运行时/上下文请求 | 读取新状态，不自动执行旧写入 |
| PermissionDenied | 当前授权拒绝 | 停止；敏感资源映射为 404 |
| ResourceBusy / QueueFull | 预算/队列超限 | 等待或用户取消；不得创建额外连接 |
| TransactionResolutionRequired | 事务阻止切换/关闭 | 用户选择处理事务 |
| CapabilityUnsupported / UnsupportedPlan | 无所需能力/无法证明事务边界 | 拒绝执行或重新制定计划 |
| EndpointOverlap | 源目标对象危险重叠或无法排除自覆盖 | 修改目标或选用允许的计划模式 |
| SessionQuotaExceeded | 逻辑会话/编辑器额度超限 | 关闭会话或管理员调整额度 |
| IdempotencyExpired | 幂等键已过提交有效期 | 查询原执行并核验，不自动用新键重投 |
| RollbackFailed / CleanupFailed | 无法证明清理 | 隔离连接、报告真实结果 |
| OutcomeUnknown | 写入/提交结果未知 | 核验执行，不自动重试 |
| PlanStale / SourceChanged / TargetConflictRows | 计划/数据变化 | 重比对或处理冲突 |
| IdempotencyConflict | 相同键不同输入 | 修正请求键，不能覆盖记录 |
| Unauthenticated | 未认证 | 跳转登录；不自动重放原请求 |
| NotFound | 不可见资源、不存在资源或二者不可区分 | 不重试，不据此推断资源存在性 |
| PayloadTooLarge | 请求体超过物理上限 | 缩减请求 |
| QuotaExceeded | 组织/用户/数据库额度超限 | 等待或申请配额 |
| RateLimited | 令牌桶限流拒绝 | 按 Retry-After 退避 |
| ServiceUnavailable | 暂无可用 worker、drain 中、管理库不可达或迁移未完成 | 退避后重发；已接受的幂等提交必须复用同一 idempotencyKey |
| ConfigRevisionMismatch | ProfileRepository::compare_and_set 的 expectedRevision CAS 失败（命名空间目标本身合法，区别于目标冲突的 TargetConflict） | 重读最新 configRevision 后由用户决定，不自动覆盖 |

每条事件带 stream sequence；重复事件忽略，缺口重新读取状态。session context 按 runtimeEpoch/dbSessionId/contextRevision 更新；比较 Counter 使用 BigInt 或十进制字符串比较，不能 Number 转换。

幂等记录原子保存请求指纹和 receipt，键的有效期协议见 13.1。首版执行/Job 的完整 durable receipt 保留至少至 `expiresAt + 24 小时`，会话回执仅内存保留；不依赖已删除记录来识别旧请求。幂等提交只保证创建 execution 一次，不保证外部 SQL exactly-once。执行记录不可用时，客户端不能根据没有事件推断未执行。

### 13.1 幂等键期限与响应分类

幂等键不是客户端随意 UUID。BackendClient 先从应用服务获取签名提交令牌（本地也使用相同 port）；令牌包含随机 nonce、operation、组织/principal/client、issuedAt、expiresAt、keyVersion；运行时会话操作还绑定 owner runtimeEpoch，默认有效期 24 小时，不含秘密。issueSubmissionToken 对 openSession 选定 owner，对已有 session 操作验证并绑定其 owner；`createProfile` 是配置写入，不绑定会话与 runtimeEpoch，签名只限定身份、组织与操作。返回的 idempotencyKey 是不透明签名令牌。服务器校验签名与作用域，再以令牌摘要为去重 key；不相信客户端时间。未过期且同指纹返回同 receipt，不同指纹 IdempotencyConflict。

超过 expiresAt：仍有记录时允许只读查询原 receipt；任何重新提交返回 IdempotencyExpired，不再次执行。完整记录至少保留至 expiresAt + 24 小时，然后可按审计策略删除；删除后过期签名令牌仍被拒绝。令牌验证密钥至少保留至已发令牌全部过期，未知 keyVersion 一律拒绝。长期离线请求需重新核验业务状态，由用户重新提交，不自动换新令牌。执行与 Job 接受记录持久化时只保存请求摘要、executionId/jobId、稳定目标/owner、状态和 receipt 的 durable 投影，禁止保存原请求里的 SessionHandle。接受记录须在 SQL/Job 派发前提交；进程重启后的非终态进入核验/OutcomeUnknown，不自动再执行。

openSession/context replacement 的完整指纹、SessionView/attachmentToken/receipt 只在 owner 内存保存至令牌过期或 runtime 终止。对应令牌绑定 runtimeEpoch，owner 丢失即使令牌未过期也返回 SessionLost，不能在新 owner 当作首次请求再次创建。响应丢失时，在原 runtime 内同键返回同 session/token/候选提交结果；不延长 attachment/idle TTL。会话已到期则返回其终态或 SessionLost，不恢复资源。close 的 tombstone 同样只在内存保留（默认 24 小时），逻辑额度在终态退出活动注册表时回收，隔离物理占用持续计数。

RequestError（ApiError.code）表示请求被拒绝；已接受 ExecutionReceipt 不等于 SQL 成功；CancelReceipt.disposition 表示控制请求结果，不用异常 CancellationUnsupported 表示正常 unsupported。执行终态 errorCode 使用 §4 定义的版本化枚举 ExecutionErrorCode，与 ApiError.code 是两个命名空间；effectOutcome 独立表达数据库生效范围。所有大小写使用 DTO 字面值，如 cancelled/rolledBack/partiallyApplied；文中 Active/Unknown 等状态展示简称不作为协议值。driver 的内部错误由网关脱敏映射，不原样暴露。

errorCode 与 effectOutcome 正交：sqlError 可对应 completed/rolledBack/partiallyApplied/unknown 任一。超时、protocolError、取消、连接丢失导致生效范围无法判定时，effectOutcome 必须为 unknown，禁止因为"看到取消"就写 rolledBack——CM-44、CM-47 的断言依赖此规则。`cancelled` 只表示取消请求已送达驱动，不构成"写入已回滚"的证据；pipelineAborted 至少为 partial 或 unknown。新增取值需要提升枚举版本并同时更新全部消费者，宿主、前端与 driver 不得各自扩展。

派发后的执行终态失败只由 `ExecutionState='failed'` + ExecutionErrorCode + 事件表达，不产生 ApiError，也不出现在 RequestError 表中；该表只收"请求被拒绝"的场景（未认证、无权限、参数错误、版本或运行时冲突、预算或速率超限、暂时无可用 worker）。已建立执行记录后由宿主二次校验拒绝的记 `hostRejected`，尚未建立记录时直接返回 ApiError。

## 14. 开发步骤与文件职责

拟新增模块放在 `packages/runtime/src/connection/`，每文件按职责拆分，不写巨型 manager：

| 顺序 | 模块 | 开发任务 | 完成证据 |
| --- | --- | --- | --- |
| 1 | types/error | newtype、DTO、enum、schema 校验 | 类型编译、序列化往返、错误码 |
| 2 | testing/fake_resource | 假资源、状态、屏障、故障注入；可返回事务/游标句柄的 fake 命令 | 可观察创建/关闭/执行次数、resourceId 与句柄登记状态 |
| 3 | budget | 单机多维许可、可取消等待、幂等核销 | 并发与失败会计测试 |
| 4 | resource | acquire/cleanup/quarantine、隧道引用 | 每阶段故障无泄漏 |
| 5 | registry/actor | session 生命周期、队列、epoch、§6.5 会话级句柄登记 | 连续旅程与取消竞态、句柄释放顺序（CM-73/74） |
| 6 | execution | 幂等、授权、来源、事件 | 重复提交、旧事件、结果 ACL |
| 7 | adapters | IPC/BackendClient、旧接口窄适配 | 桌面路径行为一致 |
| 8 | consumers | Query/Table/metadata、三件套、Workflow | 功能契约旅程 |
| 9 | routing | worker 目录、总预算、drain | 多实例与分区测试 |

fake driver 模拟一般会话语义，不包含 MySQL/PostgreSQL 方言实现。真实方言测试严格放对应驱动。旧 API 每迁移一个 consumer 即删除它的旧路径；同一请求不允许同时经过新旧管理器。注册目录、依赖和脚本在实施阶段添加，本次仅交付文档。

## 15. 测试夹具、落点和执行规则

### 15.1 基础夹具

Host fake 提供：两个组织 O1/O2、用户 U1/U2、profile P、不同个人执行身份、A/B 命名空间、资源创建 journal、命令屏障、fake clock、可注入 observe/reset/rollback/close 失败。测试断言 driver 收到的 opaqueResourceId、实际目标、调用顺序和预算计数，不只断言 UI 文本。

真实 driver fixture：创建独立测试 database/schema，A/B 的同名 `dz_target_marker` 返回不同标记；有状态驱动提供临时对象、事务和上下文探测脚本。SQL 和测试数据由对应 driver 测试包定义。测试前验证目标属于专用测试环境，结束删除夹具。

不打开、读取、解析、source 或打印 `.env` / `.env.test`，不运行隐式加载它们的程序。真实测试凭据由已配置 CI secret 或获授权的进程环境注入；开发前检查测试启动器不会加载受保护文件。使用 fake 可完成大多数宿主测试。

### 15.2 测试分层

| 层 | 落点 | 覆盖 |
| --- | --- | --- |
| runtime 单元/集成 | 新 runtime crate 内 | fake 资源、actor、预算、生命周期 |
| Host 集成 | `src-tauri` 通用测试 | IPC 与用例、身份/Job consumer |
| driver Rust | `packages/drivers/<id>/tests/` 或同文件 | 真实协议、USE、事务、清理、快照 |
| driver UI | `packages/drivers/<id>/ui/__tests__/` | driver 专属界面 |
| 前端 Vitest | Host/SDK 对应测试目录 | backend 选择、事件投影、tab 旅程 |
| Host WDIO | `e2e/specs/`、`e2e/contract/` | 通用旅程与契约矩阵 |
| driver E2E | `packages/drivers/<id>/e2e/` | 方言场景 |
| Web 服务集成 | 拟新增 server crate 内 | HTTP、认证、CSRF、SSE、worker 路由 |

现有可用命令：`pnpm typecheck`、`pnpm test:unit`、`pnpm test:unit:drivers`、`cargo test -p datazen --lib`、`cargo test -p datazen-driver-api`、`cargo test -p datazen-driver-postgres`、`pnpm test:layers`、`pnpm test:boundaries`。新 crate 测试命令待其 Cargo package 创建后添加，不能现在声称可运行。

WDIO 使用 `pnpm tauri:build:webdriver` 或经检查的 E2E wrapper，不能裸 cargo build。测试文件参加 typecheck，不用 any。三件套已生成可运行 app 后仅 DMG 打包失败，不阻塞其 WDIO 功能验收。

### 15.3 测试范围与基准 harness

H 是部署无关 runtime 契约，F 是前端契约，D 是真实驱动协议；W 再分 W1（单实例 HTTP/SSE）与 WN（多 worker）。CM-57/58 的故障路由和 CM-60 多 worker 部分属于 WN/P9；CM-54/55/56 的 H/F 部分在 P3/P4 必须验收，不能推迟到 Web。单进程 SessionDirectory 实现同一 port，P9 替换路由基础设施。

CM-60 使用 release 构建，4 vCPU/8 GiB、无数据库网络、单进程固定 fake command 10 ms，以并发 8 预热 1000 请求；每轮采集 10000 个获准且未排队的请求，运行 5 轮。附加耗时从 gateway 完成鉴权/参数校验开始到派发 driver，以及 driver completion 到 receipt/event 状态登记完成的两段单调时间之和，不包含 fake SQL、预算/actor 排队、网络传输。采用 nearest-rank p95（排序后第 ceil(0.95*N) 项），每轮均须 ≤10 ms；排队请求另报等待分位数，不删失败样本，失败数单列。

事件断言限单次存活订阅、缓冲未溢出的 sequence 连续性；重连允许重放重复原始事件，但投影后无重复行/状态遗漏。journal 在每次建连/关闭/permit 变化时断言额度，无随机采样盲区。注入连接持有、慢 consumer 和 drain 的压力部分另运行，不混入非排队延迟样本；保存环境、构建参数、原始计时与 journal 作为 CI artifact，不新增仓库评审记录。

## 16. 详细测试用例

每个用例实现时保留编号。H=Host fake/runtime，D=driver，F=前端，W=Web/worker 泛称（§15.3 再分 W1=单实例 HTTP/SSE、WN=多 worker）；标题写 (W) 表示该 Web 断言横跨两种部署形态，阶段性落点按 §15.3 与 §17 判定，与标题直接写 (W1)/(WN) 的用例同属一类层标签。涉及真实方言的 D 断言放驱动目录，Host 只运行统一 contract。

### 16.1 类型、身份、目标与懒连接

**CM-01 ID 类型与序列化（H/F）**

- 前置：所有 DTO 已定义；Counter 使用大于 `2^53` 的十进制值。
- 步骤：Rust/TS 往返序列化；编译时把 connectionId newtype 传入 session API 的负例；传空 namespace 字符串。
- 断言：计数不丢精度；Rust 负例编译失败；空值/缺字段请求返回参数错误；null 按 namespaceShape 与操作需求解释。

**CM-02 打开空白编辑器不建连（H/F）**

- 前置：profile P、预算使用 0，driver creation journal 为空。
- 步骤：创建 20 个 New session；只在其中 2 个执行读取。
- 断言：创建时物理连接为 0；执行后为 2 条固定资源；其他 18 个仍为 New；未占用事务。

**CM-03 session 创建幂等（H/W）**

- 前置：同一用户、client、owner 和 key K。
- 步骤：并发提交 10 次相同 open；再用 K 提交不同 target。
- 断言：相同 dbSessionId/attachmentToken、一个注册记录；不同输入 IdempotencyConflict；没有额外连接。

**CM-04 禁止配置 ID 回退（H/W）**

- 前置：存在 profileId，不存在同字符串 session。
- 步骤：向 executeInSession 传 profileId；向 executeAtTarget 传未知 profile。
- 断言：分别 SessionNotFound、不可见配置错误；driver execute 次数为 0。

**CM-05 跨用户/组织资源访问（H/W）**

- 前置：O1/U1 有 session、execution、job、artifact；O1/U2 和 O2 用户无授权。
- 步骤：分别读取、执行、关闭、取消、订阅和下载这些 ID。
- 断言：每个接口均拒绝；不能观察资源存在性/数据；不能调用 driver cancel/execute。

**CM-06 前端伪造 owner（H/W）**

- 前置：U1 与 U2 的 editor/job 各自存在。
- 步骤：U1 请求 owner 指向 U2 editor 或 job，或在 body 伪造 organizationId/principalId。
- 断言：拒绝 owner；请求身份字段无法覆盖 RequestContext；不创建会话。

**CM-07 目标缺失与冲突（H/D）**

- 前置：声明必须 database/schema 的 driver；Command 同时接受 envelope 与 input 目标的过渡适配。
- 步骤：省略必需层；传 schema=null；传两个不同 database；传无能力目标。
- 断言：TargetRequired/TargetConflict/TargetUnsupported；不在默认库执行；D 测试验证真实 driver。

### 16.2 会话连续性、切库和事务

**CM-08 两个编辑器隔离（H/D/F）**

- 前置：A/B 同名 marker，两个独立 session。
- 步骤：S1 切 B，S2 保持 A；交错执行 100 次读取。
- 断言：S1 始终读 B，S2 始终读 A；各自 resourceId 固定且不同；UI 上下文独立。

**CM-09 跨调用临时状态（D/F）**

- 前置：driver 声明有状态会话。
- 步骤：第一次调用创建临时表并设置会话变量；第二次插入；第三次读取；另一 session 尝试访问。
- 断言：原会话可读取值，另一会话不可访问该临时状态；原会话底层身份连续。

**CM-10 原始 SQL 顺序（D）**

- 前置：初始 A；A/B marker 值不同。
- 步骤：同一脚本执行 A 查询、USE B、B 查询。
- 断言：结果依次 A/B，各结果来源分别 A/B，最终上下文 B；执行前没有提前切 B；语句顺序保留。

**CM-11 USE 失败（D/F）**

- 前置：A 会话；B 不存在或无权限。
- 步骤：执行 USE B，再在下一次请求读取当前库。
- 断言：错误明确；实际与 UI 保留 A；contextRevision 不虚增为 B。

**CM-12 USE 成功后查询失败（D/F）**

- 前置：A 会话，B 存在，B 内 missing 表不存在。
- 步骤：USE B 后查询 missing，再读取当前库。
- 断言：查询失败但当前库 B；不能按整体错误恢复 UI 为 A；之后合法读取在 B。

**CM-13 限定名与补全不切库（D/F）**

- 前置：A 会话；补全能加载 B。
- 步骤：输入残缺 `B.`、补全对象、执行 B 限定名查询，再执行无前缀 A marker 查询。
- 断言：推测/补全能变化；observedContext 仍 A；后续读取 A；无 changeContext 调用。

**CM-14 字符串、注释、过程与批次（D）**

- 前置：driver fixture 提供该方言合法字符串、注释、过程/批次分隔语法。
- 步骤：执行含文本 USE 和嵌入分号的 SQL；实际支持的动态 SQL改变上下文；读取状态。
- 断言：文本不触发切库，语法不被 Host 分号正则破坏；动态变化由实际观察确认或明确 Unknown。

**CM-15 PostgreSQL 切 database 替换（D/F）**

- 前置：A/B、旧 session 已建立且无事务、预算允许候选连接。
- 步骤：UI 切 B；记录候选建立、绑定替换、旧连接关闭顺序。
- 断言：新 dbSessionId，真实 current_database 为 B；先成功后替换，旧 ID不能执行。

**CM-16 替换失败保留旧连接（H/D/F）**

- 前置：旧 A 正常，候选 B 建立失败或预算不足。
- 步骤：请求切 B，再在 A 执行读取。
- 断言：旧 session 和 A 仍有效，UI 不显示 B；候选资源和许可释放；提示失败。

**CM-17 手写事务旅程（D/F）**

- 前置：事务型表及固定 session。
- 步骤：用户 SQL BEGIN、INSERT、读取；另一会话读取；原 session ROLLBACK；再次读取。
- 断言：真实可见性符合所选隔离级别；状态 Active→None；UI 同步；数据回滚。

**CM-18 aborted/unknown 事务（H/D/F）**

- 前置：可观察 aborted 的 driver；另设 observe 失败 fake。
- 步骤：事务内制造错误；请求切库/requireNoTransaction 关闭；执行显式回滚或关闭。
- 断言：不能把 aborted/unknown 显示为 none；切库被阻止；处理后下一合法操作顺畅。

**CM-19 driver 无状态能力（H/D）**

- 前置：statefulSession 为 unsupported/unknown。
- 步骤：请求固定 SQL session；执行支持的受控目标操作。
- 断言：前者 CapabilityUnsupported，后者按能力可成功；不把单连接池假装成固定资源。

### 16.3 并发、预算、资源清理与取消

**CM-20 会话串行且跨会话并行（H）**

- 前置：S1/S2、fake command barrier。
- 步骤：阻塞 S1/E1，提交 S1/E2 与 S2/E3；释放 E1。
- 断言：E2 不早于 E1 完成，E3 可先完成；同 resource 并发计数最大为 1。

**CM-21 切库与执行竞态（H/F）**

- 前置：revision R 的慢查询；UI 切库被禁用。
- 步骤：直接 API 排队 context change；再提交携带旧 R 的执行；完成查询和切换。
- 断言：切换按序；旧 revision 执行 ContextConflict，不在新库默默执行。

**CM-22 精确取消不阻塞（H/D）**

- 前置：长查询 E1、另一个 session E2；driver 支持精确取消。
- 步骤：E1 阻塞时发 cancel，并继续 E2。
- 断言：控制路径实际收到 cancel，不等待 E1 队列结束；E2 不受影响；终态与事务观察一致。

**CM-23 旧取消与已完成取消（H/D）**

- 前置：E1 已结束/旧 runtimeEpoch 或旧 resourceBindingId；新执行 E2 正在运行。
- 步骤：重复取消 E1，伪造旧 cancelHandle 指向 E2。
- 断言：CancelReceipt.disposition=alreadyFinished；内部伪造绑定被拒绝；不会取消 E2；registry 不残留 E1。

**CM-24 无取消能力（H/D/F）**

- 前置：preciseCancel unsupported 的长执行。
- 步骤：请求 cancel，再显式选择关闭自己的 session。
- 断言：先返回 CancelReceipt.disposition=unsupported；不调用 legacy session-wide cancel；显式关闭按实际效果终结。

**CM-25 结果未消费不能归池（H/D）**

- 前置：流结果屏障，短资源池容量 1。
- 步骤：E1 已产出部分数据但未完成协议，提交 E2；结束/取消 E1 并清理。
- 断言：E2 之前不能取得同资源；清理确认后可复用或拿新资源；无残留数据混入。

**CM-26 reset/rollback/close 故障矩阵（H/D）**

- 前置：三个阶段分别注入失败/超时；记录资源和 budget journal。
- 步骤：每种故障执行短操作并释放，随后申请另一操作。
- 断言：旧资源不复用；回滚失败不报告 RolledBack；close 未确认继续占预算；确认关闭后只核销一次。

**CM-27 部分建连与初始化失败（H）**

- 前置：在 permit、隧道、socket、握手、初始化、注册阶段分别注入失败。
- 步骤：逐阶段尝试首次执行，再关闭/重试。
- 断言：没有已执行 SQL；可确认关闭的资源许可归零；隧道引用正确；未确认关闭进入隔离。

**CM-28 重复 release/close（H）**

- 前置：同 Lease、多个关闭请求；cleanup 与 timeout 竞态。
- 步骤：并发释放 20 次，再重复查询 tombstone。
- 断言：driver close 至多一次有效关闭；预算不负数；隧道不多减引用；重复响应一致。

**CM-29 多维预算与队列取消（H）**

- 前置：目标总额度 4、控制预留 1、用户额度 2。
- 步骤：多个用户同时请求 20 个资源；取消等待者；释放一个资源。
- 断言：总物理数不超过 4、普通占用不侵占控制预留、用户不超过 2；取消者不建连；等待者公平取得资源。

**CM-30 idle 连接计入预算（H/D）**

- 前置：pool 容量 2，预算 2。
- 步骤：两次短操作归池；请求独立固定 session；关闭一个 idle 连接。
- 断言：归池后预算仍占 2；固定 session 先等待；确认 idle close 后才有额度。

**CM-31 多端 AB/BA 申请（H）**

- 前置：端点 A/B 各容量 1，两任务需要两端。
- 步骤：同时申请 A→B 与 B→A，取消一个任务或推进调度。
- 断言：无循环等待；一个任务可取得完整资源并结束；超时者无半边资源泄漏。

**CM-32 隧道引用与并行任务（H）**

- 前置：两个 session/Job 共用同版本隧道。
- 步骤：关闭第一个，继续第二个；第二个结束。
- 断言：第一步不关闭隧道；最后一个引用释放才关闭；隧道失败传播给依赖资源。

### 16.4 表格、缓存、结果与配置

**CM-33 多表 tab 共用短操作池（H/F）**

- 前置：20 个 TablePanel，pool max=2，两个已连接 QueryPanel。
- 步骤：并发刷新表格，查询编辑器当前库和临时状态。
- 断言：表格并发资源不超过 2，不为每 tab 建连接；编辑器资源/上下文不变。

**CM-34 表编辑事务范围（H/D/F）**

- 前置：表有 PK/version，driver 支持该表事务；界面累计两条修改。
- 步骤：编辑但不保存；保存成功；再制造第二条写入冲突/错误。
- 断言：编辑阶段无长期事务；保存同 Lease 短事务；第二条失败则两条修改均回滚；没有事务保证的目标须预先拒绝原子保存，或由用户显式选择非原子模式并报告部分结果。

**CM-35 旧结果正确归属（H/D/F）**

- 前置：A 表结果，之后编辑器切到 B。
- 步骤：对旧结果导出/诊断；对可写映射提交 PK 修改。
- 断言：仍使用 A 的来源与重新授权；不写 B；没有可写映射的任意 SQL 结果保持只读。

**CM-36 临时对象结果失效（D/F）**

- 前置：结果来自临时表和 session S。
- 步骤：关闭 S，再尝试编辑旧结果。
- 断言：SessionLost；不在新 session 建同名对象或写普通表；结果本身按策略仍可阅读。

**CM-37 缓存竞态与权限隔离（H）**

- 前置：A 元数据慢读取；刷新 revision；U1/U2 权限范围不同。
- 步骤：刷新后返回旧读取；分别读取缓存；配置版本变化后再读取。
- 断言：旧读取不能重新填充新 cacheRevision；不同权限/版本不串缓存；临时对象不进入全局缓存。

**CM-38 配置修改与凭据轮换（H/W）**

- 前置：旧 session 与 idle pool，configRevision=1。
- 步骤：CAS 更新为 2、轮换凭据；提交旧 expectedRevision 请求；建立新 session。
- 断言：旧请求冲突/按明确政策拒绝；新资源用新材料；旧 idle 不再发放；旧 session 不被悄悄改配置。

**CM-39 配置禁用/删除（H/W）**

- 前置：已有 session、排队请求与 Job。
- 步骤：管理员禁用，分别测试 drain 与 force 策略。
- 断言：新/排队请求不执行；已运行操作按实际取消结果记录；审计与 Job 历史仍可定位原配置版本。

### 16.5 三件套与 Workflow

**CM-40 任务关闭窗口继续（H/F/W）**

- 前置：运行中的迁移 Job，源目标 Lease 均归 Job。
- 步骤：关闭窗口/浏览器，重新打开并按 jobId 订阅。
- 断言：driver 未被 UI release；任务继续；重订阅不重新创建 Job；权限仍检查。

**CM-41 计划过期与自覆盖（H/D）**

- 前置：两个 profile 指向同一对象；另有 review 完成的计划。
- 步骤：执行危险自覆盖；修改目标结构或配置版本后应用旧计划。
- 断言：阻止自覆盖；旧计划 PlanStale；没有首条目标写入。

**CM-42 Schema Diff 非事务 DDL 失败（D）**

- 前置：真实 driver 声明计划含不可回滚 DDL。
- 步骤：第一条 DDL 成功，第二条失败，调用回滚/取消。
- 断言：PartiallyApplied，列出已生效对象；不能报告完整 rollback；缓存失效范围正确。

**CM-43 Data Sync review 后冲突（D/F）**

- 前置：比较生成 ChangeSet，审阅阶段不占快照；另一连接修改目标行。
- 步骤：应用原 ChangeSet，再处理冲突重比对。
- 断言：检测旧值/version 冲突，不无提示覆盖；新比对可继续；门闸和结构再次验证。

**CM-44 Data Sync 批次取消（D）**

- 前置：3 批写入，第一批已提交、第二批事务内阻塞。
- 步骤：取消并释放阻塞，查询目标和 Job 结果。
- 断言：第一批保留，第二批回滚，第三批 NotStarted；终态明确部分提交。

**CM-45 session mode 生命周期（D）**

- 前置：需要 identity/约束会话模式的 driver fixture。
- 步骤：开启、写入、关闭；再注入关闭失败。
- 断言：整个流程同资源；失败资源销毁；下一 consumer 不继承模式。

**CM-46 Transfer 背压与超大值（H/D）**

- 前置：源高速、目标 barrier，缓冲上限 8 MiB；有超大单字段。
- 步骤：持续读取后阻塞目标；恢复；触发取消。
- 断言：已缓冲字节不越配置，超大值按分块/上限明确处理；取消唤醒两端，无悬挂 reader。

**CM-47 提交与检查点崩溃窗口（H/D）**

- 前置：目标批次事务内记录幂等 batchId。
- 步骤：目标 commit 成功、持久化本地 checkpoint 前模拟进程崩溃，再恢复。
- 断言：查询目标记录确认提交，不重复插入；无证据时 OutcomeUnknown，不按旧 offset 重跑。

**CM-48 源变化与快照并行（D）**

- 前置：读取中源发生更新；分别开启支持/不支持协调快照的能力。
- 步骤：并行 reader 或断点恢复。
- 断言：不支持时拒绝严格一致并行；支持时 reader 使用同一已证明快照；源证据变化拒绝无条件恢复。

**CM-49 SQL 文件传输（H/D）**

- 前置：source→SQL artifact Job，无 database target。
- 步骤：生成文件、取消中途文件、重新下载成功文件。
- 断言：目标连接创建数为 0；未完成产物不伪装成功；下载按 ACL 校验。

**CM-50 Workflow 独立 step（H/D）**

- 前置：workflow 默认 A；step1 在自己的 Lease USE B。
- 步骤：step2 不指定目标；step3 显式目标 C。
- 断言：step2 使用 A，step3 使用 C；step1 状态不泄漏；资源全部清理。

**CM-51 Workflow session block（H/D）**

- 前置：3 个 step 共用 block，第一步创建临时对象。
- 步骤：后续写读临时对象；尝试同 block 并行；再启动另一 block。
- 断言：原 block 同资源连续；并行配置被拒绝；另一 block 独立；block 结束关闭。

**CM-52 Workflow transaction block（H/D）**

- 前置：事务型目标，2 个写 step；runtime 控制事务。
- 步骤：第二步失败；尝试用户 COMMIT/跨 connection/隐式提交 DDL；经完整回滚后重试块。
- 断言：不支持计划执行前被拒绝；合法失败完整回滚；重试整个块而非中间 step；未知 commit 不自动重试。

**CM-53 AI/MCP/Wapp 归属（H/W）**

- 前置：用户 editor、MCP 客户端、Wapp 各自 owner。
- 步骤：默认 AI 读取临时对象失败；用户授权绑定 editor 后读取；其他客户端伪造 session。
- 断言：授权后同队列可见；未授权拒绝；客户端断开只清理自己的资源。

### 16.6 Web、多实例、断线与性能

**CM-54 请求超时与幂等写入（H/W）**

- 前置：请求已接受并执行写入，响应丢失。
- 步骤：同 key 重发、getExecution、恢复事件订阅。
- 断言：同 executionId、数据库写入一次；无法读取记录时要求核验，不用新 key 自动再写。

**CM-55 事件重复/乱序/超大 revision（F/W）**

- 前置：旧 epoch、旧 session ID、乱序 sequence 和大 Counter 事件。
- 步骤：依次投递旧 context、重复 chunk、新 context、缺口。
- 断言：旧事件不覆盖状态、不重复行；大计数精确；缺口读取快照或补订阅。

**CM-56 网络掉线与事务 TTL（H/F/W）**

- 前置：session 已建立、有事务；fake clock。
- 步骤：断开客户端，grace 内重附着；再次断开并超过事务空闲/保留期限；持续发登录心跳。
- 断言：grace 内原资源连续；到期显式关闭并记录回滚；心跳不延长事务业务空闲；下一执行要求新 session。

**CM-57 多 API 实例会话路由（W）**

- 前置：API A/B、worker W1/W2；session 归 W1。
- 步骤：通过 A/B 交错执行、切库、取消；不启用 sticky session。
- 断言：均到 W1，同物理资源串行；W2 不创建替代连接；每次鉴权。

**CM-58 worker 故障与租约分区（H/W）**

- 前置：W1 有 session 和执行中的写 Job，协调器租约可失效。
- 步骤：断开 W1 与协调器；尝试 W2 接管和申请旧额度；恢复 W1。
- 断言：W1 停止新派发；旧 session SessionLost；旧额度未确认隔离前不复用；Job 核验/Unknown，不盲重写。

**CM-59 登出/权限撤销/CSRF（W）**

- 前置：cookie 登录、SSE、排队执行和运行 Job。
- 步骤：伪造跨站修改请求；撤销权限/登出；继续订阅、下载和提交请求。
- 断言：CSRF 拒绝；旧订阅关闭/停止数据；排队不执行；运行操作按权限取消政策记录真实结果。

**CM-60 资源压力与 drain（H/W）**

- 前置：总额度 20、控制预留 2；100 用户逻辑 session、短请求 fake 固定耗时 10 ms、队列上限 32。
- 步骤：提交 1000 次操作与取消，两个 worker 分配额度；将一个 worker drain；全部完成后关池。
- 断言：每次资源 journal 变化点真实资源不超过总额度；普通资源不侵占预留；拒绝/等待有界；drain 不接新资源；结束后非保留资源、任务、取消句柄和许可均归零。
- 性能门槛：单机 fake 基准（4 vCPU/8 GiB、预热后 5 轮）非排队网关附加耗时 p95≤10 ms，事件重复/丢失数为 0；记录实测环境。它是 runtime 回归指标，不是远程数据库 SQL 延迟 SLA。

### 16.7 补充契约与边界用例

**CM-61 持久化白名单与临时结果恢复（H/F）**

- 前置：普通表/临时表结果，包含 runtimeBinding 的实时 ExecutionView。
- 步骤：保存执行、产物、事件审计和 checkpoint；销毁 runtime 后恢复；尝试写回两种结果。
- 断言：落盘结构无 dbSessionId/SessionHandle/resourceBindingId/lease/cancel 句柄；普通表重新授权并验证映射后可写，临时结果只读；相同配置新 session 不能接替原绑定。

**CM-62 命名空间规范化矩阵（H/D）**

- 前置：driver 提供 namespaceShape、session/object 两类 targetRequirements、path 别名规则。
- 步骤：分别提交缺字段、不存在层级非 null、required null、optional null、同义 path、冲突 path、大小写别名。
- 断言：前三种执行前拒绝，optional 按操作允许；同义 ID 规范化一致；冲突 TargetConflict；缓存/权限/driver 使用相同 CanonicalTarget，无文件路径误用。

**CM-63 Attachment、TTL 与竞态（H/F/W1）**

- 前置：fake clock、事务 session、原 owner token 与同 principal 另一 client。
- 步骤：detach 后重复 detach；同 principal 无 token attach；原 owner grace 内 attach；推进事务 TTL 并同时 attach/close。
- 断言：重复 detach 不延期限，无凭据拒绝；合法 attach 不刷新事务 idle；最早期限生效；Closing/expired 不重附着；heartbeat 不改变期限。

**CM-64 无消费者、截断与有界 drain（H/D/F）**

- 前置：8 MiB buffer、低 artifact quota、无消费者，分别设可完成/不可完成协议 barrier。
- 步骤：unsubscribe/关闭结果视图；超时或耗尽额度；执行 drain、取消与故障清理。
- 断言：unsubscribe 不立即取消 SQL；缓冲/产物有界，截断可见；健康固定资源可 Ready；短资源满足宿主检查与 Clean 才复用；未知资源隔离，不误伤同 session 的健康临时状态。

**CM-65 类别保留与无饥饿（H）**

- 前置：服务总额 8，control/metadata/interactive 各保留 1，共享 5；固定编辑器和长 Job 占共享额度。
- 步骤：持续注入交互请求并发元数据/控制取消；再释放共享额度，多个用户排队；取消部分等待者。
- 断言：类保留不被侵占，控制可用；持续队列按 4:2:1 获派发；用户轮转无饥饿；不抢占固定资源；队列取消无许可泄漏。

**CM-66 逻辑与物理额度（H）**

- 前置：用户逻辑上限 20、已连接编辑器上限 2，组织逻辑上限 30，总物理上限 4。
- 步骤：打开 20 个 New，再开第 21 个；并发连接 3 个；关闭一已连接编辑器后重试；另一用户尝试超过组织上限。
- 断言：New 无 socket 但计逻辑数；超限 SessionQuotaExceeded；连接名额原子预留，失败回收；物理、用户、组织额度各自正确。

**CM-67 PoolKey 版本和多 database（H/D）**

- 前置：database-bound driver，两个 database、共享 DB 账号但不同 policy 的用户；低总预算。
- 步骤：轮流浏览 A/B；轮换凭据、路由和 ACL；慢缓存结果晚到；推进 idle TTL。
- 断言：按 database/policy 分 key；总 socket 不越预算；旧 idle 停发并关闭；旧缓存不能回填，撤权即时生效；空 pool 元数据受 LRU 上限约束。

**CM-68 候选替换发布与响应丢失（H/W1/WN）**

- 前置：可替换 driver、旧 session；目录与替换提交可注入故障。
- 步骤：候选建连时尝试 get/execute；提交前失败；提交成功但目录发布/响应丢失，再同 key 重试。
- 断言：候选不可公开使用；提交前旧 session 有效且候选清理；提交后旧 ID 不执行，只恢复同一新 session/receipt；无重复候选或已成功却无目录的回复。

**CM-69 Clean 不能绕过宿主检查（H）**

- 前置：fake driver 返回 Clean，但分别存在 active execution、未完成协议或取消句柄。
- 步骤：请求归池，随后申请另一短 Lease。
- 断言：每种情况均拒绝复用并走关闭/隔离；全部宿主条件及 driver Clean 通过时才可 Returned。

**CM-70 过期幂等键与记录删除（H/W1）**

- 前置：签名令牌、fake clock，写入已接受且响应丢失。
- 步骤：有效期内重发同/不同输入；过期重发；超过记录保留期删除记录，再重放令牌；伪造 issuedAt/keyVersion。
- 断言：同 receipt、不同输入冲突；过期和删除后均不再执行；伪造拒绝；客户端不能用新键自动重试未知写入；open/context 的 owner 重启后旧令牌 SessionLost，运行时 receipt/token 不落盘。

**CM-71 worker ID 唯一性与原子登记（H/WN）**

- 前置：两个 worker 使用随机 runtimeEpoch/dbSessionId，目录支持唯一约束和 CAS；fake 强制 ID 碰撞。
- 步骤：并发创建，碰撞后重生成；目录丢失/owner epoch 失效后发送旧请求和取消。
- 断言：一个 ID 只登记一个 owner；未登记不返回成功；旧 handle 明确失效；不借配置找替代 session；不需要中央 ID 分配器。

**CM-72 错误、控制回执与能力审计（H/F/W1）**

- 前置：取消支持/不支持/已结束三种执行；UnsupportedPlan、EndpointOverlap 和旧 epoch 请求。
- 步骤：逐项调用，序列化 IPC/HTTP，重放前端状态；读取 durable provenance。
- 断言：CancelReceipt 三种 disposition 与终态分离，无 accepted 字段漂移；错误 code/大小写一致；effectOutcome 不被请求成功覆盖；审计包含非敏感能力版本，不含 live handle 或凭据。

**CM-73 空闲淘汰与活动事务句柄连续旅程（H）**

- 前置：同一 session/resource 上通过 `begin_session_transaction` 创建真实 fake TransactionHandle 并按 §6.5 完成登记；宿主侧检查全部通过（无活动 execution、无消费者、已收到 protocolDrained、预算与 owner 合法）；fake clock 可驱动 idle timeout。
- 步骤：开始事务并写入未提交 marker；推进到空闲淘汰期限并运行 cleanup；查询事务状态、尝试 commit/rollback，再用旧 dbSessionId 发起查询；记录物理 resourceId、事务 map、回滚/关闭调用。
- 断言：淘汰不得在活动事务仍映射时静默关掉物理资源后保留可提交句柄。目标行为是在回收时先在原 resource 回滚并移除/终结事务映射，再关闭资源；回滚结果未知则转 OutcomeUnknown/SessionLost，不重新连接后复用旧 dbSessionId 或在新 resource 提交旧 handle。事务状态不得在物理 session 丢失后继续报告 Active。失败后的显式新 session 使用新 ID，marker 不提交。
- 基线说明：旧实现应先记录复现证据：`cleanup_idle_connections` 保留 owner map，`get_session` 通过该 map 同 ID 重连，而 `session_transactions` 由命令层单独管理。此用例在连接管理重构完成前可作为已知失败，完成后必须转绿；不可删掉断言或用仅检查 UI 文案替代。
- 保留声明：以上仅为缺陷来源记录。本用例的断言只依赖行为（存在已登记活动句柄时不得淘汰并同 ID 重建），不依赖上述任何函数名或命令层映射结构。§14 逐步删除旧管理器与旧接口时必须保留本用例与 CM-74，不得按死代码一并清理。

**CM-74 会话级句柄登记与释放顺序（H/F）**

- 前置：fake driver 的命令分别返回事务句柄与游标句柄各一个，均已按 §6.5 登记；另备一个未登记的句柄。
- 步骤：推进到 idle 期限触发淘汰，在淘汰中途发起 commit；再分别走归池、setSessionContext 替换、closeSession 三条路径；最后提交未登记句柄。
- 断言：句柄在物理资源关闭前已在原 resource 上回滚/关闭并从 actor 注销；commit 落在旧 resource 或明确失败，不出现在新 resource；driver 返回 Clean 时若宿主仍有已登记句柄，宿主检查必须失败（§9.4）；未登记句柄被拒绝返回；actor 终止后不重建任何句柄，恢复只能以新 dbSessionId 显式重建。

## 17. 验收标准与证据

| 门槛 | 必须通过的用例 | 必须提交的证据 |
| --- | --- | --- |
| 类型/身份/目标 | CM-01～07 | 类型与 schema 测试、越权入口矩阵 |
| 会话正确性 | CM-08～19 | driver 真实状态 journal、连续 UI 旅程 |
| 并发/清理/预算 | CM-20～32 | 故障注入、许可会计、竞态测试 |
| 表格/来源/缓存 | CM-33～39 | 目标/权限/版本断言 |
| 三件套/Workflow | CM-40～53 | 提交边界、恢复核验、Job owner 断言 |
| 断线/事件/幂等 | CM-54～56、70、72 | runtime/前端与 W1 适配一致性 |
| Web/多实例/压力 | CM-57～60、71 | 按 W1/WN 阶段提交路由、分区与基准证据 |
| 事务失效回收 | CM-73、74 | idle cleanup、句柄登记与释放顺序、物理 resource 与后续显式重连的连续旅程 |
| 补充契约 | CM-61～69 | 持久化、规范化、TTL、结果流、调度、额度与故障 journal |

阶段性验收按开发计划选择对应门槛；最终验收覆盖全部适用用例，包括 CM-73、CM-74；基线阶段可暂时记录该遗留缺陷，P3 连接运行时阶段必须转绿。能力 Unsupported 的 driver 测试断言“正确拒绝”，不能静默 skip 后宣称能力已验证。真实测试环境不可用必须报告未验证范围，不能用 fake 代替真实协议结论。

结束时还必须满足：生产无裸 unwrap/expect、公共 API 无实现库类型、测试 typecheck 干净、生成文件未提交、无新增宿主数据库名称分支、无敏感日志。新驱动实现现有能力时，修改范围仅驱动包/选型/注册/元数据和测试。

每个功能同时验证：原缺陷解决、合法同类操作未误伤、失败后的下一步可继续。比如 USE 失败后仍能查 A，取消后能明确重新执行，计划过期后能重新比对，worker 丢失后能显式创建新会话。

## 18. 文档与交付维护

实现者按测试编号提交代码和正式架构说明，不新增进度台账/缺陷清单文件。协议变化同步所有 driver 实现；外部 Git driver 在其独立仓库更新。开发仅改 en.ts，发布前补齐翻译。迁移完 consumer 后删除旧路径与失效引用，最终将本文目标设计改写为已实现事实。
