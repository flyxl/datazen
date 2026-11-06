# AI 功能前端

> [返回架构总览](../README.md)

### 1.1 AI 组件概览

```
packages/frontend/src/components/ai/
├── AiInput.tsx            # 统一 AI 输入组件（@ 上下文引用、文件 chips 显示）
├── ContextPicker.tsx      # @ 上下文文件选择器（下拉列表、搜索、键盘导航）
├── Nl2SqlPanel.tsx        # NL2SQL 输入面板（生成中流式预览，完成后写回编辑器）
├── DiagnosisPanel.tsx     # SQL 错误诊断结果展示（含修正 SQL 一键应用）
├── ExplainPanel.tsx       # EXPLAIN 可视化面板（树形展示 + AI 分析）
├── AiChatPanel.tsx        # 侧边栏 AI 对话面板（模式、会话历史、推理折叠、工具调用）
├── AiSessionHistory.tsx   # 会话列表、恢复、删除与导出
├── AiMessageContent.tsx   # 助手消息内容渲染（Markdown 与代码块）
├── AiCodeBlock.tsx        # 代码块渲染与运行/新查询动作
├── AiEgressNotice.tsx     # AI 请求上下文/egress 提示
├── NlFilterInput.tsx      # 自然语言筛选输入组件
├── WorkflowPanel.tsx      # Workflows 管理面板（嵌入 AiChatPanel 的 Workflows 标签页）
└── WorkflowChatPanel.tsx  # AI 辅助 Workflow 生成对话

packages/frontend/src/components/AiWriteProposalHost.tsx  # 写入提案审阅、精确 SQL、Allow once / Deny

packages/frontend/src/windows/settings/
├── AiSettingsSection.tsx      # Provider/Profile、安全门、本地 CLI 可用性
├── AiToolBridgeSection.tsx    # 插件/Workflow 工具 opt-in
├── McpSettingsSection.tsx     # MCP Server、stdio/HTTP、审批与审计
├── McpClientSection.tsx       # 外部 MCP Server 配置与连接
├── MemorySection.tsx          # 私人记忆 + 只读组织记忆
├── PromptSettingsSection.tsx  # Prompt 覆盖
└── SkillsSection.tsx          # Skills 开关、目录、显式选择

packages/frontend/src/windows/workflow/
├── WorkflowPage.tsx     # Workflow 视图（主工作区内嵌；含 tab + DataTable）
└── WorkflowForm.tsx       # Workflow 创建/编辑表单
```

### 1.2 AiInput — 统一输入组件

`AiInput` 是所有 AI 输入区域的共享组件，提供：
- 文本输入区域（可自适应高度或固定高度）
- `@` 触发上下文文件引用（激活 `ContextPicker`，默认 `position="above"`，因组件位于面板底部）
- 已选上下文文件的 chips 显示在输入框边框容器内部起始位置（与 textarea 同行内联，Cursor 风格），可单独移除；不再作为独立行渲染在输入框上方
- 外层输入容器使用 `flex-wrap items-center` 布局，chips 与 textarea、发送按钮在同一边框内换行排列
- 发送 / 停止按钮
- 键盘快捷键（Enter 发送，Shift+Enter 换行）

### 1.3 ContextPicker — @ 上下文文件选择器

用户在 AI 输入中键入 `@` 时弹出的文件选择器：
- 从全局上下文目录（`settings.contextDir`）列出可用文件
- 支持搜索过滤、键盘上下导航、回车选择
- 显示文件大小信息
- `.ctx.yaml` / `.ctx.yml` 文件使用 `Layers` 图标区分（表组上下文文件）
- 选中的文件内容作为前缀注入到发送给 AI 的 user message 中（`.ctx.yaml` 文件则提取表名获取实时 DDL）
- 支持 `position?: 'above' | 'below'` 属性（默认 `'above'`），控制下拉列表相对锚点元素的弹出方向：
  - `AiInput` 使用默认 `'above'`（位于 Chat 面板底部，向上弹出避免被裁切）
  - `Nl2SqlPanel` 传入 `position="below"`（位于 QueryEditorSection 内容区顶部，向下弹出）

### 1.4 Nl2SqlPanel — NL2SQL 输入面板

`QueryEditorSection` 顶部的自然语言转 SQL 面板，布局与交互要点：
- 未配置 AI 时仅显示「未配置」提示与跳转 Settings 的「配置」按钮
- 输入行含带边框的输入容器与右侧操作按钮（Sparkles 图标在「生成」按钮内），生成中按钮切换为停止
- 未选中数据库或输入为空时生成按钮禁用；placeholder 在未选数据库时显示「请先选择数据库」
- `@` 上下文 chips 渲染在输入容器内部起始位置（与 textarea 内联），配合 `ContextPicker position="below"`
- 发送后清空已选上下文；生成完成后将 SQL 写回编辑器（`onSqlChange`）
- 预览区在生成中或已有结果时显示，生成中展示 `streamingPreview`，完成后展示 `generatedSql`，并提供复制与清除按钮
- 错误信息以独立红色提示条展示

### 1.5 AI 状态管理（aiStore）

`packages/frontend/src/stores/aiStore.ts` 使用 Zustand 管理所有 AI 状态（会话与流式拆分在同目录 `stores/ai/`）：

| 状态域 | 说明 |
|--------|------|
| AI 配置 | profiles、activeProfileId、providerType、model、apiKey、endpoint、maxTokens、safetyGate |
| Chat | chatSession（messages、isStreaming、streamContent、streamReasoning、requestId）、contextSnapshot；`agentMode` 由面板在每次发送时传入 |
| 会话历史 | chatSessions、sessionHistoryLoading、sessionHistoryError；切换连接时按 `connectionId` 选会话 |
| NL2SQL | nl2sql（input、streamingSql、streamingPreview、generatedSql、isGenerating、requestId） |
| 诊断 | diagnosis、isDiagnosing |
| EXPLAIN | explainAnalysis、isAnalyzingExplain |
| Workflow Chat | workflowChat |
| 筛选 | parsedFilters、isParsingFilter |
| Schema 文档 | schemaDoc、isGeneratingSchemaDoc |
| 连接诊断 | connectionDiagnosis、isDiagnosingConnection |
| 查询分析 | queryAnalysis、isAnalyzingQueries |
| Workflows | workflows、workflowsLoading、isExecutingWorkflow、workflowExecutionResult |
| MCP Client | mcpServers、mcpTools、mcpConnecting、mcpError |

AI Chat 的 Ask 模式不挂任何 DB/MCP 工具，只保留 `ask_questions`；Agent 模式在 `attach_db_tools` 为真时挂 5 个只读元数据工具，绑定连接时再追加 `query_readonly`、`sample_rows`、`propose_write`，并由 `dbToolPolicy` 过滤（`read_only` 去掉 `propose_write`，`disabled` 去掉全部）。MCP 工具只在 Agent 且 Server 启用给 AI 时注入。写操作永远只生成提案，必须由用户在 `AiWriteProposalHost` 确认精确 SQL 后才可一次应用。工具调用、审批状态和写入提案都不持久化到会话正文，只保留必要的消息/引用元数据。

Memory、Skills 与写入提案的编辑态由各自的设置/组件局部状态管理，不塞进 `aiStore`：Memory 只在打开设置或插入引用时按需查询，Skills 在设置页加载可用列表，写入提案由 `AiWriteProposalHost` 持有当前审阅对象。

### 1.6 AI IPC 封装（commands/ai.ts）

所有 AI 后端调用封装在 `packages/frontend/src/commands/ai.ts`，按域导出而不是散落 invoke：
- 流式调用经平台事件端口 `requirePlatform().events.listenCrossWindow()` 监听 `ai:stream-chunk` / `ai:stream-error`；配置变更监听 `ai:config-changed`
- 非流式调用直接 `invoke()` 返回结果
- `generateSql` 和 `chat` 支持传入 `contextFiles` 与结构化 `contextSnapshot`；`chat` 额外传 `agentMode`、`connectionId`、`targetClaim`
- Memory：`ai_memory_*`、`ai_org_memory_list`；组织记忆在本机只读，发布/撤销直接走 server API，不调用桌面写命令
- 会话：`ai_sessions_*`；导出支持 markdown/json
- Skills：`ai_list_skills`；是否可见/可调用由后端设置决定
- 写入：`ai_propose_write` / `ai_apply_write` / `ai_deny_write` / `ai_list_write_audit`；apply 必须带 token、连接、库和精确 SQL
- MCP：`mcp_*`（Server 启停/HTTP/审批/审计，Client 连接/工具）

上下文文件操作封装在 `packages/frontend/src/commands/context.ts`：
- `contextCommands.getDir()` — 获取上下文目录路径
- `contextCommands.listFiles(query?)` — 列出/搜索文件
- `contextCommands.readFiles(paths)` — 批量读取文件内容

### 1.7 集成方式

| 集成点 | 组件 | AI 功能 |
|--------|------|---------|
| QueryEditorSection 顶部 | `Nl2SqlPanel` | NL2SQL 输入（含生成、复制、清除、`@` 上下文引用） |
| QueryPanel 错误区域 | `DiagnosisPanel` | SQL 错误诊断 |
| QueryPanel 结果 Tab | `ExplainPanel` | EXPLAIN AI 分析 |
| `ContentView` 侧边栏（`ContentViewDrawers`） | `AiChatPanel` | AI 对话 + Workflows 标签页（`@` 上下文引用） |
| `AiChatPanel` Workflows 标签页 | `WorkflowPanel` | Workflows 管理和执行 |
| 主窗口功能 sidebar | `ConnectionPage` 内 Workflow / Dashboard 入口 | 主工作区内导航 |
| TableView 筛选区域 | `NlFilterInput` | 自然语言筛选 |
| `SettingsContent` | `AiSettingsSection` | Provider Profile、模型、Key、MaxTokens、安全门、本地 CLI 可用性 |
| `SettingsContent` | `McpSettingsSection` / `McpClientSection` | MCP Server 启停、stdio/HTTP、审批/审计；外部 Server 连接 |
| `SettingsContent` | `MemorySection` | 私人记忆管理 + 只读组织记忆 |
| `SettingsContent` | `SkillsSection` | Skills 开关、目录、显式选择 |
| `SettingsContent` | `AiToolBridgeSection` | 插件/Workflow 工具 opt-in |
| `SettingsContent` | 上下文目录 | `contextDir` 路径配置 |
| `SettingsContent` | `PromptSettingsSection` | 各场景 Prompt 自定义 |

### 1.8 AI 入口可见性

- AI 入口始终可见，即使未配置 API Key
- 进入 AI 相关页面时显示未配置提示
- 点击「配置」按钮直接跳转 Settings 页的 AI 配置部分

### 1.9 Chat 模式、上下文与 egress

`AiChatPanel` 顶部显示 Ask/Agent 切换、当前工具权限和模型 Profile：

- **Ask** 默认；发送时不带 DB/MCP/Skill/插件/Workflow 工具，只允许 `ask_questions`。切换模式不会重写历史消息。
- **Agent** 显示当前可用的只读工具；查询权限由 Profile 的安全门决定。工具调用以折叠卡片展示参数摘要和结果，不展示凭据或原始 payload。
- 发送前 `AiEgressNotice` 展示本次会离开本机的范围：连接、库、schema、手选上下文文件、记忆引用、MCP 工具结果。egress 摘要来自后端 `egressSummary`，不是前端猜测。
- 上下文 chips 显示自动快照（当前 SQL/选区/错误/结果预览）和手选 `@` 文件；发送后快照冻结，删除 chip 只影响下一次发送。
- 代码块提供「运行/新查询」动作；运行写语句前必须先进入写入提案审阅，不允许直接执行。

### 1.10 Memory、Skills 与组织记忆

**Memory 设置页**分为「私人记忆」和「组织记忆」：

- 私人记忆：首次使用需显式开启并确认隐私说明；关闭时请求固定 `off`，开启后 Chat 默认 `auto` 自动检索，生成类请求可显式传 `ask` 把候选带入流程。可切换 `autoLearn`、cloud policy，按 candidate/active/rejected/archived/stale 筛选，执行编辑、确认、拒绝、纠正、标失效、删除、导出。候选只有在用户确认后才成为 active；关闭时不显示检索入口。
- 组织记忆：只读列表，展示 subject/kind/内容与只读标记；没有编辑、删除、发布按钮。桌面没有组织管理员角色，发布/撤销由 server 端真实管理员完成，UI 不伪造本地管理员入口。
- 引用在消息里显示为 `M1`、`M2`…；点击可查看来源 scope 与版本，不展示审计中的敏感内容。

**Skills 设置页**：总开关默认关闭；开启后可选择一个自定义目录、切换 `autoUse`、显式勾选 stable skill id。关闭后工具立即消失，已选 id 保留但不生效。列表只显示可解析的 `SKILL.md`；错误以可用性提示呈现，不把绝对路径回显到消息。

### 1.11 写入提案 UI

`AiWriteProposalHost` 是写操作的唯一确认入口：

- Agent 返回 `propose_write` 结果后，以卡片展示逐条语句、verb、表、是否有 WHERE、是否 destructive、schema 变更、review diff 和过期时间。
- 生产/只读连接的卡片明确标记不可应用，只提供关闭。
- 用户可选择 **Allow once** 或 **Deny**；Allow once 只调用一次 `ai_apply_write`，不会把 token 存入会话或 localStorage。
- 应用成功/失败都显示结构化结果；失败不自动重试、不换连接、不改 SQL。
- 提案过期、token 重用、SQL/连接/库不匹配都在后端拒绝，前端只展示错误码的安全文案。

### 1.12 会话历史与审批

- `AiSessionHistory` 按当前 `connectionId` 列出会话，支持恢复、删除和导出 JSON/Markdown；流式进行中禁止删除当前会话。
- 会话保存消息、模式、连接/库和引用元数据，不保存完整工具结果或写入 token。
- MCP/桥接审批以全局模态出现，显示工具、连接、脱敏参数摘要、5 分钟倒计时和 Allow once / Deny / Stop；关闭模态或 Stop 等同拒绝，不默认放行。

---

## 2. SQL 编辑器方言

### 2.1 动态方言选择

`SqlEditor` 和 `SqlCodeBlock` 根据连接的 `databaseType` 动态选择 CodeMirror SQL 方言：

| databaseType | CodeMirror 方言 |
|-------------|----------------|
| postgresql | PostgreSQL |
| mysql | MySQL |
| mariadb | MariaSQL |
| sqlite | SQLite |
| 其他 | StandardSQL（默认） |

方言映射通过 `resolveCmDialect()` 函数统一处理，`databaseType` 从 `ContentView` 经 `QueryPanel` 传递到 `SqlEditor`。
