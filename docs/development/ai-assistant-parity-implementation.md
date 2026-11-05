# DataZen AI Assistant 对齐实现文档

> 基线：`main@06a745291`  
> 分支：`feature/ai-assistant-parity`  
> 输入：`docs/development/ai-assistant-parity-prd.md`  
> 范围：PRD 的 R1–R3 全量  
> 状态文档：`docs/development/ai-assistant-parity-progress.md`  
> 缺陷汇总：`docs/development/ai-assistant-parity-test-report.md`  
> 单缺陷详情：`docs/development/ai-assistant-parity-bugs/BUG-<id>.md`

## 0. 执行规则

1. 任务按编号串行执行；前置任务未通过测试，不开始后置任务。
2. 每个任务先由 coder 实现并自测，再交给全新 tester 独立测试。
3. coder 不宣布任务完成；tester 通过后才把任务改为 `已完成`。
4. 每次任务状态变化，更新进度文档并 commit + push。
5. 每个任务至少一个代码 commit；缺陷修复 commit 必须带 bug 编号。
6. 进度文档只允许任务名和状态；测试报告只允许 bug 列表和状态；详情写入单 bug 文件。
7. 过程文档只存在于本分支；功能验收后删除，结论折叠进 `docs/architecture/` 与 `docs/features/`。
8. 不新增任何对竞品名称的引用；文档统一用“竞品”。

## 1. 现状与实现边界

- AI 共享层在 `crates/datazen/src/ai/{domain,providers,application}`；桌面宿主在 `crates/datazen-desktop`。
- AI Chat 当前只有 5 个只读元数据工具；`db_tools::query()` 已存在并被 MCP 使用。
- 前端会话 key 已支持 `connectionId`，但调用方仍传 `undefined`。
- 工具循环固定 6 轮 / 24k token / 180s / 2KB；调用方传入的 10 轮未生效。
- AppDb 当前 `SCHEMA_VERSION = 6`；本实现升级到 v7，一次性加入 AI 笔记/记忆/审计/FTS。
- 会话历史当前只在 localStorage；R2 改为 AppDb 持久化并保留 localStorage 迁移。
- MCP Server 当前 13 个工具、仅 stdio、无逐次审批/审计/HTTP。
- 本地 CLI Agent、Skills、插件工具桥、组织记忆当前不存在，按 R2/R3 新建。

## 2. 任务总览

| ID | 任务 | 依赖 | 阶段 |
|---|---|---|---|
| T01 | 校准 AI/MCP 正式文档 | — | R1 |
| T02 | 稳定连接身份与会话 key | T01 | R1 |
| T03 | Ask / Agent 模式与工具面 | T02 | R1 |
| T04 | 只读查询与采样工具 | T03 | R1 |
| T05 | 可配置工具循环与 API 重试 | T04 | R1 |
| T06 | 自动上下文快照 | T02 | R1 |
| T07 | Chat 动作、模式与 egress UI | T04,T06 | R1 |
| T08 | 安全开关真正生效 | T03,T04 | R1 |
| T09 | AppDb v7：AI 笔记/记忆/FTS/审计 | T01 | R1/R2 |
| T10 | 连接笔记、全局指令、Prompt 模板 | T09 | R1 |
| T11 | Memory 领域、检索、注入 | T09 | R1 |
| T12 | Memory IPC、Chat 注入、引用流 | T11 | R1 |
| T13 | Memory 设置、列表、候选 UI | T12 | R1 |
| T14 | Memory v1：纠正、模式、冲突、失效 | T13 | R2 |
| T15 | 会话持久化、导出、提示历史 | T09,T12 | R2 |
| T16 | Provider 预设扩展 | T05 | R2 |
| T17 | Skills 加载与工具 | T08 | R2 |
| T18 | CLI 与 Agent Skill | T16,T17 | R2 |
| T19 | MCP scope、逐次审批、审计 | T08 | R2 |
| T20 | MCP batch 与 Redis 只读工具 | T19 | R2 |
| T21 | MCP Streamable HTTP | T19 | R2 |
| T22 | 插件/Workflow 工具桥 | T19 | R3 |
| T23 | 本地 CLI Agent Provider | T16 | R2 |
| T24 | 写入提案与精确确认 | T07,T19 | R3 |
| T25 | 组织记忆与管理员审批 | T14,T15 | R3 |
| T26 | 全量回归、文档折叠、过程文件清理 | 全部 | R3 |

## 3. 详细任务

### T01 校准 AI/MCP 正式文档

**目标**：让正式文档与当前 5 工具、deny-all 白名单、DDD 路径一致。

**改动**：

- `crates/datazen/resources/prompts/chat.md`：删除“SQL execution”表述；明确 Chat 只有元数据工具，真实查询仅在 Agent 模式。
- `docs/architecture/frontend/ai.md`：路径改为 `packages/frontend/src/...`；补 Ask/Agent、上下文、记忆章节占位。
- `docs/architecture/backend/mcp.md`：把“空 allowlist = 暴露全部”改为 deny-all；实现路径改为 `crates/datazen-desktop/src/mcp/`。

**验收**：

- 三处不再出现旧路径/旧语义。
- 文档与 `tools.rs`、`allowlist.rs` 的当前行为一致。

**测试**：`rg` 断言旧文案/旧路径不存在；不新增运行时测试。

---

### T02 稳定连接身份与会话 key

**目标**：`ai_chat` 使用持久化 `connectionId`，不再依赖运行时 `db_session_id` 作为会话身份。

**改动**：

- `crates/datazen-desktop/src/commands/ai/chat.rs`：`ai_chat` 增加 `connection_id: Option<String>`；传给 `ChatSessionInput`。
- `crates/datazen/src/ai/application/chat_session.rs`：输入增加 `connection_id`，用于日志与后续记忆/allowlist。
- `packages/frontend/src/stores/aiStore.ts`：`initChatSession` / `sendChatMessage` 接收并传 `connectionId`。
- `packages/frontend/src/components/ai/AiChatPanel.tsx`：把已有 `connectionId` 传入 init/send。
- `packages/frontend/src/stores/ai/sessions.ts`：已有 `computeSessionKey`，补测试覆盖 connectionId 优先。

**验收**：

- 同一连接重开、dbSessionId 变化后，会话 key 不变。
- 前端切换连接不会串会话。

**测试**：Rust IPC 反序列化/日志测试；Vitest `computeSessionKey` 与 aiStore 参数传递测试。

---

### T03 Ask / Agent 模式与工具面

**目标**：引入显式模式；Ask 不调用任何数据库工具，Agent 才挂载 DB 工具。

**改动**：

- `crates/datazen/src/ai/domain/types.rs`：新增 `AiAgentMode { Ask, Agent }`（serde snake_case）。
- `chat_request.rs` / `chat_session.rs`：请求输入增加 `agent_mode`；Ask 强制 `attach_db_tools = false`，Agent 才允许。
- `tools.rs`：`db_tool_definitions(mode)` 或按模式过滤；Ask 只保留 `ask_questions`。
- `packages/frontend/src/types/ai.ts`、`aiStore.ts`、`AiChatPanel.tsx`：模式状态与切换；默认 Ask。
- `ContentViewDrawers.tsx`：透传模式所需 props（如需要）。

**验收**：

- Ask 的 provider 请求中 `tools` 不含 DB/MCP 工具。
- Agent 的请求含元数据工具；未开启查询能力时不含 query/sample。
- 模式在 UI 可见、可切换，并随请求发送。

**测试**：Rust 组装请求测试；Vitest 模式切换与请求参数测试。

---

### T04 只读查询与采样工具

**目标**：Agent 可对当前连接执行只读查询和表采样，复用既有 `db_tools::query()`。

**改动**：

- `tools.rs`：新增 `query_readonly`、`sample_rows` 定义与 dispatch；`is_readonly_db_tool` 纳入。
- `db_tools.rs`：新增 `sample_rows(host, connection_id, database, schema, table, limit)`；用驱动方言 quote 标识符，生成 `SELECT * FROM ... LIMIT n`，再走 `query()`。
- 连接限制：工具只允许 `connection_id == 当前会话 connection_id`；请求中不暴露其他连接（或执行时拒绝）。
- 行/列/字节上限：默认 100 行，最大 1000；采样默认 `gate.max_sample_rows`。
- SQL 必须经 `check_sql_allowed` + `check_sql`，写语句拒绝。

**验收**：

- `SELECT 1` 可执行；`UPDATE/DELETE/DDL` 被拒。
- 跨连接查询被拒。
- 采样只返回当前表、受限行数。

**测试**：共享层工具 dispatch 单测；宿主执行路径集成测试；前端不需要新增测试。

---

### T05 可配置工具循环与 API 重试

**目标**：把固定 6 轮/24k/180s/2KB 改为可配置；调用方传入的轮数生效。

**改动**：

- `ToolLoopGuard` 增加 `with_limits`；默认保留现有值，Agent 用 30 轮、单工具 8KB。
- `run_streaming_tool_loop` 使用 `_max_rounds` 参数；增加 `ToolLoopLimits` 参数。
- `AiSafetyGateConfig` 或 `AiModelProfile.extra` 增加 `maxToolRounds`、`maxToolResultBytes`、`maxToolTokens`、`maxToolDurationSecs`、`apiRetries`。
- Provider 调用增加 0–10 次重试与指数退避；仅对可重试错误。
- 前端 Settings 暴露轮数/重试；默认 30/2。

**验收**：

- 传入 30 时确实允许 30 轮；传入 1 时第二轮停止。
- 重试次数与退避可测试；取消不重试。

**测试**：ToolLoopGuard 边界单测；mock provider 重试测试；Settings 表单测试。

---

### T06 自动上下文快照

**目标**：Chat 自动带上编辑器 SQL、选区、最近错误、结果预览，而不是只靠手选 `@`。

**改动**：

- `ai_chat` 请求增加 `editor_sql`、`selection`、`last_error`、`result_preview`（结构化：columns/rows/rowCount/elapsedMs，前端截断）。
- `chat_session.rs`：新增 `build_auto_context_block()`；顺序：连接笔记 → 当前 SQL/选区 → 错误 → 结果预览 → 手选 `@`；按 `max_context_bytes` 截断。
- 前端 `AiChatPanel.tsx`：从编辑器/结果面板收集上下文；发送时冻结；chip 可移除。
- `queryErrorChatPrompt.ts` 路径继续复用。

**验收**：

- 发送后修改编辑器不影响已发请求。
- 结果预览最多 N 行，且默认脱敏。
- 无上下文时不产生空块。

**测试**：共享层上下文拼装单测；Vitest 快照/chip 测试。

---

### T07 Chat 动作、模式与 egress UI

**目标**：代码块可运行/新查询；发送前显示 egress；模式与工具状态可见。

**改动**：

- `ContentViewDrawers.tsx`：接 `onRunCode` / `onNewQuery`；运行前确认（写操作尤其）。
- `AiChatPanel.tsx`：代码块动作接线；顶部模式切换；egress summary 展示；工具调用折叠状态。
- `types/ai.ts`：`StreamChunkPayload` 增加 `egressSummary?: string`；`aiStore` 消费并清空。
- `AiEgressNotice.tsx`：消费后端 `egress_summary`，显示连接/库/文件/记忆范围。

**验收**：

- Ask/Agent 切换后按钮状态正确。
- Run 能把代码送到编辑器并执行；New Query 新建查询。
- 首个 stream chunk 的 egress 在 UI 可见。

**测试**：Vitest 代码块动作、egress 消费、模式切换。

---

### T08 安全开关真正生效

**目标**：`db_tool_policy`、`mcp_tool_policy`、`require_sql_confirm`、`redact_credentials` 不再是无消费者字段。

**改动**：

- `chat_session.rs` / `tools.rs`：按 `db_tool_policy` 决定 DB 工具集合；`Disabled` 不挂载，`ReadOnly` 只挂只读，`RequireConfirm` 走确认端口。
- `mcp_tools.rs`：按 `mcp_tool_policy` 过滤；`RequireConfirm` 调用确认端口。
- `safety.rs`：`redact_credentials` 决定是否强制凭据脱敏；保留默认 true。
- `AiToolLoopHost` 增加 `confirm_tool_call(tool, args) -> bool` 端口；桌面实现弹窗，超时拒绝。
- 生产库只读/写入拒绝沿用现有 SQL guard。

**验收**：

- 四个字段分别有行为测试。
- Disabled 模式 provider 请求无工具。
- 确认拒绝时工具不执行，返回可读错误。

**测试**：共享层策略单测；宿主确认端口测试；前端确认弹窗测试。

---

### T09 AppDb v7：AI 笔记/记忆/FTS/审计

**目标**：一次性建立 AI 笔记、记忆、FTS、审计表，供 T10–T15、T25 使用。

**改动**：

- `crates/datazen-desktop/src/store/app_db.rs`：`SCHEMA_VERSION = 7`；新增 `if current < 7` 迁移，使用 `Immediate` 事务。
- 表：`ai_connection_notes`、`ai_memories`、`ai_memories_fts`、`ai_memory_audit`，按 PRD §6.9；FTS 三触发器。
- 新增 `crates/datazen-desktop/src/store/ai_memory.rs`：notes/memories CRUD、FTS search、audit。
- 删除连接时同事务清理 notes + memories（找现有 delete connection 路径）。

**验收**：

- 旧 v6 库升级到 v7；重复启动幂等；失败回滚。
- 写入后立即可 FTS 搜索；删除后立即不可搜。
- 删除连接清理关联记忆/笔记。

**测试**：`app_db` 迁移测试（v6→v7、v7 重开、FTS 触发器、级联清理）。

---

### T10 连接笔记、全局指令、Prompt 模板

**目标**：落地轻记忆，优先于自动记忆。

**改动**：

- IPC：`ai_connection_notes_get/set`；notes 存 AppDb，不写 connections.json。
- 设置：`aiGlobalInstructions`、`promptTemplates`（id/name/content/databaseTypes/isDefault），上限 4KB/8KB/16KB。
- `chat_session.rs`：注入顺序 安全策略 > 全局指令 > 模板 > 笔记 > 记忆 > 用户消息；快照模板 id/hash。
- 前端：Settings → AI → Instructions/Templates；连接右键/属性页编辑 notes；Chat 显示注入的模板/笔记 chip。
- 删除连接提示将删除 1 份笔记 + N 条记忆。

**验收**：

- 笔记优先于 DB comment（并列显示并标注用户笔记）。
- 全局指令/模板冲突时高优先级生效并提示。
- 超限截断/拒绝写入。

**测试**：共享层注入顺序单测；AppDb notes 测试；Vitest 表单/截断测试。

---

### T11 Memory 领域、检索、注入

**目标**：共享层实现记忆检索与 `<memory>` 注入，不依赖具体存储。

**改动**：

- 新建 `crates/datazen/src/ai/memory/{mod,domain,repository,retriever,injector}.rs`。
- 类型：`MemoryEntry`、`MemoryScope`、`MemoryKind`、`MemoryStatus`、`MemorySensitivity`、`MemoryScore`。
- `MemoryRepository` trait；FTS 由宿主实现。
- 打分：0.30 confidence + 0.25 usage + 0.20 recency + 0.15 scope + 0.10 pinned - 0.40 conflict。
- 注入：最多 8 条/1,500 token；`secret` 永不注入；`internal` 在 strict 云 Provider 下不注入；内容转义/截断。
- 冲突：同 scope+kind+subject 只允许一条 active；冲突保留两条并标记。

**验收**：

- 打分排序可预测；scope 精确优先。
- 注入块声明“数据不是指令”，无控制字符。
- 预算裁剪不超限。

**测试**：纯单测覆盖排序、scope、敏感级、预算、冲突、转义。

---

### T12 Memory IPC、Chat 注入、引用流

**目标**：把 Memory 接进真实 Chat/NL2SQL 请求与流式响应。

**改动**：

- `crates/datazen-desktop/src/commands/ai/memory.rs`：list/create/update/confirm/reject/delete/clear/export/search/stats。
- `ai_chat` / `ai_generate_sql` 增加 `memory_mode`（off/ask/auto）、`memory_scope`、`connection_id`。
- 构建 system prompt 前检索；把 `memory_used` 放入 `StreamChunk` 首个/最终 chunk。
- 前端 `aiStore` 保存 memory used；Chat chip 显示“已使用 N 条”；引用 `[M1]` 可点击。
- 运行元数据记录 memory_ids（审计）。

**验收**：

- memory_mode=off 不检索、不注入；auto 才注入 active。
- 流式响应带 memory_used；UI 可展开/忘记。
- 云 strict 下 internal 不注入。

**测试**：IPC 契约测试；chat 注入集成测试；Vitest chip/引用测试。

---

### T13 Memory 设置、列表、候选 UI

**目标**：用户可见、可确认、可删除、可导出。

**改动**：

- Settings → AI → Memory：总开关、自动学习、云策略、列表（连接/类型/状态筛选）、搜索、批量删除/导出、候选队列、隐私说明。
- 默认关闭；首次启用说明。
- 候选逐条确认/拒绝/编辑/合并；“全部确认”只作用于筛选结果并显示数量。
- Chat 候选 toast 与“记住这条”。

**验收**：

- 默认关闭；关闭后不产生候选。
- 删除后列表/检索立即消失。
- 导出 JSON/Markdown 可用，提示业务信息。

**测试**：Vitest MemorySection、候选操作、导出；Rust export IPC 测试。

---

### T14 Memory v1：纠正、模式、冲突、失效

**目标**：从纠正与重复模式生成候选；冲突/失效可治理。

**改动**：

- 前端在执行成功且 SQL 被用户修改时，提交 correction 候选（原 SQL/新 SQL/diff/连接/库）。
- 共享层归一化 SQL 字面量，抽取 relationship/pattern/negative 候选（严格 JSON schema；失败丢弃）。
- 14 天内同 JOIN/过滤/表达式 ≥3 次生成 pattern 候选。
- 同 scope+kind+subject 冲突检测；Schema 删除表/列时标 stale。
- 候选抽取可关闭；显示 token 成本。

**验收**：

- 纠正不直接 active；必须确认。
- 重复模式去重；冲突不静默覆盖。
- stale 不注入；UI 提示复核。

**测试**：归一化/去重/冲突/stale 单测；前端候选提交测试。

---

### T15 会话持久化、导出、提示历史

**目标**：会话不再只依赖 localStorage 20×50×4KB。

**改动**：

- AppDb 新增 `ai_sessions`、`ai_session_messages`（或 JSON 快照）；保留 localStorage 迁移。
- IPC：list/get/upsert/delete/export；按 connectionId/db 恢复。
- 前端 `sessions.ts` 改为后端优先、localStorage 回退；会话列表/导出/提示历史 UI。
- 取消/失败状态保留。

**验收**：

- 重启应用后会话恢复；跨连接不串。
- 导出 Markdown/JSON 可用；提示历史可导航。
- 旧 localStorage 数据可迁移。

**测试**：AppDb 会话 CRUD；Vitest 恢复/迁移/导出。

---

### T16 Provider 预设扩展

**目标**：支持 PRD 的 API 预设，不止 OpenAI/DeepSeek/Ollama/Custom。

**改动**：

- 后端 `AiProviderType` 增加 Gemini/Qwen/MiniMax/AnthropicCompatible（或复用 Custom + preset）。
- Provider factory/registry 支持 OpenAI-compatible 与 Anthropic-compatible 端点。
- 前端 `AiProviderType`、`PROVIDER_ORDER`、预设默认 endpoint/model、表单。
- 迁移旧 anthropic 配置；不破坏现有配置。

**验收**：

- 10 个预设可选；Gemini/Qwen/MiniMax 走兼容协议。
- 旧配置可加载；API key 不明文日志。

**测试**：Provider factory 单测；前端表单/序列化测试。

---

### T17 Skills 加载与工具

**目标**：兼容 SKILL.md 目录，按需加载正文。

**改动**：

- 后端：`~/.agents/skills` + 可选自定义目录；解析 `SKILL.md` frontmatter。
- 工具：`list_skills`、`use_skill`、`read_skill_file`；listing 只发 name/description。
- 安全：路径 canonicalize，禁止越界；附件按需读取；技能不能授权工具。
- 前端：Settings → AI → Skills；启用/目录/自动使用开关；Chat 加载状态按会话。
- CLI Provider 不接收 Skills。

**验收**：

- 非法路径/超大文件被拒。
- listing 不含正文；use 后才读正文。
- 自动使用默认关闭。

**测试**：路径越界/解析/工具单测；Vitest 设置测试。

---

### T18 CLI 与 Agent Skill

**目标**：提供只读 CLI 与官方 Agent Skill。

**改动**：

- 新建 `crates/datazen-cli`（或 `crates/datazen/src/bin/datazen-cli`），`clap` 子命令：doctor/capabilities/connections/schema/query/context/open。
- 复用共享 connection/execution 端口；默认只读；`--allow-writes` 与 `--allow-dangerous-sql` 分离；生产库拒绝写。
- 输出 JSON/CSV、`--limit`、`--timeout`、`--file`；凭据走应用数据目录/keychain。
- 官方 `SKILL.md` 安装到 `~/.agents/skills/datazen`。
- 前端/文档给安装说明。

**验收**：

- `doctor` 可用；`query` 只读默认；写需显式 flag 且生产库拒绝。
- JSON/CSV 输出可解析；超时/错误码稳定。
- 凭据不打印。

**测试**：CLI 参数/输出快照；只读/生产保护集成测试。

---

### T19 MCP scope、逐次审批、审计

**目标**：把 MCP 从全局三档升级到连接/库 scope + 逐次确认 + 审计。

**改动**：

- `McpToolPolicy` 增加 database/namespace scope；与连接 allowlist 组合。
- 非只读/高风险调用：暂停，显示精确 SQL/参数，Allow once/Deny；5 分钟超时拒绝；Stop 取消。
- 审计表/日志：who/when/connection/tool/rowcount/decision；不记录参数中的凭据。
- 前端 MCP 设置/审批弹窗。

**验收**：

- scope 外调用拒绝；确认拒绝不执行。
- 审计可查；超时拒绝；Stop 取消待审批。

**测试**：策略矩阵单测；审批端口集成测试；审计查询测试。

---

### T20 MCP batch 与 Redis 只读工具

**目标**：扩展 MCP 工具面。

**改动**：

- `execute_batch`：多语句，默认只读；事务显式开启；逐语句 guard。
- Redis 只读命令工具：按命令分类阻断写；限制 key pattern/返回量。
- 工具定义/描述/注解；审计接入。

**验收**：

- batch 中任一写语句默认拒绝；事务失败回滚。
- Redis 写命令/危险命令拒绝。

**测试**：batch 解析/guard；Redis 命令分类；MCP 工具契约测试。

---

### T21 MCP Streamable HTTP

**目标**：支持 stdio + Streamable HTTP，默认关闭。

**改动**：

- 桌面 MCP server 增加 HTTP transport：loopback + bearer token；默认关闭；设置页启用/生成 token。
- 复用现有 13 工具 + T19/T20 扩展；会话/取消/超时语义保持一致。
- 文档补 curl/MCP client 配置。

**验收**：

- 默认不监听；启用后仅 loopback；错误 token 拒绝。
- HTTP 与 stdio 工具行为一致；断连清理。

**测试**：HTTP 集成测试（bind 随机端口）；auth/断连测试。

---

### T22 插件/Workflow 工具桥

**目标**：插件与 Workflow 工具进入 AI 工具面，默认 opt-in。

**改动**：

- 插件工具桥：按插件 opt-in；只读直跑，非只读暂停确认（Allow once/Deny）；5 分钟超时；Stop 取消。
- Workflow 工具：只读 workflow 可直跑；非只读需审批；凭据不进模型。
- 工具命名 `plugin_<id>__<tool>` / `workflow_<id>`；参数 schema 校验。
- 前端插件/Workflow 设置开关。

**验收**：

- 未 opt-in 不出现；非只读必须确认。
- 凭据/结果按 gate 脱敏；取消可中断。

**测试**：工具桥单测；确认/超时/取消集成测试。

---

### T23 本地 CLI Agent Provider

**目标**：支持 8 种本地 CLI Agent（Claude Code、Codex、Cursor、OpenCode、Grok、Qoder、CodeBuddy、Pi）。

**改动**：

- Provider adapter：检测可执行文件、启动子进程、流式 stdout/stderr、超时/取消、错误映射。
- 预设命令/参数；不让 CLI 继承敏感环境变量；工作目录隔离。
- Settings 选择本地 Agent；可用性检测；不回传凭据。
- 与 API Provider 统一 `AiProvider` trait。

**验收**：

- 未安装时给出可读错误；取消能杀进程。
- stdout 流式显示；stderr 不泄露敏感信息。
- 8 个预设可选。

**测试**：fake CLI process 集成测试；取消/超时；预设解析。

---

### T24 写入提案与精确确认

**目标**：非只读 SQL 走提案 → 精确文本确认 → 一次授权。

**改动**：

- 后端 `propose_write`：生成提案 id，绑定 connection/database/exact SQL/hash/expiry；不执行。
- `apply_write`：校验一次性 token + 精确 SQL + 连接 + 库；生产库拒绝；审计。
- Agent 工具：写操作只返回提案；UI 展示 diff/影响，Allow once/Deny。
- 不做自动写入。

**验收**：

- 文本/连接/库任一不匹配拒绝；token 重用拒绝；过期拒绝。
- 生产库永不执行；审计完整。

**测试**：token 绑定/过期/重用；生产保护；UI 确认测试。

---

### T25 组织记忆与管理员审批

**目标**：组织级记忆只读共享，管理员发布/撤销。

**改动**：

- 服务端/共享层：org scope 记忆，管理员审批；成员只读；跨组织隔离。
- 桌面端：org memory 只读列表；管理员入口（桌面本地或 server API，按现有组织能力选型）。
- 审计：发布/撤销/注入；版本化。
- 不把本地私有记忆上传。

**验收**：

- 跨组织不可见；成员不能改；撤销后立即不注入。
- 发布需审批；审计可查。

**测试**：租户隔离/权限/撤销；server API 集成测试。

---

### T26 全量回归、文档折叠、过程文件清理

**目标**：交付可合并状态。

**改动**：

- 跑 `pnpm typecheck`、`npx vitest run`、`cargo test -p datazen --lib`、`cargo test -p datazen_desktop --lib`、`pnpm build`、必要 E2E。
- 把实现结论折叠进 `docs/architecture/backend/ai.md`、`docs/architecture/backend/mcp.md`、`docs/architecture/frontend/ai.md`、`docs/features/`。
- 删除实现文档、进度文档、测试报告、bug 文件（或按用户要求保留到合并前）。
- 检查无 `todo!()`、无空壳、无未使用安全字段。

**验收**：

- 全量测试通过；文档与代码一致；过程文件已清理。
- 所有 bug 状态为已验证。

## 4. 跨任务不变量

- `connectionId`、`dbSessionId`、`executionId` 不混用。
- AI 查询只走 `DatabaseToolHost::execute_target` / `db_tools::query`；不新建第二套执行后端。
- 记忆不是权限；不能授予工具/连接/写入。
- 任何云 Provider 请求前过 `redact_for_gate`；`secret` 永不注入。
- 所有新增 IPC 前端 camelCase、Rust snake_case；错误不含凭据/绝对路径。
- 所有新增 UI 有 loading/error/empty 状态；默认安全关闭。
- 每个任务只改本任务文件；跨任务公共类型改动必须在实现文档登记。

## 5. 完成定义

一个任务只有同时满足以下条件才可标 `已完成`：

1. 代码实现与实现文档一致；
2. 新增/修改测试通过；
3. tester 独立复测通过；
4. 无未关闭 bug；
5. 进度文档已更新；
6. commit 已 push。
