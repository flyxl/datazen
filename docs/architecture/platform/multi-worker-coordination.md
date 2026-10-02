# 多 worker 路由、认领与全局预算详细设计

> 状态：P9 目标设计，2026-10-02。本文补齐 [概要 §9.2](system-overview.md#92-p9-协调与失租协议目标设计) 的实施协议；不表示当前已有集群。P5 的 Job claim fencing 先实现，P9 增加跨实例调度和全局预算。

## 1. 选型与安全前提

第一版支持多个无状态 API、多个 execution worker，采用 PostgreSQL 管理库保存 Job 与全局预算分配账。SessionDirectory 由一个权威内存协调服务提供原子 CAS；它不写磁盘、不快照、不使用 AOF。第一版不提供目录无损热切换，协调器重启显式使全部旧 session 失效。

Job 与预算写入使用同一个管理库主写端，禁止从滞后副本作认领、续租或分配决策。内存目录协调器只运行一个有效实例；部署切换必须先隔离旧实例并停止旧 worker 接受新 session。无法证明旧实例已隔离时，不启动替代协调器。P9 首版的这个可用性限制必须进入发布说明，不能宣称全控制面自动 HA。

所有 API/worker/协调器间使用部署签发的服务身份和认证通道，组织上下文由 API 签名或受认证 RPC 信封携带。RPC 不允许任意 worker 自报其他 workerId。workerId 为每次进程启动生成的全局唯一 ID，不能用 pod 名或固定 hostname。

全局预算账与 SessionDirectory 不同：它只保存节点获授的数量、代次、期限和核销证据，不保存 live handle、runtimeEpoch、ResourceLease、事务或池状态。协调器重启不将数据库预算账清零。新增表见 [持久化模型](persistence-model.md)。

## 2. 身份、时钟与消息

| 字段 | 来源与作用 |
| --- | --- |
| workerId | 每次启动唯一；用于服务身份绑定、Job claim 与预算归属 |
| runtimeEpoch | worker 启动时生成；只在内存、令牌和路由信封中存在 |
| directoryEpoch | 协调器启动随机值；只在内存，旧 epoch 的请求与 session 全部失效 |
| directoryVersion | 单条目录记录的 CAS 版本；十进制 Counter |
| claimGeneration | JobRepository 原子递增的认领代次 |
| allocationId / generation | 持久预算分配账的稳定键与递增代次；与 resource lease ID 无关 |
| operationId / payloadDigest | RPC 幂等操作键与摘要；不同摘要重用键拒绝 |

目录 TTL 使用协调器单调时钟；持久认领和预算期限使用管理库时钟。调用节点的 wall clock 不决定租约有效性，禁止把所有 TTL 判定混用一个客户端时间。

RPC 有上限 deadline、输入大小与重试次数。响应丢失先查询 operationId/receipt，再以同 key 重发；读不到确定结果时保留未知，不换 key 创建第二个候选、claim 或预算许可。计数与版本跨 JSON 用十进制字符串。

## 3. SessionDirectory 协议

### 3.1 记录与状态

目录记录只含组织、opaque sessionId、owner workerId/runtimeEpoch、directoryEpoch、owner 主体/客户端摘要、contextVersion、directoryVersion、状态、到期和候选关联。它不含连接秘密、驱动对象、attachment token 或数据库事务句柄。

状态为 prepared → active → draining → removed。prepared 不可供查询路由；removed 的关闭 tombstone 在内存保留连接契约规定的期限。目录丢失使所有记录和 tombstone 一并丢失，旧调用返回 SessionLost，不能根据配置创建同 ID session。

### 3.2 新 session 登记

1. API 完成授权与幂等接受，选择具备 driver/网络路由能力和可用预算的 worker。
2. worker 取得完整预算、建立 actor 和固定资源，向目录发送 prepare(operationId, owner, contextVersion)。协调器原子检查 ID 无碰撞、worker 存活和 epoch，返回 prepared 版本。
3. worker 完成本地 attachment 绑定与初始状态，发送 activate(expectedVersion)。协调器 CAS 激活后才发布可路由句柄与 receipt。
4. activate 响应丢失时按 operationId 查询：已 active 返回同结果；仅 prepared 则续作同 CAS；已失效则清理候选，原请求返回明确失效，不再另建第二个 session。

步骤 2/3 失败必须清理候选及其预算。API 不能在 prepared 状态把 sessionId 返回为可使用。worker 本地维护 directoryEpoch 与自身 actor 映射，接收路由时再次核验，不能只信 API 之前查过目录。

### 3.3 上下文替换

替换使用 expectedSessionId、expectedContextVersion 和 expectedDirectoryVersion。旧 actor 排空并通过句柄/事务前置检查后，新 worker 或原 worker 建立 prepared 候选。提交操作原子核验旧记录仍 active、版本一致、双方 owner/epoch 有效，再将旧记录 draining、新记录 active，并生成唯一 replacement receipt。

新 handle/attachment 只在该提交成功后可见。两个并发替换最多一个 CAS 成功；输家关闭候选。提交响应丢失查同 operationId，不能恢复旧 session 可执行状态。旧 actor 拒绝新业务命令，完成已允许的 cleanup 后删除旧目录记录与释放预算。活动事务/句柄不能被强行迁移。

### 3.4 Owner 路由

API 先按组织、主体和 handle 可见性授权，再校验 token epoch 和 active 目录。跨组织、不可见或伪造 owner 不暴露 worker 存在信息。有效路由带 directoryEpoch/version、runtimeEpoch/contextVersion、deadline、request/receipt 和认证上下文。

worker 在 actor 派发前重新核验本地 owner/context 与权限版本。路由过期最多重查一次当前目录；仅当原 receipt 尚未派发且目标仍是同 session 才转发。写操作未知、session 被替代或目录丢失时禁止自动重路由执行。

### 3.5 心跳、失联与目录重启

worker 心跳续其目录记录，但不延期客户端 attachment、事务或 Job claim。心跳到期将记录标 lost 并拒绝新路由，不宣称物理连接已经关闭。worker 与目录失联立即停止接受新增业务，按本地安全期限取消活动操作并 cleanup；确认取消、回滚、关闭前保留 unknown 与预算占用。

目录重启使用新 directoryEpoch，API 清空路由缓存，worker 隔离全部旧 actor 并清理。重新开 session 要新 ID 与新授权；不能将旧内存 actor“补登记”来恢复旧令牌。已接受 Job 按仓储核验恢复，与 session 路由恢复无关。

## 4. Job claim 与故障接管

### 4.1 原子认领

调度在管理库事务内锁定可认领行（`FOR UPDATE SKIP LOCKED`），检查 kind/handler 能力、授权、取消意图与恢复策略，再写 workerId、claimGeneration + 1、claimExpiresAt、stateVersion + 1。首次 queued 可直接认领；过期 running 必须先进入恢复核验，不能等同 queued 直接重跑。

renew、阶段推进、提交边界、checkpoint、Artifact 引用和终态写入统一使用以下谓词：组织 + jobId + workerId + claimGeneration + `claim_expires_at > 管理库当前时间`；需要状态变化的写入另检查 stateVersion。零行更新表示 claim 失效，调用方停止推进，不能无条件补写或重新获取旧代次。

claim 校验与写入必须是同事务或单条条件 UPDATE，不允许“先 SELECT 有效、随后 UPDATE”造成 TOCTOU。checkpoint 插入、Job 投影与版本更新同事务；byte Artifact 先写不可变字节，引用发布受 claim fencing，失效后的孤儿字节由清理器回收。

### 4.2 取消与接管

cancelJob 在授权后持久写 cancelRequestedAt，并发布版本事件；queued 取消不必等 worker 到达。worker 派发阶段前、获取资源后及批次边界检查取消。终态由 handler 的真实外部效果决定，取消请求本身不是 cancelled 完成。

旧 worker 暂停超过租约后恢复，所有仓储写被拒绝，亦不得启动新 SQL 或续用许可。新 worker 只读核验旧提交边界、源指纹和目标证据：证明未发生且可安全重放才能继续；证明已提交则补记边界；不可证明则保留 unknown 并结束为 failed/人工核验。

管理库 fencing 不能撤销已发送的外部 SQL。旧 worker 在失去 claim 前发送的 commit 可能晚于新核验到达；只有目标库同事务批次标记、目标级幂等机制，或旧进程与连接已被证明终止后进行的核验，才能消除此竞争。无法取得上述证据时禁止副作用接管；只读查询恢复不提升为可写资格。

## 5. 全局预算分配协议

### 5.1 两级账与原子分配

全局协调服务从 `quota_counters` 与 `budget_allocations` 在管理库事务内分配节点配额；worker 本地 BudgetCoordinator 从有效配额中发 ResourceLease。组织、用户、物理服务各级限制以及 control/interactive/metadata/job 保留同时检查，多端 Job 按 [迁移设计 §3](data-migration-jobs.md#3-多端预算重叠与授权) 一次申请整组。

分配按稳定 quota key 顺序锁行，条件是所有维度 `已分配且未核销数量 + 本次增加 <= 上限`。写分配记录与累加 reserved 同事务提交。相同 operationId/摘要返回原 allocation；响应丢失先查分配账，禁止另发一笔。

allocation 固定 workerId、服务 key、资源种类、类别及数量，generation 在变更/核销转换时 CAS 递增。已发数量只在 worker 明确归还且无活资源后减少；将保留改成另一个类别必须重新检查各级保留，不能借超时突破类别边界。

### 5.2 到期与保守核销

租约到期意味着 worker 不可新增资源，不意味着其已有数据库 socket 关闭。分配状态 issued → expiredHeld → reclaimed；expiredHeld 仍计入 reserved。续租只允许未过期且同 worker/generation；到期后不能通过 renew 复活原许可。

worker 正常释放时提交同 allocation/generation 的 closure receipt，证明本地活动资源与在途申请为零，协调器原子核销。部分归还只核销已关闭数量，不能把整笔许可清零。旧 generation 的归还/续租拒绝。

worker 失联后的核销需要独立证据：节点/进程已停止且数据库连接已确认清除，或网络隔离加上目标连接已关闭的证明。仅“心跳超时”“pod 被标 NotReady”或“撤销凭据”都不足以证明现存连接死亡。证据带操作者、时间、核验方式与摘要；无证据则 held，管理员可见容量受限，不能自动超额再分配。

预算协调服务重启读取所有未核销账，不清零。管理库不可用停止分配/续租；worker 到安全截止时间停止新工作并清理，已发许可持续占账。目录失效同样不核销预算。

### 5.3 本地执行门

worker 在准备资源、driver open、阶段派发、每批开始前检查 claim、directory（仅 session）、allocation generation 与期限。打开后但登记前失联的连接也计入本地 outstanding，并纳入 closure receipt。

P9 不要求每个 SQL 往返查询管理库，但本地安全截止时间不得超过权威期限；时钟不确定或续租结果不明立即停止新增工作。在途 I/O 必须纳入未知效果与保守占账，不能靠“本地判断过期”假定它自动取消。

## 6. 节点选择与 drain

节点声明 driverId、协议/能力快照、handlerVersion、允许 networkRouteRef、原生工具版本与 artifact 可访问能力；调度只选完整满足计划的节点。客户端不能指定 worker 绕过约束。候选建立时再核验 capability/config/credential revision，避免选择后配置变化。

drain 状态为 accepting → draining → stopped。进入 draining 后停止新 session/Job claim，API 不向它分派新工作；已有任务可在截止时间前完成，不能借 drain 延长事务或 attachment TTL。到截止时间请求精确取消、等待清理、记录未知范围，并逐笔核销已关闭资源。

升级一个 worker：drain → 验证活资源与分配归还 → 停旧进程 → 启动新 workerId → 注册能力与健康 → 开放调度。活 session 不迁移，用户得到 SessionLost/重开提示；持久 Job 只按恢复策略接管。API 单独重启可继续查询 receipt 与路由；目录升级须按 §1/§3.5 整体失效处理。

## 7. 可观测性与接口归属

目标 RPC 包括目录 prepare/activate/replace/lookup/remove/heartbeat、预算 allocate/lookup/renew/return/verifyClose，以及 worker execute/cancel/drain。先在 contracts 定义有界信封与错误映射，API 不自建另一套重试状态机。SessionDirectory 和 BudgetCoordinator port 的 P9 adapter 实现这些协议，签名修订与 fake 同步提交。

记录 pending/active/lost session 数、Job claim 代次与冲突、各级 issued/expiredHeld 数量、续租失败、队列等待、清理耗时和 unknown 效果。指标不带 SQL、秘密、live token；workerId/operationId 只进入受控日志，避免无限高基数指标标签。

## 8. 验收矩阵

| 故障 | 关键断言 | 用例 |
| --- | --- | --- |
| 两 API 同时打开/替换，提交响应丢失 | 一次 active 发布；查原 operation；输家资源与预算回收 | CM-57、68、71 |
| owner 死亡 / 暂停后恢复 | 旧路由失效、旧 claim 写拒绝、外部未知不重放 | CM-58、71 |
| 目录丢失与协调器切换 | epoch 全部变化；无旧 session 自动登记；预算账仍保守 | CM-57、58 |
| 预算 allocate 响应丢失 / 多端压力 | 无重复许可、无超额与 ABBA、control 保留可用 | CM-31、60、65 |
| permit 到期而 SQL 仍在途 | expiredHeld 继续占账；新 worker 无额度不能打开连接 | CM-58、60 |
| 旧 claim commit 晚到 / checkpoint 丢失 | 记录确认部分或 unknown；无证明不接管写阶段 | CM-47、58 |
| drain / 凭据与 ACL 更新 | 不再接新活；不重用旧 policy 池；清理证据完整 | CM-59、60、67 |

H 层 fake 验证 CAS/代次/保守核销，WN 至少两 API、两 worker 和独立管理库验证网络分区与进程暂停；D 层验证真实外部 commit 与连接消亡。单实例通过不替代 WN，暂停节点恢复后的观察窗口须覆盖最长 claim/许可期限。
