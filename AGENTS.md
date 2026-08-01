# AGENTS.md

> 本文件面向 AI 编程助手（如 Cursor Agent, GitHub Copilot, Codex 等），帮助其快速理解项目结构和约定。

## 项目概述

DataZen 是一个跨平台桌面数据库管理工具，基于 **Tauri v2**（Rust 后端 + React 前端）构建，集成了 AI 辅助功能。

- **框架**：Tauri v2 + React + TypeScript + Tailwind CSS
- **后端语言**：Rust
- **包管理**：pnpm（前端）、Cargo workspace（Rust）
- **状态管理**：Zustand
- **测试**：Vitest（单元）、WebdriverIO（E2E）
- **AI 集成**：多 Provider 支持（OpenAI / Anthropic / 自定义 OpenAI 兼容）、MCP Server/Client

## 目录结构

```
datazen/
├── src/                         # React 前端源码
│   ├── components/              # 通用 UI 组件
│   │   ├── ai/                  # AI 功能组件（Nl2SqlPanel, DiagnosisPanel, ExplainPanel, AiChatPanel, NlFilterInput, SkillsPanel）
│   │   ├── connection/          # 连接表单组件
│   │   ├── DataTable/           # 数据表格组件
│   │   └── ui/                  # 基础 UI 组件（Button, Dialog, Input, Select 等）
│   ├── windows/                 # 各窗口页面（main, connection, settings, backup, data-sync, new-connection）
│   ├── stores/                  # Zustand 状态（aiStore, connectionStore, queryStore, schemaStore 等）
│   ├── commands/                # Tauri IPC 命令封装（ai.ts, connection.ts, query.ts 等）
│   ├── lib/                     # 工具库（databaseTypes, sqlDialects, connectionViews, windowManager）
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
│   │   ├── mcp/                 # MCP 模块（server, client, skills）
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
├── docs/                        # 文档（RFC、进度跟踪、代码审查报告）
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
- `ai_generate_sql` — NL2SQL（流式，通过 Tauri Events 推送）
- `ai_diagnose_error` — SQL 错误诊断
- `ai_analyze_explain` — EXPLAIN 计划 AI 分析
- `ai_chat` — AI 对话（流式）
- `ai_parse_filter` — 自然语言筛选解析
- `ai_generate_schema_doc` — Schema 文档生成
- `ai_diagnose_connection` — 连接故障排查
- `ai_analyze_queries` — 查询历史分析
- `ai_list_skills` / `ai_execute_skill` / `ai_save_skill` / `ai_delete_skill` / `ai_reload_skills` — Skills 管理

**前端 AI 组件**（`src/components/ai/`）：
- `Nl2SqlPanel` — 自然语言转 SQL 输入面板
- `DiagnosisPanel` — SQL 错误诊断结果展示
- `ExplainPanel` — EXPLAIN 可视化 + AI 分析
- `AiChatPanel` — 侧边栏 AI 对话面板（含 Skills 标签页）
- `NlFilterInput` — 自然语言筛选输入
- `SkillsPanel` — Skills 管理和执行

### MCP（Model Context Protocol）

DataZen 同时作为 **MCP Server** 和 **MCP Client**：

- **MCP Server**（`src-tauri/src/mcp/server.rs`）— 暴露数据库操作给外部 LLM 应用（Claude Desktop、Cursor 等）
  - Tools: `list_connections`, `list_databases`, `list_tables`, `query`, `get_schema`, `explain_query`, `describe_table`, `list_skills`, `run_skill`
  - Resources: `datazen://connections`, `datazen://query-history`, `datazen://schema/{id}/{db}`, `datazen://skills`
  - Prompts: `nl2sql`, `diagnose_error`, `explain_plan`
- **MCP Client**（`src-tauri/src/mcp/client.rs`）— 连接外部 MCP Server 获取工具能力
- **Skills**（`src-tauri/src/mcp/skills.rs`）— 用户自定义 AI 工作流（YAML 定义、变量替换、SQL + AI 步骤执行）

### 前端约定

- **零硬编码**：行为差异通过 `DB_REGISTRY` 元数据驱动，避免 `if (type === 'xxx')`
- **表单路由**：`ConnectionFormBody.tsx` 通过 `connectionForm` 字段选择渲染哪个表单组件
- **视图路由**：`connectionViews/index.ts` 的 `CONNECTION_VIEWS` 映射视图组件
- **SQL 方言**：`sqlDialects/` 下的策略对象处理 DDL/索引/备份差异
- **SQL 编辑器方言**：`SqlEditor` 根据连接的 `databaseType` 动态选择 CodeMirror 方言
- **多窗口管理**：`windowManager.ts` 通过 Rust 命令 `create_sub_window` 创建原生窗口（确保 macOS `acceptFirstMouse`）
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
