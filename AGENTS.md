# AGENTS.md

> 本文件面向 AI 编程助手（如 Cursor Agent, GitHub Copilot, Codex 等），帮助其快速理解项目结构和约定。

## 项目概述

DataZen 是一个跨平台桌面数据库管理工具，基于 **Tauri v2**（Rust 后端 + React 前端）构建，集成了 AI 辅助功能。

- **框架**：Tauri v2 + React + TypeScript + Tailwind CSS
- **后端语言**：Rust
- **包管理**：pnpm（前端）、Cargo workspace（Rust）
- **状态管理**：Zustand
- **测试**：Vitest（单元）、WebdriverIO（E2E）、手工黑盒测试（`test/`）
- **AI 集成**：多 Provider 支持（OpenAI / Anthropic / 自定义 OpenAI 兼容）、MCP Server/Client

## 目录结构

```
datazen/
├── src/                         # React 前端源码
│   ├── components/              # 通用 UI 组件
│   │   ├── ai/                  # AI 功能组件（Nl2SqlPanel, DiagnosisPanel, ExplainPanel, AiChatPanel, NlFilterInput, WorkflowPanel）
│   │   ├── chart/               # 图表可视化组件（ChartView, ChartToolbar, AxisConfigurator, ChartCanvas, renderers/）
│   │   ├── connection/          # 连接表单组件
│   │   ├── DataTable/           # 数据表格组件
│   │   └── ui/                  # 基础 UI 组件（Button, Dialog, Input, Select 等）
│   ├── windows/                 # 各窗口页面（main, connection, settings, backup, data-sync, new-connection, workflow）
│   ├── stores/                  # Zustand 状态（aiStore, connectionStore, queryStore, schemaStore 等）
│   ├── commands/                # Tauri IPC 命令封装（ai.ts, connection.ts, query.ts 等）
│   ├── lib/                     # 工具库（databaseTypes, sqlDialects, connectionViews, windowManager, extractSql, chart/）
│   ├── plugins/generated.ts     # 自动生成的插件注册（勿手动编辑）
│   ├── plugin-sdk/              # 插件前端 SDK
│   ├── hooks/                   # React hooks（useI18n, useTheme, usePlatform 等）
│   ├── locales/                 # i18n 翻译文件（zh-CN, en）
│   └── types/                   # TypeScript 类型定义
├── src-tauri/                   # Rust 后端
│   ├── src/
│   │   ├── ai/                  # AI 模块（openai, anthropic, custom, registry, context, prompt）
│   │   ├── commands/            # Tauri IPC 命令实现（ai, connection, query, schema, mcp 等）
│   │   ├── db/                  # 数据库驱动（postgres, mysql, sqlite, redis_driver, registry）
│   │   ├── mcp/                 # MCP 模块（server, client, workflows）
│   │   ├── services/            # 服务层（ConnectionManager, QueryExecutor）
│   │   ├── cache/               # 缓存（SchemaCache）
│   │   ├── store/               # 持久化存储（Store — 连接、设置、AI 配置加密存储）
│   │   ├── sync/                # 数据同步（adapter 注册、DDL 生成、IR 中间表示）
│   │   ├── ssh_tunnel.rs        # SSH 隧道
│   │   └── lib.rs               # 入口（AppState、初始化、菜单）
│   ├── tests/                   # 集成测试
│   └── Cargo.toml
├── packages/
│   ├── driver-api/              # 公共数据库驱动 API crate（traits + types + inventory 宏）
│   └── ai-api/                  # 公共 AI Provider API crate（AiProvider trait + AiError + factory）
├── scripts/resolve-plugins.mjs  # 插件解析构建脚本
├── plugins-registry.json        # 插件注册表
├── .plugins/                    # 构建时生成的插件目录（gitignored）
├── e2e/                         # WebdriverIO E2E 测试
├── test/                        # 手工黑盒测试（测试计划、用例、结果、Bug 报告）
├── docs/                        # 文档（RFC、架构设计、进度跟踪、代码审查报告）
└── Cargo.toml                   # Workspace 根配置
```

## 核心架构模式

### 插件系统

DataZen 采用**编译时插件系统**（类似 Caddy 2）：

1. `plugins-registry.json` 定义所有可用插件
2. `scripts/resolve-plugins.mjs` 在构建前执行：
   - 从 Git 克隆插件到 `.plugins/`
   - 生成 `src/plugins/generated.ts`（前端集成）
   - 生成 `.plugin-features.json`（Cargo features）
3. 插件通过 `inventory` crate 实现链接时自动注册

**控制构建包含哪些插件**：
```bash
pnpm tauri:dev                         # 不含插件的开发模式
pnpm tauri:dev --plugins=kiwi          # 含 kiwi 插件
pnpm tauri:dev --plugins=kiwi,olap     # 含多个插件
DATAZEN_PLUGINS=all pnpm build         # 所有插件（默认）
DATAZEN_PLUGINS=none pnpm build        # 仅内置驱动
```

### 数据库驱动注册

- **内置驱动**（PostgreSQL, MySQL, MariaDB, SQLite, Redis）在 `src-tauri/src/db/registry.rs` 直接注册
- **插件驱动** 通过 `datazen-driver-api` 的 `register_driver!` 宏 + `inventory` 自动发现
- 前端 `DB_REGISTRY` 由 `src/lib/databaseTypes.ts`（内置）+ `src/plugins/generated.ts`（插件）合并

### 查询取消

各驱动实现 `cancel_query()` trait 方法，前端通过 `queryStore.cancelQuery` 调用：

| 驱动 | 取消机制 |
|------|---------|
| PostgreSQL | `pg_cancel_backend(pid)`：通过 `pg_stat_activity` 查找同数据库活跃查询并取消 |
| MySQL | `KILL QUERY thread_id`：通过 `information_schema.processlist` 查找活跃线程并终止 |
| SQLite | No-op（进程内单连接，无独立取消机制） |
| Redis | No-op（命令为原子操作） |

### AI 模块架构

AI 功能采用与数据库驱动相同的 **Provider 抽象 + Registry** 模式：

```
packages/ai-api/          → AiProvider trait + AiError + AiProviderFactory (inventory)
src-tauri/src/ai/
├── openai.rs              → OpenAI Chat Completions / Responses API
├── anthropic.rs           → Anthropic Messages API
├── custom.rs              → 自定义 OpenAI 兼容 Provider（支持远程模型列表获取）
├── registry.rs            → AiProviderRegistry（动态注册/获取 Provider）
├── context.rs             → SchemaContextBuilder（DDL 上下文构建，token 预算控制）
└── prompt.rs              → PromptBuilder（多语言 prompt 模板，随 i18n 设置切换）
```

**AI IPC 命令**（`src-tauri/src/commands/ai.rs`）：
- `ai_generate_sql` — NL2SQL（流式，通过 Tauri Events 推送，含 `extractSqlFromResponse` 后处理）
- `ai_diagnose_error` — SQL 错误诊断
- `ai_analyze_explain` — EXPLAIN 计划 AI 分析
- `ai_chat` — AI 对话（流式，支持思考/回答分离）
- `ai_parse_filter` — 自然语言筛选解析
- `ai_generate_schema_doc` — Schema 文档生成
- `ai_diagnose_connection` — 连接故障排查
- `ai_analyze_queries` — 查询历史分析
- `workflow_list` / `workflow_execute` / `workflow_save` / `workflow_delete` / `workflow_reload` — Workflows 管理

**前端 AI 组件**（`src/components/ai/`）：
- `Nl2SqlPanel` — 自然语言转 SQL 输入面板
- `DiagnosisPanel` — SQL 错误诊断结果展示
- `ExplainPanel` — EXPLAIN 可视化 + AI 分析
- `AiChatPanel` — 侧边栏 AI 对话面板（含 Workflows 标签页，支持推理过程折叠显示）
- `NlFilterInput` — 自然语言筛选输入
- `WorkflowPanel` — Workflows 管理和执行（嵌入 ConnectionView 侧边栏）
- `WorkflowWindow` — Workflow 独立窗口（`src/windows/workflow/`），采用 ConnectionWindow 风格的 tab 系统展示执行结果

### MCP（Model Context Protocol）

DataZen 同时作为 **MCP Server** 和 **MCP Client**：

- **MCP Server**（`src-tauri/src/mcp/server.rs`）— 暴露数据库操作给外部 LLM 应用（Claude Desktop、Cursor 等）
  - Tools: `list_connections`, `list_databases`, `list_tables`, `query`, `get_schema`, `explain_query`, `describe_table`, `list_workflows`, `run_workflow`
  - Resources: `datazen://connections`, `datazen://query-history`, `datazen://schema/{id}/{db}`, `datazen://workflows`
  - Prompts: `nl2sql`, `diagnose_error`, `explain_plan`
- **MCP Client**（`src-tauri/src/mcp/client.rs`）— 连接外部 MCP Server 获取工具能力
- **Workflows**（`src-tauri/src/mcp/workflows.rs`）— 用户自定义 AI 工作流（YAML 定义、变量替换、SQL + AI 步骤执行）
  - 支持跨数据库查询（通过 `connection` 变量绑定不同连接）
  - 步骤类型：`Query`（SQL 查询）、`Ai`（AI 推理）、`Condition`（条件分支）、`ForEach`（循环）
  - 模板引擎支持深层 JSON 路径（`steps.<id>.rows.0.field`）和通配符（`steps.<id>.rows.*.field`）
  - 错误处理策略：`abort`、`skip`、`fallback`
  - 执行历史持久化（`src-tauri/src/mcp/workflow_history.rs`）

### 图表可视化

查询结果支持表格/图表双视图切换，基于 **Recharts** 实现：

```
QueryPanel
├── [📋 表格] [📈 图表]  ← SegmentedControl 视图切换
├── 表格视图 — ResultTable（DataTable）
└── 图表视图 — ChartView
    ├── ChartToolbar           — 图表类型 + 选项 + NL输入 + 导出
    ├── AxisConfigurator       — 轴/字段/聚合/排序/配色配置
    └── ChartCanvas            — 图表渲染（5种类型）
        ├── BarChartRenderer
        ├── LineChartRenderer
        ├── PieChartRenderer
        ├── ScatterChartRenderer
        └── AreaChartRenderer
```

**数据流**：
1. `inferAllFields()` — 从 `StatementResult` 推断字段类型（numeric/datetime/categorical）
2. `recommendChart()` — 基于字段类型规则推荐图表类型和轴配置
3. `transformData()` — 将 `StatementResult` + `ChartConfig` 转换为 Recharts 数据点
4. 配置持久化在 `queryStore.QueryTab.chartConfig`

**核心模块**（`src/lib/chart/`）：
- `fieldInference.ts` — 字段类型推断
- `recommend.ts` — 智能推荐引擎
- `transform.ts` — 数据转换 + 聚合 + 排序
- `colors.ts` — 5 套配色方案
- `format.ts` — 数值千分位格式化
- `nlConfig.ts` — 自然语言图表配置解析
- `export.ts` — PNG/SVG 导出

**类型**：`src/types/chart.ts` — `ChartType`, `ChartConfig`, `ChartField`, `ChartDataPoint`

**功能**：
- 5 种图表类型（柱/线/饼/散/面积）+ 智能推荐
- 多 Y 轴支持 + 分组 + 聚合（sum/avg/count/min/max）
- 图表↔表格联动（点击数据点跳转表格行）
- NL2SQL「应用并图表化」（生成 SQL → 自动执行 → 切换图表）
- 自然语言调整配置（"换成饼图"、"按销量排序"）
- 导出 PNG/SVG、配色切换、数值标签
- 大数据集采样（>1000 行自动截断）

### 前端约定

- **零硬编码**：行为差异通过 `DB_REGISTRY` 元数据驱动，避免 `if (type === 'xxx')`
- **表单路由**：`ConnectionFormBody.tsx` 通过 `connectionForm` 字段选择渲染哪个表单组件
- **表单验证**：`useConnectionForm.ts` 的 `validate()` 在 `onTest()` / `onSave()` 前检查必填字段（host/port 或 database 文件路径），失败时 `validationErrors` 驱动红色边框 + 错误提示
- **视图路由**：`connectionViews/index.ts` 的 `CONNECTION_VIEWS` 映射视图组件
- **SQL 方言**：`sqlDialects/` 下的策略对象处理 DDL/索引/备份差异
- **SQL 编辑器方言**：`SqlEditor` 根据连接的 `databaseType` 动态选择 CodeMirror 方言
- **多窗口管理**：`windowManager.ts` 通过 Rust 命令 `create_sub_window` 创建原生窗口（确保 macOS `acceptFirstMouse`）。窗口种类由 `windowKind.ts` 的 URL 参数路由，`App.tsx` 按 kind 懒加载不同窗口组件
- **ErrorBoundary**：全局错误边界组件防止白屏

### IPC 通信

前端通过 `src/commands/` 调用后端，后端实现在 `src-tauri/src/commands/`。两侧按领域对齐：
- `connection.rs` / `connection.ts` — 连接管理
- `query.rs` / `query.ts` — SQL 查询
- `schema.rs` / `schema.ts` — Schema 操作
- `kv.rs` / `kv.ts` — Redis 键值操作
- `ai.rs` / `ai.ts` — AI 功能
- `mcp.rs` — MCP Server/Client 管理
- `window.rs` — 原生窗口创建

### 错误处理

后端 IPC 命令使用结构化 `CommandError` 枚举（`src-tauri/src/commands/error.rs`）：
- 分类：`Store`, `Connection`, `Driver`, `Ai`, `Io`, `Json`, `NotFound`, `NotConfigured`, `Validation`, `Internal`
- 序列化为纯字符串以保持前端兼容
- `CmdExt` trait 提供统一的错误日志记录和转换
- AI JSON 解析使用 `parse_ai_json()` 辅助函数，自动检测 `finish_reason` 截断并给出友好提示
- 所有字符串截断使用 `truncate_str()` 确保在 UTF-8 字符边界上截断，避免 panic

### AI 流式响应

`StreamChunk` 包含 `content` 和 `reasoning` 两个独立字段：
- `content` — 正式回答内容
- `reasoning` — 模型思考/推理过程（`reasoning_content` from OpenAI/DeepSeek）
- 前端 `aiStore` 分别累积两个字段，完成后合并到 `AiChatMessage`
- 聊天界面将推理过程渲染为可折叠区域（默认折叠）
- NL2SQL 在流完成时通过 `extractSqlFromResponse()` 过滤非 SQL 内容

## 开发命令

```bash
pnpm install                           # 安装依赖
pnpm dev                               # 启动 Vite dev server
pnpm tauri dev                         # 启动完整开发模式（前端 + Rust）
pnpm tauri:dev --plugins=kiwi          # 带插件的开发模式
pnpm build                             # 构建前端（含 resolve-plugins）
pnpm tauri build                       # 构建完整应用
npx vitest run                         # 运行单元测试
pnpm e2e                               # 运行 E2E 测试
cargo test -p datazen                  # 运行 Rust 单元测试
cargo test -p datazen-ai-api           # 运行 AI API 单元测试
```

## 代码风格

- **Rust**：标准 `rustfmt` 格式，使用 `thiserror` 处理错误，`tracing` 记录日志，`CommandError` 统一 IPC 错误
- **TypeScript**：严格模式，无 `any`（除 generated 文件），使用 absolute imports
- **CSS**：Tailwind CSS utility classes，暗色主题为默认
- **安全**：CSP 策略限制脚本/连接源，Argon2id 密码派生，路径遍历防护

## 重要注意事项

- `src/plugins/generated.ts` 是自动生成的，修改后会被覆盖
- `.plugins/` 目录是 gitignored 的，运行 `pnpm build` 或 `node scripts/resolve-plugins.mjs` 会自动生成
- Cargo workspace 在项目根目录，`target/` 也在根目录
- 协议版本 `PROTOCOL_VERSION`（在 `packages/driver-api/src/lib.rs`）变更时需同步更新所有插件
- `AI_PROTOCOL_VERSION`（在 `packages/ai-api/src/lib.rs`）变更时需同步更新所有 AI Provider 插件
- AI 配置（API Key 等）加密存储在 `ai_config.enc`，不会出现在日志中
- Prompt 模板支持 i18n，自动跟随应用语言设置
