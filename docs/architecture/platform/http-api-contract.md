# HTTP API 契约面（web ready WR-C2）

> 状态：**已裁决**（2026-10-31，web-ready 计划 WR-C2 交付物）。
> 本文是团队 Web 服务 HTTP 面的**唯一权威契约**：哪个方法上 HTTP、以什么路径与状态码出现、哪些能力**永不上** HTTP。
> 服务端实现（WR-D）逐条照此落地；`packages/backend-client` 的域客户端是契约的**唯一来源**。

---

## 1. 本文解决的问题

系统概要 §6.3 给出了首版 HTTP 映射的**骨架**（连接/会话/执行/Job/产物/事件六域），团队服务设计（team-server-and-auth）定死了中间件链、认证、CSRF、SSE 与错误映射。但骨架没有覆盖 `packages/backend-client` 已有的全部域：settings、identity 的平台身份读取、tableData 的全部 14 个方法、driverGateway 的过渡面。没有一个补全并**冻结**的契约面，WR-D 的 server host 就会边写边发明路由，HTTP 面与域客户端面各自漂移。

本文补全并冻结 v1 契约面。裁决与例外都在正文里给理由；路由一经落地，变更走版本化（`/api/v2`），不走原地改语义。

## 2. 裁决：镜像域客户端面，不镜像 IPC 命令面

**HTTP 面与 `packages/backend-client` 的域客户端面逐方法一一对应。**

- 契约来源是 `MethodMap`（`packages/backend-client/src/transport.ts` 聚合的十个域切片），不是 `src/commands/*` 的 IPC 命令簇（legacy rail，约 200 个 snake_case 命令）。IPC 簇里混着平台服务（dialog/window/clipboard/updater），它们不属于后端。
- 每个域客户端方法必须有且只有一个 HTTP 端点；**多余端点为零**（验收口径，§9）。服务自举路由（§5）与过渡例外（§6.5 的 uploads、§6.9 的 driverGateway）单独列出，不混入域面。
- 同一用例、两种薄绑定：桌面 Tauri command 与 server HTTP handler 都只做「取 repository → 组 request → 调 modules application 用例」。HTTP 面镜像用例，因此 command 薄壳化（WR-A2）完成到哪个域，HTTP 面就能开到哪个域——两者共享同一条编排收敛线。
- 新能力的增长规则：**先在 backend-client 落域客户端方法，再在此表登记镜像端点**。反向（先开端点后补方法）只允许出现在已立项的服务自举路由上（§6.5 的 uploads 是唯一在账者）。

## 3. 通用约定（引用权威文档，不重述）

| 主题 | 权威 |
| --- | --- |
| 中间件链、认证（OIDC/native）、CSRF、统一 404 | [team-server-and-auth](team-server-and-auth.md) §3–§7 |
| SSE 握手、`Last-Event-ID` 回放、心跳 | 同上 §8 |
| 请求限制、`ApiError` → HTTP 映射、成功侧状态码 | 同上 §9 |
| 连接资源 DTO（`ProfileView`/`SessionView`/`ExecutionView`…） | [connection-management](connection-management.md) §4.1 |
| 其余 wire 类型 | `packages/backend-client/src/types/`（Rust 侧镜像的编译期奇偶校验在那里） |

本文补充三条 v1 面内规则：

1. **请求体一律 JSON**（`Content-Type: application/json`）；产物字节读取（§6.5）的响应体是原始字节。查询参数只用于简单标量（id、`chunkIndex`、`afterSequence`、named-view 的 scope 四元组），结构化条件（过滤、排序、翻页游标、上下文）一律进请求体。
2. **POST 读是有意的**：tableData 域的读与预览（§6.7）请求体是完整查询文档（`TableDataRequest` 一类，含上下文、过滤、排序、键集游标），塞进查询字符串必然截断或失真。POST 读不携带幂等令牌（非变更），但照常过 CSRF 头校验。
3. **异步接受语义**：凡返回结果需另行查询的（执行、取消、Job 启动），一律 `202` + 稳定 ID，客户端不得把 202 读作成功执行（team-server §9.3 的既有裁决，此处只确认适用全表）。

## 4. 契约面总账

十三个域、73 个域客户端方法（§6.14 workflow 十二方法随 W8 刀3c 入账）：

| 域 | 方法数 | HTTP 服务 | 排除 | 备注 |
| --- | --- | --- | --- | --- |
| settings | 2 | 2 | 0 | |
| identity | 2 | 2 | 0 | |
| connection | 4 | 4 | 0 | PATCH 一条路由双服务 update/disable（§6.3） |
| session | 6 | 6 | 0 | |
| execution | 4 | 4 | 0 | |
| job | 4 | 4 | 0 | |
| artifact | 1 | 1 | 0 | uploads 端点另列（§6.5） |
| events | 2 | 2 | 0 | SSE（v1.1：订阅建立 + 流） |
| tableData | 14 | 14 | 0 | |
| driverGateway（过渡） | 5 | 4 | 1 | `open_base64_with_dialog` 排除（§6.9） |
| queryAssets | 7 | 7 | 0 | §6.12（W8 刀3a；web 半边随本版收口：域客户端 + 名单收缩） |
| schemaDiff | 11 | 11 | 0 | §6.13（W8 刀3b；web 半边随本版收口；`get_migration_run`/`list_migration_runs` 读面留 sync 域） |
| workflow | 12 | 11 | 1 | §6.14（W8 刀3c；`getWorkflowDir` 桌面文件语义排除） |
| **合计** | **73** | **71** | **2** | |

路由总数 70 条服务 71 个方法（PATCH 双服务），另有服务自举路由 5 条（§5）与已立项待补方法的 1 条（§6.5）。

## 5. 服务自举路由（非域面）

登录会话与服务健康是 server 自身的机器面，不属于任何域客户端：

| 路由 | 用途 | 权威 |
| --- | --- | --- |
| `GET /api/v1/auth/login`、`GET /api/v1/auth/callback`、`POST /api/v1/auth/logout` | OIDC 登录三步 | team-server §4 |
| `POST /api/v1/auth/desktop/start`、`POST /api/v1/auth/desktop/exchange` | 桌面团队认证（P8） | 同上 §4.6 |
| `GET /healthz`、`GET /readyz` | 存活/就绪探针 | 同上 §2.4 |

## 6. 逐域路由表

约定：`{id}` 为路径参数；「令牌」列 = 请求需携带 `issueSubmissionToken` 签发的幂等令牌（`SubmitRequest` 的 `submissionToken` 字段）；「成功」列为响应状态码。请求/响应类型名以 `packages/backend-client` 为准，本文不复制形状。

### 6.1 settings

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `getSettings` | `GET /api/v1/settings` | — | 200 | — → `SharedSettings` |
| `saveSettings` | `PUT /api/v1/settings` | — | 200 | `{ settings }` → `SharedSettings` |

Web 形态只服务 `WEB_SETTINGS_SECTIONS`（general / appearance / dataBrowsing / editor / behavior）；桌面专属分节（updater 等）不出现在 HTTP 面，差异由 `WEB_CAPABILITIES` 静态表声明（§7）。设置写入不要求幂等令牌：它是整体覆盖写（last-write-wins），没有「重复提交产生两个副作用」的风险面。

### 6.2 identity

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `getPlatformIdentity` | `GET /api/v1/identity` | — | 200 | — → `{ clientInstanceId, organizationId, principalId }` |
| `issueSubmissionToken` | `POST /api/v1/submission-tokens` | — | 201 | `{ operation, handle?, connectionId?, owner? }` → `SubmissionToken` |

令牌有效期、绑定与回执保留期限的权威值在 [connection-management §13.1](connection-management.md#131-幂等键期限与响应分类) 与 team-server §3.1 的路由表注记，本文不另立数值。桌面形态的 `getPlatformIdentity` 返回本地单机身份；web 形态返回登录主体——**同一方法、两形态语义已在域客户端设计时裁决**，HTTP 侧不再分叉。

### 6.3 connection

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `listConnections` | `GET /api/v1/connections` | — | 200 | — → `ProfileView[]`（不分页） |
| `createConnection` | `POST /api/v1/connections` | ✓ | 201 | `ProfileDraft & SubmitRequest` → `ProfileView` |
| `updateConnection` | `PATCH /api/v1/connections/{id}` | ✓ | 200 | `{ expectedRevision, patch }` → `ProfileView` |
| `disableConnection` | `PATCH /api/v1/connections/{id}` | ✓ | 200 | `{ expectedRevision, enabled: false }` → `ProfileView` |

`PATCH` 一条路由双服务 update/disable 是概要 §6.3 的既有裁决，本文保留。判别无歧义：`ProfilePatch = Partial<Omit<ProfileDraft, 'driverId'>>` 而 `ProfileDraft` **没有** `enabled` 字段，`patch` 文档永远碰不到启用态；顶层 `enabled: false` 只属于 disable 语义（含 drain/force 政策）。凭据 `credentials` write-only，只交 `SecretProvider`，任何响应不回显（connection-management §4.1）。

### 6.4 session

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `openSession` | `POST /api/v1/sessions` | ✓ | 201 | `OpenSessionRequest & SubmitRequest` → `OpenSessionReceipt` |
| `getSession` | `GET /api/v1/sessions/{id}` | — | 200 | — → `SessionView` |
| `setSessionContext` | `PUT /api/v1/sessions/{id}/context` | ✓ | 200 | `SetSessionContextRequest & SubmitRequest` → `ContextChangeReceipt` |
| `attachSession` | `POST /api/v1/sessions/{id}/attach` | — | 200 | `AttachmentRequest` → `SessionView` |
| `detachSession` | `POST /api/v1/sessions/{id}/detach` | — | 200 | `AttachmentRequest` → `SessionView` |
| `closeSession` | `POST /api/v1/sessions/{id}/close` | — | 200 | `CloseSessionRequest` → `CloseReceipt`（幂等关闭） |

### 6.5 artifact

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `readArtifact` | `GET /api/v1/artifacts/{id}` | — | 200 | `?metadata=1` / `?chunkIndex=` / `?offset=&limit=`（互斥）→ `ArtifactChunk` 或元数据 |

- 三种取法映射查询参数（team-server §11 的既有裁决）；响应体是原始字节（`ArtifactChunk.bytes`），元数据模式返回 JSON。
- 产物过期在端口层是 `ArtifactExpired`，HTTP 层有意统一为 404（team-server §11）。
- **在账端点 `POST /api/v1/artifacts/uploads`（分块受控上传）**：概要 §6.3 与 team-server §11 已立项，消费方是 web 形态的导出/保存流（浏览器不能像桌面那样落盘，导出内容上传产物库后下载）与产物写入器协议。**当前 `ArtifactMethodMap` 尚无对应方法**——这是全表唯一「先有端点后有方法」的在账项，收口动作：web 导出流立项时在 `artifactClient` 补上传方法（如 `createUploadWriter` / `appendUploadChunk` / `finalizeUpload`），或在 WR-D 实现时把写入器协议定形为方法；两头合拢前该端点不进 CI 契约断言。

### 6.6 events

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `subscribeEvents` | `GET /api/v1/events/{streamId}` | — | 200 `text/event-stream` | `?afterSequence=` 续传 → `EventEnvelope` 流 |
| **v1.1：`createEventSubscription`** | `POST /api/v1/events/subscriptions` | CSRF（普通 fetch） | 200 `{ "streamId" }` | 认证会话 → streamId 登记 |

`SubscribeEventsRequest` 的 `streamId` 进路径、`afterSequence` 进查询参数（与 `Last-Event-ID` 头二选一，优先头）。握手鉴权不用 CSRF 头（EventSource 无法设置），用 cookie + Origin/Fetch-Metadata 证明（team-server §8.1 的既有裁决）。**v1.1（2026-11-02，WR-D5 落地登记）**：`streamId` 的分配走 `POST /api/v1/events/subscriptions`（认证链内，CSPRNG 铸造 + 归属盖章，注册表 TTL 内一次性消费——重连先 POST 再 GET，旧 id 不重放）；回放缺口与慢消费者在流内以 `streamResetRequired` 事件（无 `id` 行）落地，登录/权限复核失败以 `auth_required` / `permission_revoked` 终态事件收流（team-server §8.2–§8.4）；不另开传输。

### 6.7 tableData：读与预览（查询文档体，POST）

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `getTableData` | `POST /api/v1/table-data/reads` | — | 200 | `TableDataRequest` → `TableDataResult` |
| `previewTableReadSql` | `POST /api/v1/table-data/read-sql-previews` | — | 200 | `TableReadPreviewRequest` → `TableReadPreviewSql`（不执行任何语句） |
| `estimateTableRows` | `POST /api/v1/table-data/row-count-estimates` | — | 200 | `EstimateTableRowsRequest` → `number \| null`（null = 驱动不能估算，是答案不是错误） |
| `resolveForeignKeyTargets` | `POST /api/v1/table-data/foreign-key-targets` | — | 200 | 请求 → 外键目标解析（`exists:false` 是成功答案） |
| `listInboundForeignKeys` | `POST /api/v1/table-data/inbound-foreign-keys` | — | 200 | 请求 → 入向外键列表 |
| `searchTableValues` | `POST /api/v1/table-data/value-searches` | — | 200 | 请求 → 值搜索结果 |
| `resolveResultOrigin` | `POST /api/v1/table-data/result-origins` | — | 200 | 请求 → 结果来源解析 |

### 6.8 tableData：变更（计划-确认两段式）

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `previewPendingChanges` | `POST /api/v1/table-data/change-plan-previews` | — | 200 | `PreviewPendingChangesRequest` → `RowChangePlan`（不执行） |
| `commitPendingChanges` | `POST /api/v1/table-data/change-commits` | ✓ | 200 | `CommitPendingChangesRequest`（计划指纹护栏）→ `CommitPendingChangesResponse` |
| `commitQueryResultCell` | `POST /api/v1/table-data/result-cell-commits` | ✓ | 200 | `ResultCellWrite` → `undefined`（204 语义，v1 用 200 + 空体，避免客户端分支） |
| `previewResultCellChange` | `POST /api/v1/table-data/result-cell-change-previews` | — | 200 | 请求 → 单元格变更预览 |

变更提交是同步回执（apply 已指纹确认的计划），不是「接受后另查」，故 200 而非 202——与执行的 202 语义区分：**凡是结果在响应体里的用 200，凡是结果需另行查询的用 202**。

### 6.9 tableData：命名视图 + driverGateway（过渡域）

命名视图（用户内容，scope 绑定**持久** connectionId——`dbSessionId` 是运行时状态，永不入 scope）：

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `listNamedViews` | `GET /api/v1/named-views` | — | 200 | `?connectionId=&database=&schema=&table=`（全部可选，缺省列全部）→ `NamedView[]` |
| `saveNamedView` | `PUT /api/v1/named-views` | — | 200 更新 / 201 铸造 | `{ view }`（空 `id` 由后端铸造）→ `NamedView` |
| `deleteNamedView` | `DELETE /api/v1/named-views/{id}` | — | 200 | — → 剩余 `NamedView[]`（未知 id 无操作） |

> 命名视图上 HTTP 是**存储形态裁决**，不是能力裁剪：`types/tableData.ts` 注记的「desktop-only for now」描述的是桌面把存储放本机持久库的现状；web 形态同一能力改存服务端库（per principal + scope），端口形状零改动。桌面继续本地存储，两形态各自实现同一 `MethodMap` 切片。
>
> `DELETE` 是 v1 面**唯一**删除路由，是对概要 §6.3「无 DELETE」口径的显式扩展，理由：命名视图是用户自建内容，没有「停用」中间态（连接配置有 `enabled=false`，视图没有）；「删未知 id 无操作」正是 DELETE 的幂等原生语义；响应返回剩余集合是域客户端契约（`deleteNamedView` 返回 `readonly NamedView[]`），HTTP 不改写它。此例外只此一处，不构成先例。

driverGateway（**过渡域**，snake_case 名保留——它们是现存 IPC 命令名，不是内核名；整个域在内核实现驱动命令面后删除）。请求体是**不透明透传**的嵌套驱动命令（`request: unknown`），不携带 `SubmitRequest`，v1 不引入幂等令牌——收编进内核域时按各域规则补：

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `get_driver_commands` | `GET /api/v1/driver-gateway/commands?driverType=` | — | 200 | — → 命令描述列表 |
| `get_connection_commands` | `GET /api/v1/driver-gateway/commands?dbSessionId=` | — | 200 | — → 命令描述列表 |
| `execute_driver_command` | `POST /api/v1/driver-gateway/executions` | — | 200 | 嵌套驱动命令请求 → `CommandResult` |
| `execute_driver_command_stream` | `POST /api/v1/driver-gateway/stream-executions` | — | 202 | 嵌套请求（`onEvent` 通道由传输替换，见下）→ `{ streamId }`，事件走 `GET /api/v1/events/{streamId}` |
| `open_base64_with_dialog` | **排除** | — | — | 对话框语义，永不上 HTTP（§8） |

流式驱动命令在桌面用 Tauri `Channel` 推送；HTTP 形态把 `onEvent` 替换为既有 SSE 机制：`POST` 接受执行（202 + `streamId`），事件经统一事件端点投递，复用 team-server §8 的回放、心跳与授权复核。`applyResultLimit`/`recordHistory` 随请求体透传。

### 6.10 execution

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `executeInSession` | `POST /api/v1/sessions/{id}/executions` | ✓ | 202 | `ExecuteInSessionRequest & SubmitRequest` → `ExecutionReceipt` |
| `executeAtTarget` | `POST /api/v1/executions` | ✓ | 202 | `ExecuteAtTargetRequest & SubmitRequest` → `ExecutionReceipt` |
| `getExecution` | `GET /api/v1/executions/{id}` | — | 200 | — → `ExecutionView` |
| `cancelExecution` | `POST /api/v1/executions/{id}/cancel` | ✓ | 200 | `SubmitRequest` → `CancelReceipt` |

裁决：

- **异步接受语义（§3 规则 3，team-server §9.3）**：两条 execute 路由回 `202` + 稳定
  `executionId`（`ExecutionReceipt`：`executionId` / `streamId` / `state`）。已接受 ≠ SQL 成功
  ——终态经 `getExecution` 或事件流（§6.6）取。
- **会话内执行闸序**（`setSessionContext` 同款，§6.4）：令牌在场 → 可见性（越组织/未知会话
  `404` 合并）→ 体解析 → `handle.dbSessionId` 与路由 `{id}` 一致性（`400`）→ DTO 校验 → 用例。
  体 handle 原样传递、不以目录条目改写——epoch 核对归用例层。
- **组织闸**：`executeAtTarget` 的 `target.connectionId` 归属与 `getExecution` /
  `cancelExecution` 的可见性由用例层闭环（sink `visible_to` 的 `404` 合并口径，未命中统一
  404「execution unavailable」，无存在性预言机），HTTP 层不做第二套组织判定。
  `GET /api/v1/executions/{id}` 在 L6 登记 `ResourceType::Execution` + `Read`（rbac 基线
  矩阵裁决——W8 刀1对账补行；`/cancel` 子动作与集合级提交不入两层段表，action 语义
  在用例层）。`cancelExecution` 体只留幂等凭据（`submissionTokenBody` 半边），
  `disposition: unsupported` / `alreadyFinished` 是正常取值。
- **令牌签发的会话绑定操作**：`issueSubmissionToken`（§6.2）对 `openSession` /
  `executeInSession` / `executeAtTarget` / `setSessionContext` / `cancelExecution` 经**执行用例
  面**签发（令牌注册进运行时令牌表，与 execute/cancel 路径的核验同一实例闭环）；
  `startJob` / `createProfile` 维持 HMAC 签发器（`SubmissionService` 的 `verify` 是同一签发器）；
  两轨不得混用（HMAC 轨令牌过不了 runtime 核验，`409 idempotencyExpired`）。
- **限流严格档（§9.1，W8 刀1对账补行）**：`POST …/executions`（executeAtTarget /
  executeInSession；过渡域 driver-gateway 的同词尾同语义）与
  `POST /api/v1/ai-host/execute-target` 入严格档；`stream-executions` 不是 `/executions`
  词尾，维持缺省档。

### 6.11 job

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `startJob` | `POST /api/v1/jobs` | ✓ | 201 | `{ submissionToken, definition }` → `JobRecord` |
| `listJobs` | `GET /api/v1/jobs` | — | 200 | `?states=&after=&limit=`（过滤包平铺为查询参数）→ `JobRecord[]` 页 |
| `getJob` | `GET /api/v1/jobs/{id}` | — | 200 | — → `JobRecord`（跨组织按不存在答 `404`） |
| `cancelJob` | `POST /api/v1/jobs/{id}/cancel` | ✓ | 202 | — → `{ outcome: registered \| alreadyRequested \| alreadyTerminal }`（幂等登记；已终态如实回答） |

### 6.12 查询资产：收藏与查询历史（W8 刀3a 服务端半边，2026-11-07）

legacy IPC 命令轨（`add_favorite_query` 等）收敛进 MethodMap 的第一域。词汇一条
（共享层 `datazen::history::{FavoriteQuery, QueryHistoryEntry}`，camelCase）、存储两
形态：桌面 = 收藏 `.sql` 文件 + 本机 history.sqlite；web = 服务端 v10 双表（per
`(organization, principal)` 隔离，named_views v9 同款口径；id 服务端铸造）。

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `listQueryFavorites` | `GET /api/v1/query-favorites?connectionId=` | — | 200 | — → `FavoriteQuery[]` |
| `createQueryFavorite` | `POST /api/v1/query-favorites` | ✓ | 201 | `FavoriteDraft & SubmitRequest` → `FavoriteQuery` |
| `deleteQueryFavorite` | `DELETE /api/v1/query-favorites/{id}` | ✓ | 204 | 未知 id = 幂等 no-op 同码（防枚举不区分） |
| `getQueryHistoryPage` | `GET /api/v1/query-history` | — | 200 | `?limit=&connectionId=&database=&schema=&search=&since=&until=&order=`（过滤平铺，search 在 limit 前）→ `QueryHistoryPage` |
| `deleteQueryHistory` | `DELETE /api/v1/query-history/{id}` | ✓ | 200 | — → `{ deleted }`（行数让调用方拒绝 no-op，桌面同语义） |
| `clearQueryHistory` | `DELETE /api/v1/query-history` | ✓ | 200 | — → `{ purged }`（经 purge 的 query 作用域） |
| `purgeHistory` | `POST /api/v1/query-history/purge` | ✓ | 200 | `{ scope, retainDays? }` → `{ purged }`（`workflow` 作用域按 0 行如实回答） |

- **桌面专属两命令不进 HTTP 面**：`get_favorites_root`（收藏目录路径，对接同步
  服务）与 `refresh_favorites`（重扫目录接收同步文件）是文件系统语义——服务端收
  藏是库行，没有根目录也没有需要重扫的目录；`WEB_CAPABILITIES` 声明缺位。
- **查询历史的写面不在本域**：落账点是 execution 域路由的受理后钩子
  （`execution::history_recorder`）——SQL 命令（`query`/`execute`/`explain`）终态后
  best-effort 追加（桌面 `record_sql_command_outcome` 的 transport 对偶），执行应
  答不被落账失败阻断；`rows_affected` 视图无此字段，v1 记 null（登记差异）。
- **归属**：per principal——同组织两个主体各自持有自己的收藏/历史集合，跨主体
  读取天然落空、删除无操作（CM-05）；请求体没有归属字段（INV-01）。
- **写面令牌走 HMAC 轨**（配置类操作，不绑 runtime 会话；§6.2 的两轨分派）。

### 6.13 Schema Diff：对比、计划与部署（W8 刀3b，2026-11-07）

legacy IPC 命令轨的第二域（`compare_table_schemas` / `prepare_schema_diff_plan` 等
11 命令）。词汇（`SchemaDiffProfile` / `SchemaDiffPlan` / `SchemaDiffJobAccepted` /
`SchemaDiffJobDetails`）一条——共享层 `datazen::migration::schema_diff`，camelCase；
存储两形态：桌面 = 加密 JSON 文件（profile）+ 本机 job 库；web = 服务端 v12
（`schema_diff_profiles` / job 运行时三伴表 / `migration_run_history`）。SQL 计划
正文是进程内评审产物（`PlanRepository` 内存 + `job_result_details` 冻结元数据），
不落盘不回显——重启后 `planUnavailableAfterRestart` 如实回答。

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `listSchemaDiffProfiles` | `GET /api/v1/schema-diff/profiles` | — | 200 | — → `SchemaDiffProfile[]` |
| `saveSchemaDiffProfile` | `PUT /api/v1/schema-diff/profiles` | ✓ | 204 | `SchemaDiffProfile & SubmitRequest` → —（upsert；`createdAt` 缺席保留/铸造，`updatedAt` 恒服务端时刻） |
| `deleteSchemaDiffProfile` | `DELETE /api/v1/schema-diff/profiles/{id}` | ✓ | 204 | 未知 id = 幂等 no-op |
| `compareTableSchemas` | `POST /api/v1/schema-diff/comparisons` | — | 200 | 对比文档体（`sourceDbSessionId`/`targetDbSessionId`/表名/可选 schema）→ `TableSchemaComparison`（POST 读，§6.7 同款裁决） |
| `prepareSchemaDiffPlan` | `POST /api/v1/schema-diff/plans` | ✓ | 202 | 准备文档体 → `SchemaDiffJobAccepted` |
| `prepareSchemaUnifiedPlan` | `POST /api/v1/schema-diff/unified-plans` | ✓ | 202 | 统一准备文档体（+`sourceObjects`/`targetObjects`）→ `SchemaDiffJobAccepted` |
| `executeSchemaDiffDeploy` | `POST /api/v1/schema-diff/deploys` | ✓ | 202 | `{ targetDbSessionId, planId+selectionRevision }` 或 `{ plan }`（兼容路径：先注册再受理）→ `SchemaDiffJobAccepted` |
| `listSchemaDiffJobs` | `GET /api/v1/schema-diff/jobs?states=&limit=` | — | 200 | 状态过滤平铺 → `JobView[]` |
| `getSchemaDiffJobDetails` | `GET /api/v1/schema-diff/jobs/{id}` | — | 200 | — → `SchemaDiffJobDetails`（details + prepared 信封 + deployResult） |
| `cancelSchemaDiffDeploy` | `POST /api/v1/schema-diff/jobs/{id}/cancel` | ✓ | 200 | — → `{ cancelled }`（未知/他类/终态 no-op 语义） |
| `verifySchemaDiffJobRecovery` | `POST /api/v1/schema-diff/jobs/{id}/recovery-verification` | ✓ | 200 | `{ targetDbSessionId }` → `SchemaDiffJobDetails`（只读核验 → RequireManualReview 结论族） |

- **受理 = 先持久化后取资源**：prepare/deploy 的 202 回执先于任何目录读与 DDL
  执行（job 受理行落库后才派发）；执行面归因 = 受理主体的真实身份（P3 授权语，
  不落合成服务主体）。
- **门与桌面 IPC 同权**：目标只读连接拒绝、`requireRollback` 需事务型 DDL + 完整
  回滚计划、`Unsupported` 计划要求阻断而 `Backfill` 只告警、破坏性计划需
  `confirm_destructive = "DEPLOY"`、复核消费（reviewed plan 一次性 take）与源侧
  快照复核、部署前目标指纹比对（漂移 = `PlanStale`）。
- **端点物理身份 fail-closed**：`ensure_distinct_schema_scope` 沿用共享层裁决——
  驱动无法证明源/目标是不同库作用域时拒绝（拼写不构成迁移许可）。
- **归属**：profile 与 job 按 organization 隔离（桌面单租户全体共享的服务端收
  敛）；会话可见性走目录闸（越组织/未知 → 404 固定文案，防枚举）；请求体没有
  归属字段（INV-01）。
- **写面令牌走 HMAC 轨**（配置类操作）；plans/unified-plans/deploys 三受理面入
  限流严格档（每受理一条 job，派发占活连接与 DDL 执行资源）。
- **迁移历史落账**：部署编排 start/finish 两段进 v12 `migration_run_history`
  （best-effort，桌面 history.sqlite 的服务端对偶）；`get_migration_run` /
  `list_migration_runs` 的读面在 sync 域收口（后续刀）。

### 6.14 workflow：定义管理、同步执行与运行历史（W8 刀3c，2026-11-07）

| 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
| --- | --- | --- | --- | --- |
| `listWorkflows` | `GET /api/v1/workflows` | — | 200 | — → `WorkflowListItem[]` |
| `saveWorkflow` | `PUT /api/v1/workflows/{id}` | ✓ | 204 | `WorkflowDefinition & SubmitRequest` → — |
| `saveWorkflowYaml` | `POST /api/v1/workflows/from-yaml` | ✓ | 200 | `{ yaml } & SubmitRequest` → `WorkflowDefinition` |
| `getWorkflow` | `GET /api/v1/workflows/{id}` | — | 200 | — → `WorkflowDefinition` |
| `getWorkflowYaml` | `GET /api/v1/workflows/{id}/yaml` | — | 200 | — → `{ yaml }`（标量包装口径与 `{ deleted }` 家族一致） |
| `deleteWorkflow` | `DELETE /api/v1/workflows/{id}` | — | 204 | —（未知 id = 幂等 no-op） |
| `executeWorkflow` | `POST /api/v1/workflows/{id}/execute` | ✓ | 200 | `{ variables, connectionId? } & SubmitRequest` → `WorkflowExecutionResult` |
| `reloadWorkflows` | `POST /api/v1/workflows/reload` | — | 204 | —（库后端 no-op，桌面同语义） |
| `listWorkflowRuns` | `GET /api/v1/workflow-runs?workflowId=` | — | 200 | — → `WorkflowRunListItem[]` |
| `getWorkflowRun` | `GET /api/v1/workflow-runs/{id}` | — | 200 | — → `WorkflowRunRecord` |
| `clearWorkflowRuns` | `DELETE /api/v1/workflow-runs?workflowId=` | — | 200 | — → `{ cleared }` |

域裁决：

- **存储**：v13 双表（`workflows` / `workflow_run_history`，组织复合主键）；
  登记规则单家在共享层 `workflow::registry`（visibility 过滤、YAML 编码、
  列表投影、时间戳服务端权威——create 铸造 / update 保留 createdAt）。
  服务端**不播内置样例**（桌面三个 builtin 是单机 onboarding 供给，
  团队服务器的组织自建）。
- **归属 = 组织级**（CM-05）：v1 单组织部署，域状态持有宿主配置组织的
  scoped 件，每 handler 先过归属闸（请求组织 ≠ 宿主组织 ⇒ 404 合并
  口径；aiHost 同款）。多组织扩容时改 per-request 装配。
- **执行是同步应答**（桌面 IPC 同语义；aiHost.executeTarget 同款先例）：
  `WorkflowExecutor::execute` await 到底返回整份结果。长跑工作流超过
  传输超时是调用方的重试策略问题，不在本域发明 202/job 语义（执行域的
  job 面是另一个用例）。限流严格档（受理面，与 deploys 同族）。
- **AI 步骤 fail-closed**：server 形态本刀无 AI 供应商注册表。含 AI 步骤
  的交互执行按执行器默认错误策略应答（200 + `success:false` + 步骤
  `error` 携带明确拒绝文案——桌面同语义，步骤失败不是命令错误）；调度
  面被授权单派生更早拒绝（服务身份不放行 AI 步骤）。**定时调度器**：
  共享层核心调度器（arm-not-fire、上膛冻结 → 触发比对、授权闸门失败
  关闭）server 接线（spawn 于装配尾、graceful drain 后停车）——「双端
  调度」补齐：server 形态的定时工作流按授权单派生派发，服务身份
  （`workflow-schedule:{id}:{revision}`）执行、审计归因不落请求主体。
- **桌面专属一命令不进 HTTP 面**：`getWorkflowDir` 是文件系统语义
  （工作流目录路径对接本机文件管理）——服务端定义是库行，无目录可给；
  `WEB_CAPABILITIES` 声明缺位，web 端如实报 `CapabilityUnsupported`。
- **令牌**：带体写面（save / save-yaml / execute）核 `SubmitRequest`
  在场性与 `idempotencyKey` 绑定（HMAC 轨签发，`saveUserAsset` 操作族）。
  DELETE 面（delete / clear）不带体，与 queryAssets / schemaDiff 的
  DELETE 现行口径一致。
- **错误 wire**：存储故障 → 503 `serviceUnavailable`；定义校验失败 →
  400 `invalidArgument`；未知/越组织 id → 404 固定文案（防枚举）。

## 7. 能力差异：`WEB_CAPABILITIES` 与 `unsupportedMethods`

- **桌面专属能力不出现在 HTTP 面**：updater、window、native dialog、本机 OS 集成。它们的形态差异由 `packages/frontend/src/platform/capabilities.ts` 的静态表声明（`WEB_CAPABILITIES.updater: false`、`settingsSections` 收窄），前端**编译期分支**，不做运行时形态探测。
- **域内方法级差异**走 `BackendTransport` 的 `unsupportedMethods` 声明 + guard 的 `CapabilityUnsupported`（[连接管理 §13](connection-management.md#13-错误重试和事件) 错误表已登记，机制见 `packages/backend-client/src/guard.ts`）：一个 web 后端若暂不服务某方法（如 server 未实现前的 `saveNamedView`），在装配时声明，**使用时**才失败并携带 `retryDisposition`，不在绑定时报错。
- **`CapabilityUnsupported` 的 HTTP 映射是本文裁决**（team-server §9.2 现表未列，WR-D 落地时折回该表）：guard 的两种 `UnsupportedMethodKind` 分开映射——`capability`（此形态不服务）→ **405**；`not-implemented`（尚未实现）→ **501**。两者 `retryDisposition` 都是 `never`。不得用 404（「方法存在但此形态不服务」与「资源不可见」是两类事实，混同会破坏 team-server §7 的防枚举口径），也不得用 403（不是权限问题）。

## 8. 明确不在 HTTP 面的东西

| 类别 | 例 | 去处 |
| --- | --- | --- |
| 平台服务 | `saveTextWithDialog`、`openDirectoryWithDialog`、`writeClipboard*`、`openExternal`、`toggleFullscreen`、`open_base64_with_dialog` | 非后端能力。web 形态用浏览器 API 实现（`window.open`、async clipboard、Fullscreen API），永远不绕行服务器 |
| 桌面宿主专属 | updater、window 管理、OS 集成 | `WEB_CAPABILITIES` 声明缺位 |
| legacy IPC 命令轨 | 约 200 个 snake_case 命令（`ai_chat`、`connect`、`begin_save_with_dialog`…） | 不镜像。随 WR-A2 command 薄壳化逐域收敛进 `MethodMap`，收敛一个域、本表长一域；命令轨整体消亡时不留 HTTP 遗迹 |
| 浏览器直连数据库 | 任何把数据库凭据交给浏览器端的路径 | 架构禁令（SQL 执行全服务端），无 HTTP 形态 |

## 9. 验收核对（WR-C2 门槛）

1. **一一对应**：十个域 43 个方法，42 个有端点（§4 总账），逐方法可在 §6 表中点到路由；反向每条域面路由可点到方法。
2. **多余端点为零**：域面 41 条路由全部有方法背书；唯一无方法端点（`artifacts/uploads`）为已立项在账项，带收口动作（§6.5）。
3. **`WEB_CAPABILITIES` 不含桌面专属项**：updater `false`、settings 分节收窄（§7）；桌面专属能力零 HTTP 路由（§8）。

WR-D 落地时的机器核对（WR-C3 接管）：同用例双传输测试逐方法断言 IPC 与 HTTP 的请求/响应形状同源；CI 契约断言扫描本表路由与 axum router 逐条对齐。

## 10. 与既有文档的关系

| 文档 | 关系 |
| --- | --- |
| [system-overview](system-overview.md) §6.2/§6.3 | §6.3 的骨架路由全部原样收编进 §6；本文是其补全与冻结，此后路由变更以本文为准 |
| [team-server-and-auth](team-server-and-auth.md) §3–§9、§11 | 中间件、认证、CSRF、SSE、错误映射、产物实现的权威；本文不重述，只引用 |
| [connection-management](connection-management.md) §4.1、§13.1 | 连接域 DTO 与幂等令牌期限的权威 |
| [shared-boundaries-and-ports](shared-boundaries-and-ports.md) | `MethodMap` 与域客户端的边界设计（F3） |
| web-ready-plan WR-C2 | 本文的立项来源与验收口径 |

## 11. 变更记录

- 2026-10-31：首版。收编概要 §6.3 骨架，补齐 settings / identity / tableData（14 方法）/ driverGateway（4+1 排除）；裁决命名视图上 HTTP（服务端存储）、`DELETE` 唯一例外（命名视图）、流式驱动命令走 SSE、`open_base64_with_dialog` 永不上 HTTP。
- 2026-11-02：v1.1。events 域登记订阅建立路由 `POST /api/v1/events/subscriptions`（WR-D5 落地，§6.6 表）；域方法数 42 → 43。
- 2026-11-03：v1.2（WR-F1 扩展面登记）。**mcp 客户端面**（`mcp.initialize/listTools/callTool/closeSession`）走服务端 `/mcp` streamable HTTP（WR-F2 传输，§6.6 之外的服务面；JSON-RPC 2.0 信封 + `Mcp-Session-Id` 头，L2 cookie 即身份——INV-S1），wire-direct 不入 `MethodMap` 主表（`transport.ts` 类型冻结，逐域登记于 web 侧 `WR_F1_HTTP_ROUTES`）；web broker 的 `serverId` 固定 `"datazen"`（v1 语义：工具面 = datazen 自有 13 工具，与桌面多外部服务器 broker 的差异如实登记）。**security 域**（`security.resolveSecret` → `POST /api/v1/secrets/resolve`、`security.putSecret` → `PUT /api/v1/secrets/{domain}/{key}`）与 **aiHost 域**（`aiHost.getSessionConfig` → `POST /api/v1/ai-host/session-config`、`aiHost.executeTarget` → `POST /api/v1/ai-host/execute-target`）客户端面已定形（web 侧真实现 + 契约测试），**服务端路由未点亮**（部署未挂 = 501/404 显式形态）——服务端半边随执行域路由切片落地，点亮前 web 端如实报 `CapabilityUnsupported` 的替身面保持。
- 2026-11-06：v1.3（execution 域路由收口刀）。§6.10 execution 四端点点亮（`POST /api/v1/sessions/{id}/executions`、`POST /api/v1/executions`、`GET /api/v1/executions/{id}`、`POST /api/v1/executions/{id}/cancel`）——§4 总账的 execution 4/4 由「计账」变「实服务」；§6.11 job 表补登（四端点此前已服务但无逐域路由表）；§6.2 `issueSubmissionToken` 的会话绑定操作改为经执行用例面签发（修复 openSession/cancelExecution 在 HTTP 面必然 `TokenInvalid` 的端到端断点）；aiHost.executeTarget（§6.2 表）501 → 200 `CommandResult`。
- 2026-11-07：v1.4（W8 刀3a，查询资产域服务端半边）。**§6.12 收藏与查询历史**七路由（收藏 list/create/delete + 历史 page/delete/clear/purge）——legacy IPC 命令轨（`add_favorite_query`/`get_query_history_page` 等 7 命令）收敛进 MethodMap 的第一域；v10 双表 expand-only 迁移（per (org, principal) 隔离，收藏文件形/历史本机库形的服务端对偶）；历史落账 = execution 域受理后钩子（best-effort，桌面同语义）；`get_favorites_root`/`refresh_favorites` 文件系统语义不进 HTTP 面（桌面专属）。web 半边（域客户端 + RouteSpec + 不支持名单收缩）随下刀收口。
- 2026-11-07：v1.5（W8 刀3b，Schema Diff 域）。**§6.13 对比、计划与部署**十一路由（profiles 三 + compare + 计划准备二 + 部署受理 + job 读回/取消/恢复核验四）——legacy IPC 命令轨的第二域；v12 expand-only 迁移（job 运行时三伴表 + `migration_run_history` + `schema_diff_profiles`，桌面加密 JSON 文件形的服务端对偶）；`ServerJobs` 实现 `JobRuntimeRepository`（桌面 runtime_ops 家族的服务端投影，幂等回执走受理行 `idem_scope_key` 不开表）；受理 = 先持久化后取资源（202 回执先于任何 DDL）；plans/unified-plans/deploys 三受理面入限流严格档。§4 总账 43→61 方法 / 41→59 路由（queryAssets 7 + schemaDiff 11 入账）。web 半边（域客户端 + 不支持名单收缩）随下刀收口。
- 2026-11-07：v1.4.1（W8 刀1对账——PR #204 关闭后的增量移植）。§6.10 补三行裁决：`GET /api/v1/executions/{id}` 的 L6 两段登记（`ResourceType::Execution` + `Read`，rbac 基线矩阵裁决）、令牌两轨不混用的 409 wire（`idempotencyExpired`）、限流严格档补行（`POST …/executions` 与 `POST /api/v1/ai-host/execute-target`）。aiHost.executeTarget 的 claim 纪律补齐（claim 必填：`independent` 真执行、`editorGrant` 501 fail-closed、缺失/未知 400——与 web 端 `AiHostExecuteTargetRequest` 冻结形状对齐）；`PrincipalScopedHost` 上移为共享装饰器（aiHost 与 MCP 七工具同一真主体注入路径）。
- 2026-11-07：v1.5.1（W8 刀3a/刀3b web 半边收口——域客户端落地）。**queryAssets / schemaDiff 两域客户端**进 `@datazen/backend-client`（`QueryAssetsMethodMap` 7 方法 + `SchemaDiffMethodMap` 11 方法，`MethodMap` 组合挂接、门面 spread 平铺）；web `HTTP_ROUTES` 分发表 42→60 条（queryAssets 7 + schemaDiff 11 的 RouteSpec：写面体折令牌、`{deleted}`/`{purged}`/`{cancelled}` 包装由 decoder 拆封到桌面域形状、save 的 `{profile}` 包装在 body 构造器展开）；**tagged-enum 归一化上移**（plan requirements 的 `{backfill|unsupported}` 折叠进域客户端，桌面 IPC 与 HTTP 两轨同形，归一化函数 `normalizeSchemaDiffJobDetailsWire` 出面供宿主复用）；`webUnsupportedMethods` 名单收缩 18 条（legacy 命令轨第三、四域消亡；`get_favorites_root`/`refresh_favorites` 桌面文件语义与 `get_migration_run`/`list_migration_runs`（留 sync 域）保持登记）；桌面 transport 三处命令别名（`listQueryFavorites→get_favorite_queries` 等）+ 收敛域令牌剥除（IPC 轨无幂等回执）；**`IdempotentOperation` 增 `SaveUserAsset`**（用户资产写面共用配置类操作，不绑会话；签发面白名单 + `saveUserAsset` wire 词入账——服务器消费闸维持在场性核对，HMAC 细分随未来接线退役拆开）。前端 `queryCommands`/`schemaDiffCommands`/`historyCommands.purgeHistory` 全量换域门面方法（`getFavoritesRoot`/`refreshFavorites` 等桌面专属留 legacy 轨）。
- 2026-11-07：v1.6（W8 刀3c，workflow 域 + 双端定时调度）。**§6.14 定义管理、同步执行与运行历史**十一路由（定义 CRUD 五 + from-yaml + execute + reload + 运行历史三）——legacy IPC 命令轨的第五域（12 命令收敛，`getWorkflowDir` 桌面文件语义排除）；v13 expand-only 迁移（`workflows` + `workflow_run_history` 双表，桌面 AppDb/HistoryDb 同名表的组织键投影；服务端不播内置样例——单机 onboarding 供给的形态差异）；执行 = 同步 200 全量结果（桌面 IPC 同语义，aiHost.executeTarget 同款先例；受理面入限流严格档）；**AI 步骤 fail-closed**（server 形态本刀无 AI 供应商注册表：交互面按执行器默认错误策略 200 + `success:false` + 步骤 error 明确拒绝文案，调度面被授权单派生更早拒绝）；**定时调度器 server 接线**（「双端调度」补齐）：共享层核心调度器（arm-not-fire、上膛冻结→触发比对、授权闸门失败关闭）+ `ServerScheduleGrantReader`（定义 YAML + profile 版本的确定性派生，收集规则自桌面下沉共享层）+ `ServerScheduleHost`（owner 归属核对 + 服务身份执行上下文 + 审计日志），spawn 于装配尾、graceful drain 后停车；共享层 `WorkflowScheduler` 增公开 `stop`/`is_stopped`（两形态受益）。§4 总账 61→73 方法 / 59→70 路由（workflow 12 入账，11 服务 1 排除）。web 半边（域客户端 + 不支持名单收缩）随下刀收口。
