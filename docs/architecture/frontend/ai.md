# AI 功能前端

> [返回架构总览](../README.md)

### 1.1 AI 组件概览

```
src/components/ai/
├── Nl2SqlPanel.tsx        # NL2SQL 输入面板（可折叠，流式显示生成 SQL）
├── DiagnosisPanel.tsx     # SQL 错误诊断结果展示（含修正 SQL 一键应用）
├── ExplainPanel.tsx       # EXPLAIN 可视化面板（树形展示 + AI 分析）
├── AiChatPanel.tsx        # 侧边栏 AI 对话面板（消息渲染、代码块提取、SQL 插入）
├── NlFilterInput.tsx      # 自然语言筛选输入组件
└── WorkflowPanel.tsx      # Workflows 管理面板（嵌入 ConnectionView 侧边栏）

src/windows/workflow/
└── WorkflowWindow.tsx     # Workflow 独立窗口（含 tab 系统 + DataTable 结果展示）
```

### 1.2 AI 状态管理（aiStore）

`src/stores/aiStore.ts` 使用 Zustand 管理所有 AI 状态：

| 状态域 | 说明 |
|--------|------|
| AI 配置 | provider_type、model、api_key、base_url、max_tokens |
| NL2SQL | generatedSql、isGenerating、streamingContent |
| 诊断 | diagnosis、isDiagnosing |
| EXPLAIN | explainAnalysis、isAnalyzing |
| Chat | chatSessions、activeChatId、streamingChatContent |
| 筛选 | parsedFilters、isParsing |
| Schema 文档 | schemaDoc、isGeneratingSchemaDoc |
| 连接诊断 | connectionDiagnosis、isDiagnosingConnection |
| 查询分析 | queryAnalysis、isAnalyzingQueries |
| Workflows | workflows、isExecutingWorkflow |
| MCP | mcpServers、mcpStatus |

### 1.3 AI IPC 封装（commands/ai.ts）

所有 AI 后端调用封装在 `src/commands/ai.ts`：
- 流式调用通过 `listen()` 监听 Tauri Events（如 `ai:sql-chunk`、`ai:chat-chunk`）
- 非流式调用直接 `invoke()` 返回结果

### 1.4 集成方式

| 集成点 | 组件 | AI 功能 |
|--------|------|---------|
| QueryPanel 工具栏 | `Nl2SqlPanel` | NL2SQL 输入（含「应用并图表化」） |
| QueryPanel 错误区域 | `DiagnosisPanel` | SQL 错误诊断 |
| QueryPanel 结果 Tab | `ExplainPanel` | EXPLAIN AI 分析 |
| SqlConnectionView 侧边栏 | `AiChatPanel` | AI 对话 + Workflows |
| 主窗口侧边栏 | `ActionPanel` → `WorkflowWindow` | 独立 Workflow 窗口入口 |
| TableView 筛选区域 | `NlFilterInput` | 自然语言筛选 |
| ChartToolbar NL输入 | `nlConfig.ts` | 自然语言图表配置调整 |
| SettingsWindow | AI 配置 | Provider/Model/Key/MaxTokens |
| SettingsWindow | MCP Server/Client | 启停/连接管理 |

### 1.5 AI 入口可见性

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
