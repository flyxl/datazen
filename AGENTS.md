# AGENTS.md

> 本文件面向 AI 编程助手，帮助其快速理解项目结构和约定。详细架构设计见 [docs/architecture/](docs/architecture/README.md)。

## 项目概述

DataZen 是一个跨平台桌面数据库管理工具，基于 **Tauri v2**（Rust 后端 + React 前端）构建，集成 AI 辅助功能。

- **框架**：Tauri v2 + React 18 + TypeScript + Tailwind CSS 4
- **包管理**：pnpm（前端）、Cargo workspace（Rust）
- **状态管理**：Zustand
- **测试**：Vitest（单元）、WebdriverIO（E2E）、手工黑盒测试（`test/`）
- **AI**：多 Provider（OpenAI / Anthropic / DeepSeek / Custom）、MCP Server/Client
- **运行模式**：GUI 桌面应用 或 无头 MCP stdio 服务器（`--mcp-stdio`）

## 目录结构

```text
datazen/
├── src/                         # React 前端源码
│   ├── components/              # UI 组件（ai/, chart/, connection/, DataTable/, ui/）
│   ├── windows/                 # 窗口页面（main, connection, settings, workflow, backup, data-sync）
│   │   └── connection/er/       # ER 图模块（React Flow）
│   ├── stores/                  # Zustand stores（8 个：ai, connection, query, schema, settings 等）
│   ├── commands/                # Tauri IPC 封装
│   ├── lib/                     # 工具库
│   ├── hooks/                   # React hooks
│   ├── locales/                 # i18n
│   ├── plugins/generated.ts     # 自动生成（勿手动编辑）
│   └── plugin-sdk/              # 插件前端 SDK
├── src-tauri/                   # Rust 后端
│   ├── src/
│   │   ├── ai/                  # AI Provider / protocol / context
│   │   ├── commands/            # Tauri IPC 命令
│   │   ├── db/                  # DriverRegistry
│   │   ├── theme/               # 运行时主题包
│   │   ├── mcp/                 # MCP Server/Client
│   │   ├── workflow/             # YAML Workflow 引擎
│   │   ├── services/            # ConnectionManager, QueryExecutor, DbTools
│   │   ├── cache/               # SchemaCache
│   │   ├── store/               # AES-256-GCM 加密持久化
│   │   └── sync/                # 跨库同步
│   └── resources/               # 菜单翻译、Prompt 模板
├── packages/
│   ├── driver-api/              # DatabaseDriver + Command API + inventory + ReuseDriver
│   ├── ai-api/                  # AiProvider trait + factory
│   ├── drivers/                 # 可选 path 驱动 + http-support
│   └── themes/                  # 主题包预留
├── e2e/                         # WebdriverIO E2E 测试
├── test/                        # 手工黑盒测试
├── docs/                        # 架构文档、RFC、进度
└── .plugins/                    # 构建时生成（gitignored）
```

## 核心架构模式

### 驱动选型（编译时，类似 Caddy 2）

1. `drivers-registry.json` 定义 path 驱动 + git 驱动；Git 可钉 `ref`
2. `scripts/resolve-drivers.mjs` 构建前执行选型、克隆 Git driver，并生成 `generated.ts`、`plugin_init.rs`、`.plugin-features.json`
3. 通过 `inventory` crate 实现链接时自动注册；宿主 `DriverRegistry` 仅走 factories

```bash
pnpm tauri:dev                         # 默认 basic
pnpm tauri:dev --drivers=all
pnpm tauri:dev --drivers=basic,kiwi,superset
pnpm tauri:dev --drivers=postgres,mongodb,kiwi
DATAZEN_DRIVERS=all pnpm tauri:dev
DATAZEN_DRIVERS=all pnpm tauri:build
```

### 数据库驱动

- Path 驱动：`packages/drivers/*`（crate 名 `datazen-driver-<id>`），经 optional Cargo feature 注入
- Git 驱动：克隆到 `.plugins/`，同样 inventory 注册
- 前端 `DB_REGISTRY` 合并 `generated.ts` 的 `DRIVER_DB_ENTRIES`
- 默认 DB 图标来自 `packages/drivers/*/ui/icons/{dbType}.svg`
- 关键 trait 方法包括 `supports_offset()`、`supports_explain()`、`prompt_overrides()`

### Driver Command API

Driver 不再只以 SQL Query/Execute 作为扩展边界。`packages/driver-api` 提供统一 Command 抽象：

```text
DatabaseDriver
├── command_definitions()
│      └── DriverCommandDefinition
│           ├── name / description / input_schema
│           └── metadata (category, risk, workflow, ui, deprecated, requiresConnection)
│
└── execute_command(command, input)
       └── CommandResult
```

- `query` / `execute` 是标准 Command，并有默认实现，保持现有 SQL Driver 兼容。
- 非 SQL 驱动应覆盖 `command_definitions()`：改 `sql` 字段 title，只读驱动不要暴露 `execute`。
- Driver 可以通过 `command_definitions()` 暴露 Driver-specific Command。
- `execute_command()` 是 Driver-specific 能力的统一执行入口。
- Workflow、IPC、前端 Command Editor 都依赖 Command Definition，而不是按具体 Driver 类型硬编码。
- `metadata.workflow = false` 的 Command 不进入 Workflow 选择器，workflow runtime 也会拒绝。
- `ReuseDriver` 必须转发 Command discovery 与 execution。
- `metadata.requiresConnection = false` 的 Command（如 Kiwi `login`）可通过 `driverType` 执行，不必先有 Connection。
- Kiwi `login` / `list_instances` 走 `execute_driver_command({ driverType: 'kiwi', command })`，不再使用 `plugin:kiwi|*`。

### Redis 驱动（E1–E4）

深度能力集中在 `packages/drivers/redis`（UI + Driver Command API），宿主 `RedisConnectionView` 仅为薄 Tab 壳；**禁止** Host 按 `pluginId === 'redis'` 写设置分支。 Redis Tauri plugin 仅用于安装 Pub/Sub 事件 sink，操作一律走 `execute_command` / `execute_driver_command`。

- Tabs：Workbench / Console / Monitor / Pub/Sub
- 拓扑：Standalone / Cluster / Sentinel + mTLS
- 连接扩展字段走 `ConnectionConfig.options`
- 设置位于 `AppSettings.pluginSettings.redis`
- UI 使用语义主题色

### AI 模块

- Provider：OpenAI / Anthropic / DeepSeek / Custom
- `ai/protocol/` 共享 HTTP 协议实现
- `PromptResolver` 优先级：用户覆盖 → 驱动覆盖 → 资源文件 → 编译时英文嵌入
- `@` 上下文引用有白名单、512KB 限制和路径遍历防护
- `StreamChunk` 区分 `content` 与 `reasoning`

### MCP

- Server（`mcp/server.rs`）暴露 Tools/Resources/Prompts，包括 `list_workflows` / `run_workflow`
- DB tools/prompts 使用持久化 connection ID，即 `config_id`
- Client（`mcp/client.rs`）连接外部 MCP Server
- `main.rs --mcp-stdio` 启动无头 MCP server

## Workflows

Workflow 是 YAML 驱动的通用执行引擎，GUI、Tauri IPC 和 MCP 共用同一 runtime。

模块拆分：

```text
src-tauri/src/workflow/
├── model.rs             # WorkflowDefinition / WorkflowStep 等数据模型
├── registry.rs          # YAML 注册、加载、保存、删除
├── context.rs           # 模板变量、路径解析、循环上下文
├── conditions.rs        # Condition 求值
├── executor.rs          # WorkflowExecutor / 步骤编排 / 错误策略
├── command.rs           # Workflow Command Step
├── command_runtime.rs   # Connection 解析、Command discovery、Driver 执行
├── history.rs           # 执行历史
└── workflows.rs         # 兼容 facade / re-export
```

核心执行链：

```text
WorkflowDefinition
      ↓
WorkflowExecutor
      ↓
WorkflowStep::Command
      ↓
command_runtime
      ↓
resolve effective connection
      ↓
Driver::command_definitions()
      ↓
validate / resolve input
      ↓
Driver::execute_command()
```

### Connection 继承

Workflow 可以定义默认 `connection`。Step 未指定 connection 时继承它；Step 显式指定时覆盖默认值。因此一个 connection 下可以连续执行多个 Step，而不必重复选择 connection。

### Legacy Query 兼容

旧版配置仍支持：

```yaml
type: query
connection: mysql-prod
database: reporting
sql: SELECT * FROM users
```

执行前会规范化为内部 `Command("query")`，并保留 `database` 等旧字段语义。旧 Query 和新 Command 进入同一执行路径。

### Command Discovery / UI

Workflow UI 不应硬编码 Driver Command。编辑 Command Step 时：

```text
Effective Connection
      ↓
get_connection_commands()
      ↓
Driver::command_definitions()
      ↓
Command selector
      ↓
input_schema
      ↓
schema-driven input editor
```

Connection 改变后重新 discovery；没有 Step override 时使用 Workflow 默认 connection。

详细设计：[Workflow 架构文档](docs/architecture/backend/workflow.md)；用户手册：[docs/workflow-guide.md](docs/workflow-guide.md)。

### 运行时主题包

与驱动选型完全独立：

- 安装路径：`{appData}/themes/{id}/`
- 设置：`theme: { mode: 'light'|'dark'|'system', packId: string | null }`
- IPC：`list_theme_packs`、`install_theme_pack_with_dialog`、`remove_theme_pack`、`read_theme_pack_file`
- Rust：`src-tauri/src/theme/`、`commands/theme.rs`
- 前端：`src/commands/theme.ts`、`src/lib/themePackApply.ts`、`src/lib/iconResolver.ts`
- 商店/CDN 下载暂未实现

## 前端约定

- 零硬编码：行为差异通过 `DB_REGISTRY` + `DatabaseTypeMeta` 元数据驱动
- `ConnectionFormBody.tsx` 通过 `connectionForm` 选择表单
- `connectionViews/index.ts` 映射 `connectionMode` → 视图
- `hasMultiDatabase` 表示驱动能力；切库走 `use_database`
- 多窗口：`windowManager.ts` + `windowKind.ts` URL 参数路由
- IPC：前端 camelCase，Rust snake_case；Tauri 自动映射

## IPC 通信

前端 `src/commands/` ↔ 后端 `src-tauri/src/commands/`，按领域对齐：`connection`、`database/schema`、`query`、`ai`、`context`、`settings/config`、`theme`、`file`、`adb`、`kv`、`backup`、`sync`、`mcp`、`window`。

Driver Command IPC 负责：

- 获取指定 Connection 支持的 Command Definitions
- 获取 Driver 支持的 Command Definitions（无需 live Connection）
- 执行指定 Driver Command（`connectionId` 或 `driverType`；后者仅允许 `requiresConnection = false`）
- SQL 编辑器 / `queryCommands.executeQuery` 走 `execute_driver_command` 的 `query` Command；兼容 IPC `execute_query` 也转发到同一路径

## 错误处理

`CommandError`（`commands/error.rs`）覆盖 Store / Connection / Driver / Ai / Io / Json / NotFound / NotConfigured / Validation / Internal；`CmdExt` 统一日志。

## 关键功能模块

| 功能 | 前端入口 | 后端入口 |
|------|---------|---------|
| 图表可视化 | `components/chart/` + `lib/chart/` | — |
| ER 图 | `windows/connection/ErDiagramView.tsx` + `er/` | `commands/schema.rs → get_er_data` |
| 数据导出 | `DataTable/DataExportDialog.tsx` + `lib/exportData.ts` | — |
| AI Chat | `components/ai/AiChatPanel.tsx` | `commands/ai.rs` |
| Workflows | `windows/workflow/WorkflowWindow.tsx` | `workflow/executor.rs` / `workflow/command_runtime.rs` |
| 数据同步 | `windows/data-sync/` | `sync/` |
| Redis 深度运维 | `packages/drivers/redis/ui/*` | `execute_command` / `execute_driver_command` |
| 主题包 | `windows/settings/ThemePackSection.tsx` | `theme/` + `commands/theme.rs` |

## 开发命令

```bash
pnpm install                           # 安装依赖
pnpm dev                               # Vite dev server
pnpm tauri:dev                         # 完整开发（前端 + Rust；默认 basic 驱动）
pnpm build                             # 构建前端（不 inject；打包前由外层 resolve）
pnpm build:with-drivers                # 单独前端构建并 inject/restore
pnpm tauri:build                       # 完整应用（外层 inject 一次）
npx vitest run                         # Host 前端单元测试（不含 packages/drivers）
pnpm test:unit:drivers                 # Path 驱动 UI 单测（含 Redis）
cargo test -p datazen                  # Rust 单元测试
```

### E2E

完整流程见 [docs/e2e-testing.md](docs/e2e-testing.md)。

1. 必须使用 Tauri CLI 构建：`pnpm tauri build --debug --features webdriver`
2. 禁止裸 `cargo build --features webdriver` 作为 E2E 二进制
3. 必须启用 `webdriver` feature，监听 `127.0.0.1:4445`

```bash
pnpm e2e                               # 完整构建（webdriver）+ 跑全部 Host E2E（推荐首次）
pnpm e2e:minimal                       # 更快：DATAZEN_DRIVERS=basic，跳过 Git / 非核心 path 驱动
pnpm e2e:skip-build                    # 跳过构建（仅当已有合格的 webdriver debug 二进制）
pnpm e2e:skip-build -- --spec e2e/specs/path-ipc-hardening.ts
pnpm e2e:core                          # 核心 UI（默认 skip-build）
pnpm e2e:db / e2e:ai                   # 分组
pnpm e2e:redis                         # Redis 深度 E2E（显式；specs 在 packages/drivers/redis/e2e/；不进默认 e2e）
pnpm e2e:i18n-backup / e2e:path-ipc    # 备份·i18n / 路径 IPC
# Kiwi E2E：在 datazen-driver-kiwi 仓执行 `pnpm e2e:kiwi`（不进 Host 默认 pnpm e2e；Host `pnpm e2e:kiwi` 仅提示并 exit 1）
```

PR 合并前：`pnpm test:unit` + `cargo test -p datazen --lib`（见 `.github/workflows/ci.yml`）。Path 驱动 UI 单测：`pnpm test:unit:drivers`（**不**含在 `pnpm test:unit` 内）。代码审查修复对照：[docs/code-review-2026-08-07-full.md](docs/code-review-2026-08-07-full.md)、[docs/progress-code-review-fix.md](docs/progress-code-review-fix.md)。

编排脚本：`e2e/run.mjs`。环境变量：复制 `e2e/.env.example` → `e2e/.env`。

## 代码风格

- Rust：`rustfmt` + `thiserror` + `tracing` + `CommandError`
- TypeScript：严格模式，无 `any`（除 generated 文件），absolute imports
- CSS：Tailwind utility classes，暗色主题默认
- 安全：CSP、AES-256-GCM、路径遍历防护、文件扩展名白名单

## 重要注意事项

- Path 驱动 Rust crate：`datazen-driver-<id>`；Git 驱动 Rust crate 名以插件仓库为准
- `Cargo.toml` 中的插件占位段在 git 中应保持为空；`resolve-drivers.mjs` 构建时填充
- `src/plugins/generated.ts` 和 `src-tauri/src/plugin_init.rs` 是自动生成的，git 中必须保持 stub；禁止提交已注入内容
- `.plugins/` 是 gitignored，由 driver resolve/build/dev 流程生成
- `PROTOCOL_VERSION`（`packages/driver-api`）变更时需同步更新所有插件
- `AI_PROTOCOL_VERSION`（`packages/ai-api`）变更时需同步更新所有 AI Provider 插件
- AI 配置加密存储在 `ai_config.enc`，不会出现在日志中
- Prompt 模板在 `src-tauri/resources/prompts/{lang}/`，支持用户覆盖
- 日志文件位于 `{data_dir}/logs/`
- 主题包与驱动选型独立：`{appData}/themes/` 不由 `resolve-drivers.mjs` 管理；删除主题包不影响 `.plugins/`
