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
│   ├── commands/                # Tauri IPC 封装（ai, connection, database, query, context, settings, theme, file, adb）
│   ├── lib/                     # 工具库（chart/, sqlDialects/, connectionViews/, themePackApply, iconResolver 等）
│   ├── hooks/                   # React hooks（useI18n, useResizable, usePlatform 等）
│   ├── locales/                 # i18n（10 语系：en/zh-CN/zh-TW 完整，其余 Beta；~739 keys）
│   ├── plugins/generated.ts     # 自动生成（勿手动编辑）
│   └── plugin-sdk/              # 插件前端 SDK
├── src-tauri/                   # Rust 后端
│   ├── src/
│   │   ├── ai/                  # AI（openai, anthropic, deepseek, custom, registry, context, prompt_resolver, protocol/）
│   │   ├── commands/            # IPC 命令（17 个模块：ai, connection, query, schema, context, theme, mcp 等）
│   │   ├── db/                  # DriverRegistry（inventory-only；实现见 packages/drivers）
│   │   ├── theme/               # 运行时主题包校验与安装（validate, install）
│   │   ├── mcp/                 # MCP Server/Client
│   │   ├── workflow/            # YAML Workflow 引擎与执行历史（独立于 mcp）
│   │   ├── services/            # ConnectionManager, QueryExecutor, DbTools
│   │   ├── cache/               # SchemaCache（两级 TTL）
│   │   ├── store/               # AES-256-GCM 加密持久化
│   │   └── sync/                # 跨库同步（IR 中间表示 + 适配器）
│   └── resources/               # 菜单翻译（menu-labels.json）、Prompt 模板（prompts/）
├── packages/
│   ├── driver-api/              # DatabaseDriver trait + types + inventory 宏 + ReuseDriver
│   ├── ai-api/                  # AiProvider trait + AiError + factory
│   ├── drivers/                 # 可选 path 驱动（postgres/mysql/…）+ http-support
│   └── themes/                  # 主题包预留
├── e2e/                         # WebdriverIO E2E 测试（35 spec）
├── test/                        # 手工黑盒测试
├── docs/                        # 架构文档、RFC、进度（含 [代码审查修复进度](docs/progress-code-review-fix.md)）
└── .plugins/                    # 构建时生成（gitignored）
```

## 核心架构模式

### 驱动选型（编译时，类似 Caddy 2）

1. `drivers-registry.json` 定义 path 驱动 + git 驱动（kiwi, olap, superset）；Git 可钉 `ref`
2. `scripts/resolve-drivers.mjs` 构建前执行：选型 →（git 则克隆）→ 生成 `generated.ts` + `plugin_init.rs` + `.plugin-features.json`
3. 通过 `inventory` crate 实现链接时自动注册；宿主 `DriverRegistry` 仅走 factories

```bash
pnpm tauri:dev                         # 默认 basic（postgres/mysql/sqlite/redis）
pnpm tauri:dev --drivers=all           # 全部 path 驱动（不含 git）
pnpm tauri:dev --drivers=postgres,mongodb,kiwi   # 显式列表（git 需列出）
DATAZEN_DRIVERS=all pnpm tauri:dev     # 环境变量同样生效
DATAZEN_DRIVERS=all pnpm tauri:build   # 全部 path 原生驱动（不含 kiwi/superset/olap）
```

发布 SKU（CI，非 `resolve-drivers` 预设）：**Basic** / **All**（path）/ **Akulaku**（CI 显式 `postgres,mysql,sqlite,redis,mongodb,kiwi,superset`）。自定义包只在 CI 传逗号列表，不要在脚本里加新的与 `basic`/`all` 同级预设名。

### 数据库驱动

- Path 驱动：`packages/drivers/*`（crate 名 `datazen-driver-<id>`），经 optional Cargo feature 注入
- Git 驱动：克隆到 `.plugins/`，同样 inventory 注册
- 前端 `DB_REGISTRY`：仅合并 `generated.ts` 的 `DRIVER_DB_ENTRIES`（无 Builtin 二分）
- **默认 DB 角标**：`packages/drivers/*/ui/icons/{dbType}.svg`；`resolve-drivers.mjs` 生成 `DRIVER_ICON_ENTRIES`（`getDriverIconMap()`）
- **关键 trait 方法**：`supports_offset()`（默认 true）、`supports_explain()`（默认 true）、`prompt_overrides()`

### Redis 驱动（E1–E4）

深度能力集中在 `packages/drivers/redis`（UI + `plugin:redis|*`），宿主 `RedisConnectionView` 仅为薄 Tab 壳；**禁止** Host 按 `pluginId === 'redis'` 写设置分支。

- **Tabs**：数据浏览（Workbench）/ 命令台（Console）/ Monitor（Info·Memory·Slowlog·Stream 概览）/ Pub/Sub
- **拓扑**：Standalone / Cluster / Sentinel + mTLS；连接扩展字段走 `ConnectionConfig.options`（opaque bag，Host 原样 round-trip）
- **设置**：`AppSettings.pluginSettings.redis`（如 `allowFlush`、`clusterRouting: auto|pinnedNode`）
- **主题**：UI 用语义色（`accent` / `danger` / `success` / `warning` / `surface` / `fg`）；主题包可覆盖 `icons/db.redis.*`；与全局换肤一致
- **设计 / 计划**：[E1](docs/superpowers/specs/2026-08-09-redis-deep-ops-e1-design.md)、[E2–E4](docs/superpowers/specs/2026-08-09-redis-deep-ops-e2-e4-design.md)

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
- **用户手册**：[docs/workflow-guide.md](docs/workflow-guide.md)（YAML 语法、模板、跨库、排错）

### 运行时主题包

与驱动选型**完全独立**（安装路径、注册表、生命周期分离；不受 `drivers-registry.json` / `.plugins` / `DATAZEN_DRIVERS` 影响）：

- **安装路径**：`{appData}/themes/{id}/`（ZIP 解压；仅 CSS / JSON / SVG|PNG|WebP / WOFF2|WOFF，**无 JS**）
- **设置**：`theme: { mode: 'light'|'dark'|'system', packId: string | null }`（旧版扁平字符串自动迁移）
- **IPC**：`list_theme_packs`, `install_theme_pack_with_dialog`, `remove_theme_pack`, `read_theme_pack_file`
- **Rust**：`src-tauri/src/theme/`（validate/install）、`commands/theme.rs`
- **前端**：`src/commands/theme.ts`、`src/lib/themePackApply.ts`（注入 `<style id="datazen-theme-pack">`）、`src/lib/iconResolver.ts`、`ThemedIcon`、`DbTypeBadge`；设置页 `ThemePackSection`
- **图标解析**：pack → Lucide/驱动 → 占位；`db.*` 走 pack → 驱动 SVG → shortLabel 色块（含 `db.redis`）
- **字体**：`--font-sans` / `--font-mono` / `--font-editor`；用户 `editorFontFamily` 显式设置优先
- **与驱动 UI**：驱动页面应使用语义 Tailwind（`bg-surface` / `text-accent` / `text-danger` 等），勿写死 `blue-400` / `red-400` 调色板类，以便换 pack 时与主应用一致
- **设计 / 计划 / 样例**：[spec](docs/superpowers/specs/2026-08-08-runtime-theme-packs-design.md)、[plan](docs/superpowers/plans/2026-08-09-runtime-theme-packs.md)、`fixtures/themes/community.fixture-dark/`
- **商店 / CDN 下载**：未实现（后续独立计划）

### 前端约定

- **零硬编码**：行为差异通过 `DB_REGISTRY` + `DatabaseTypeMeta` 元数据驱动
- **表单路由**：`ConnectionFormBody.tsx` 通过 `connectionForm` 字段选择表单组件
- **视图路由**：`connectionViews/index.ts` 映射 `connectionMode` → 视图组件
- **多库会话**：`hasMultiDatabase` 为驱动能力；未配置逻辑库时列出全部可见库（`MultiDatabaseSchemaTree`）；配置了**逻辑库名**（且出现在 `get_databases` 列表）时锁定单库。Kiwi 的 `database` 字段是实例 **domain**（`databaseFieldType: 'domain'`），不参与锁定。会话 `isMultiDatabase = hasMultiDatabase && 可见库数量 > 1`；切库走 `use_database`
- **多窗口**：`windowManager.ts` + `windowKind.ts` URL 参数路由，`App.tsx` 按 kind 懒加载
- **IPC 约定**：前端 `invoke()` 传参对象键名使用 camelCase（如 `connectionId`）；Rust 命令形参为 snake_case，Tauri 自动映射；嵌套 DTO 通常 `#[serde(rename_all = "camelCase")]`

### IPC 通信

前端 `src/commands/` ↔ 后端 `src-tauri/src/commands/`，按领域对齐：
`connection`, `database/schema`, `query`, `ai`, `context`, `settings/config`, `theme`, `file`, `adb`, `kv`, `backup`, `sync`, `mcp`, `window`

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
| Redis 深度运维 | `packages/drivers/redis/ui/*` + 薄壳 `RedisConnectionView` | `plugin:redis|*`（drivers/redis） |
| 主题包 | `windows/settings/ThemePackSection.tsx` + `lib/themePackApply.ts` | `theme/` + `commands/theme.rs` |

## 开发命令

```bash
pnpm install                           # 安装依赖
pnpm dev                               # Vite dev server
pnpm tauri:dev                         # 完整开发（前端 + Rust；默认 basic 驱动）
pnpm build                             # 构建前端（不 inject；打包前由外层 resolve）
pnpm build:with-drivers                # 单独前端构建并 inject/restore
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
pnpm e2e:minimal                       # 更快：DATAZEN_DRIVERS=basic，跳过 Git / 非核心 path 驱动
pnpm e2e:skip-build                    # 跳过构建（仅当已有合格的 webdriver debug 二进制）
pnpm e2e:skip-build -- --spec e2e/specs/path-ipc-hardening.ts
pnpm e2e:core                          # 核心 UI（默认 skip-build）
pnpm e2e:db / e2e:ai / e2e:kiwi / e2e:redis  # 分组（redis 含 topology 可选用例）
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

- Path 驱动 Rust crate：`datazen-driver-<id>`；Git 驱动仓库名 `datazen-driver-xxx`，其 Rust crate 名仍可能为 `datazen-plugin-xxx`（以插件仓库为准）
- `Cargo.toml` 中的插件占位段（`<<plugin-dependencies>>`、`<<plugin-features>>`、`<<plugin-patches>>`）在 git 中应保持为空；`resolve-drivers.mjs` 在构建时填充
- `src/plugins/generated.ts` 和 `src-tauri/src/plugin_init.rs` 是自动生成的，修改后会被覆盖；**git 中必须是空 stub**（`DatabaseType = never` / 无 `extern crate`）。用 `node scripts/resolve-drivers.mjs --drivers=stub` 刷新 baseline；本地/CI 构建前再 inject。禁止提交已注入内容（CI：`scripts/check-managed-stubs.mjs`）。裸跑 `resolve-drivers` 后请 `pnpm drivers:restore`；`with-plugin-inject` / `tauri:dev` / `dev` 会在结束时 restore（残留 `.plugin-file-stash/` 会被当作 orphan 先清再 inject）
- `.plugins/` 是 gitignored，由 `resolve-drivers.mjs` / `tauri:build` / `tauri:dev` 生成；`pnpm build` 本身不 inject
- `PROTOCOL_VERSION`（`packages/driver-api`）变更时需同步更新所有插件
- `AI_PROTOCOL_VERSION`（`packages/ai-api`）变更时需同步更新所有 AI Provider 插件
- AI 配置加密存储在 `ai_config.enc`，不会出现在日志中
- Prompt 模板在 `src-tauri/resources/prompts/{lang}/`，支持用户覆盖
- 菜单翻译：前端 `src/locales/{zh-CN,en}.ts` 为唯一来源；`scripts/generate-menu-labels.mjs` 生成 `src-tauri/resources/menu-labels.json` 供 Rust 原生菜单使用（`pnpm menu:labels` / build / tauri:dev 自动执行）
- 日志文件在 `{data_dir}/logs/`
- E2E：详见 [docs/e2e-testing.md](docs/e2e-testing.md)；禁止用裸 `cargo build` 当 E2E 二进制
- 主题包与驱动选型独立：`{appData}/themes/` 不由 `resolve-drivers.mjs` 管理；删除主题包不影响 `.plugins/`
