# AI、MCP、Wapp 与辅助任务接入详细设计

> 状态：P6 目标设计；核对基线：2026-10-02，`d329539b9`。本文定义消费者接入规则，不表示团队后端已实现。基础接口、错误码与验收编号以 [连接管理](connection-management.md) 为准；Workflow 块语义以 [Workflow 资源模型](workflow-resource-model.md) 为准。

## 1. 消费者边界

所有消费者通过 application 服务与 ExecutionGateway 访问 driver；可恢复或脱离窗口的任务通过 JobRuntime。消费者不调用旧 ConnectionManager、不取得裸 driver 资源、不自行建立共享池。

| 消费者 | 默认资源政策 | 状态与归属 |
| --- | --- | --- |
| AI schema/context 查询 | metadata 短操作 | 单次工具调用，独立于编辑器事务 |
| AI 受控数据库工具 | interactive 短操作；明确授权后可绑定编辑器 session | principal + AI invocation；共享授权不是资源所有权转移 |
| MCP stateless tool | 完整稳定目标、短操作 | principal + MCP client instance |
| MCP stateful tool | 显式打开的专属 session | 当前 MCP client instance 独占 |
| Wapp command | manifest 许可范围内的短操作 | principal + wapp instance + request |
| Dashboard / Monitor | metadata 或 job 短资源 | 手工运行使用本人；调度使用显式服务身份 |
| Workflow、导出、备份、恢复 | Job 与独立资源 | Job 生命周期；窗口只负责订阅 |

`clientInstanceId`、owner、组织与委托范围由宿主适配器根据已认证入口生成。HTTP、MCP 参数和 postMessage 都不能指定任意 owner；与调用主体不匹配的资源按不可见处理。

## 2. 统一请求与授权顺序

入口按如下顺序处理：验证认证上下文 → 验证入口许可 → 规范化完整目标 → 检查组织与资源可见性 → 获取能力快照与 policyIsolationKey → 接受幂等请求 → 入队 → 派发前再授权 → 申请资源 → 执行与清理。

固定目标包括 connectionId、database、schema/namespace 和配置版本；缺必需字段返回 TargetRequired。GUI 当前连接、上一次查询或调用方偶然持有的 dbSessionId 都不是缺目标时的后备值。已有完整目标被拒绝后不改连另一个目标。

长请求使用 runtime 的 accepted receipt；网络或模型调用超时只表示调用方未收到结果，必须查询 receipt。相同请求令牌与相同摘要返回原执行；未知副作用不能通过改 key、重新连库或“让模型再试一次”重放。

请求上下文同时记录主调用者、委托者与消费入口。审计区分 AI invocation、MCP tool、wapp instance、schedule 与 execution/job，但不记录凭据、live handle、attachment token、完整模型上下文或未经脱敏的参数。

## 3. AI 工具与编辑器共享授权

### 3.1 默认独立工具执行

AI Provider 调用与数据库 execution 是两个生命周期。模型请求在 AiProvider 接口中运行；数据库工具由 invocation registry 创建子 execution 或 Job，并记录 parent invocationId。模型文本不直接进入 SQL 执行，必须经过工具 schema、目标授权和原有 SQL 风控。

metadata 工具只读；写工具仍需执行权限与破坏性确认。模型不能设置“已经确认”、绕过 readOnly 或扩大 delegation。批量 schema 读取遵守 metadata 预算、结果上限与 Artifact 分块，不能一次性把所有数据库内容放进模型上下文。

### 3.2 共享编辑器上下文

用户显式允许某次 AI 会话使用编辑器状态时，宿主建立内存 grant：grantId、principal、clientInstanceId、AI invocation 范围、目标 session 身份、contextVersion、允许工具/action、到期时间与撤销状态。grantId 不是 attachment token，模型和 Provider 不得到 live session handle。

> 2026-10-17 实现现状：桌面 grant 已落地 `{grantId, clientInstanceId, editorSessionId, connectionId, database, configRevision, editorSessionId 对应的活 SessionHandle, deadline, revoked}`（`modules/src/ai/application/ownership.rs`）；`principal`、AI invocation 范围、允许工具/action 与 contextVersion 的独立记录仍是目标设计，当前以 configRevision + 归属校验覆盖。

共享工具在同一 session actor 队列排队，能观察同一事务和临时对象；不建立第二套事务管理器。开启/提交/回滚用户事务必须有单独明确授权，普通“查询当前状态”授权不包含这些动作。

以下任一条件使 grant 失效：用户撤销、编辑器关闭、session 被替代或丢失、上下文版本变化、登录/权限撤销、TTL 到期。派发前检查 grant；已在途调用按精确 execution 取消能力处理，记录真实效果。失效后拒绝绑定操作，不自动改为独立连接执行；用户重新选择目标或重新授权后才创建新操作。

### 3.3 取消与模型断开

取消 AI invocation 停止后续工具调度，并向它拥有的活动 execution 发送精确取消。不会关闭用户编辑器 session 或回滚用户事务。已接受、可脱离 invocation 的 Job 必须在受理时明确告知用户；取消 AI 面板只取消订阅，取消 Job 要通过 cancelJob。

Provider 请求失败不能覆盖数据库子 execution 的成功或 unknown。界面按每个工具展示 receipt 与 effectOutcome；失败重试只对可证明无副作用的 Provider 请求或独立只读工具成立，不能重复完整写工具链。

## 4. MCP Server 与 Client

### 4.1 Stateless tools

Server tool 接受 connectionId 与命令所需的完整 database/schema，不接受 GUI 活动目标。后端生成 MCP clientInstanceId，tool requestId 绑定 accepted receipt。stdio 模式由本地启动策略创建 RequestContext，不能因无 HTTP 就跳过授权或 SQL 风控。

数据库结果以有界 inline 摘要或 Artifact 引用返回；资源读取每次重新授权。opaque Artifact ID 是查找键，不是访问凭据。MCP 通知丢失后按 execution/job 查询恢复，不靠重发 tool 创建新写操作。

### 4.2 Stateful tools

P6 在需要事务/临时对象的工具中提供显式 open/use/close 会话协议。对客户端只返回 opaque clientSessionRef；映射在 MCP 适配器内存中，绑定 principal、clientInstanceId、live handle、contextVersion 与 TTL。它不能落在 Workflow 定义、history 或数据库配置里。

use 必须校验客户端归属与映射仍存活；映射丢失返回 SessionLost，不能同 ref 重建。close 幂等且只关闭当前 client 所有资源。客户端断开停止新增请求、取消其短操作并关闭其专属 session；共享 grant 与已接受持久 Job 按各自政策处理，不扫描和关闭同 connectionId 的其他消费者资源。

桌面 MCP 的 TTL 在到期 lookup 与独立定时 worker 中检查：到期或 disconnect 先撤销 client binding，再把原 client owner、Runtime handle 留在适配器内存 cleanup record 中，引用不能重新使用。每个 client 至多一个 cleanup worker；它只对该 client 的句柄调用 `RollbackAndClose`，回滚尚未结束的事务后关闭原资源。只有 `resourceRelease=Confirmed` 才删除 cleanup record；错误、Pending 或 Quarantined 保留记录，并以 100ms 起、最多 30s 的退避重试，不能把未知回滚效果报告为成功，也不重放应用 SQL。Runtime 已隔离但不能确认物理释放时，记录保持隔离待清理；该记录不跨进程持久化。worker 在活 binding 与待清理记录均为空后退出。显式 close 收到未确认释放也转入该清理路径，而非丢弃 owner。

MCP Client 连接外部服务的网络生命周期与数据库 session 分开管理。外部 tool 返回的连接 ID、owner 或 SQL 一律作为不可信输入，不能取得宿主内部资源权限。

## 5. Wapp 受控桥

Wapp 继续通过宿主桥调用 Driver Command，沙箱不持有 BackendClient、团队凭据、dbSessionId、attachment 或 SecretProvider。manifest command 权限、已安装 publisher/name、当前 frame 实例、用户权限和 driver command 权限取交集。

桥请求信封由 host 记录 instanceNonce、requestId、method 与稳定目标，response 回显 requestId。重复 requestId 的输入摘要不同即拒绝；同摘要映射原 receipt。iframe 重新加载建立新 instanceNonce，旧响应和旧取消不能作用于新实例。

Web 的 origin/source/nonce 校验和隔离部署以 [团队服务 §15](team-server-and-auth.md) 为准。Desktop opaque origin 只在匹配 frame source 与 nonce 后允许桥消息；不得向 `*` 发送敏感信息。

默认 command 为短操作。确需连续事务的 Wapp 必须由 manifest 声明相应 capability、由用户显式批准，再由 host 创建该实例独占的 session；对沙箱仍只暴露实例绑定的逻辑引用。未声明能力、引用过期或实例结束都拒绝，不能降级为共享编辑器 session。

实例结束取消它拥有的短 execution 和桥订阅、关闭专属 session。宿主已受理的 Job 可继续，但必须保留 jobId 查询入口；其他 Wapp、编辑器和 MCP 的资源不受影响。

### 桌面 Wapp 实例会话（2026-10-19 当前实现）

`wapp_session` 宿主适配器提供 register/end/open/use/close/receipt；SDK 以 `client.session` 调用。manifest 必须同时声明 `session:stateful` 与 `command:invoke`，每次 open 还需宿主原生确认；拒绝后不申请资源。当前仅接受 PostgreSQL/MySQL，database 必须显式提供，PostgreSQL 还需显式 schema。实例/client ID 由后端生成，只留在宿主桥；沙箱只得到 opaque reference、有效期和有界 execution 结果，不得到 Runtime handle、attachment、身份或凭据。use 仅开放 query/execute，沿现有 Runtime policy、readOnly、Safe Mode 和保留资源提供者执行；不开放 driver 管理命令或任意 platform 命令。

每次 iframe load 轮换宿主 nonce 与后端实例映射；session 请求同时检查 frame source 与当前 nonce。同 requestId/type/payload 共用原 promise，改变输入被拒绝；每个文档最多保留 1024 次请求。卸载与重新加载抑制旧响应，结束旧 backend 实例；disabled/uninstall 撤销该 Wapp 的实例。实例硬期限一小时，每实例最多八个活跃或待清理 session；session 默认五分钟，最多一小时，且受实例期限限制。实例结束或 TTL 到期后不能用原引用重建，也不回退到编辑器 session。

独立 timer 在到期时撤销引用，取消该实例已记录的在途 execution，并对原 owner/handle 调用 RollbackAndClose。只有 resourceRelease=Confirmed 才删除待清理记录，失败/Pending/Quarantined 保留并以 100ms～30s 退避重试；显式 close 返回 pending，不能当作物理释放确认。worker 等待在途 open 的 admission guard 退出，避免结束与资源获取竞态丢失晚到的 handle。结果等待最多 25 秒，未终结时保留 executionId，可在当前实例内用 receipt 查询；行数上限 1000，权限仍由 Runtime 重验。引用、映射与结果索引不跨进程恢复，未知效果不自动重放。Wapp Job/Workflow 入口及通用短命令实例化未由本次实现覆盖；不表示 P6 或真实数据库链路已经验收。

## 6. Dashboard、Monitor 与调度身份

widget、monitor 和 schedule 定义保存完整稳定目标及其版本，不从 GUI 默认连接取值。手工刷新使用当前用户的短执行；定时执行使用组织内显式登记的服务 principal，不能借用最近登录用户的 cookie、token 或临时 grant。

调度配置保存 delegationId、owner、批准者、允许 connection/action、到期与撤销版本、service principal 和受控 secretRef；不保存令牌。P7 可将服务身份登记在 memberships、授权范围落在 ACL，调度定义由现有业务仓储的目标 adapter 保存。P6 桌面 adapter 保存同构的受控定义；这不授权建立 session/delegation token 持久表。

每次调度加载最新配置和授权，入队与派发均检查 enabled、到期、连接版本、readOnly。撤销或禁用停止未来运行；运行中的 DB 操作只按真实取消结果报告。修改目标产生新版本，不把既有运行中 Job 切到新库。

一次 schedule occurrence 用 scheduleId + definitionRevision + scheduledAt 作为接受去重键，由应用服务签发幂等令牌。多 worker 接管查原 Job，不创建第二个 occurrence；错过触发点按定义明确的 skip 或单次 catch-up 策略处理，默认 skip，禁止无限补跑写任务。

同 widget 的周期刷新默认合并尚未完成的同版本只读请求，不累积队列。订阅关闭不阻断已确认继续的 Job。查询结果权限、缓存 policyIsolationKey 与 Artifact ACL 随权限版本变化失效。


#### 桌面 Monitor 定时授权（已实现，2026-10-17）

Monitor 周期轮询与手动刷新现已使用不同入口。手动刷新沿用当前桌面用户上下文；周期 tick 必须取得后端保存的 widget grant，不能因 `connection_id=None`、工作流可运行或桌面用户仍在线而获得授权。用户在报表编辑器中保存 interval 配置后，显式授权一小时或撤销；授权 IPC 仅接受 dashboard/widget ID 与 60～86400 秒有效期，不接受 principal、delegation token 或用户提供的 grant。

后端从当前桌面 RequestContext 取得批准者，并生成独立 service principal / delegation ID。AppDb schema v3 的 `monitor_schedule_grants` 仅保存批准者、服务身份、dashboard/widget/workflow ID、工作流 YAML SHA-256 revision、固定 canonical connection/database/schema 与配置版本、connect/query 动作及命令范围、readOnly、到期时间、撤销状态与版本。不保存 SQL 副本、session、attachment、bearer token、秘密或凭据。新授权取代同 widget 的旧授权；变更 workflow 定义或 profile 配置必须重新授权。

当前授权范围限 PostgreSQL/MySQL 的静态独立 `query` 步骤，要求保存的 workflow 明确声明 connection/database；不接受变量、循环、条件、AI 或资源块。Monitor 使用独立的只读查询 AST 校验（单查询，拒绝 writable CTE、SELECT INTO、行锁及非查询；函数仅允许明确的内置聚合/数值/文本/空值/时钟函数白名单，未知、带 schema、引号名称及表值函数失败关闭，因此序列推进/状态、命名锁获取/状态/释放和会话配置函数不能借 SELECT 派发），不使用会禁止所有 SQL 工具的 MCP ReadOnly 模式。SQL 中的 relation 只接受单段标识符；任意 schema/database/catalog 限定名称均失败关闭，包括引号、反引号和多段名称，且递归覆盖 join、子查询与 CTE。即使限定名称看似与批准目标一致也不放宽，因为当前校验不执行方言相关的大小写与引号归一比较；单段引号表名、别名和列限定仍可用。其他驱动和写动作均失败关闭。

共享 MonitorEngine 在入队前及 semaphore / workflow lock 之后重载 dashboard/widget enabled、pause 与 grant，冻结并比较 workflow revision 和目标范围。桌面 scheduled adapter 使用同一冻结定义与 service RequestContext；Runtime 从 PolicyService 后端解析完整 grant、核对主体/客户端/有效期/动作，在 admission 与物理 dispatch 前重新核对 grant、schedule、workflow hash、目标、配置版本、enabled 和只读 SQL。delegation 不能降级为本地用户；撤销后仍允许 owner 关闭其自有资源，不能因撤权遗留短会话。原同 widget in-flight 合并保持。

撤销/禁用阻止后续派发，不伪造已在途执行的取消或回滚。已观察结果保持真实；撤权后无法继续读取 receipt 时保留 SqlUnconfirmed/Unknown。每次周期执行先接受一个 SQLite durable `monitorOccurrence` Job；其严格投影只保存 schedule、定义版本、计划时间、授权身份与只读目标范围，不保存凭据。`scheduleId + definitionRevision + scheduledAt` 派生幂等 receipt，重启或重复 tick 命中已有 receipt 时不再派发查询。启动恢复会将未完成的 Monitor Job 标记为待验证并要求人工复核，不恢复 handler 或重放查询。此闭环已覆盖本地持久化与重复提交；不表示 P6 整体退出或桌面完整 scheduler / occurrence / 真实数据库链路已经验收。

PostgreSQL 与 MySQL 的 driver 专属 `packages/drivers/{postgres,mysql}/tests/monitor_scheduled_query.rs` 现提供同层级 live 证据：安全 migration fixture gate 下，生产 Monitor 只读 SQL 校验与精确 grant scope 匹配，经 `workflow::resource_execution::execute`、Runtime 和真实 ResourceProvider 执行聚合查询，读取实际 Artifact 结果并由独立连接核对；危险 SQL、限定 relation、越界 database 与撤销 grant 不新增物理 SQL 执行，短资源确认关闭。测试 host 的 Profile/Policy/SessionBackend 是端口接缝；不覆盖 Tauri AppState、后端 grant 签发/持久化、SQLite occurrence admission、duplicate-tick receipt 或完整 scheduler，不能称为完整 Monitor scheduled E2E。

这些 live 查询暴露并修复了委托结果发布的 P6 阻塞：普通 FileArtifactRepository 拒绝 delegated create/read，过去即使 SQL 成功也会在发布结果时失败。Runtime 现使用专用内部 `ExecutionArtifactScope`，只从当前 admitted execution entry 取得完整组织、主体、客户端、认证会话、delegation 与 execution 绑定；读时先经当前 PolicyService 重验，再检查同一 live entry / artifact / execution。普通 Artifact API 仍拒绝委托任意创建、读取及导出，同 owner 的普通上下文也不能读取受限结果。scope 不序列化，metadata 只保留受限结果标记；TTL 到期清理，撤权后不可读，重开 ArtifactRepository 清理这种进程内结果，不恢复 delegated access 或重放查询。Runtime 回归另覆盖身份不符、其他有效 delegation、任意 Artifact ID、撤权、TTL 与重启拒绝。

## 7. 导出、备份与恢复的环境端口

导出、备份、恢复均以 Job 受理，冻结稳定目标、输出规格、工具/driver 版本与风险确认。文件选择或下载只属于 host adapter；领域层写 Artifact，不接受任意服务端路径。

### NativeTool 环境端口（已删除，2026-10-28）

> **历史记录**：本节原描述 NativeTool 环境端口（部署锚定的 `pg_dump` / `mysqldump --version` 探针）的目标协议、kernel 实现、桌面部署登记与产品 IPC。随前端形态拆分方案 **F1**（裁决 #3）整体删除：产品零使用、无 UI、探针登记表默认空即拒绝一切，契约先行预留却从未被消费。删除面：`modules/src/jobs/{domain,application}/native_tool*`、`src-tauri/src/{commands,platform}/native_tool*`、`src-tauri/resources/native-tool-probes.json`、`owner.rs` 的 `verify_native_selection`、codec 的 `nativeToolProbe` 校验分支与 IPC 五命令。旧库遗留的 `nativeToolProbe` 作业行由宿主 schema migration v6 在启动时一次性 purge（子表全部 ON DELETE CASCADE）。若未来需要数据库原生 CLI 能力，须重新立项；本节保留删除事实与回滚边界（revert 无数据迁移逆向，purge 不恢复旧记录——记录本身无产品价值）。

## 8. 接口落地与旧路径删除

P6 目标 adapter 为 AI tool adapter、MCP request/session adapter、Wapp bridge adapter 与 schedule adapter（native tool adapter 随 NativeTool 删除而取消，见 §7 墓碑）；名字是职责描述，不假称当前存在这些模块。新增字段与方法先在 contracts/application 注册，再同时更新桌面和团队 adapter。

每个消费者切换时按顺序完成：完整目标与 schema → owner/委托与风控 → receipt/结果/取消 → 资源清理 → 连续旅程验证 → 删除旧管理器调用。Workflow 继续使用其详细设计定义的 step/block，不在消费者 adapter 重复实现块事务。

现有平台设计的 Dashboard 目标来源和调度凭据缺口由 §6 固定；Wapp stateful 能力由 §5/§7 固定（NativeTool 已删除，见 §7 墓碑）。具体驱动不具备相应能力仍必须拒绝，不能由 Host 伪造支持。

### 8.1 当前共享实现与保留边界（2026-10-17）

Workflow 在 P6 保持既有 v1 协议与执行语义，只替换底层连接管理；保留跨数据库逐步目标、step output 和模板串联。生产路径为 `modules command_runtime → modules resource_execution → Runtime`。不接入 v2 session/transaction block、专用 Workflow Job owner/dispatch、Workflow Job IPC 或 PostgreSQL/MySQL transaction proof；模型/YAML/既有 IPC 形状不变。（NativeTool 原依赖的不可序列化通用 `StageOwnerPermit` Job fencing 保留在 Job 内核中服务其余 Handler；NativeTool 本体已删除。）

共享领域实现集中在 `modules`，Wapp 内核（协议/路由策略）作为 `modules::workspace_apps` 模块同在其中，宿主只保留 `datazen://` 协议适配器。本轮补迁 command policy、迁移端点身份算法、Dashboard 创建/归档规则、export 容器规划与迁移结果历史映射；原路径保留薄绑定和兼容导出。逐项源码边界见 [modules 残余审计](modules.md#final-residual-audit-2026-10-17)。

`execute_driver_command` 仍服务桌面 editor/session IPC；Runtime 的 `SessionRegistry` 与 `DesktopSessionBackend` 承载本地 session identity、物理资源、预算与隧道生命周期。AI/MCP/Workflow 默认 DB 工具已走共享 Runtime `ResourceExecutionHost`。Monitor 的定时调度与手动 widget 执行保持共享 MonitorHost/Workflow/Runtime 路径。

共享代码可复用不等于 §9 旅程完成：表导出与数据库备份已接入桌面 Job/Artifact 交付，表导出的 profile 查询沿用 RuntimeCommandSource，但数据不再通过完整 MultiQueryResult 读取：Host 建立专用 `tableExport` Runtime session，并在 query admission 前绑定 owner/authentication/delegation、canonical target 与 config revision。PostgreSQL/MySQL 的 awaitable ResourceProvider 批次经独立容量 1 的进程内输出槽到原 formatter，发送等待不持有 execution 全局锁；单批最多 256 行、编码后最多 2 MiB，formatter 逐批在 blocking pool 执行。仅此 Host capability 可豁免交互查询行数上限，客户端参数不能取得该权限。取消 watcher 撤销 binding、唤醒 producer 并请求取消同一 execution；错误/丢弃等待者走原 Runtime owner 关闭路径，不重放查询。已在执行的同步文件写不可强制中止，但不再交付后续批次或成功发布；只有执行成功且关闭回执确认释放资源才发送 Done，未知清理失败关闭。此实现未改变 ExportSource、format/plan 或 IPC；新增回归源码及 PG/MySQL live 导出/取消尚待独立验证。PostgreSQL/MySQL 备份通过专属 Runtime retained session 的 `backup_dump` 流式写 staging/gzip（64 KiB chunk、同步背压），其他驱动失败关闭；恢复已接入单次消费的 apply Job 和效果证据端口；PostgreSQL/MySQL 的受限 SQL 格式已接固定 Runtime Session / ExecutionGateway；其他驱动至 P10 保持 Unsupported/NotStarted，实库旅程仍须独立验收。Workflow 本轮保持 v1，仅迁移底层连接管理；块与专用 Workflow Job 不在交付范围。显式 editor grant 已接桌面 H 层：grant/revoke/touch IPC、借用不拥有的 in-session 执行与取消回执已实现；新增的真实 UI 生命周期旅程尚未执行，MCP/Wapp 归属仍待核对。不得将本轮桌面单测通过当作 P6 阶段退出证明。

## 9. 验收与故障旅程

| 旅程 | 必须观察到的结果 | 关联用例 |
| --- | --- | --- |
| AI 默认查询 → 授权共享 → 临时对象查询 → 撤销 → 下一次工具 | 共享时同队列可见；撤销后拒绝且编辑器资源仍存活 | CM-53、73、74（**H 层已交付**：`modules/src/ai/application/ownership.rs` 的 grant 生命周期 + `src-tauri/src/commands/ai/grant.rs` 的 `ai_grant_editor_session`/`ai_revoke_editor_grant`/`ai_touch_editor_grant`；F 层真实 UI 旅程待补） |
| MCP A/B 同连接 → A 开事务 → A 断开 → B 继续 | 资源隔离、A 清理完成、B 不被取消 | CM-53、54 |
| Wapp 请求 → 重载 → 旧响应/取消到达 | 旧 nonce 被拒绝，新实例状态不受影响 | CM-53、55 |
| schedule 入队 → ACL 撤销 → 派发 | 不取得资源、不写数据库，记录拒绝原因 | CM-59、65、67 |
| 写工具响应丢失 → 查询 receipt → 再次提交同 key | 返回同 execution，不重复写入 | CM-54、70 |

Host fake 验证归属与清理；真实 driver 验证共享状态、精确取消和外部效果；Frontend 验证旧事件过滤；W1 验证鉴权与桥，WN 验证调度接管。未运行的层不能记为通过。

恢复 adapter 的当前例外：`restore_sql_file` 已接 Artifact 上传与 `databaseRestoreApply` 受理/单次消费/JobRuntime，而非直接本地路径执行。PostgreSQL/MySQL 的 Gateway 通过固定 Runtime Session / ExecutionGateway 执行受限 SQL 格式并记录真实提交证据；其他驱动明确 Unsupported 至 P10，没有借旧 ConnectionManager 实现恢复。详见 [Job §2.5](data-migration-jobs.md#25-restore-apply-job已接线驱动执行失败关闭)。这不关闭真实驱动恢复验收或 Workflow v1 跨驱动连接路由的其余验收。


### P6 Workflow v1 范围

Workflow 在 P6 保持既有 v1 协议与执行语义，只替换底层连接管理；保留跨数据库逐步目标、step output 和模板串联。生产路径为 `modules command_runtime → modules resource_execution → Runtime`。不接入 v2 session/transaction block、专用 Workflow Job owner/dispatch、Workflow Job IPC 或 PostgreSQL/MySQL transaction proof；模型/YAML/既有 IPC 形状不变。
