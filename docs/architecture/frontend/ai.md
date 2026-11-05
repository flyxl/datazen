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
├── AiChatPanel.tsx        # 侧边栏 AI 对话面板（消息渲染、推理折叠、MCP 工具调用）
├── AiMessageContent.tsx   # 助手消息内容渲染（Markdown 与代码块）
├── AiCodeBlock.tsx        # 代码块渲染与操作按钮
├── AiEgressNotice.tsx     # AI 请求上下文/egress 提示
├── NlFilterInput.tsx      # 自然语言筛选输入组件
├── WorkflowPanel.tsx      # Workflows 管理面板（嵌入 AiChatPanel 的 Workflows 标签页）
└── WorkflowChatPanel.tsx  # AI 辅助 Workflow 生成对话

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
| AI 配置 | providerType、model、apiKey、endpoint、maxTokens |
| NL2SQL | nl2sql（input、streamingSql、streamingPreview、generatedSql、isGenerating、requestId） |
| 诊断 | diagnosis、isDiagnosing |
| EXPLAIN | explainAnalysis、isAnalyzingExplain |
| Chat | chatSession（messages、isStreaming、streamContent、streamReasoning、requestId） |
| Workflow Chat | workflowChat |
| 筛选 | parsedFilters、isParsingFilter |
| Schema 文档 | schemaDoc、isGeneratingSchemaDoc |
| 连接诊断 | connectionDiagnosis、isDiagnosingConnection |
| 查询分析 | queryAnalysis、isAnalyzingQueries |
| Workflows | workflows、workflowsLoading、isExecutingWorkflow、workflowExecutionResult |
| MCP Client | mcpServers、mcpTools、mcpConnecting、mcpError |

AI Chat 当前只挂载 5 个只读元数据 DB 工具（`list_connections` / `list_databases` / `list_tables` / `search_tables` / `get_table_schema`），不能执行 SQL 或读取表行；Ask/Agent 模式与 `query_readonly` / `sample_rows` 属于实现文档 T03/T04 的规划，尚未实现。

### 1.6 AI IPC 封装（commands/ai.ts）

所有 AI 后端调用封装在 `packages/frontend/src/commands/ai.ts`：
- 流式调用经平台事件端口 `requirePlatform().events.listenCrossWindow()` 监听 `ai:stream-chunk` / `ai:stream-error`；配置变更监听 `ai:config-changed`
- 非流式调用直接 `invoke()` 返回结果
- `generateSql` 和 `chat` 支持传入 `contextFiles` 参数

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
| `SettingsContent` | AI 配置 | Provider/Model/Key/MaxTokens |
| `SettingsContent` | MCP Server/Client | 启停/连接管理 |
| `SettingsContent` | 上下文目录 | `contextDir` 路径配置 |
| `SettingsContent` | Prompt 覆盖 | 各场景 Prompt 自定义 |

### 1.8 AI 入口可见性

- AI 入口始终可见，即使未配置 API Key
- 进入 AI 相关页面时显示未配置提示
- 点击「配置」按钮直接跳转 Settings 页的 AI 配置部分

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
