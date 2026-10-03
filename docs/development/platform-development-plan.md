# 连接管理重构与桌面 / Web 多形态开发计划

> 状态：待执行计划；基线：2026-09-30，`8592b0fe1`。本文按用户明确要求入库，不记录每日进度。
> 设计依据：[系统概要](../architecture/platform/system-overview.md)、[连接管理详细设计](../architecture/platform/connection-management.md)。

## 1. 交付目标与实施原则

最终交付桌面本地、浏览器团队、桌面团队三种方式，共享 Rust 业务内核和 React 产品。连接管理对用户、会话、物理资源和任务分别建模；新增已有能力的 driver 不要求修改宿主业务流程。

先验证资源语义，再迁移 consumer，再增加团队入口和多实例。初期采用模块化单体；远程 agent、独立 driver runner、跨 backend 迁移是独立后续版本，不阻塞首版。

代码变更按阶段拆为可验收的小 PR。每个 PR 包含实现、相关测试和已落地事实文档；不创建进度台账、Bug List 或新方案副本。本计划是阶段顺序与交付门槛，完成的阶段应改为实施事实或合并到开发流程，不无限保留历史方案。

不能把后端重构、Driver API 协议迁移、所有 UI 和 Web 服务一次性切换。旧路径只做有截止点的窄兼容；同一 consumer 同一请求只使用一种资源管理器。

## 2. 阶段依赖

```mermaid
flowchart TD
    P0[P0 基线与契约夹具] --> P1[P1 共享边界与 DTO]
    P1 --> P2[P2 Driver 资源与能力]
    P2 --> P3[P3 Session / Lease / Budget / Gateway]
    P3 --> P4[P4 桌面 tab 与元数据迁移]
    P3 --> P5[P5 Job 与迁移三件套]
    P5 --> P6[P6 Workflow / AI / MCP / Wapp]
    P4 --> P7[P7 单实例团队 Web]
    P6 --> P7
    P7 --> P8[P8 桌面团队客户端与发布]
    P8 --> P9[P9 多 worker 与全局调度]
    P9 --> P10[P10 驱动扩展验收与旧路径清理]
```

P4 与 P5 代码边界独立后可以分别推进，但必须共享已稳定的 P3 契约；图表示依赖，不要求自动启用多个代理。没有用户明确授权时不派发其他聊天任务。

## 3. 每个阶段的统一完成定义

1. 所需 API、DTO、错误和生命周期都有实现，不能只交接口骨架。
2. 详细设计对应测试编号通过；Unsupported 能力有拒绝断言。
3. 生产路径不新增裸 unwrap/expect；测试文件通过 typecheck，无 any 绕过。
4. driver 方言实现及测试在 driver 目录；host 无数据库名称分支。
5. 新协议兼容声明真实，所有参与构建的 path/Git driver 已迁移或明确禁用对应新能力。
6. Agent 不读取 `.env` / `.env.test` 内容进入上下文；测试程序读取合法且输出不得泄密，不提交凭据、生成文件或外部子仓库内容。
7. 当前架构文档同步描述已经实现的部分；未实现功能仍标记目标设计。
8. 评审看到清楚的启用条件、关闭/失败行为和回退边界。

## 4. P0：现状基线与契约测试夹具

**目的**：在移动代码前识别逻辑 session、pool handle、transaction handle 的真实语义，建立可比较的行为基线。

输入为当前 ConnectionManager、Driver API、Query/Table consumer、三件套和 Workflow。重点代码链接见详细设计第 2 节。

交付：

- 清点各 consumer 的连接创建、复用、关闭、取消和配置目标路径；结论放进代码测试与当前服务文档，不写调查台账。
- 建立 transport-neutral fake resource，支持 resourceId、owner、命令 journal、屏障、fake clock 和每阶段失败注入。
- 在 driver 目录建立真实固定会话/事务/目标/清理契约测试；确认不能保证的能力。
- 确定测试环境注入方式，检查启动器/失败输出不回显受保护 env 内容。
- 记录现有 DTO 与持久化格式兼容范围；旧 session ID 不作为迁移数据。
- 用 CM-73 复现并留存“空闲淘汰关闭物理连接、同 ID 重连、事务 map 未清理”的基线；测试作为 P3 强制回归门槛，其断言只依赖行为，不得随旧管理器删除。

负责边界：runtime 负责人定义 fake 契约，driver 维护者验证真实协议，前端负责人验证 tab 行为。

退出门槛：CM-01、04、07 的基线测试，以及 CM-73 对当前事务句柄生命周期的可复现证据；真实 driver CM-08～19 的能力验证能区分已支持和缺失。已知失败作为本轮后续阶段要修复的测试，不能删除断言。

回退：仅增加夹具和测试；不改变生产执行路径。

## 5. P1：抽取共享应用边界和前端传输契约

**目的**：业务可在没有 Tauri 的测试进程中调用。

交付：

- 创建并注册拟新增 application/runtime/platform-api/backend-client 包，明确依赖方向。
- 定义 RequestContext、ID newtype、CanonicalTarget、namespaceShape/操作级 targetRequirements、会话、执行、Job、Artifact DTO 和 ApiError。
- 分开 DurableExecutionRecord 与实时 runtimeBinding，定义能力快照、CancelReceipt 和签名幂等令牌 port；所有持久化路径排除运行时句柄。
- 定义 repositories、SecretProvider、NetworkProvider、ArtifactStore、EventSink ports。
- Tauri adapter 组装本地身份，调用应用服务；保留现有 IPC 外观的窄适配。
- BackendClient 和 PlatformServices 注入，迁移 SDK 中 invoke/Channel，存量调用用薄再导出过渡。
- 加入依赖护栏：core 不引用 Tauri/HTTP，driver 不引用 host，前端领域不直接绑定平台传输。
- 加入 CI 架构检查：对 server crate 的 normal/build 依赖闭包验证不含 Tauri crate 和 UI runtime，并验证 server 独立构建；依赖检查失败即阻断合并。

退出门槛：CM-01、03～07 的**类型与 schema 断言**、DTO 往返与内核无 Tauri 编译、CI 依赖图检查通过；旧桌面基本连接/查询流程保持可用。同一批用例的 H 层行为断言归 §7（P3）、CM-05～07 的 W1 断言归 §11（P7），阶段归属见 §17，本条不重复计。

回退：Tauri adapter 可仍调用原用例实现，但不得把新 owner 和权限语义伪映射为旧共享 session。新 frontend bridge 在未注入时明确报错。

## 6. P2：Driver 固定资源与可选能力契约

**目的**：把“执行在同一真实资源上”变成可测试保证。

交付：

- opaque resource、ResourceProvider、固定执行、状态观察、reset、close 和精确 cancel 契约。
- namespace/session/transaction/snapshot/data/backup 可选能力注册与运行时能力降低机制。
- 所有实际建连的预算 port，包括 driver pool、集群节点和控制资源。
- 现有 Command 和迁移接口保持领域语义；资源获得方式通过 adapter 演进。
- 先迁移具有真实会话与事务的代表 driver，再迁移其余参与构建驱动；不支持能力明确拒绝。
- 修改公共契约时核对 crate SemVer、PROTOCOL_VERSION 和 minimum compatible；同步 path/Git driver、factory、ReuseDriver、生成选型路径。判定规则与门禁见 [driver-capability-migration.md §5.3](../architecture/platform/driver-capability-migration.md)：任何 breaking 改动强制升 `PROTOCOL_VERSION`（`pnpm test:driver-protocol`，CI 硬门禁），升 `MIN` 是主动放弃老驱动兼容性的独立决定，不是 breaking 的处理方式。

退出门槛：D 层 CM-07～19、22～26、30、45、48；不同 driver 不共享实现库类型；新增能力缺失不会 no-op 成功。

回退：过渡 adapter 可以支持旧 driver 的独立受控 Command，但不能开放未证明的 statefulSession。

「协议升级与驱动发布是原子兼容门槛，不允许用下调最低版本掩盖 breaking change」这句回退约定分两半落地，现状（`scripts/check-driver-protocol-compat.mjs`，由 `pnpm test:driver-protocol` 接入 CI 硬门禁，28 个单测）：

- **已实现——"不允许下调最低版本"这半**。脚本把它写成两条不变量并直接阻断：`min-protocol-never-lowered`（`MIN_PROTOCOL_VERSION` 不得下降，报错文案即回指本节）与 `protocol-window-non-empty`（`MIN_PROTOCOL_VERSION <= PROTOCOL_VERSION`）；另外对 10 条受管契约（`CONTRACT_RULES`：`database-driver` / `key-value-driver` / `driver-factory` / `resource-provider` / `budget-port` / `resource-error` / `driver-command-definition` / `capability-set` / `capability-registry` / `reuse-driver`）内的增删改强制 `PROTOCOL_VERSION` 上移。当前窗口是 `PROTOCOL_VERSION = 4`、`MIN_PROTOCOL_VERSION = 1`。
- **仍缺——"原子"那半，待 P0 验证**。协议号上移与驱动发布是否在同一次交付中发生，属于发布流程约束，脚本无从判定，因此没有任何门禁覆盖它；并且 `LegacyResourceAdapter`（`packages/driver-api/src/resource_adapter.rs`）不在这 10 条受管契约之内，其构造签名目前可以在不升协议号的情况下改变。**本节不新增门禁脚本**；补齐属于 CI 轨道的独立决定。

## 7. P3：统一 Session / Lease / Budget / ExecutionGateway

**目的**：建立桌面与 server 都能用的资源运行时。

交付：

- SessionRegistry actor、状态机、owner/runtimeEpoch/contextRevision、独立 attachment 与内部 resourceBindingId。
- ResourceManager lease、pool、cleanup/quarantine、隧道引用。
- 多维单机预算、control/interactive/metadata/job 保留与公平队列、逻辑 session/已连接编辑器额度、真实物理资源计数。
- attachment 令牌与 TTL 最早期限裁决；无消费者有界产物、discard/drain、取消与协议恢复；PoolKey 版本生产者与缓存失效。
- 单进程 SessionDirectory、候选替换提交屏障、过期令牌拒绝与 Clean 双重检查。
- ExecutionGateway：授权、目标校验、schema、幂等 receipt、状态、来源、事件。
- 独立取消路径，取消和关闭先等待执行/协议结束；所有失败阶段 cleanup。
- 显式 session 失效和新 session 恢复；禁止原 ID 透明重连。
- 指标：物理资源数、lease state、预算等待、执行队列、取消耗时、cleanup 失败、Unknown outcome。

退出门槛：H 层 CM-02～07、20～32、37～39、54～56、60 单机部分及 CM-61～74 的 H 断言；CM-73、CM-74 必须转绿，证明会话级句柄（事务/游标）在物理资源释放前已注销，事务状态、物理资源和 session ID 同步失效。race 测试使用 barrier/fake clock，不靠任意 sleep 猜顺序。

回退：新 runtime 仅给已迁移 consumer 使用；不同时维护一条资源的旧 refs 和新 lease。服务暂停/重启前关闭新 session，不能将活事务移回旧 manager。

P3 开始前的 DTO/port 目标契约同步修订已落地，**只有契约与口径，没有任何运行时实现**（`ArtifactStore` 至今零实现方、零调用方）：Artifact 写入期已发布块可读且不可覆盖、abort/截断终结与恢复元数据（`ArtifactLifecycle`、`AbortReason`、`ArtifactMetadata`，端口补 `describe`/`abort`）；取消请求不覆盖实际执行终态（`ExecutionState::resolve_cancel` + `CancelOutcome`，对应 CM-22 四种竞态）；CM-60 对每请求两段耗时求和后计算 p95（`runtime::latency` 的 nearest-rank 口径，禁止两段 p95 相加代替逐请求求和、禁止剔除失败样本）。旧契约修订不得标为现有行为。退出门槛增加流式块读取/重连、writer 异常终结、取消四种竞态与基准口径断言，详见共享边界 §4.6、夹具 §6.3/§11。

## 8. P4：桌面 QueryPanel、TablePanel、元数据迁移

**目的**：先在现有产品中完成连接正确性，不等待 Web。

交付：

- QueryPanel 每编辑器独立懒 session，复制 tab 不复制 runtime ID。
- 删除执行前从 SQL 推测已生效上下文的行为，保留补全与浏览推测。
- 下拉切库支持确认、revision 冲突、两阶段替换和事务处理。
- TablePanel 使用明确对象 target；ChangeSet 保存时短事务，手工事务显式开关。
- 元数据批量与缓存按身份/配置/目标隔离；临时对象绑定原 runtime handle。
- 旧结果 provenance、读写映射与临时结果失效规则。
- 关闭 tab、断线、错误后的继续操作和用户提示；开发只新增/修改 en.ts。

退出门槛：CM-08～18、21、33～39、55～56、61～64、72 的 F 断言；Host 通用 WDIO + driver 专属旅程；桌面 Community 与 Pro consumer 兼容验证。

回退：切换以 consumer 为单位，启用前 session 全部重新建立；不能在活动事务中切 feature path。存在数据写入未知时先核验，回退 UI 不等于数据库回滚。

补充交付与门槛：按连接设计 §12.1 实现块去重、缺块恢复与旧事件隔离；执行→部分结果→取消/断线→恢复→再次执行的连续旅程须覆盖切库、替代 session 和关闭结果视图。旧 consumer 在对应迁移验证完成的 PR 中删除；P10 只做最终残留核验。

## 9. P5：JobRuntime 与数据迁移三件套

详细设计：[迁移三件套与 JobRuntime](../architecture/platform/data-migration-jobs.md)。准备与应用分别受理，apply planId 的唯一消费、逐批提交证据与 claim 校验在 P5 同步实现。

**目的**：连接资源脱离 UI，任务结果真实反映提交边界。

交付：

- JobRepository、JobHandler 注册、资源阶段、进度、取消和结果。
- 持久化接受后返回 jobId，窗口只订阅；任务中心可重新附着。
- 将 `src-tauri/src/schema_diff`、`data_sync`、`data_transfer` 中与 Tauri 无关的领域实现按依赖分阶段抽取到概要 §4.1 的领域包；Tauri commands、窗口、文件选择保留在 adapter。不得把当前目录位置误当成最终共享边界。
- Schema Diff 比较/审阅/应用阶段分开、目标指纹复验、DDL 原子性按计划判断。
- Data Sync 同族/结构/PK 门闸、快照范围、review 释放、冲突检测与批次事务。
- Data Transfer 有界数据管道、IR、源一致性、目标批次记录与检查点核验。
- endpoint 重叠识别、源目标多端预算申请、防 AB/BA 死锁。
- 明确 successful/failed/cancelled 与 effectOutcome，保留部分提交和未知信息。

退出门槛：CM-31、40～49、54；Job 执行期 UI unmount 不调用资源 release；已提交/未提交/未知边界通过真实数据库断言。

回退：新 Job 暂停派发，已有任务到安全边界后结束；计划/检查点格式升级使用版本，新旧引擎不能接管不兼容计划。已经写入的任务只按核验与补偿处理。

实施前冻结连接设计 §10.1.1 的 JobHandler、plan/checkpoint 版本、取消意图、effectOutcome 聚合与恢复决策。claimGeneration 及所有 worker 写入的认领校验在 P5 落地，多 worker 调度基础设施才留到 P9。退出门槛增加提交成功/checkpoint 未写、checkpoint 已写/终态未写、版本不兼容与旧 claim 写入拒绝的故障旅程；无法证明目标边界时待核验，不自动重跑。

## 10. P6：Workflow、AI、MCP、Wapp 与辅助任务

详细设计：[Workflow 资源模型](../architecture/platform/workflow-resource-model.md) 与 [消费者接入](../architecture/platform/consumer-adapters.md)。共享授权失效后拒绝绑定操作，MCP/Wapp 只释放自有资源，调度目标和服务授权显式保存，原生工具也计入完整预算。

**目的**：消除 GUI 外入口的隐式目标和共享状态。

交付：

- Workflow 独立 step、session block、transaction block 的模型、校验和执行。
- 明确 connection/namespace 继承、并行分支资源、重试范围、checkpoint 边界。
- AI 默认独立操作，显式授权当前 editor session 时同队列。
- MCP client/session owner 与 TTL，Wapp 实例和任务归属。
- Dashboard/Monitor 独立资源；导出、备份、恢复归 Job/Artifact；原生工具由环境 port 管理。
- 服务身份/委托权限用于调度，所有入口走 ExecutionGateway。

退出门槛：CM-50～53；辅助任务共享预算、不污染编辑器；原 Workflow 的 query/command 默认目标契约仍通过。

回退：旧 Workflow 定义按版本解析，默认独立 step 语义；新 block 格式不交旧 executor。后台任务取消与运行状态通过 Job 查证，不用 GUI 关闭推断。

实施前修正 Workflow §4.2/§6.2：显式目标不完整即拒绝，不回退另一连接；block 冲突检查先于任何目标返回；session block 清理后仍默认不自动重试。退出门槛增加已自动提交一步后失败不重复写入、未知提交不重试、已确认回滚后的受控重试与 block 目标冲突。AI/MCP/Wapp 各入口分别验证 owner、授权、TTL 和 Gateway 拒绝路径。

## 11. P7：单实例团队 Web 服务

详细设计：[团队服务与认证](../architecture/platform/team-server-and-auth.md)、[持久化模型](../architecture/platform/persistence-model.md) 与 [部署、升级和恢复流程](team-service-operations.md)。运维验收使用发布镜像，备份恢复不得自动重放未知外部写入。

**目的**：先交付可部署、可授权的统一团队入口。

交付：

- server host，无 Tauri 依赖，初期同进程管理与执行。
- OIDC、membership、服务端登录会话、RBAC/资源 ACL、CSRF、同源 HTTPS 部署。
- ProfileRepository/JobRepository/Audit/ArtifactStore 的服务端实现和 schema migration。
- secretRef/轮换、个人/团队执行身份、出站网络政策。
- 概要设计 v1 HTTP API，幂等提交、SSE 重订阅、分页/大小限制、受控上传下载。
- React Web shell：窗口替换为页面/面板，文件和剪贴板使用 Web 平台实现。
- 配额、任务中心、结果 TTL、配置版本 CAS、权限撤销传播。
- API schema/client 生成或契约一致性门禁，桌面 IPC 与 HTTP 同用例测试。

退出门槛：CM-05～07、38～40、53～56、59、60 单实例部分及 CM-63、68、70、72 的 W1 断言；浏览器无需数据库凭据；组织隔离、CSRF、产物与事件越权全部拒绝。

发布门槛：部署文档明确管理数据库、SecretProvider、网络可达性、备份、迁移、TLS 和资源额度；管理员能恢复服务元数据，活动数据库会话不声称可恢复。

回退：保留上一版本镜像和向后兼容 schema；数据库 schema 采用 expand/contract，不先删字段。drain 活任务后回退；发生外部数据库写入不能通过回退镜像撤销。

上线前冻结并验收：实际授权变更与 permissionVersion 同事务提交；普通 DDL 与历史登记同事务，事务外索引迁移可核验恢复；schema 支持范围允许旧镜像运行于下一版 expand schema；授权可见性先于会话存活错误；SSE 缺失 Origin 有明确校验；Web Wapp 使用独立真实 origin、可嵌入 CSP 与真实 iframe 握手。每项均有对应浏览器/管理库故障测试，不以 fake 替代。

## 12. P8：桌面团队客户端与首版发布

详细设计：[团队服务 §4.6](../architecture/platform/team-server-and-auth.md#46-p8-桌面团队认证与传输目标设计) 定义桌面登录及原生传输；[运维设计](team-service-operations.md) 定义兼容、升级和恢复演练。

**目的**：同一客户端可选择本地或团队 backend，避免句柄和凭据混用。

交付：

- backendId、连接来源、版本握手与 per-backend client。
- 桌面登录团队服务、HTTPS/SSE 生命周期；团队 session 不经本地 Driver 执行。
- 本地 profile/session 与团队 profile/session 独立索引；服务退出不删除本地配置。
- API major/minor 兼容提示；不兼容阻止写操作，不能默默降级。
- 第一版同 backend 迁移限制，选择跨 backend endpoint 明确拒绝。
- Community/Pro 桌面及团队发行 capability 清单；高级扩展仍经原有 EP 契约。
- 发布前 en.ts source key 检查与翻译补齐、功能使用和部署文档。

退出门槛：桌面离线本地可用；桌面/浏览器团队同用例一致；本地 ID 发往团队服务拒绝；团队秘密不存入本地普通 profile；CM-55/56 与 backend 隔离旅程通过。

回退：移除远程 backend 绑定前显式关闭/保留远程任务；本地状态不受影响。不能把 remote SQL 转为本地执行来“降级”。

认证和传输以团队服务 §4.6 为权威：系统浏览器 OIDC 与 PKCE 一次性交接、native 不透明会话凭据、钥匙串分区、HTTP/SSE 共用 native adapter；浏览器 cookie 路径仍独立。补交付 backendId/clientGeneration 的迟到事件隔离、认证过期/撤销与证书错误处理、版本/能力握手。发布门槛增加交接重放/恶意回调/认证中服务重启、双 backend 同名 ID、登出后 SSE 重连与未知写入不重投的连续旅程。

## 13. P9：多 worker、网络分区与全局预算

详细设计：[多 worker 协调协议](../architecture/platform/multi-worker-coordination.md)。首版目录为单权威内存服务，Job/数量型节点预算在管理库持久化；目录切换先隔离旧实例，全部旧 session 失效。该控制面可用性限制必须进入发布说明。

**目的**：扩容不破坏会话连续性和总连接上限。

交付：

- RuntimeRouter、SessionDirectory、owner worker epoch、认证内部调用。
- worker 能力/版本/网络区域匹配，固定 session 路由，替代 session 新 ID。
- 总预算节点分配/许可机制，idle/控制/cluster 资源统计与分区保守回收。
- Job claim/renew/fencing、失租停止派发、接管前目标核验。
- worker drain、滚动升级、任务恢复和元数据/权限事件失效。
- 压力、故障注入、协调器失联、worker 暂停后恢复测试。

退出门槛：CM-57～60 的 WN 部分、CM-68/71 多 worker 断言；关闭 sticky 后仍正确；worker 故障不伪恢复事务；旧 worker 隔离未证明前不复用额度；提交未知不盲目接管。

回退：停止新任务/会话路由并 drain 至单 worker；不迁移 live session。多实例元数据可保留，但执行 owner 必须明确，不能把目录清空后继续使用旧 ID。

实施协议见 [多 worker 详细设计](../architecture/platform/multi-worker-coordination.md)，概要 §9.2 保留边界摘要：目录 owner/替换 CAS、仓储 claimGeneration、全局许可 generation 与保守核销、协调器失联、目录丢失和 drain 状态转换。P5 的管理库 fencing 与 P9 的全局预算许可分别验收，均不能自动撤回外部 SQL。退出门槛增加旧 worker 暂停超过租约后恢复、在途 commit、目录/协调器重启和许可未核销时禁止再分配。

## 14. P10：扩展性验收、旧路径删除与稳定化

详细验收：[驱动迁移 §10.2](../architecture/platform/driver-capability-migration.md#102-p10-目标通用-contract--driver-矩阵)。实际发布选型覆盖 path 与 Git driver，Git ref 冻结为 SHA，能力声明与逐层运行结果分别记录。

**目的**：验证架构目标，而不只验证代表驱动能工作。

交付：

- 选择一个实现既有能力的新增/适配 driver，限定改动在 driver 包、注册选型、schema/UI/文案和 driver 测试。
- 运行通用 contract × driver 矩阵，确认 unsupported 路径明确拒绝。
- 删除 consumer 的旧共享 session/reconnect/双执行路径，删除 SDK 重复实现。
- 更新 Driver 公共协议、独立驱动指南和已实现架构；平台计划及关联设计逐节转为事实/流程。
- 采集 fake runtime 基准、真实典型负载和资源计数，按实测调整默认预算，不承诺跨环境 SQL 延迟。

退出门槛：详细设计 CM-01～74 全部适用门槛，新增 driver 不改宿主数据库分支；未能执行的真实环境验证明确列入发布限制，不能称全部通过。

回退：只回退新增驱动选型/包；已删除旧路径不为了兼容缺失能力重新引入。公共契约确有新语义需求时走版本化演进。

最终验收逐条列出 H/D/F/W1/WN 的适用性、实际执行结果与限制；unsupported 是显式拒绝成功验证，未运行是未验证，二者不得混同。性能使用 P3 的逐请求总开销 p95；真实驱动、浏览器、多 worker 各自产出证据，不能用 fake 或单实例结果替代。旧路径由 P4/P5/P6 分 consumer 删除，P10 检查注册、SDK、重连和资源释放路径无残留。

## 15. 验证命令与 CI 规划

### 15.1 当前已有命令

以下命令已核对 `package.json`/workspace；Agent 不读取受保护 env 内容进入上下文，测试程序运行时读取合法。执行前核对日志/失败输出不会回显凭据，长输出按 AGENTS.md 落系统临时文件。

```bash
pnpm typecheck
pnpm test:unit
pnpm test:unit:drivers
pnpm test:layers
pnpm test:boundaries
cargo test -p datazen --lib
cargo test -p datazen-driver-api
cargo test -p datazen-driver-postgres
pnpm tauri:build:webdriver
pnpm e2e:skip-build
pnpm e2e:contract:matrix
```

WDIO 必须使用已注入 driver 的构建；三件套可运行 app 已生成而仅 DMG 失败时继续功能验证。driver 专属 E2E 不放 Host。新 runtime/application/server crate 创建后，其 package 名和测试命令同步写入 CI；不把未创建命令列为现有脚本。

### 15.2 新增 CI 门禁

- application/runtime 无 Tauri/server/UI 依赖构建。
- Driver API 无实现类型泄漏，driver 不 import host。
- DTO 与 API schema 的生成漂移检查。
- runtime fake 故障/竞态/预算测试。
- IPC/HTTP 适配一致性测试。
- Web 身份/权限/CSRF/SSE/Artifact 集成。
- 真实 driver 契约矩阵（各 driver 内部测试）。
- 多 worker 分区/drain/接管测试。
- 本地 Community、Pro、Web 与桌面 remote 的核心旅程。

## 16. 工作拆分和交接

每阶段可按“公共契约 → runtime → driver → consumer → 验证”拆 PR；有依赖的实现基于已合并契约，不自行复制 DTO。公共能力变更由 API/runtime 负责人评审，driver 维护者评审真实协议，应用负责人评审用户下一步流程，团队权限由 server 安全负责人评审。

实习生先完成 fake/runtime 中不含凭据的类型、会计、actor 和状态投影任务，再在维护者指导下实现 driver 协议与真实数据写入路径。工作交接必须包含测试编号、执行命令、实际结果、未验证能力和回退边界；不需要新增交接文件，写入 PR 与正式代码文档。

不承诺未经估算的日期。P0/P1 完成后根据 driver 缺口、消费者数量和团队规模估算后续工期；任何阶段不能因赶进度跳过会话、权限或提交未知的验收门槛。

## 17. 补充契约的阶段归属

| 阶段 | 必须交付的补充契约 | 验收范围 |
| --- | --- | --- |
| P0/P1 | DTO、持久化白名单、目标规范化、错误/回执、能力快照、令牌 port | CM-61/62/72 的类型与 schema 断言 |
| P2 | namespaceShape、targetRequirements、规范化资源 key、driver 清理与协议 drain | CM-62/64/67 的 D 断言（**三条均未落地**，逐条现状见本节末） |
| P3 | attachment、结果放弃、类别调度、逻辑配额、PoolKey/cache、替换发布、幂等过期、内存目录、事务失效回收与会话级句柄登记 | CM-61～74 的 H 断言，CM-60 基准 harness |
| P4 | 恢复结果只读/写回、TTL 与切库错误投影、关闭结果视图行为 | CM-61～64/72 的 F 断言 |
| P5/P6 | 能力快照计划核验、Job 多端预算、独立消费者政策 | CM-40～53 与 CM-65/67 的任务断言 |
| P7/P8 | 单实例认证重附着、HTTP 回执与持久化投影、签名令牌和 SSE | CM-63/68/70/72 的 W1 断言 |
| P7/P8 | 登出、权限撤销与 CSRF 跨站拒绝（撤销传播到本实例的订阅与排队执行） | CM-59 的 W1 断言 |
| P9 | 原子 owner 登记、碰撞/失效、跨节点替换发布、分区和全局预算 | CM-57/58/60/68/71 的 WN 断言 |

测试标签代表执行层，不代表整条用例只属于某阶段。P3/P4 必须完成 CM-54～56 的 runtime/前端部分；P7 完成单实例适配部分；只有 owner 路由、worker 分区及跨节点预算推迟到 P9。P10 逐层汇总全部 74 条适用用例，不能把未执行的 D/WN 断言计为已通过。

### 17.1 P2 验收范围的实际落地情况

P2 行的三条 D 断言都还没有落地。已经落地的是它们各自的 **Host 侧测试夹具**，落在 `packages/runtime/src/connection/testing/`（`#[cfg(any(test, feature = "test-harness"))]`，`cargo test -p datazen-runtime --lib` → `139 passed` / 0 failed，EXIT=0（计数与退出码测于 `5b7c5b49c`）。逐条对照：

- **CM-64（无消费者、截断与有界 drain）**：`testing/barrier/` 下的协议 drain barrier 夹具已可用，`barrier/tests.rs` 内 19 个单测（计数测于 `5b7c5b49c`）覆盖"无消费者时写入阻塞 → FakeClock 越过 drain 期限 → 截断"（`write_without_consumer_blocks_then_truncates_after_the_drain_deadline`）、消费者恢复后不再截断、按执行字节上限独立截断、订阅额度与执行额度分别计数、sink 失败上报。**这是测试夹具而非生产实现**：artifact 配额、unsubscribe 不立即取消 SQL、协议 barrier 的 D 侧断言均未落地。
- **CM-67（PoolKey 版本和多 database）**：`packages/platform-api/src/ports/budget/pool.rs` 的 `PoolKey` 与 `datazen-runtime` 夹具的 `policy_isolation_key` 已能支撑"换键判据"（`pool_key_equality_covers_all_eight_components` 等单测）。但**全仓没有 pool / lease 管理器的实现**——只有 `packages/platform-api/src/ports/budget/coordinator.rs:123` 的 `BudgetCoordinator` **trait**（port 契约），而 `ResourceManager`、`LeaseManager`、`SessionRegistry`、`PoolManager`、`ConnectionPool` 连类型都不存在。因此空闲资源停发并关闭、旧缓存不回填、撤权即时生效、空池元数据受 LRU 约束这四条断言未落地。
- **CM-62（命名空间规范化矩阵）**：**「代码侧零覆盖」这个结论是错的，错在检索方法**。原结论用「按用例编号 `CM-62` 检索代码扩展名」得出命中 0；该检索**可复现但无效**——测试函数不按用例编号命名，一律按被断言的行为命名，所以按编号检索必然查不到任何断言。改按行为反查（`canonicalize` / `NamespaceShape` / `validate_requirements` / `TargetResolver` / `TargetConflict`）后，7 步中 **6 步有断言**，分属两个层面，**可达性不同**：

  - **driver-api 层：生产可达。** 实现是 `packages/driver-api/src/namespace.rs` 的 `NamespaceShape::canonicalize`（`:189`）与 `validate_requirements`，14 个单测在 `packages/driver-api/src/namespace_tests.rs`（经 `namespace.rs:336` 的 `#[path]` 挂为 `namespace::tests`）。②不存在层级非 null → `canonicalize_rejects_a_value_for_a_level_that_does_not_exist`（`NonexistentNamespaceLevel { kind: Schema }`）；③required null → `canonicalize_rejects_a_missing_required_level`（`MissingRequiredNamespaceLevel { kind: Catalog }`），操作级 required / forbidden 由 `requirements_reject_a_missing_required_level`、`requirements_reject_a_forbidden_level` 覆盖；④optional 按操作允许 → `requirements_reject_a_missing_required_level` 的 `is_ok()` 分支；⑤同义 path → `canonicalize_merges_aliases_to_a_fixed_point`（`canonical.path[0] == "sales_main"`）、`canonicalize_appends_driver_specific_path_segments`；⑥冲突 path → `canonicalize_rejects_an_alias_cycle_instead_of_picking_one`（`ResourceError::AliasConflict`，**错误类型不是 CM-62 写的 `TargetConflict`**）。①缺字段在这一层**既无断言也无拒绝**：`canonicalize` 只判 `None`（`:197` 跳过）与层级存在性，`Some("")` 照常进 `resolved`（`:206`）。生产调用点：`packages/drivers/redis/src/resource/provider/contract.rs:66`、`:180`，`packages/drivers/postgres/src/resource/provider/contract.rs:64`、`:178`。

  - **application 层：已实现、已断言，但生产不可达。** 实现是 `packages/application/src/target.rs` 的 `TargetResolver::compute` 六步流水线，12 个单测在同文件 `mod tests`（`:336`）。①`empty_field_values_are_rejected_before_anything_else` → `InvalidArgument`（第 1 步 `validate_namespace_target`，`target.rs:100` → `packages/application/src/dto/requests.rs:40`）；②`the_six_steps_run_in_the_documented_order` → `TargetUnsupported` 且信息含 `catalog`，`unknown_operation_or_connection_fails_closed` 覆盖失败关闭；③`a_missing_required_layer_is_target_required_not_a_backfill` → `TargetRequired`；④`session_defaults_apply_only_when_the_command_allows_them`（`execute_in_session` 可补、`query` 不可补）与 `object_identity_is_gated_by_the_command_requirements`（schema 可省）；⑤`a_successful_computation_yields_canonical_identity_only`（别名 `main` 与真名 `app` 落到同一身份）、`the_canonical_target_round_trips_and_carries_no_display_name`（别名不出现在序列化结果里）；⑥`alias_in_the_wrong_slot_is_a_target_conflict` → `ApiErrorCode::TargetConflict`，即 CM-62 所指的那个 `TargetConflict`。**但全仓没有生产代码调用 `compute()`**：唯二两处 `TargetResolver::new` 都在本文件测试内（`target.rs:380`、`:564`），`ApplicationServices` 不构造、`application_services()` 恒为 `None`（`src-tauri/src/platform/adapter.rs:8`、`:80`、`:187`）。

  - **⑦大小写别名：未实现。** `CaseFolding`（`namespace.rs:91`）与 `CaseRules { unquoted, quoted }`（`:107`）类型存在，redis / postgres / sqlite / mysql / mongodb 填了显式值，hbase / vector 填 `Default::default()`，但 `canonicalize()` 函数体（`:189`–`:229`）**从不读 `case_rules`**。全仓 `case_rules.` 仅 2 处命中，都在 `namespace_tests.rs:246-247`，断言的是 `Default` 的 `CaseFolding::Unknown`——`case_rules_default_to_unknown_rather_than_assuming_one_behaviour` 守的是**默认值**，不是折叠行为。

  - **尾部断言（缓存 / 权限 / 驱动共用同一份 `CanonicalTarget`）未落地**，因为没有消费方。`packages/application/src/target.rs` 全文无 `PathBuf` / `std::fs` / `Path::new`，这只说明该模块不碰文件系统，不构成「驱动拿到的就是这份 `CanonicalTarget`」的断言。

按 §3 的完成定义，契约类型存在不等于行为已实现并被断言覆盖，因此 P2 的这三条不能记为已交付。

## 18. 阶段详细设计入口

| 阶段 | 权威设计与实施落点 |
| --- | --- |
| P0 | [假资源夹具](../architecture/platform/fake-runtime-fixtures.md) |
| P1 | [共享边界与端口](../architecture/platform/shared-boundaries-and-ports.md)、[持久化白名单](../architecture/platform/persistence-model.md) |
| P2 | [驱动能力迁移](../architecture/platform/driver-capability-migration.md) |
| P3/P4 | [连接管理](../architecture/platform/connection-management.md)：actor/lease/budget、Gateway 与 §12.1 结果消费 |
| P5 | [迁移任务](../architecture/platform/data-migration-jobs.md)、持久化 §3.6、共享端口 §4.3 |
| P6 | [Workflow](../architecture/platform/workflow-resource-model.md)、[消费者接入](../architecture/platform/consumer-adapters.md) |
| P7/P8 | [团队服务与认证](../architecture/platform/team-server-and-auth.md)、[运维流程](team-service-operations.md)、持久化 §5/§7 |
| P9 | [多 worker 协议](../architecture/platform/multi-worker-coordination.md)、持久化 §3.11 |
| P10 | 驱动能力迁移 §10.2；各消费者退役条件与本计划逐层门槛 |

本表是设计入口，不是完成进度。目标设计、现有实现和实际运行证据分别标明；阶段完成后同步将对应内容改写为事实。
