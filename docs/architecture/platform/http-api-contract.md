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

十个域、43 个域客户端方法：

| 域 | 方法数 | HTTP 服务 | 排除 | 备注 |
| --- | --- | --- | --- | --- |
| settings | 2 | 2 | 0 | |
| identity | 2 | 2 | 0 | |
| connection | 4 | 4 | 0 | PATCH 一条路由双服务 update/disable（§6.3） |
| session | 6 | 6 | 0 | |
| execution | 4 | 4 | 0 | |
| job | 4 | 4 | 0 | |
| artifact | 1 | 1 | 0 | uploads 端点另列（§6.5） |
| events | 1 | 1 | 0 | SSE |
| tableData | 14 | 14 | 0 | |
| driverGateway（过渡） | 5 | 4 | 1 | `open_base64_with_dialog` 排除（§6.9） |
| **合计** | **43** | **42** | **1** | |

路由总数 41 条服务 42 个方法（PATCH 双服务），另有服务自举路由 5 条（§5）与已立项待补方法的 1 条（§6.5）。

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

`SubscribeEventsRequest` 的 `streamId` 进路径、`afterSequence` 进查询参数（与 `Last-Event-ID` 头二选一，优先头）。握手鉴权不用 CSRF 头（EventSource 无法设置），用 cookie + Origin/Fetch-Metadata 证明（team-server §8.1 的既有裁决）。`streamId` 的分配随 WR-D5 落地：订阅建立路由（或会话回执内含 streamId）在实现时按 §8 机制登记进本表 **v1.1**，不另开传输。

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
