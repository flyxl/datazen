# AI 功能前端

> [返回架构总览](../README.md)

### 1.1 AI 组件概览

```
src/components/ai/
├── AiInput.tsx            # 统一 AI 输入组件（@ 上下文引用、文件 chips 显示）
├── ContextPicker.tsx      # @ 上下文文件选择器（下拉列表、搜索、键盘导航）
├── Nl2SqlPanel.tsx        # NL2SQL 输入面板（自适应/可调整高度，流式显示生成 SQL）
├── DiagnosisPanel.tsx     # SQL 错误诊断结果展示（含修正 SQL 一键应用）
├── ExplainPanel.tsx       # EXPLAIN 可视化面板（树形展示 + AI 分析）
├── AiChatPanel.tsx        # 侧边栏 AI 对话面板（消息渲染、推理折叠、MCP 工具调用）
├── NlFilterInput.tsx      # 自然语言筛选输入组件
├── WorkflowPanel.tsx      # Workflows 管理面板（嵌入 ConnectionView 侧边栏）
└── WorkflowChatPanel.tsx  # AI 辅助 Workflow 生成对话

src/windows/workflow/
├── WorkflowWindow.tsx     # Workflow 独立窗口（含 tab 系统 + DataTable 结果展示）
└── WorkflowForm.tsx       # Workflow 创建/编辑表单
```

### 1.2 AiInput — 统一输入组件

`AiInput` 是所有 AI 输入区域的共享组件，提供：
- 文本输入区域（可自适应高度或固定高度）
- `@` 触发上下文文件引用（激活 `ContextPicker`，默认 `position="above"`，因组件位于面板底部）
- 已选上下文文件的 chips 显示在输入框边框容器内部起始位置（与 textarea 同行内联，Cursor 风格），可单独移除；不再作为独立行渲染在输入框上方
- 外层输入容器使用 `flex-wrap items-end` 布局，chips 与 textarea、发送按钮在同一边框内换行排列
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
  - `Nl2SqlPanel` 传入 `position="below"`（位于 QueryPanel 内容区顶部，向下弹出）

### 1.4 Nl2SqlPanel — NL2SQL 输入面板

`QueryPanel` 顶部的自然语言转 SQL 面板，布局与交互要点：
- 输入行仅含带边框的输入容器与右侧操作按钮，**不在输入框前**单独放置 AI 图标（Sparkles 图标仅出现在「生成」按钮内）
- 未选中数据库时面板仍渲染：生成按钮禁用，placeholder 显示「请先选择数据库」；选中数据库后 placeholder 切换为常规上下文提示
- `@` 上下文 chips 渲染在输入容器内部起始位置（与 textarea 内联，Cursor 风格），配合 `ContextPicker position="below"`
- 无生成 SQL 时面板高度随内容自适应；仅当存在 SQL 输出且展开时，才应用固定高度并通过底部拖拽手柄调整（`useResizable`，持久化 key `nl2sql-panel-height`）
- SQL 输出区域独立于输入行、**全宽**渲染（非嵌套在输入行内）：
  - 顶栏含折叠切换（ChevronUp / ChevronDown + 「SQL」标签）及操作按钮（应用、应用并图表化、复制）
  - 折叠时隐藏 SQL 内容并移除固定高度与拖拽手柄；生成中始终展示 SQL 内容
- 流式生成 SQL，结果写入 `aiStore.nl2sql.generatedSql`

### 1.5 AI 状态管理（aiStore）

`src/stores/aiStore.ts` 使用 Zustand 管理所有 AI 状态：

| 状态域 | 说明 |
|--------|------|
| AI 配置 | provider_type、model、api_key、base_url、max_tokens |
| NL2SQL | generatedSql、isGenerating、streamingContent |
| 诊断 | diagnosis、isDiagnosing |
| EXPLAIN | explainAnalysis、isAnalyzing |
| Chat | chatSessions、activeChatId、streamingChatContent、streamingReasoning |
| 筛选 | parsedFilters、isParsing |
| Schema 文档 | schemaDoc、isGeneratingSchemaDoc |
| 连接诊断 | connectionDiagnosis、isDiagnosingConnection |
| 查询分析 | queryAnalysis、isAnalyzingQueries |
| Workflows | workflows、isExecutingWorkflow |
| MCP | mcpServers、mcpStatus |

### 1.6 AI IPC 封装（commands/ai.ts）

所有 AI 后端调用封装在 `src/commands/ai.ts`：
- 流式调用通过 `listen()` 监听 Tauri Events（如 `ai:sql-chunk`、`ai:chat-chunk`）
- 非流式调用直接 `invoke()` 返回结果
- `generateSql` 和 `chat` 支持传入 `contextFiles` 参数

上下文文件操作封装在 `src/commands/context.ts`：
- `contextCommands.getDir()` — 获取上下文目录路径
- `contextCommands.listFiles(query?)` — 列出/搜索文件
- `contextCommands.readFiles(paths)` — 批量读取文件内容

### 1.7 集成方式

| 集成点 | 组件 | AI 功能 |
|--------|------|---------|
| QueryPanel 工具栏 | `Nl2SqlPanel` | NL2SQL 输入（含「应用并图表化」、`@` 上下文引用） |
| QueryPanel 错误区域 | `DiagnosisPanel` | SQL 错误诊断 |
| QueryPanel 结果 Tab | `ExplainPanel` | EXPLAIN AI 分析 |
| SqlConnectionView 侧边栏 | `AiChatPanel` | AI 对话 + Workflows（`@` 上下文引用） |
| SqlConnectionView 侧边栏 | `WorkflowPanel` | Workflows 管理和执行 |
| 主窗口侧边栏 | `ActionPanel` → `WorkflowWindow` | 独立 Workflow 窗口入口 |
| TableView 筛选区域 | `NlFilterInput` | 自然语言筛选 |
| ChartToolbar NL输入 | `nlConfig.ts` | 自然语言图表配置调整 |
| SettingsWindow | AI 配置 | Provider/Model/Key/MaxTokens |
| SettingsWindow | MCP Server/Client | 启停/连接管理 |
| SettingsWindow | 上下文目录 | `contextDir` 路径配置 |
| SettingsWindow | Prompt 覆盖 | 各场景 Prompt 自定义 |

### 1.8 AI 入口可见性

- AI 入口始终可见，即使未配置 API Key
- 进入 AI 相关页面时显示未配置提示
- 点击「配置」按钮直接跳转 Settings 窗口的 AI 配置部分

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

方言映射通过 `resolveCmDialect()` 函数统一处理，`databaseType` 从 `SqlConnectionView` 通过 `QueryPanel` 传递到 `SqlEditor`。
