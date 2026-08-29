# DataZen v0.2.0 Web 平台化 PRD

> 状态：Draft  
> 目标版本：v0.2.0  
> 更新时间：2026-08-29  
> 产品形态：单节点、自托管、桌面端与 Web 端共享业务核心

## 1. 文档目的

本 PRD 定义 DataZen v0.2.0 Web 平台化的产品范围、用户角色、核心流程、功能要求、非功能要求、桌面与 Web 能力边界、技术约束、发布阶段和验收标准。

本 PRD 只讨论 Web 平台底座及其承载的核心 DataZen 能力。SQL 审计、AI Workflow、长期记忆/知识库、Dashboard 告警有各自的领域设计，但必须遵循本文定义的身份、工作区、API、事件、数据隔离和安全边界。

相关架构文档：

- [系统架构总览](../architecture/README.md)
- [IPC 命令层](../architecture/backend/commands.md)
- [持久化存储](../architecture/backend/store.md)
- [安全措施](../architecture/security.md)
- [Workflow](../architecture/backend/workflow.md)
- [Dashboard](../architecture/backend/dashboard.md)
- [ID 术语规范](../architecture/naming.md)

## 2. 执行摘要

### 2.1 产品定义

DataZen Web 是部署在用户自有服务器上的团队数据工作台。数据库连接、AI 调用、Workflow 执行和 Dashboard 监控均发生在 DataZen Server，浏览器只负责交互和结果展示。

v0.2.0 同时保留两种运行形态：

1. **Desktop Standalone**：现有 Tauri 应用，本地存储、本地数据库连接，可离线运行。
2. **Web Self-hosted**：Docker 部署的 DataZen Server，浏览器通过 HTTPS/REST/SSE 使用，支持本地账号、工作区和团队权限。

两种形态共享 React 页面、领域 DTO、Driver、Workflow、AI、Dashboard 和后续 SQL Audit 核心实现，但使用不同 transport 和持久化适配器。

### 2.2 v0.2.0 必须成立的五个判断

1. Web 不是把 Vite 静态页面发布出去，而是新增持续运行的 Rust Server。
2. 浏览器不直接连接数据库，也不能读取已保存的数据库密码或 AI Key。
3. 所有持久化对象都归属工作区，所有请求都经过身份认证和工作区授权。
4. Tauri IPC 和 HTTP API 只能是适配层，不能各自实现一套业务逻辑。
5. 浏览器关闭后，Workflow 调度和 Dashboard 监控仍由服务端继续运行。

## 3. 背景与现状

### 3.1 可复用基础

- React 主工作区已包含连接、Schema、Query、AI、Workflow、Dashboard、Settings 等页面。
- Rust 后端已有 AppState、ConnectionManager、DriverRegistry、SchemaCache、Workflow Engine、MonitorEngine、AI Provider Registry 和统一 Driver Command API。
- Workflow、Dashboard、查询历史已逐步迁入 SQLite；连接配置和部分设置仍使用本地文件存储。
- 前端已有少量浏览器 fallback：BroadcastChannel、浏览器子 Tab、基于 User-Agent 的平台识别。
- 后端已有无头 MCP stdio 模式，证明核心运行时不必始终依赖可见 Tauri 窗口。

### 3.2 主要差距

- 前端多个 command/store 直接依赖 @tauri-apps/api 的 invoke、listen 和插件 API。
- src-tauri 同时包含 Tauri 命令、业务实现、持久化和运行时组装，尚未形成可独立启动的 Server Core。
- 当前数据模型基本按“单用户、单实例”设计，缺少 user、workspace、membership 和对象归属。
- OS Keychain 不适用于 Linux 容器中的无人值守服务端密钥管理。
- 现有子窗口、原生文件对话框、托盘、更新器、插件自定义协议等能力不能原样搬到浏览器。
- 尚无 HTTP 鉴权、Cookie Session、CSRF/CORS、反向代理、限流和 Web 安全测试体系。

## 4. 目标与非目标

### 4.1 业务目标

- 团队可在 15 分钟内通过 Docker Compose 部署并完成首次管理员初始化。
- 用户无需安装桌面客户端即可使用连接、查询、AI、Workflow 和 Dashboard 核心能力。
- 团队成员共享由工作区管理的连接、Workflow、Dashboard 和知识，同时保持权限隔离。
- Dashboard 和定时 Workflow 在无人打开浏览器时持续运行。
- 后续 SQL 审计、长期记忆、告警中心拥有统一的 actor、workspace 和 audit context。

### 4.2 工程目标

- 核心业务逻辑不依赖 Tauri 类型。
- 前端业务组件不感知 Tauri/HTTP 差异。
- API 有版本、类型生成、结构化错误、流式事件和兼容策略。
- Desktop v0.1 数据可原地升级，不因 Web 化破坏现有离线能力。
- Host 能力测试与 Driver 方言测试继续遵循现有测试落点规则。

### 4.3 非目标

以下能力不进入 v0.2.0：

- DataZen 官方公有云 SaaS、计费、套餐、组织邀请邮件。
- 多节点、高可用、分布式调度、跨区域容灾。
- 企业 SSO、SAML、OIDC、LDAP/AD。
- 浏览器直连数据库或将数据库凭据发送到浏览器。
- 桌面客户端连接远程 DataZen Server。
- Web 端运行时 UI Extension 安装与 datazen:// 自定义协议。
- Web 端 MCP stdio Client、系统托盘、自动更新、原生菜单和 OS Keychain。
- 完整桌面功能 1:1 parity；备份恢复、Data Sync、Data Transfer、Schema Diff 按后续版本推进。

## 5. 目标用户与角色

### 5.1 用户画像

#### 实例管理员

负责部署、升级、备份、全局安全设置、用户生命周期和运行状态。通常是 DevOps 或平台工程师。

#### 工作区管理员

负责团队成员、数据库连接、AI Provider、审计策略、知识源和 Dashboard 监控设置。

#### 编辑者

日常使用 SQL、Schema、AI、Workflow 和 Dashboard，能创建和修改工作区内容，但不能管理成员或读取凭据。

#### 查看者

只查看已发布 Dashboard、历史运行和允许展示的结果，不可建立连接、执行任意 SQL 或修改工作区对象。

### 5.2 RBAC 权限矩阵

| 能力 | 实例管理员 | 工作区管理员 | 编辑者 | 查看者 |
|---|---:|---:|---:|---:|
| 管理实例用户与系统设置 | 是 | 否 | 否 | 否 |
| 创建/删除工作区 | 是 | 否 | 否 | 否 |
| 管理工作区成员 | 是 | 是 | 否 | 否 |
| 创建/编辑/删除连接 | 是 | 是 | 否 | 否 |
| 查看连接非敏感信息 | 是 | 是 | 是 | 否 |
| 连接并执行 SQL/Command | 是 | 是 | 是 | 否 |
| 查看 Schema/Table Data | 是 | 是 | 是 | 否 |
| 创建/编辑/执行 Workflow | 是 | 是 | 是 | 否 |
| 创建/编辑 Dashboard | 是 | 是 | 是 | 否 |
| 查看已发布 Dashboard/历史 | 是 | 是 | 是 | 是 |
| 手工刷新 Dashboard Widget | 是 | 是 | 是 | 否 |
| 配置 AI/知识库/告警通道 | 是 | 是 | 否 | 否 |
| 查看安全与 SQL 审计日志 | 是 | 是 | 否 | 否 |

v0.2.0 权限粒度以工作区为边界，不提供单连接、单 Workflow、单 Dashboard 的独立 ACL。

## 6. 核心产品原则

### 6.1 一套核心，两种 Transport

前端领域层调用 PlatformClient，不直接调用 Tauri 或 fetch：

    React / Zustand / Domain UI
                │
          PlatformClient
           ┌────┴────┐
           ▼         ▼
    TauriTransport  HttpTransport
           │         │
     Tauri Adapter  REST + SSE Adapter
           └────┬────┘
                ▼
           datazen-core
                ▼
     Driver / Workflow / AI / Dashboard / Audit

### 6.2 服务端掌握信任边界

- 浏览器只持有 HttpOnly 登录 Cookie 和短生命周期页面状态。
- 数据库密码、SSH Key、AI Key、Webhook Secret 永不返回浏览器。
- 任何 UI 隐藏都不能替代服务端授权。
- query、execute、Workflow、Dashboard、AI tool call 均在服务端重新校验 workspace 和 role。

### 6.3 长任务使用 Job 模型

连接测试、查询、AI 流式生成、Workflow、导出和 Dashboard 刷新不能绑定单个 HTTP 请求等待完整结果：

    POST action → 202 + jobId/requestId
    SSE event   → progress/chunk/result/error/cancelled
    GET job     → 当前状态与最终摘要
    POST cancel → 幂等取消

### 6.4 桌面与 Web 允许能力差异

Web 版以浏览器和服务端语义重新设计交互，不通过弹出窗口或伪造文件路径模拟桌面能力。不可用能力必须明确隐藏并给出原因，不展示点击后失败的入口。

## 7. 信息架构

### 7.1 登录前

- 登录页
- 首次初始化页（仅未初始化实例）
- 服务不可用/升级中页面

### 7.2 登录后全局壳层

- 工作区切换器
- 主导航：连接、Workflow、Dashboard、AI/知识、审计、设置
- 当前用户菜单：个人偏好、修改密码、退出
- 通知中心：系统通知、告警、任务完成/失败

### 7.3 工作区内

现有主工作区的 Connection Page、Workflow Page、Dashboard Page 保持主要布局。Web 中的 Backup、Data Sync、Data Transfer、Schema Diff 入口在 v0.2.0 不显示。

## 8. 核心用户 Journey

### J1：部署与首次初始化

1. 管理员准备 MySQL、持久化 volume、DATAZEN_DATABASE_URL 和 DATAZEN_MASTER_KEY。
2. 运行官方 Docker Compose。
3. 访问 Web 地址，使用一次性 bootstrap token 创建首个实例管理员。
4. 系统立即使 bootstrap token 失效。
5. 管理员创建首个工作区并进入工作台。

验收：

- 未配置 master key 时，生产模式拒绝启动。
- 初始化只能成功执行一次，并写入安全审计日志。
- 健康检查能区分 process alive、MySQL unavailable、ready 和 migration failed。

### J2：登录与工作区切换

1. 用户输入账号密码，服务端建立 Cookie Session。
2. 用户进入上次使用的工作区，或选择可访问工作区。
3. 切换工作区时清空当前 dbSession、页面缓存和事件订阅。

验收：

- 用户不能通过修改 URL/ID 访问未加入的工作区。
- 切换后不得显示上一工作区的连接、结果、历史或 Dashboard。
- 退出后 Session 立即失效。

### J3：创建连接并查询

1. 工作区管理员创建数据库连接并填写凭据。
2. 服务端测试连接；保存时加密敏感字段。
3. 编辑者打开连接，服务端创建 scoped dbSessionId。
4. 用户浏览 Schema，输入 SQL 并执行。
5. UI 通过 SSE 接收进度、结果块、统计和错误。
6. 用户取消查询或关闭页面；服务端执行取消与超时清理。

验收：

- 保存后的密码字段只显示“已配置”，任何 API 不返回密文或明文。
- dbSessionId 只绑定当前 user、workspace、connection，并有 idle TTL。
- 页面重载后通过 connectionId 建立新 session，不恢复过期 runtime ID。
- 查询结果遵循行数、字节数和执行超时限制。

### J4：AI 与 Workflow

1. 工作区管理员配置 AI Provider Secret。
2. 编辑者使用 NL2SQL、AI Chat 或 AI Workflow。
3. AI stream、tool call 和 AskQuestion 通过统一事件协议返回。
4. 生成 SQL/Workflow 在执行前走权限和 SQL 审计。

验收：

- AI Secret 不返回浏览器。
- 用户取消后，Provider 请求和相关 tool execution 尽最大可能停止。
- AI 不能通过 tool call 使用用户无权访问的 connection/workspace。

### J5：Dashboard 持续监控

1. 编辑者创建 Dashboard/Widget 并绑定 Workflow。
2. 工作区管理员启用 interval refresh 和告警规则。
3. MonitorWorker 在服务端按计划执行。
4. 浏览器打开时通过 SSE 接收最新运行和告警状态。
5. 浏览器关闭后监控继续。

验收：

- 服务重启后调度自动恢复。
- Widget 执行不依赖发起配置的用户保持登录。
- Viewer 可查看已发布结果，但不能触发任意数据库执行。

### J6：成员与权限管理

1. 实例管理员创建用户。
2. 工作区管理员将用户加入工作区并分配角色。
3. 角色变更后，旧 Session 的下一次请求立即使用新权限。
4. 移出工作区后，该用户的 SSE、dbSession 和运行中 Job 被终止或失去访问权。

## 9. 功能需求

需求优先级：P0 为 v0.2.0 Stable 必须；P1 可进入 v0.2.x；P2 为后续候选。

### 9.1 部署与实例生命周期

| ID | 优先级 | 需求 |
|---|---|---|
| WEB-DEP-001 | P0 | 提供 Linux amd64/arm64 Docker 镜像与 Docker Compose 示例。 |
| WEB-DEP-002 | P0 | 单个 datazen-server 进程同时提供 SPA、REST API、SSE 和后台 Worker。 |
| WEB-DEP-003 | P0 | 数据、日志和加密数据写入显式 volume。 |
| WEB-DEP-004 | P0 | 提供 /health/live、/health/ready 和版本信息。 |
| WEB-DEP-005 | P0 | migration 失败时 fail closed，不接收业务请求。 |
| WEB-DEP-006 | P0 | 支持反向代理下的 HTTPS、Base URL 和可信代理配置。 |
| WEB-DEP-007 | P1 | 提供 systemd 原生部署包。 |
| WEB-DEP-008 | P0 | Core 持久化只依赖统一抽象接口；Desktop 使用 SQLite 实现，Web 使用 MySQL 实现。 |

建议环境变量：

- DATAZEN_BIND_ADDR：默认 0.0.0.0:8080
- DATAZEN_PUBLIC_URL：外部访问地址
- DATAZEN_DATA_DIR：持久化目录
- DATAZEN_DATABASE_URL：Web 元数据库 MySQL 连接串，生产环境必须通过 Secret 注入
- DATAZEN_MASTER_KEY：安全编码值或 secret-file 引用
- DATAZEN_BOOTSTRAP_TOKEN：首次初始化一次性 token
- DATAZEN_TRUST_PROXY：可信反向代理配置
- DATAZEN_LOG_LEVEL

生产模式禁止使用只存在于临时容器层的自动生成 master key。

### 9.2 身份认证

| ID | 优先级 | 需求 |
|---|---|---|
| WEB-AUTH-001 | P0 | 本地账号支持 username、display name、password、enabled 状态。 |
| WEB-AUTH-002 | P0 | 密码使用 Argon2id，参数可迁移升级。 |
| WEB-AUTH-003 | P0 | 使用服务端 Session + Secure/HttpOnly/SameSite Cookie。 |
| WEB-AUTH-004 | P0 | 写请求具备 CSRF 防护；登录有速率限制。 |
| WEB-AUTH-005 | P0 | 用户可修改密码；实例管理员可强制重置。 |
| WEB-AUTH-006 | P0 | 禁用、密码重置、主动退出会撤销已有 Session。 |
| WEB-AUTH-007 | P1 | TOTP MFA。 |
| WEB-AUTH-008 | P2 | OIDC/SAML/LDAP。 |

### 9.3 工作区与 RBAC

| ID | 优先级 | 需求 |
|---|---|---|
| WEB-WS-001 | P0 | 所有业务对象包含 workspaceId。 |
| WEB-WS-002 | P0 | 资源 API 从认证上下文解析 workspace，不信任 body 内 actor/role。 |
| WEB-WS-003 | P0 | 实现四种角色。 |
| WEB-WS-004 | P0 | 权限变更实时作用于后续请求、事件和 runtime session。 |
| WEB-WS-005 | P0 | 删除工作区进入 pending deletion，不立即物理擦除。 |
| WEB-WS-006 | P1 | 单资源 ACL 与只读 connection role。 |

### 9.4 连接与会话

| ID | 优先级 | 需求 |
|---|---|---|
| WEB-CONN-001 | P0 | 工作区管理员可创建、编辑、测试、禁用和删除连接。 |
| WEB-CONN-002 | P0 | Secret 字段 write-only；编辑时空值表示保持原 Secret。 |
| WEB-CONN-003 | P0 | connectionId 为持久化 ID；dbSessionId 为内存态 ID，永不落盘。 |
| WEB-CONN-004 | P0 | dbSessionId 校验 user/workspace/connection owner，默认 idle TTL 30 分钟。 |
| WEB-CONN-005 | P0 | 每连接和全局连接池上限可配置，工作区之间不共享 session。 |
| WEB-CONN-006 | P0 | 删除/禁用连接时关闭 session，并标记引用它的 Dashboard/Workflow。 |
| WEB-CONN-007 | P1 | SSH 私钥上传；P0 仅支持文本粘贴或服务器 Secret 引用。 |

### 9.5 Query、Schema 与数据浏览

| ID | 优先级 | 需求 |
|---|---|---|
| WEB-QUERY-001 | P0 | 支持 Schema 树、SQL 编辑、执行、取消、分页结果和错误展示。 |
| WEB-QUERY-002 | P0 | Query/Execute 统一走 Driver Command Runtime。 |
| WEB-QUERY-003 | P0 | 支持结果分块；默认 5,000 行，硬限制 50,000 行或 32 MiB。 |
| WEB-QUERY-004 | P0 | 默认超时 60 秒，可在安全范围内配置。 |
| WEB-QUERY-005 | P0 | 历史记录 actor、workspace、connectionId、来源、耗时和审计结论。 |
| WEB-QUERY-006 | P0 | 刷新页面不恢复 result rows，只恢复草稿和历史摘要。 |
| WEB-QUERY-007 | P1 | 浏览器下载 CSV/JSON/XLSX；大结果服务端 streaming export。 |
| WEB-QUERY-008 | P1 | ER、EXPLAIN、Privilege View 完整 parity。 |
| WEB-QUERY-009 | P0 | SQL 执行采用 prepare → audit → execute_prepared；审计同时记录用户提交 SQL、Driver 最终执行 SQL、改写链和最终状态。 |

### 9.6 Workflow、AI 与 Dashboard

| ID | 优先级 | 需求 |
|---|---|---|
| WEB-WF-001 | P0 | Workflow CRUD、双模编辑、执行、取消和历史。 |
| WEB-WF-002 | P0 | Schedule 由服务端 Worker 执行，不依赖用户 Session。 |
| WEB-AI-001 | P0 | AI Provider 按工作区保存，Secret write-only。 |
| WEB-AI-002 | P0 | AI Chat、NL2SQL、诊断和 Workflow AI 使用 SSE。 |
| WEB-AI-003 | P0 | DB/MCP tool 继承授权；Web 禁止本地 stdio MCP Client。 |
| WEB-DASH-001 | P0 | Dashboard CRUD、查看、手工刷新、历史和 interval monitor。 |
| WEB-DASH-002 | P0 | Viewer 只读查看已发布结果。 |
| WEB-DASH-003 | P0 | 浏览器关闭或用户退出后 MonitorWorker 继续执行。 |

### 9.7 设置、文件与桌面专属能力

设置拆分为：

- **个人设置**：语言、主题、编辑器字体、页面尺寸。
- **工作区设置**：AI Provider、SQL policy、知识、监控、Webhook。
- **实例设置**：用户、日志、Session、安全、备份保留。
- **桌面设置**：托盘、关闭行为、更新器、本地 context directory，仅 Desktop 显示。

Web 文件交互规则：

- 浏览器只看到上传控件和下载响应，不显示服务器绝对路径。
- Context/知识文档通过受限上传或管理员配置的 server-side mount 导入。
- 导入限制扩展名、MIME、大小、压缩展开比例和路径遍历。

## 10. Desktop 与 Web 能力矩阵

| 能力 | Desktop v0.2.0 | Web v0.2.0 | 说明 |
|---|---:|---:|---|
| 连接 CRUD/Test/Connect | 是 | 是 | Web Secret write-only |
| SQL/Schema/Table Data | 是 | 是 | P0 核心 Journey |
| Query History/Favorite | 是 | 是 | Web 按 workspace/actor 隔离 |
| AI Chat/NL2SQL | 是 | 是 | Secret 服务端保存 |
| Workflow CRUD/Run/Schedule | 是 | 是 | Web Worker 持续运行 |
| Dashboard/Monitor | 是 | 是 | Web 使用站内通知/Webhook |
| SQL Audit | 是 | 是 | 同一 AuditEngine |
| 长期记忆/知识库 | 是 | 是 | Web 有 workspace ACL |
| Data Sync/Data Transfer | 是 | 否 | v0.2.x 评估 |
| Schema Diff | 是 | 否 | v0.2.x 评估 |
| DB Backup/Restore | 是 | 否 | 服务器文件与高风险权限 |
| 本地 UI Extension | 是 | 否 | Web 安全模型另行设计 |
| 本地 context directory | 是 | 否 | Web 使用上传/挂载知识源 |
| MCP stdio Server/Client | 是 | 否 | Web 后续考虑远程 transport |
| 托盘、更新器、原生菜单 | 是 | 否 | Desktop-only |

## 11. API 与事件契约

### 11.1 API 基础规范

- Base path：/api/v1
- Content type：application/json
- 时间：UTC RFC 3339
- ID：服务端生成 UUID，不接受客户端伪造 owner/workspace/actor
- 分页：cursor-based；短列表可使用受限 limit
- 所有响应包含 requestId
- DTO 从 Rust schema 生成 TypeScript，禁止手工维护重复接口

建议资源：

    /api/v1/auth/*
    /api/v1/users/*
    /api/v1/workspaces/*
    /api/v1/connections/*
    /api/v1/db-sessions/*
    /api/v1/commands/*
    /api/v1/query-jobs/*
    /api/v1/schema/*
    /api/v1/workflows/*
    /api/v1/workflow-runs/*
    /api/v1/dashboards/*
    /api/v1/widget-runs/*
    /api/v1/ai/*
    /api/v1/audit-events/*
    /api/v1/events

### 11.2 结构化错误

    {
      "error": {
        "code": "WORKSPACE_FORBIDDEN",
        "message": "You do not have access to this workspace",
        "details": {},
        "requestId": "...",
        "retryable": false
      }
    }

错误 code 稳定，message 可本地化。客户端不得解析 message 判断逻辑。

### 11.3 SSE 事件

统一 envelope：

    {
      "eventId": "...",
      "type": "query.chunk",
      "requestId": "...",
      "workspaceId": "...",
      "occurredAt": "...",
      "payload": {}
    }

必须支持：

- Last-Event-ID 短窗口重连。
- 心跳和连接过期。
- 工作区切换时取消旧订阅。
- 服务端按当前授权过滤事件。
- Job 最终状态可通过 GET 查询，不能只依赖瞬时事件。

v0.2.0 优先 REST + SSE，不为双向通信引入 WebSocket。

### 11.4 幂等与并发控制

- create/update 支持 Idempotency-Key 或稳定 requestId。
- Workflow run、Dashboard refresh 和危险 command 防止浏览器重试造成重复执行。
- 可编辑资源包含 version/updatedAt，冲突返回 409 CONFLICT，不静默覆盖。

## 12. 后端架构要求

### 12.1 Rust 包边界

建议新增：

    packages/core/                 # crate: datazen-core
      domain / services / runtime / ports

    packages/server/               # crate/bin: datazen-server
      http / auth / middleware / sse / workers / mysql-adapter

    packages/persistence-api/      # Core 依赖的 Repository/Unit-of-Work 契约
    packages/persistence-sqlite/   # Desktop 实现
    packages/persistence-mysql/    # Web 实现

    src-tauri/                     # Desktop adapter
      Tauri commands / native UI / tray / updater / keychain

datazen-core 禁止依赖 tauri、HTTP framework、Cookie、桌面插件和窗口类型。Core 不负责认证，只接收由 Adapter 建立的执行上下文；Web 注入已认证用户，Desktop 注入固定 LocalDesktop context，不引入登录、Session 或 RBAC。

迁移不是一次性移动全部代码：先完成 Connection + Query 纵切，再迁移 Workflow、Dashboard、AI。每个纵切必须证明 Tauri 和 HTTP 调用同一个 service。

### 12.2 Core Ports

核心至少抽象：

- ExecutionContext / 可选 Authorization（Web 用户需要，Desktop 使用 LocalDesktop）
- PersistenceProvider / UnitOfWork
- WorkspaceRepository
- ConnectionRepository / SecretStore
- SettingsRepository
- HistoryRepository
- WorkflowRepository
- DashboardRepository
- AuditSink
- EventSink
- Clock / IdGenerator

Desktop adapter 使用本地默认 identity/workspace；Server adapter 使用真实 user/workspace context。

### 12.3 AppState 拆分

- CoreState：Driver、ConnectionManager、SchemaCache、Workflow、AI、Monitor/Audit。
- DesktopState：Tauri handle、窗口、dialog、tray、updater、keychain。
- ServerState：Auth、Session、MySQL Persistence、HTTP Event Hub、Worker。

## 13. 前端架构要求

### 13.1 PlatformClient

建议目录：

    src/platform/
      client.ts                 # 领域接口
      runtime.ts                # 选择 desktop/web transport
      tauri/                    # invoke/listen adapter
      http/                     # fetch/SSE adapter
      capabilities.ts           # 平台能力声明

迁移完成后：

- src/commands 保留领域入口，但内部只委托 PlatformClient。
- 除 src/platform/tauri 和 desktop-only 模块外，禁止 import @tauri-apps/*。
- 增加 CI 守护依赖边界。
- Zustand Store 只依赖领域接口和统一事件，不判断 __TAURI_INTERNALS__。

### 13.2 Capability-driven UI

统一 capability 至少包含：

    nativeWindows
    nativeDialogs
    localFilesystem
    systemTray
    updater
    runtimeExtensions
    scheduledWorkers
    webAuth

UI 按 capability 呈现能力，不散落 isTauri 分支。

### 13.3 路由与窗口

- Web 主功能使用同一 SPA route，不依赖 popup。
- 桌面子窗口可保留，但页面组件必须能在普通 route 容器中运行。
- Web 刷新后恢复 workspace、导航位置和编辑器草稿；不恢复 result rows 或 dbSessionId。

## 14. 数据模型与迁移

### 14.1 Server 基础表

- users
- auth_sessions
- workspaces
- workspace_memberships
- connections
- connection_secrets
- workspace_settings
- user_preferences
- workflows / workflow_runs
- dashboards / widgets / widget_runs
- audit_events
- 后续知识库和告警状态表

所有业务表包含 workspace_id；用户操作记录 actor_user_id。外键和唯一约束必须包含工作区边界，避免仅靠应用代码隔离。

### 14.2 双持久化策略

v0.2.0 的业务持久化层必须提供统一抽象接口，Core 和领域服务不得直接依赖 rusqlite、sqlx、SQLite 或 MySQL 方言。至少提供两套实现：

- Desktop：SQLite，保持本地离线、单用户和现有数据目录升级能力；启用 WAL、busy timeout、短事务和 migration 前一致性备份。
- Web：MySQL 8.0+，使用 InnoDB、utf8mb4、UTC 时间和独立连接池；连接信息通过 DATAZEN_DATABASE_URL/Secret 注入。
- 两套实现共享同一组 Repository/Unit-of-Work 契约测试，保证 workspace 隔离、事务边界、唯一约束、分页和审计写入语义一致。
- migration 使用相同逻辑版本号，但允许 SQLite/MySQL 各自维护方言 SQL；禁止运行时把一套 migration SQL 自动翻译为另一种方言。
- Core 的跨表原子操作通过 Unit-of-Work 接口表达，不向领域层暴露具体数据库 transaction 类型。

PostgreSQL 元数据库实现不进入 v0.2.0；后续可在不修改 Core 服务的前提下新增适配器。

### 14.3 Desktop 兼容

- Desktop Standalone 保持现有数据目录和升级路径。
- Desktop 可视为隐式 local workspace，但 UI 不显示团队管理。
- Web Server 使用独立 data directory，不读取桌面 OS 用户目录。
- Desktop 数据导入 Web 进入 P1；P0 只保证桌面升级不丢数据。

## 15. Secret 与加密设计

### 15.1 Server Master Key

- Server 不依赖 OS Keychain。
- master key 通过容器 secret、环境变量或只读 secret file 提供。
- 已有 encrypted data 但 key 缺失/错误时 fail closed。
- master key 不写入日志、健康检查、API 或备份明文。

### 15.2 Secret 类型

以下字段 write-only、AES-256-GCM 加密：

- 数据库密码、SSH 密码/私钥
- AI API Key
- Webhook Secret
- 后续 SMTP/OAuth Secret

加密绑定 AAD（workspace、secret type、resource ID），避免密文跨对象搬移。

### 15.3 日志脱敏

日志和审计默认不得包含：

- 密码、Key、Cookie、Authorization/CSRF token
- 完整连接串中的 Secret
- 查询结果行
- 未经策略允许的完整 SQL literal

## 16. Web 安全要求

### 16.1 必须实现

- Secure + HttpOnly + SameSite Cookie。
- CSRF token 或同源 double-submit 防护。
- 严格 CORS allowlist，默认只允许同源。
- Content Security Policy，禁止任意 inline script。
- 登录、连接测试、AI、导出和高成本 query 的速率/并发限制。
- 上传文件扩展名、MIME、大小、压缩炸弹和路径遍历防护。
- SSRF 防护：Webhook、AI endpoint、自定义 URL 的协议、IP 范围与重定向策略。
- Host Header、trusted proxy 和 secure cookie 配置验证。
- 所有 object lookup 使用 workspace-scoped query，防止 IDOR。
- Session fixation、注销、角色撤销测试。

### 16.2 安全默认值

- 首次启动无弱默认密码。
- Viewer 不能执行 SQL。
- Custom AI endpoint 和 Webhook 默认禁止 loopback/link-local/cloud metadata；管理员可配置内网 allowlist。
- Server 不向普通 Web 用户暴露任意文件读取、子进程或本地 MCP stdio。
- API 错误不返回堆栈、绝对路径和内部 SQL。

## 17. 非功能需求

### 17.1 性能

- 工作台首屏：同区域网络 p95 小于 2.5 秒。
- 普通 API（不含外部 DB/LLM）：p95 小于 300ms。
- SSE 事件产生到浏览器渲染：p95 小于 1 秒。
- 单实例目标：20 名并发活跃用户、50 个 dbSession、50 个 interval Widget。
- Query hard limit：50,000 行或 32 MiB。

### 17.2 可靠性

- Server 非计划重启后 5 分钟内恢复 Monitor/Workflow schedule。
- 所有后台 Job 有 terminal state。
- SSE 断线不导致 Job 重复创建。
- Webhook 使用 outbox/retry，不在业务事务中同步等待。
- RC 前完成备份恢复演练。

### 17.3 可用性与兼容性

- 支持当前及前一个主要版本的 Chrome、Edge、Firefox、Safari。
- 最小有效宽度 1024px；移动端不进入 v0.2.0。
- Web UI 支持现有 i18n；开发期只新增 en.ts 与可选 zh-CN.ts key。

### 17.4 可观测性

- 结构化日志包含 requestId、actorId、workspaceId、route、latency、status。
- 指标覆盖 HTTP latency/error、登录失败、Session、DB pool、Job queue、SSE client、Monitor lag。
- /health/ready 检查 migration、Store 和 Worker，不主动测试所有外部数据库。

## 18. 测试策略

### 18.1 测试层级

- datazen-core 单元测试：无 Tauri/HTTP，使用 in-memory repository 和 mock identity。
- Persistence contract：同一组契约分别运行 SQLite 与 MySQL adapter，覆盖事务、约束、分页、迁移和审计。
- Server integration：HTTP、Cookie/CSRF、RBAC、workspace isolation、SSE、migration。
- Frontend unit：Store 分别使用 fake TauriTransport 和 HttpTransport contract。
- Web E2E：沿用 WebdriverIO，新增 browser runner。
- Desktop E2E：现有 Host 路径继续执行。
- Driver tests：方言与专属行为继续放在 packages/drivers/<id>/。

### 18.2 必须覆盖的安全测试

- 替换 workspaceId、connectionId、workflowId、dashboardId 的横向越权。
- Viewer 直接调用执行 API。
- 被移出工作区后的 SSE 和 dbSession。
- CSRF、Cookie、Session fixation、暴力登录限流。
- Secret API 返回、日志和错误脱敏。
- 上传路径遍历、压缩炸弹、MIME 欺骗。
- SSRF：loopback、link-local、metadata IP、重定向。

### 18.3 Web P0 E2E Journey

1. Bootstrap → 管理员 → 工作区。
2. 登录 → 切换工作区 → 退出。
3. 连接 → Schema → Query → Cancel。
4. Editor 与 Viewer 权限差异。
5. Workflow 创建 → 执行 → 历史。
6. AI stream → tool/AskQuestion → cancel。
7. Dashboard interval → 关闭浏览器 → 重新登录查看新 run。
8. 移出工作区后立即失去资源、事件和 session。
9. Server restart → Monitor 恢复。

## 19. 发布阶段

### Phase 0：架构与契约（第 1–2 周）

- 冻结 scope、角色、workspace 和 API conventions。
- 建立 datazen-core、Server、PlatformClient skeleton。
- 完成 Connection + Query vertical slice RFC 和威胁模型。

退出条件：同一 query service 可被 Tauri test adapter 与 HTTP integration test 调用。

### Phase 1：内部 Alpha（第 3–7 周）

- Bootstrap、登录、工作区、连接、Schema、Query、SSE。
- Docker 开发部署、SQLite/MySQL 双 migration、Repository 契约测试、Server master key。
- Desktop 迁移 Connection + Query 到 PlatformClient。

退出条件：内部用户可只通过浏览器完成连接和查询；无 P0 隔离漏洞。

### Phase 2：Feature Beta（第 8–13 周）

- Workflow、AI、Dashboard API 化。
- Server Worker、SQL Audit context、通知中心。
- Settings 分层和 RBAC 管理 UI。

退出条件：五条核心 Journey 可用，浏览器关闭后 Dashboard 继续运行。

### Phase 3：Hardening / RC（第 14–20 周）

- 安全评审、迁移、备份恢复、性能、浏览器兼容、无障碍。
- Web + Desktop 全量回归。
- 部署、升级、反向代理、Secret 管理和故障排查文档。

退出条件：全部 Release Gates 通过。

## 20. Release Gates

以下任一 P0 未通过，不发布 v0.2.0 Stable：

### 架构

- Core 不依赖 Tauri/HTTP。
- Connection、Query、Workflow、AI、Dashboard 不存在 Desktop/Web 双业务实现。
- 非 platform/desktop-only 前端模块不直接 import @tauri-apps/*。

### 安全

- 工作区横向越权测试为零失败。
- Secret 不通过 API、日志、错误、备份明文泄露。
- Cookie/CSRF/CORS/SSRF/上传安全测试通过。
- Viewer 无法直接或间接执行任意 SQL。

### 数据

- Desktop v0.1 → v0.2 升级不丢连接、历史、Workflow 和 Dashboard。
- Desktop SQLite migration 失败后可从自动备份恢复；Web MySQL migration 要求升级前存在可验证的恢复点，并提供对应 runbook。
- Desktop 使用 SQLite；Web 使用 MySQL；两套 adapter 的逻辑 migration 版本一致。
- SQL 审计保留 submitted/effective SQL 的 hash、改写链和最终 outcome；全文按策略加密保存。
- master key 错误时 fail closed。

### 可靠性

- Server 重启后 Monitor/Workflow 调度恢复。
- SSE 重连不重复执行 Job。
- Query cancel、timeout 和 session cleanup 有确定性测试。

### 产品

- Bootstrap、Auth、Connection/Query、Workflow、AI、Dashboard、RBAC P0 Journey 全通过。
- 不支持的桌面能力在 Web 明确隐藏，不存在死入口。
- 部署、升级、备份、反代 HTTPS 文档可由非开发人员复现。

## 21. 成功指标

v0.2.0 发布后四周观察：

- 启动容器到首次成功查询的中位时间小于 15 分钟。
- Web 核心 Journey 无崩溃完成率大于 95%。
- 连接/查询服务端 5xx 率小于 0.5%（排除外部数据库错误）。
- 100% DB 操作拥有 actor/workspace/audit context。
- 0 个已确认跨工作区或 Secret 泄露问题。
- Dashboard 后台调度准时率大于 99%（30 秒窗口，不含外部 DB 超时）。

## 22. 风险与控制

| 风险 | 等级 | 控制 |
|---|---|---|
| Web 范围膨胀为 SaaS | 很高 | 锁定单节点自托管；SSO、计费、HA 排除。 |
| Tauri 耦合迁移被低估 | 高 | 先做 Query vertical slice；完成后删除直连 IPC。 |
| 多用户模型污染桌面 | 高 | Core 使用 repository/identity port；Desktop 使用隐式 local workspace。 |
| SQLite/MySQL 实现语义漂移 | 高 | 统一 Persistence API、相同逻辑 migration 版本、双适配器契约测试与升级 fixture。 |
| 浏览器暴露高风险操作 | 很高 | Server RBAC + SQL Audit + Secret write-only。 |
| Extension/文件系统攻击面 | 很高 | Web v0.2.0 禁用 runtime extension 和任意 server path。 |
| SSE 断线状态错乱 | 中 | Job 可查询、Last-Event-ID、幂等 request、terminal state。 |

## 23. 待决策项

Phase 0 结束前确定：

1. Docker 是否同时提供 amd64 与 arm64；建议两者均提供。
2. Dashboard 是否增加 draft/published；建议增加，Viewer 仅看 published。
3. master key 是否首发接 Vault/KMS；建议只做 secret file，保留 SecretStore port。
4. Web P0 是否包含 XLSX 大结果导出；建议降为 P1。
5. Custom AI Endpoint/Webhook 是否允许内网；建议默认禁止特殊地址，由实例管理员配置 allowlist。
6. Viewer 是否允许手工刷新 Dashboard；建议不允许。

## 24. Definition of Done

单个 Web 平台功能只有同时满足以下条件才算完成：

- 有服务端授权检查，不依赖前端隐藏。
- 有 workspace-scoped repository query。
- 有结构化错误 code 和 requestId。
- 长任务有进度、取消、terminal state 和重连行为。
- Secret/日志/审计符合脱敏规则。
- Rust 单元/集成、前端单元和 Web E2E 同步更新。
- Desktop 对应 Journey 回归通过。
- 英文 i18n key 完成，可选同步中文；不批量修改其他语言。
- 架构、部署或用户行为变化已更新文档。
