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

```
datazen/
├── src/                         # React 前端源码
│   ├── components/              # UI 组件（ai/, chart/, connection/, DataTable/, ui/）
│   ├── windows/                 # 窗口页面（main, connection, settings, workflow, backup, data-sync）
│   │   └── connection/er/       # ER 图模块（React Flow）
│   ├── stores/                  # Zustand stores（8 个：ai, connection, query, schema, settings 等）
│   ├── commands/                # Tauri IPC 封装（ai, connection, database, query, context, settings, file, adb）
│   ├── lib/                     # 工具库（chart/, sqlDialects/, connectionViews/, exportData 等）
│   ├── hooks/                   # React hooks（useI18n, useResizable, usePlatform 等）
│   ├── locales/                 # i18n（10 语系：en/zh-CN/zh-TW 完整，其余 Beta；~739 keys）
│   ├── plugins/generated.ts     # 自动生成（勿手动编辑）
│   └── plugin-sdk/              # 插件前端 SDK
├── src-tauri/                   # Rust 后端
│   ├── src/
│   │   ├── ai/                  # AI（openai, anthropic, deepseek, custom, registry, context, prompt_resolver, protocol/）
│   │   ├── commands/            # IPC 命令（16 个模块：ai, connection, query, schema, context, mcp 等）
│   │   ├── db/                  # 驱动（postgres, mysql, sqlite, redis_driver, registry）
│   │   ├── mcp/                 # MCP Server/Client
│   │   ├── workflow/            # YAML Workflow 引擎与执行历史（独立于 mcp）
│   │   ├── services/            # ConnectionManager, QueryExecutor, DbTools
│   │   ├── cache/               # SchemaCache（两级 TTL）
│   │   ├── store/               # AES-256-GCM 加密持久化
│   │   └── sync/                # 跨库同步（IR 中间表示 + 适配器）
│   └── resources/               # 菜单翻译（menu-labels.json）、Prompt 模板（prompts/）
├── packages/
│   ├── driver-api/              # DatabaseDriver trait + types + inventory 宏
│   └── ai-api/                  # AiProvider trait + AiError + factory
├── e2e/                         # WebdriverIO E2E 测试（35 spec）
├── test/                        # 手工黑盒测试
├── docs/                        # 架构文档、RFC、进度（含 [代码审查修复进度](docs/progress-code-review-fix.md)）
└── .plugins/                    # 构建时生成（gitignored）
```

## 核心架构模式

### 插件系统（编译时，类似 Caddy 2）

1. `plugins-registry.json` 定义插件（4 内置 + 3 Git 外部：kiwi, olap, superset）；Git 插件可钉 `ref`（commit/tag）
2. `scripts/resolve-plugins.mjs` 构建前执行：克隆/检出 ref → 生成 `generated.ts` + `plugin_init.rs` + `.plugin-features.json`
3. 通过 `inventory` crate 实现链接时自动注册

```bash
pnpm tauri:dev                         # 无插件（main 默认）
pnpm tauri:dev --plugins=kiwi          # 含 kiwi 插件
pnpm tauri:dev --plugins=none          # 仅内置驱动（显式）
DATAZEN_PLUGINS=none pnpm tauri:dev    # 环境变量同样生效
DATAZEN_PLUGINS=all pnpm tauri:build   # 全部插件打包
```

### 数据库驱动

- 内置：PostgreSQL, MySQL, MariaDB, SQLite, Redis（`src-tauri/src/db/registry.rs`）
- 插件：通过 `register_driver!` 宏 + `inventory` 注册
- 前端 `DB_REGISTRY`：`databaseTypes.ts`（内置）+ `generated.ts`（插件）合并
- **关键 trait 方法**：`supports_offset()`（默认 true）、`supports_explain()`（默认 true）、`prompt_overrides()`

### AI 模块

- **Provider**：OpenAI / Anthropic / DeepSeek / Custom（三种协议：Chat/Responses/Anthropic 兼容）
- **协议层**：`ai/protocol/` 下共享 HTTP 实现，避免 Provider 间重复代码
- **Prompt**：`PromptResolver` 管理模板，优先级：用户覆盖 → 驱动覆盖 → 资源文件 → 编译时英文嵌入
- **上下文引用**：AI 输入中 `@` 引用本地文件（`commands/context.rs`），白名单 + 512KB 限制 + 路径遍历防护
- **流式响应**：`StreamChunk` 分 `content` + `reasoning` 两个字段

### MCP

- **Server**（`mcp/server.rs`）：暴露 Tools/Resources/Prompts（含 `list_workflows` / `run_workflow` 适配）；DB tools/prompts 入参为 **`config_id`**（持久化连接 ID，来自 `list_connections`）
- **Client**（`mcp/client.rs`）：连接外部 MCP Server，供 AI Chat 工具调用
- **双模式**：`main.rs` 通过 `--mcp-stdio` 启动无头 MCP 服务器

### Workflows

- **Workflows**（`workflow/`）：YAML 定义，步骤类型 Query/Ai/Condition/ForEach；GUI / IPC / MCP 共用引擎

### 前端约定

- **零硬编码**：行为差异通过 `DB_REGISTRY` + `DatabaseTypeMeta` 元数据驱动
- **表单路由**：`ConnectionFormBody.tsx` 通过 `connectionForm` 字段选择表单组件
- **视图路由**：`connectionViews/index.ts` 映射 `connectionMode` → 视图组件
- **多窗口**：`windowManager.ts` + `windowKind.ts` URL 参数路由，`App.tsx` 按 kind 懒加载
- **IPC 约定**：前端 `invoke()` 传参使用 `snake_case` key 与后端对齐

### IPC 通信

前端 `src/commands/` ↔ 后端 `src-tauri/src/commands/`，按领域对齐：
`connection`, `database/schema`, `query`, `ai`, `context`, `settings/config`, `file`, `adb`, `kv`, `backup`, `sync`, `mcp`, `window`

### 错误处理

`CommandError` 枚举（`commands/error.rs`）：Store / Connection / Driver / Ai / Io / Json / NotFound / NotConfigured / Validation / Internal。序列化为字符串，`CmdExt` trait 统一日志。

## 关键功能模块

| 功能 | 前端入口 | 后端入口 |
|------|---------|---------|
| 图表可视化 | `components/chart/` + `lib/chart/` | — |
| ER 图 | `windows/connection/ErDiagramView.tsx` + `er/` | `commands/schema.rs → get_er_data` |
| 数据导出 | `DataTable/DataExportDialog.tsx` + `lib/exportData.ts` | — |
| 路径输入 | `ui/PathInput.tsx` | Tauri Dialog API |
| AI Chat | `components/ai/AiChatPanel.tsx` | `commands/ai.rs` |
| @ 上下文 | `components/ai/ContextPicker.tsx` | `commands/context.rs` |
| Workflows | `windows/workflow/WorkflowWindow.tsx` | `workflow/workflows.rs` |
| 数据同步 | `windows/data-sync/` | `sync/` (IR 适配器) |

## 开发命令

```bash
pnpm install                           # 安装依赖
pnpm dev                               # Vite dev server
pnpm tauri:dev                         # 完整开发（前端 + Rust；默认无插件）
pnpm build                             # 构建前端（不 inject；打包前由外层 resolve）
pnpm build:with-plugins                # 单独前端构建并 inject/restore
pnpm tauri:build                       # 完整应用（外层 inject 一次）
npx vitest run                         # 前端单元测试
cargo test -p datazen                  # Rust 单元测试
```

### E2E 测试（WebdriverIO）

> **完整流程、排错与 Agent 检查清单见 [docs/e2e-testing.md](docs/e2e-testing.md)。**

**硬性要求：**

1. 必须用 **Tauri CLI** 构建：`pnpm tauri build --debug --features webdriver`
2. **禁止**裸 `cargo build --features webdriver`（常见报错：`asset not found: index.html`）
3. 必须启用 `webdriver` feature（监听 `127.0.0.1:4445`）

```bash
pnpm e2e                               # 完整构建（webdriver）+ 跑全部 E2E（推荐首次）
pnpm e2e:minimal                       # 更快：DATAZEN_PLUGINS=none，跳过 Git 插件
pnpm e2e:skip-build                    # 跳过构建（仅当已有合格的 webdriver debug 二进制）
pnpm e2e:skip-build -- --spec e2e/specs/path-ipc-hardening.ts
pnpm e2e:core                          # 核心 UI（默认 skip-build）
pnpm e2e:db / e2e:ai / e2e:kiwi        # 分组
pnpm e2e:i18n-backup / e2e:path-ipc    # 备份·i18n / 路径 IPC
```

PR 合并前：`pnpm test:unit` + `cargo test -p datazen --lib`（见 `.github/workflows/ci.yml`）。代码审查修复对照：[docs/code-review-2026-08-07-full.md](docs/code-review-2026-08-07-full.md)、[docs/progress-code-review-fix.md](docs/progress-code-review-fix.md)。

编排脚本：`e2e/run.mjs`。环境变量：复制 `e2e/.env.example` → `e2e/.env`。

## 代码风格

- **Rust**：`rustfmt` + `thiserror` + `tracing` + `CommandError`
- **TypeScript**：严格模式，无 `any`（除 generated 文件），absolute imports
- **CSS**：Tailwind utility classes，暗色主题默认
- **安全**：CSP、AES-256-GCM 加密存储、路径遍历防护、文件扩展名白名单

## 重要注意事项

- 插件 Git 仓库命名使用 `datazen-driver-xxx` 格式，Rust crate 名称仍为 `datazen-plugin-xxx`
- `Cargo.toml` 中的插件占位段（`<<plugin-dependencies>>`、`<<plugin-features>>`、`<<plugin-patches>>`）在 git 中应保持为空；`resolve-plugins.mjs` 在构建时填充
- `src/plugins/generated.ts` 和 `src-tauri/src/plugin_init.rs` 是自动生成的，修改后会被覆盖
- `.plugins/` 是 gitignored，由 `resolve-plugins.mjs` / `tauri:build` / `tauri:dev` 生成；`pnpm build` 本身不 inject
- `PROTOCOL_VERSION`（`packages/driver-api`）变更时需同步更新所有插件
- `AI_PROTOCOL_VERSION`（`packages/ai-api`）变更时需同步更新所有 AI Provider 插件
- AI 配置加密存储在 `ai_config.enc`，不会出现在日志中
- Prompt 模板在 `src-tauri/resources/prompts/{lang}/`，支持用户覆盖
- 菜单翻译：前端 `src/locales/{zh-CN,en}.ts` 为唯一来源；`scripts/generate-menu-labels.mjs` 生成 `src-tauri/resources/menu-labels.json` 供 Rust 原生菜单使用（`pnpm menu:labels` / build / tauri:dev 自动执行）
- 日志文件在 `{data_dir}/logs/`
- E2E：详见 [docs/e2e-testing.md](docs/e2e-testing.md)；禁止用裸 `cargo build` 当 E2E 二进制
