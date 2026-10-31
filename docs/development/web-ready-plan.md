# Web Ready 完整计划：从当前基线到团队 Web 交付

> **状态：已确认的计划（拼装既有切片 + 补齐无既有文档覆盖的缺口），代码尚未按本文改动。**
> 立项日期：2026-10-31。基线：`review/code-organization-2026-10` @ `08a92d5`。
>
> 「web ready」分两级门槛，本文同时管理两者：
>
> - **可开工（ready-to-build）**：共享层形态中立、端口完整、契约可测——P7 开工后不因架构返工。
> - **已交付（delivered）**：[平台计划 §11 P7](platform-development-plan.md#11-p7单实例团队-web-服务) 退出门槛达成。
>
> 本文不复制既有方案的内容，只做三件事：**核对现状、列全差距、排定顺序**。切片编号 **WR-A…WR-G**（Web Ready 轨道），
> 与 F（宿主边界）、M（跨形态收敛）、P（平台产品 P0–P10）、D（DDD）、R（闸门）互不冲突。
> 既有文档保持权威：P7 交付项以[平台计划](platform-development-plan.md)为准；M 系列刀法以
> [边界方案 §8](host-boundary-followups.md#8-跨形态一致性收敛m-系列)为准；端口适配差异以
> [共享边界 §4.7](../architecture/platform/shared-boundaries-and-ports.md#47-三种交付形态的适配差异)为准；
> server 形态与认证以[团队服务设计](../architecture/platform/team-server-and-auth.md)为准。

## 1. 现状基线（2026-10-31 @ `08a92d5` 已核实）

| # | 层 | 现状 | web ready 判定 |
|---|---|---|---|
| 1 | Rust 内核 `modules` | 无 `tauri`/`axum` 依赖；F-14 闭包断言禁宿主 crate 与 web 框架 | ✅ 形态中立达成 |
| 2 | 端口矩阵 | 共享边界 §4.7 已定义桌面 / server 单进程 / 多 worker 三形态差异（`ProfileRepository`、`EventSink`、`SessionDirectory`、`BudgetCoordinator` 等） | ✅ 设计就绪；服务端适配器零实现 |
| 3 | MCP 共享层 | 策略层 751 行在 `modules/src/mcp`；13 个工具 + 有状态会话协议 + Resources/Prompts + MCP Client 约 4,200 行在桌面宿主 | ❌ M1 / M2 未动 |
| 4 | AI 工具面 | 工具定义/循环/守卫/schema 上下文全部共享（`modules/src/ai`）；providers 是 modules 内 HTTP 客户端；缺口 = MCP 工具面直绑桌面 `McpClientManager` | ⚠️ 差 M3 |
| 5 | runtime 读取路径 | 同一能力两条路径：AI 走 `ResourceExecutionHost` 端口，MCP 直连桌面 `RuntimeCommandSource` | ❌ 差 M4 |
| 6 | 桌面 command 层 | `commands/` 169 个 pub fn、IPC 面 215 簇；薄壳化重构已立项未完成 | ⚠️ WR-A2 |
| 7 | 前端 | 形态拆分 F1–F5 落地（编译期同形、域客户端契约测试、web stub 全 throw、`platform-extension-playbook`）；F6 收口中（workspace 协议已建、生产/测试树 `@tauri-apps` 归零、R6/R7 shim 焚烧完成） | ⚠️ F6 收口 = WR-A1 |
| 8 | web 宿主 | 不存在。`apps/` 只有 `desktop`；前端 `src/platform/web/` 只有全 throw transport stub（`CapabilityUnsupported`，编译期同形验证用） | ❌ WR-E |
| 9 | server 详细设计 | `team-server-and-auth.md`（server crate 形态、中间件链、OIDC、CSRF、授权模型）、`persistence-model.md`、`team-service-operations.md` 已就绪 | ✅ 设计就绪 |
| 10 | 跨端能力 | 导出 web 半边已示范（`createBlobTablesExporter`，Blob 下载）；Wapp 沙箱 iframe + `datazen://` 双源 CSP（桌面形态）；Tunnel 实现在 `modules` tunnel feature（共用）；AI providers 为 modules 内 HTTP 客户端 | 部分就绪，WR-F 逐项 |
| 11 | CI | 无 web 构建腿；modules 测试经桌面宿主腿编译（宿主 feature 集 = 共享 feature 集），无「无宿主独立构建」腿 | ❌ WR-C |
| 12 | P6 | 未关闭：Workflow v1 跨库 Runtime 路由、授权调度、真实驱动旅程验收、CM-53 F 层 e2e 未跑 | ⚠️ P7 的硬依赖 |

## 2. 轨道总览与依赖

```mermaid
flowchart TD
    A1[WR-A1 F6 前端收口] --> C4[WR-C4 web 构建进 CI]
    A2[WR-A2 command 薄壳化] --> C2[WR-C2 HTTP 契约面]
    A4[WR-A4 P6 关闭] --> D1[WR-D server host]
    B1[WR-B M4 读取统一] --> B3[WR-B M1 工具归位]
    B2[WR-B M3 broker 端口] --> F1[WR-F1 AI web 装配]
    B3 --> B4[WR-B M2 会话归位]
    B3 --> F2[WR-F2 MCP Server web]
    B2 --> F3[WR-F3 MCP Client web]
    C1[WR-C1 无宿主构建腿] --> D1
    C2 --> C3[WR-C3 同用例双传输] --> D1
    D1 --> D2[WR-D2 服务端仓储] --> D4[WR-D4 执行接线]
    D1 --> D3[WR-D3 认证授权]
    D4 --> D5[WR-D5 SSE EventSink] --> E2
    D3 --> E1[WR-E apps/web]
    E1 --> E2[WR-E2 WebClient transport]
    E1 --> E3[WR-E3 PlatformServices web]
    D6[WR-D6 配额/任务中心] --> G1
    F4[WR-F4 Wapp 独立 origin] --> G1[WR-G 验收发布]
    E2 --> G1
    E3 --> G1
```

关键路径：**WR-B（M 系列）与 WR-A4（P6 关闭）并行 → WR-C（契约/CI）→ WR-D（server host）→ WR-E（web 前端）→ WR-G（验收）**。
WR-F（能力落地）在 WR-D/WR-E 期间交叉插入，单项依赖见各节。

## 3. WR-A 收口轨（既有工作，web ready 的输入）

| 切片 | 内容 | 对 web ready 的意义 | 验收 |
|---|---|---|---|
| **WR-A1** | F6 前端包拆分收口：`apps/desktop` 宿主 app + `packages/frontend` 完整 workspace 包、目录迁移、R5 闸门（合法消费者只有 `apps/*`） | `apps/web` 是 F6 目标拓扑的第一位消费者；F6 不收口则 web app 无处落 | 形态拆分计划 §5.6 原验收 |
| **WR-A2** | host command 薄壳化（已立项）：command 层只做请求接线与转发，编排归 modules application；起点 `commands/ai/config.rs` | HTTP 端点镜像的是**用例**而非 IPC 命令；command 薄壳化完成后，server 的 HTTP handler 与桌面 command 是同一用例的两个薄绑定，不需要二次编排 | 每域验收：宿主 command 无业务编排；IPC 载荷不变 |
| **WR-A3** | P2 驱动注入静态化、P3 feature 图修剪 | P5 已裁决「Web 宿主重新立项时一并做」——即 WR-D1 的输入；P2/P3 降低 server 构建面的脆弱性 | 各自方案验收 |
| **WR-A4** | P6 关闭：Workflow v1 跨库 Runtime 路由、授权调度、真实驱动旅程验收、CM-53 F 层 e2e（`pnpm e2e` 实测回写） | 平台计划依赖图 P6 → P7 的硬边；AI/MCP/Wapp 的 GUI 外入口资源语义先在此关闭 | 平台计划 §10 退出门槛 |

## 4. WR-B 一致性轨（= M 系列，刀法引用边界方案 §8）

原则（2026-10-31 用户指令）：**AI 的 tool 是 web 与 desktop 共同能力——实现方式可以不同，基础能力必须一致；MCP 差异只允许是 Server 形态（stdio 子进程 / 内嵌 duplex / streamable HTTP + 服务端身份），其余完全一致；runtime 读取路径不允许「同一能力、两条适配」。**

| 切片 | 内容 | 引用 |
|---|---|---|
| **WR-B1 = M4** | 元数据读取统一：MCP 的 `runtime_schema.rs` 改走共享用例 + `ResourceExecutionHost` 端口 | 边界方案 §8.3 |
| **WR-B2 = M3** | AI 的 MCP 工具面端口化：`McpToolBroker` 消费者端口，`commands/ai/chat.rs` 两函数改走端口；桌面实现 = 现有 `McpClientManager` | 边界方案 §8.3 |
| **WR-B3 = M1** | MCP 工具分发归位：`modules/src/mcp/application/tools/`（13 工具定义 + 分发 + 无状态用例），宿主 `tools.rs` 变 rmcp 薄绑定 | 边界方案 §8.3 |
| **WR-B4 = M2** | 有状态会话语义归位：TTL、归属校验、cleanup 退避策略进 `modules/src/mcp/application/session/` | 边界方案 §8.3 |

顺序与理由沿边界方案：M4 先行消除双路径并给 M1 铺路；M3 独立且小；M1 主体；M2 可后置。

## 5. WR-C 契约与 CI 轨（新增缺口，无既有文档覆盖）

| 切片 | 内容 | 验收 |
|---|---|---|
| **WR-C1（核实后收窄，无需新刀）** | ~~modules 无宿主独立构建腿~~ 立项时误判：`pnpm test:platform-crates`（`scripts/run-platform-crate-tests.mjs`，CI rust job 既有步骤）已独立编译并测试 modules——无选择器腿用 27/29 显式 feature 清单（与宿主腿对齐，rust-cache 共享），另有 `FEATURE_TESTS` 分 feature profile 腿（settings/ai/connection+execution/named_views/monitor/dashboard/tunnel）。服务器 crate 落地后由同一腿自动覆盖（`TESTED_LAYERS` 已含 `'server'`，按目录发现，无需改 CI） | 已达成（既有）；本文记录核实修正 |
| **WR-C2（已完成 2026-10-31）** | HTTP API 契约面定义。**已裁决**：HTTP 面镜像 `packages/backend-client` 的域客户端面（十个域、43 方法，42 上 HTTP、1 排除），**不镜像 IPC 命令面**。完整契约见 [http-api-contract](../../architecture/platform/http-api-contract.md)：收编概要 §6.3 骨架，补齐 settings / identity / tableData（14 方法）/ driverGateway（4+1 排除）；裁决命名视图上 HTTP（服务端存储）、`DELETE` 唯一例外、流式驱动命令走 SSE、`CapabilityUnsupported` → 405/501 | 已达成：逐方法可点到路由（§4 总账）；多余端点为零（uploads 在账带收口动作）；`WEB_CAPABILITIES` 不含桌面专属项 |
| **WR-C3** | 同用例双传输测试基础设施：同一用例（组装 request → 调 application → 断言）在 IPC 与 HTTP 两绑定下跑（P7 交付项「桌面 IPC 与 HTTP 同用例测试」的前置抽离） | 新用例测试两腿同绿；形态差异只允许出现在装配层断言 |
| **WR-C4** | `apps/web` 建立后 web 构建腿进 CI（typecheck + vite build + 域契约测试） | CI 腿绿；web stub 的全 throw 语义不被绕过（能力未实现时显式 `CapabilityUnsupported`） |

## 6. WR-D server host 轨（P7 服务端本体）

形态与组装以[团队服务设计 §2](../architecture/platform/team-server-and-auth.md#2-server-crate-形态与-composition-root) 为权威（目录树、依赖闭包检查、组装顺序、进程模式）。

| 切片 | 内容 | 引用 / 验收 |
|---|---|---|
| **WR-D1（骨架已落地 2026-10-31；P5 拆出）** | `server/` crate 骨架：composition root（`bootstrap.rs`，serve | migrate | check-schema 三子命令，后两者显式拒绝待 WR-D2）、中间件链 L0 requestId + L1 字节闸（Content-Length 自研 + tower-http 流式）已上线、L2–L6 落点已记录、`ApiError` wire 复用 modules 定义 + 状态码表（§9.2 + 405/501 裁决）、依赖闭包门禁 `scripts/check-server-deps.mjs`（自测全绿）进 CI。**P5 目录语义化拆为独立切片**：仓库级目录大迁移（modules→crates/datazen 等，50+ 路径引用）必须跟 14 道门禁脚本一起机器验证，盲迁会把全仓 CI 推红，不盲随本刀 | §2.2 依赖闭包检查已上 CI；无 Tauri 依赖 ✓（沙箱无 cargo，编译验证由 CI 首跑，Cargo.lock 待首个有 cargo 的机器提交） |
| **WR-D2** | 服务端仓储适配器：`ProfileRepository` / `JobRepository` / `Audit` / `ArtifactRepository` / `TunnelRepository` / `KnownHostsRepository` / `ConnectionRepository` 的服务端 SQLite 实现 + schema migration（expand/contract，不先删字段） | 同一 domain trait、另一适配器；不复用宿主 AES 落盘实现（§4.7 既定）；migration 可核验恢复 |
| **WR-D3** | 认证授权：OIDC 登录、membership、服务端登录会话、RBAC/资源 ACL、CSRF、同源 HTTPS；授权变更与 `permissionVersion` 同事务提交 | 团队服务 §4–§6；越权/CSRF/组织隔离全部拒绝断言 |
| **WR-D4** | 执行接线：`ExecutionGateway`、`SessionDirectory`（单进程实现）、`BudgetCoordinator`（节点额度）、`SubmissionTokenIssuer`（服务端签名密钥 + `keyVersion`）、`SecretProvider`（服务端密钥管理 + secretRef 轮换）、`NetworkProvider`（出站白名单）、`IdentityResolver`（数据库侧服务身份 + 委托） | §4.7 端口表逐项「同一端口、另一适配器」；全部入口走 ExecutionGateway |
| **WR-D5** | `EventSink` SSE 适配：SSE 重订阅、分页/大小限制、Origin 校验、revision 缺口重读 | CM-55 的 W1 断言（超大 revision、乱序、重连） |
| **WR-D6** | 配额、任务中心、结果 TTL、配置版本 CAS、权限撤销传播 | 平台计划 §11 交付项 |
| **WR-D7** | 幂等提交、受控上传下载 | 平台计划 §11 交付项 |

## 7. WR-E web 前端轨

| 切片 | 内容 | 验收 |
|---|---|---|
| **WR-E1** | `apps/web` 宿主应用：vite + 路由；窗口能力替换为页面/面板（形态拆分既定形态） | 与 `apps/desktop` 同用 `@datazen/frontend`；无 `@tauri-apps` import（R5/R6 闸门天然合规） |
| **WR-E2** | `WebClient` transport：`BackendTransport` 的 HTTP/SSE 实现，替换 `src/platform/web/` 全 throw stub；域客户端（F3）零改动 | 域契约表 `domainContractTables` 测试在 HTTP 腿同绿 |
| **WR-E3** | `PlatformServices` web 实现：文件/剪贴板/对话框走浏览器原生；导出走 Blob 半边（已示范）；宿主叠加能力（`platform.host.config.get()`）web 注入空缺省 | F5 契约测试 `platformContracts` 在 web 腿绿 |
| **WR-E4** | capabilities 激活：`WEB_CAPABILITIES` 静态表（F5 已建）成为 web 装配的实际输入；updater/versionSource 等桌面专属项按表降级 | 无运行时形态探测（编译期同形原则不破） |
| **WR-E5** | 部署形态：静态资源由 server host 同源服务（SSE 缺失 Origin 有明确校验的前提） | 同源 HTTPS 部署验收 |

## 8. WR-F 能力落地轨（跨端能力，逐项「能力一致、仅形态差异」）

| 切片 | 内容 | 形态差异（允许的部分） | 验收 |
|---|---|---|---|
| **WR-F1** | AI 落地 web：providers 已是 modules 内 HTTP 客户端（服务端可跑）；装配点 = `AiRequestHost` web 实现、`McpToolBroker` web 实现（WR-B2 端口）、AI 密钥经 `SecretProvider` | 密钥存放（钥匙串 → 服务端密钥管理） | AI 工具循环测试不感知形态（M3 验收的延伸） |
| **WR-F2** | MCP Server web 形态：M1/M2 完成后宿主只剩 rmcp 传输绑定，web 侧实现 streamable HTTP（或内嵌 duplex）传输 + 服务端身份 | **仅 Server 形态**：stdio 子进程 → streamable HTTP | golden 契约 `mcp_external_contract.json` 两形态逐字节一致 |
| **WR-F3** | MCP Client web：`McpClientManager` 端口的 HTTP/SSE 客户端实现；stdio spawn 白名单、token 校验留桌面 | 仅传输与进程模型 | 同一 broker 端口两形态测试 |
| **WR-F4** | Wapp web：独立真实 origin + 可嵌入 CSP + 真实 iframe 握手（P7 上线前冻结验收项）；`wappBridge` postMessage 协议不变；`datazen://` 资产服务替换为真实 origin 静态服务 | 仅资产服务协议（自定义 scheme → 真实 origin）；CSP 双源约束随之简化为单源 | 桥契约测试两形态同绿；审计（`wapp_audit_log`）等价 |
| **WR-F5** | Tunnel：实现已在 `modules` tunnel feature 共用；web 宿主只注入存储根、凭据、监听点 | 仅注入来源 | 桌面/Web 同隧道语义测试 |
| **WR-F6** | 导出/备份/恢复 web：服务端 `ArtifactRepository` + 受控下载；前端 Blob 下载半边已示范 | 仅字节出口（本机路径 → 受控下载） | 产物越权拒绝断言 |
| **WR-F7** | 驱动执行：全部服务端执行；浏览器零数据库凭据；`NetworkProvider` 出站网络政策 | 无（执行位置由架构决定，非形态差异） | P7 门槛「浏览器无需数据库凭据」 |

## 9. WR-G 验收与发布轨

| 切片 | 内容 | 引用 |
|---|---|---|
| **WR-G1** | CM W1 断言：CM-05～07、38～40、53～56、59、60 单实例部分及 CM-63、68、70、72 的 W1 断言 | 平台计划 §11 退出门槛 |
| **WR-G2** | 上线前冻结验收：授权同事务、DDL 与历史登记同事务、expand schema 兼容、授权可见性先于会话存活错误、SSE Origin 校验、Wapp 独立 origin/CSP/iframe 握手——每项配浏览器/管理库故障测试，不以 fake 替代 | 平台计划 §11 冻结清单 |
| **WR-G3** | 部署运维：发布镜像、管理数据库、SecretProvider、网络可达性、备份恢复（不自动重放未知外部写入）、TLS、资源额度、drain 回退 | `team-service-operations.md` |
| **WR-G4** | P8 衔接：桌面团队客户端（backendId、版本握手、per-backend client）不在本文展开，P7 交付后按平台计划 §12 | 平台计划 §12 |

## 10. 推荐执行顺序

1. **并行启动（当前即可）**：WR-B1（M4，最小刀）∥ WR-A2（command 薄壳化，逐域推进）∥ WR-A1（F6 收口）∥ WR-A4（P6 关闭项）。
2. **第二波**：WR-B2（M3）→ WR-B3（M1）→ WR-B4（M2）；同期 WR-C1（无宿主构建腿）、WR-C2（HTTP 契约面决策与定义）。
3. **第三波**：WR-D1（server 骨架 + P5 目录语义化）→ WR-D2/D3/D4 并行 → WR-D5/D6/D7；WR-C3（同用例双传输）随 D1 落地。
4. **第四波**：WR-E1 → WR-E2/E3/E4 并行；WR-F 各切片在对应端口就绪后交叉插入（F1/F3 依赖 B2；F2 依赖 B3+B4；F4 随 E1）。
5. **收口**：WR-C4（web CI 腿，E1 建立即插）→ WR-G1–G3 验收发布。

**可开工门槛的判定**：WR-A（1/2/4）+ WR-B（全四刀）+ WR-C（1/2/3）完成即视为 ready-to-build——此后 P7 主体（WR-D/E/F/G）不再有架构性返工风险。

## 11. 进度回写区

> 纪律：每项切片完成 → 验证 → 提交推送 → 在此登记（日期、提交、一句话事实）。

- **2026-10-31 — 测试分层政策 + commands 层逐文件总账 + B0 迁移落地**：用户新指令三条（单元测试测 modules 不测 command 层；何场景必须直测 command 层；commands 层逐文件 review）。成文 [commands-layer-review.md](commands-layer-review.md)：§1 四类合法宿主测试（C1 IPC 线格式 / C2 装配守卫 / C3 宿主资源绑定 / C4 适配器烟测）+ 判据「换 axum 宿主还能原样过 → 位置错了」；§3 全部 150 文件逐个裁决（保留/守卫/瘦身/测试迁移/随刀下沉）；测量基线：51,793 行中测试相关 24,751 行（48%），`_impl` 残留 115 个。B0 执行：`filter_validation_tests`（501 行）整迁 `modules/src/migration/data_sync/`；`backup_tests` 纯逻辑 5 测迁 `modules/src/backup/options_tests.rs`；`ai/tests.rs` 纯逻辑 25 测迁 `modules/src/ai/`（response_tests 18 + tools_tests 4 + api 内联 1 + single_turn 内联 2，另删 1 个与 modules 既有覆盖重复的 provider_defaults 测试）——共 1,355 行测试归位，桌面 commands 层净减约 1,050 行；顺手消灭 `sync/mod.rs` 的 filter_validation 转发 re-export（使用方 `sync/host/mod.rs` 直连源头）。静态验证脚本全过（沙箱无 cargo，真实验证在用户机器）。B1–B4 批次见 review 文档 §4。
- **2026-10-31 — WR-D2 第二切片落地：`KnownHostsRepository` 服务端适配器**：v2 迁移加 `known_hosts` 表（复合主键 `(organization_id, host_key)`——端口无鉴权维度，组织隔离由「每组织一个仓储实例」实现，列是实例边界、端口形状不动）；`ServerKnownHosts` 钉住端口两条特殊语义：`load` 任何失败降级为空集合（TOFU：坏库不阻断隧道启动，warn 入日志）、`save` 单事务全表替换且失败如实报 `BackendUnavailable`。server 的 datazen features 增至 `connection,execution,tunnel`（端口在 tunnel::domain；WR-F5 本就要求服务端共用隧道机制）。4 个新测试（空默认、往返、全表替换、每组织实例隔离）。**推迟并记录理由**：`ConnectionRepository` 与 `TunnelRepository` 的端口语义是内存明文凭据（`get` 返回全量定义），服务端适配器的存储形状依赖 WR-D4 的 `SecretProvider` 裁决，不强行定形。
- **2026-10-31 — WR-D2 第一切片落地：管理库 + `ProfileRepository` 服务端适配器**：`server/src/storage/` 新增 `AdminStore`（SQLite，`Mutex<Connection>` + `spawn_blocking` 异步边界，与桌面 `AppDb` 同款并发模型）、expand-only 迁移执行器（`admin_schema_version` 版本表、逐迁移单事务、幂等；contract 步骤显式不执行，待 WR-G3 备份核验）与 v1 基线 `connection_profile` 表（复合主键 `(organization_id, connection_id)` 把 CM-05 落进存储层；`secret_ref` 只存引用，明文永不入行）；`ServerProfiles` 实现 `ProfileRepository` + `ProfileReader`，语义与桌面 `DesktopProfiles` 逐方法对齐（CAS 只承诺 `CasConflict`、create 幂等重放返回既有行、disable 递增 config_revision、credential 轮换只动凭据版本、敏感公开选项同词表拒绝）。`migrate`/`check-schema` 子命令从 fail-closed 拒绝变真实实现（落后退出码 3、超前退出码 4）；`serve` 组装根插入 §2.3 的 D 步（打开管理库 + 版本检查，不自动迁移）。server 的 datazen features 从零点亮为 `connection,execution`（CI FEATURE_TESTS 钉的最小有效组合）。9 个行为钉子测试 + 2 个迁移执行器测试。**裁决差异登记**：team-server §10.1 与系统概要 §6.4 推荐生产首版 PostgreSQL 管理库，本计划（用户已确认）裁决首版 SQLite、PostgreSQL 为后续生产化切片——差异已写进 `storage/mod.rs` 模块文档，trait 语义不动，届时只新增 adapter。**遗留**：`ConnectionRepository`/`JobRepository`/`Audit`/`ArtifactRepository`/`TunnelRepository` 适配器按同刀法后续切片（Connection/Tunnel 待 WR-D4）；server 依赖闭包门禁对 rusqlite/chrono/async-trait 放行（denylist 只禁宿主/UI crate）。
- **2026-10-31 — WR-A2 AI 域收口：`commands/ai` 转发层全数拆除**：`config.rs`（8 命令）、`prompts.rs`（1）、`generate.rs`（7）、`chat.rs`（1）的接线逻辑全部直接住进 `#[tauri::command]` 处理函数，24 个 `*_impl` 转发体删除（含 3 个 `#[allow(dead_code)]` 短版）；净 -107 行。命令测试改为**直调真实命令入口**——新增 `commands/ai/command_test_host.rs`（cfg(test)）：`tauri::test::mock_app()` 托管 harness 的 `AppState`（`TestAppState::into_state()` 拆出 state + keepalive），流式命令 `<R: Runtime>` 泛型化后注入 `WebviewWindow<MockRuntime>`，断言改走真实事件面（`window.listen("ai:stream-chunk")`，tauri 事件系统为纯进程内投递，已核实 2.11.5 源码）。IPC 契约零变化（命令名、载荷、`ai:config-changed`/`ai:stream-chunk` 事件、错误文案逐字保留）；`ipc_contract_guards` 参数提取适配泛型签名（新逻辑在本沙箱以脚本模拟验证全过）。遗留：`ai_chat`/`ai_generate_sql` 的编排本体（schema 管线 + 取消登记）仍在宿主命令层，待 WR-A2 后续刀下沉 modules 用例；其余域（connection 89 处、dashboard 35 处等）的 `_impl` 拆除按同刀法逐域推进。

- **2026-10-31 — WR-B1 = M4 完成**：runtime 读取路径统一。`read_target_command`/`driver_for_connection` 用例化进 `modules/src/workflow/application/runtime_reads.rs`（feature 环裁决：原计划 mcp 域不能反向依赖 workflow/ai，落点调整为 workflow + ai 两处），`connection_schema_context` 姊妹用例落 `ai/application/schema_context.rs`；宿主删 `mcp/runtime_schema.rs` 与 `RuntimeCommandSource` 的 inherent 捷径，`DataZenMcpServer` 以 `resource_host: Arc<dyn ResourceExecutionHost>` 端口字段装配；Wapp 读取同步换轨。验收达成：`mcp/` 不再引用 `RuntimeCommandSource`；MCP 错误文本逐字不变；modules 新增 6 个行为钉子测试。详见[边界方案 §8.3](host-boundary-followups.md#83-收敛切片每刀独立-pr) M4 行。
- **2026-10-31 — WR-B2 = M3 完成**：AI 的 MCP 工具面端口化。`McpToolBroker` 消费者端口 + `McpBrokeredTool` 中立结构落 `ai/application/ports.rs`，装配用例（qualified 命名、`ToolDefinition` 映射、`String` 执行契约与错误文案）落 `ai/application/mcp_tools.rs`；桌面 `AppState` 实现端口（`services/feature_ports.rs`，stdio/白名单/token 留桌面），`commands/ai/chat.rs` 两函数变薄壳，`mcp_qualified_name` 变共享规则转发。验收达成：工具循环测试不感知形态；桌面 MCP client 测试零改动；错误文案逐字保留；共享层 5 个用例测试。详见边界方案 §8.3 M3 行。
- **2026-10-31 — WR-B3 = M1 主体完成（10/13 工具）**：MCP 工具分发归位。前置刀把 `permission`/`command_policy` 迁入 execution 解开 feature 环（mcp 保持旧路径垫片）；十个无状态工具用例进 `mcp/application/tools.rs`（七个走 `ResourceExecutionHost`，list_connections 走新 `RedactedConnectionSource` 端口，workflows 二连走 `WorkflowCatalog` 端口 + 共享 executor）；错误通道中立化为 `ToolError` 两分；宿主 `tools.rs` 全部无状态函数变薄壳，`#[tool]`/JsonSchema/rmcp 错误留绑定层。会话三工具留 M2。详见边界方案 §8.3 M1 行。
- **2026-10-31 — WR-B4 = M2 完成，WR-B 轨道收口**：有状态会话映射语义（`McpSessionRegistry`/TTL/归属校验/清理登记 + 语义测试）整体迁 `mcp/application/session.rs`，宿主变垫片；cleanup worker 与 open 未交付守卫按传输驱动裁决留宿主。**M 系列四刀全部落地**。详见边界方案 §8.3 M2 行。
- **2026-10-31 — main 合并（冲突解决）**：PR #150（P2/P3/P5 轨）合入 main 后与本分支 M 系列工作分叉；唯一文本冲突 `modules/src/ai/application/ports.rs`（我们侧 M3 `McpToolBroker` vs main 侧 `AiConfigStore`）以双保留解决，双侧同改的另三文件（`chat.rs`/`ai/mod.rs`/followups 文档）逐一人工核验语义一致；合并提交 `1f325a6`。main 侧带入的 AI 配置用例下沉（`ai::api` + `AiConfigStore`）正好完成 WR-A2 的 AI 配置域起点。
- **2026-10-31 — WR-A1 状态核实（无需新刀）**：F6 机械工作全部完成——c1 目录分家、c2 workspace 协议 + alias 退役 + 源码包模式、c3 redis 集成测试归位 + R1 豁免注销；R5 闸门已上线；剩余仅退出标准验证（typecheck/全量测试/tauri build 三绿 + e2e 冒烟），属机器侧（CI/用户机器），不阻塞后续轨道。
- **2026-10-31 — WR-C2 完成**：HTTP 契约面冻结成文 [http-api-contract](../../architecture/platform/http-api-contract.md)（`docs/architecture/platform/`）。十域 43 方法总账：42 上 HTTP（41 路由，PATCH 双服务 update/disable）、1 排除（`open_base64_with_dialog` 对话框语义永不上 HTTP）；关键裁决：命名视图上 HTTP（服务端存储形态裁决，`DELETE` 唯一例外带理由）、流式驱动命令走 SSE（202+streamId）、`CapabilityUnsupported` 按 kind 分射 405/501、`artifacts/uploads` 为唯一在账待补方法端点；概要 §6.3 与 team-server §7.2 已挂指针，两表不再各自演化。
- **2026-10-31 — CI 全绿收口（fe8412d）**：WR-D1 落地过程共修四类破损——① M1 前置刀遗留的 feature 向上引用（execution 硬引 schema，`connection,execution` 腿编译失败，cfg 门控修复，行为在 schema 开启时逐字不变）；② 骨架盲写三处 Rust 修正（axum `Uri` 路径、match 字面量绑定、501 覆盖逻辑）；③ Cargo.lock 随 server 入 workspace（临时 snapshot 工作流取回 runner 算好的锁——纯增量 19 行，axum/tower/http 早已在图内；`--locked` 的 publish 工作流恢复安全）；④ platform-arch 自证 M6 适应 server/ 已存在（改走 M7 的备份覆写模式）并按注释预告把 server 入 `DEFAULT_REQUIRED_LAYERS`（F-06 从 vacuous 转武装），边界测试套件同步跟上。**附带发现记录在案**：共享词汇表 `NotFound` 的 `retryDisposition` 实际为 `safeRead`，与 team-server §9.2 表的 `never` 不一致——留给 WR-D4 逐行裁决，server 侧只断言 wire 真值不自行改写。
- **2026-10-31 — WR-D1 骨架落地**：`server/`（crate `datazen-server`）入 workspace。`bootstrap.rs` 唯一组装点（子命令 serve|migrate|check-schema，后两者 fail-closed 拒绝待 WR-D2）；中间件链 L0（requestId 透传/铸造，最外层裁决：413/500 也带 id）+ L1（Content-Length 自研闸带 `ApiError` 体 + tower-http 流式闸 + CatchPanic）上线，L2–L6 落点记录在 `http/middleware/mod.rs`；`/healthz` `/readyz` 真实、`/api/v1` 统一 501 `capabilityUnsupported`（契约 §7 映射）；错误 wire 复用 `datazen::shared::error::ApiError`（newtype 破 orphan 规则）+ `status_for` 全 31 变体映射（§9.2 未列 8 个 code 临时归 409，WR-D4 逐行确认）；门禁 `scripts/check-server-deps.mjs`（自测四案例全绿）+ `pnpm test:server-deps` + CI 步骤；集成测试 7 项钉住行为。既有门禁零改动接入：cargoWorkspace LAYERS 预置 server 路径、platform-arch F-06 随 crate 落地自动激活、platform-crates 腿自动发现。**待办**：Cargo.lock 未含新依赖（沙箱无 cargo），首个有 cargo 的机器跑任意 cargo 命令后提交锁文件；P5 目录语义化拆独立切片（盲迁风险）。
