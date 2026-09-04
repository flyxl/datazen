# AGENTS.md

> 本文件面向 AI 编程助手，帮助其快速理解项目结构和约定。详细架构设计见 [docs/architecture/](docs/architecture/README.md)。

## 项目概述

DataZen 是一个跨平台桌面数据库管理工具，基于 **Tauri v2**（Rust 后端 + React 前端）构建，集成 AI 辅助功能。

- **框架**：Tauri v2 + React 18 + TypeScript + Tailwind CSS 4
- **包管理**：pnpm（前端）、Cargo workspace（Rust）
- **状态管理**：Zustand
- **测试**：Vitest（Host 单元）、驱动 crate 内单测/E2E、WebdriverIO（Host E2E）、手工黑盒（`test/`）
- **AI**：多 Provider（OpenAI / Anthropic / DeepSeek / Custom）、MCP Server/Client
- **运行模式**：GUI 桌面应用 或 无头 MCP stdio 服务器（`--mcp-stdio`）

## 目录结构

```text
datazen/
├── src/                         # React 前端源码
│   ├── components/              # UI 组件（ai/, chart/, connection/, DataTable/, ui/）
│   ├── windows/                 # 主工作区 *Page + 子窗口 *Window（见 architecture/windows.md）
│   │   └── connection/er/       # ER 图模块（React Flow）
│   ├── stores/                  # Zustand stores（connection / panel / schema / settings / ai / dashboard 等）
│   ├── commands/                # Tauri IPC 封装
│   ├── lib/                     # 工具库
│   ├── hooks/                   # React hooks
│   ├── locales/                 # i18n
│   ├── plugins/generated.ts     # 自动生成（gitignore；pnpm install / resolve-drivers）
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
│   │   ├── data_sync/           # 同族 Data Synchronization（门闸 / 比较 / ChangeSet / 执行）
│   │   └── transfer/            # 异构 IR 适配器 / DDL 生成（非 data_sync 执行引擎）
│   └── resources/               # 菜单翻译、Prompt 模板
├── packages/
│   ├── driver-api/              # DatabaseDriver + Command API + inventory + ReuseDriver
│   ├── ai-api/                  # AiProvider trait + factory
│   ├── drivers/                 # path 驱动 crate（测试也写在各 crate 内）
│   │   └── <id>/                # Rust `src/` + `tests/`；UI `ui/__tests__/`；E2E `e2e/`
│   ├── extensions/              # 运行时插件源码包（UI 页 + 主题；安装测试见其 README）
│   ├── extension-sdk/           # 插件侧 SDK：类型化 RPC 客户端 / useTheme / theme.css
│   └── themes/                  # （已迁移）旧 v1 ThemePack 存档，见 extensions/
├── e2e/                         # Host WebdriverIO E2E（通用 UI / IPC；非驱动方言）
├── test/                        # 手工黑盒测试
└── docs/                        # 文档：features/（功能）、architecture/（架构）、development/（开发发布）
```

## 核心架构模式

### 驱动选型（编译时，类似 Caddy 2）

1. `drivers-registry.json` 定义 path 驱动 + git 驱动；Git 可钉 `ref`
2. `scripts/resolve-drivers.mjs` 构建前执行选型、克隆 Git driver，并生成 `generated.ts`、`driver_init.rs`、`.driver-features.json`（前两者 gitignore；`pnpm install` / `pnpm build` 会 `--codegen-only` 补齐）
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
- Git 驱动：克隆到 `packages/drivers/<id>/`（gitignore，非 Cargo workspace member），同样 inventory 注册
- 前端 `DB_REGISTRY` 合并 `generated.ts` 的 `DRIVER_DB_ENTRIES`
- 默认 DB 图标来自 `packages/drivers/*/ui/icons/{dbType}.svg`
- 关键 trait 方法包括 `supports_offset()`、`supports_explain()`、`prompt_overrides()`
- **驱动相关测试必须写在该驱动 crate 内**，不要加到 Host（`src-tauri/`、`src/`、`e2e/specs/`）。详见下方「驱动测试落点」

### Driver Command API

`packages/driver-api` 提供统一 Command 抽象（`command_definitions()` + `execute_command()`）。`query` / `execute` 是内置 Command 有默认实现。Workflow、IPC、前端 Command Editor 都依赖 Command Definition，不按 Driver 类型硬编码。`ReuseDriver` 必须转发 Command discovery 与 execution。`metadata.requiresConnection = false` 的 Command 可通过 `driverType` 执行。

### Redis 驱动

深度能力集中在 `packages/drivers/redis`（UI + Driver Command API），宿主仅为薄 Tab 壳。**禁止** Host 按 `pluginId === 'redis'` 写设置分支。操作一律走 `execute_command` / `execute_driver_command`。

### AI 模块

多 Provider（OpenAI / Anthropic / DeepSeek / Custom）；`PromptResolver` 优先级：用户覆盖 → 驱动覆盖 → 资源文件 → 编译时英文嵌入。详见 [docs/architecture/backend/ai.md](docs/architecture/backend/ai.md)。

### MCP

Server 暴露 Tools/Resources/Prompts（DB tools 使用持久化 `connection_id`）；Client 连接外部 MCP Server；`--mcp-stdio` 启动无头模式。详见 [docs/architecture/backend/mcp.md](docs/architecture/backend/mcp.md)。

## Workflows

YAML 驱动的通用执行引擎，GUI、Tauri IPC 和 MCP 共用同一 runtime。Step 通过 Driver Command API 执行；Workflow 默认 connection 可被 Step 继承或覆盖；旧 `type: query` 自动规范化为 `Command("query")`。Workflow UI 通过 `command_definitions()` 动态发现可用 Command，不硬编码。

详细设计：[Workflow 架构文档](docs/architecture/backend/workflow.md)；用户手册：[docs/features/workflow-guide.zh-CN.md](docs/features/workflow-guide.zh-CN.md)。

### 运行时插件系统（Extensions：UI 页面 + 主题）

统一运行时扩展：`{appData}/plugins/{publisher}.{name}/`，manifest v2（`contributes.pages/themes` + 权限声明），沙箱 iframe + 受控 postMessage 桥（`extensionBridge.ts`），取数一律走 `execute_driver_command`。Rust `src-tauri/src/extensions/`，前端 `windows/workspace|extensions/`。主题贡献完整保留旧 ThemePack 能力（editorJson/chartsJson/iconsDir）；旧 `{appData}/themes/` 运行时入口已移除。宿主不感知具体插件 id；驱动相关测试规则同样适用于插件示例包（守护测试在 Host）。源码包与安装测试见 [packages/extensions/](packages/extensions/)；详细设计：[docs/architecture/backend/extensions.md](docs/architecture/backend/extensions.md)。

### 运行时主题包（遗留）

已被运行时插件系统的 themes 贡献取代；`{appData}/themes/` 不再被主窗口读取，遗留代码仅为测试保留（Q8 一次性切换）。

## 前端约定

- 零硬编码：行为差异通过 `DB_REGISTRY` + `DatabaseTypeMeta` 元数据驱动
- **主工作区 Page**：`main` 内用 `*Page` 导航（`WelcomePage` / `ConnectionPage` / `SettingsPage` 等）；Settings / 新建连接为 main 内嵌；Docs 跳转官网（非子窗口）。子窗口仅 backup / data-sync / schema-diff — 详见 [docs/architecture/windows.md](docs/architecture/windows.md)
- IPC：前端 camelCase，Rust snake_case；Tauri 自动映射
- 右键菜单统一使用 Web Context Menu，禁止 Tauri 原生 `Menu.popup()`
- **主题包 DataTable 色**：`--dt-*` token + `src/lib/dataTypeColors.ts`（CellRenderer、StructureView、TableHeader 等共用）
- **Data Synchronization ≠ Transfer ≠ Structure Sync**：Sync 仅同族 + 结构/PK 完全一致；异构 IR 是 Transfer。详见 [docs/architecture/backend/data-sync.md](docs/architecture/backend/data-sync.md)

## ID 术语

**`connectionId`** = 持久化连接配置 id（原 configId，落盘）；**`dbSessionId`** = 运行时数据库会话 id（内存态，永不落盘）。配置/归属/调度语义用 connectionId，操作已建立会话用 dbSessionId；新代码不得依赖 `resolve_session` 双模回退。详见 [docs/architecture/naming.md](docs/architecture/naming.md)。

## IPC 通信

前端 `src/commands/` ↔ 后端 `src-tauri/src/commands/`，按领域对齐。Driver Command IPC 统一走 `execute_driver_command`；SQL 编辑器的 `query` / `execute`、流式 `query_stream`、Schema 对象浏览（`list_objects` 等）、Redis KV、管理对话框均通过同一路径，Host 不按驱动类型硬编码。

## 错误处理

`CommandError`（`commands/error.rs`）统一覆盖所有错误类型；`CmdExt` 统一日志。

## 关键功能模块

| 功能 | 前端入口 | 后端入口 |
|------|---------|---------|
| 图表可视化 | `components/chart/` + `lib/chart/` | — |
| ER 图 | `windows/connection/ErDiagramView.tsx` + `er/` | `commands/schema.rs → get_er_data` |
| 数据导出 | `DataTable/DataExportDialog.tsx` + `lib/exportData.ts` | — |
| 导出（多表） | `windows/connection/BatchExportDialog.tsx` + `lib/batchExport.ts` | — |
| 导出表结构 | `TableStructureEditor` + `lib/exportTableStructure.ts` | — |
| AI Chat | `components/ai/AiChatPanel.tsx` | `commands/ai.rs` |
| Workflows | `windows/workflow/WorkflowPage.tsx` | `workflow/executor.rs` / `workflow/command_runtime.rs` |
| 数据同步 | `windows/data-sync/` | `data_sync/` + `commands/sync/`（`inspect_data_sync` / `execute_data_sync`） |
| 权限管理 | `windows/connection/PrivilegeView.tsx` | `execute_driver_command` + Driver `admin_commands`（动态 schema） |
| 管理命令 | `Create*Dialog.tsx` + `schemaTreeContextMenu.ts` | `execute_driver_command` + Driver `admin_commands`（create/drop DB/schema/user, grant/revoke） |
| Schema 对象 | 连接树 routines/triggers 等 | `execute_driver_command`（`list_objects` / `get_object_ddl` / `list_privileges`） |
| Redis 深度运维 | `packages/drivers/redis/ui/*` | `execute_command` / `execute_driver_command` |
| 扩展主题 | `windows/settings/AppearanceSection.tsx` | `extensions/` + `themePackApply.ts` |
| 插件系统 | `windows/workspace/` + `windows/extensions/ExtensionManagementPage.tsx` | `extensions/` + `commands/extensions.rs`（`datazen://` 协议 / 桥接 `lib/extensionBridge.ts`） |
| 插件示例包 | `packages/extensions/*`（含 README 安装测试步骤） | 同上；守护测试 `plugins::fixture_tests` + `extensionThemes.test.ts` |

## 开发命令

```bash
pnpm install                           # 安装依赖
pnpm dev                               # Vite dev server
pnpm tauri:dev                         # 完整开发（前端 + Rust；默认 basic 驱动）
pnpm build                             # 构建前端（缺 codegen 时 --codegen-only；不改 Cargo.toml）
pnpm build:with-drivers                # 单独前端构建并 inject/restore
pnpm tauri:build                       # 完整应用（外层 inject 一次）
npx vitest run                         # Host 前端单元测试（不含 packages/drivers）
pnpm test:unit:drivers                 # Path 驱动 UI 单测（packages/drivers/*/ui）
cargo test -p datazen                  # Host Rust 单元测试（不含驱动 crate）
cargo test -p datazen-driver-postgres  # 示例：某个 path 驱动的 Rust 测试
```

多功能需求由主代理协调编码/测试子代理并行开发：三角色模型、bug 流转状态机、worktree 轨道编排与子代理恢复协议见 [docs/development/subagent/](docs/development/subagent/README.md)（各角色专享手册及简报模板位于该目录；轨道 bootstrap 用 `scripts/new-feature-worktree.sh <track>`；总览聚合用 `node scripts/aggregate-hub.mjs`）。

### E2E

完整流程见 [docs/development/e2e-testing.md](docs/development/e2e-testing.md)；覆盖矩阵见 [docs/development/e2e-coverage.md](docs/development/e2e-coverage.md)。

- 必须用 `pnpm tauri build --debug --features webdriver` 构建
- **硬性规则**：所有 Host UI 交互路径都必须被 E2E 覆盖；新增/变更 Host UI 必须同 PR 更新 E2E
- **驱动 E2E** 写在 `packages/drivers/<id>/e2e/`，不进 Host `e2e/specs/`
- **契约矩阵**：`e2e/contract/` 定义统一 journeys，`pnpm e2e:contract:matrix` 跨 PG/MySQL/SQLite 运行
- 无法自动化的路径须在 `docs/development/e2e-coverage.md` 登记例外

```bash
pnpm e2e                    # 完整构建 + 全部 Host E2E
pnpm e2e:minimal             # DATAZEN_DRIVERS=basic 快速跑
pnpm e2e:skip-build          # 跳过构建
pnpm e2e:contract:matrix     # Host 契约 × 驱动矩阵
```

PR 合并前：`pnpm test:unit` + `cargo test -p datazen --lib`。改了驱动还要跑 `cargo test -p datazen-driver-<id>` 和 `pnpm test:unit:drivers`。

## 驱动测试落点

**规则：驱动实现 / 方言 / 专属 UI / 专属 Command 的测试，写到该驱动 crate 目录（`packages/drivers/<id>/`），禁止放到 Host。** Git 驱动测试写在插件自己的仓库。

| 类型 | 位置 | 运行 |
|------|------|------|
| Rust 单元 | 同文件 `#[cfg(test)]` | `cargo test -p datazen-driver-<id>` |
| Rust 集成 | `packages/drivers/<id>/tests/` | `cargo test -p datazen-driver-<id> --test <name>` |
| 驱动 UI 单测 | `packages/drivers/<id>/ui/__tests__/` | `pnpm test:unit:drivers` |
| 驱动 E2E | `packages/drivers/<id>/e2e/` | 显式脚本，不进默认 `pnpm e2e` |

**Host 只测宿主能力**（不编码驱动方言）。禁止在 `src-tauri/`、`e2e/specs/`、`src/**/__tests__/` 新增驱动专属测试。详细策略见 [docs/architecture/testing.md](docs/architecture/testing.md)。

## i18n 国际化规则

- **开发期间**：只修改 `en.ts`（英文）和可选的 `zh-CN.ts`（中文），不要同时修改其他语言文件
- **发布前**：使用 `node scripts/i18n-sync-check.mjs` 检查翻译完整性，然后通过 i18n-sync skill 补齐所有语言
- `en.ts` 是唯一的翻译 source of truth，其他语言文件必须保持相同的 key 集合
- `scripts/i18n-sync-check.mjs --from <tag>` 可检查自上一个 tag 以来英文文件的变更，识别需要翻译的 key
- 该脚本返回 exit code 1 表示有未完成的翻译，适合 CI 检查

## 代码风格

- Rust：`rustfmt` + `thiserror` + `tracing` + `CommandError`
- **生产路径禁止裸 `unwrap()` / `expect()`**（`#[cfg(test)]` 除外；确需 panic 须注释说明）。详见 [docs/development/panic-policy.md](docs/development/panic-policy.md)
- TypeScript：严格模式，无 `any`（除 generated 文件），absolute imports
- CSS：Tailwind utility classes，暗色主题默认
- **单文件规模与模块拆分**：严格限制单源码文件大小（推荐单文件不超过 500 行，严禁出现数千行的超大单文件）；大型组件/模块必须按功能和职责拆分成高内聚、低耦合的小组件/子模块（如将复杂主视图拆分为独立抽屉、对话框、工具栏、子视图和独立 hook/工具库）
- 安全：CSP、AES-256-GCM、路径遍历防护、文件扩展名白名单

## 重要注意事项

- Path 驱动 Rust crate：`datazen-driver-<id>`；Git 驱动 Rust crate 名以插件仓库为准
- 驱动相关测试写在对应 crate（`packages/drivers/<id>/` 或 Git 插件仓），不要写到 Host
- `Cargo.toml` 中的插件占位段在 git 中应保持为空；`resolve-drivers.mjs` 构建时填充
- 以下文件均为 gitignore 的 codegen，由 `resolve-drivers` / `ensure-generated-drivers` 生成，不要提交：
  - `src/plugins/generated.ts`
  - `src/plugins/generated-locales.ts`
  - `src-tauri/src/driver_init.rs`
  - `src-tauri/capabilities/default.json`（由 `default_host.json` + 插件 capabilities 合并生成）
- `pnpm install`（prepare）若上述文件缺失则 `--codegen-only --drivers=basic`；已存在则保留当前选型
- **Capabilities 管理**：`src-tauri/capabilities/default.json.host` 是 git 跟踪的 host 权限源文件（`.json.host` 扩展名避免 Tauri 构建系统扫描）；需要添加新 host capability 时直接修改该文件。`src-tauri/capabilities/default.json` 在构建时由 `resolve-drivers.mjs` 合并 `default.json.host` + 活跃插件权限自动生成，**不要手动编辑或提交**
- Git 驱动 clone 目录（`packages/drivers/{kiwi,olap,superset}/` 等非 path 驱动）是 gitignored，由 driver resolve/build/dev 流程生成；勿提交
- `PROTOCOL_VERSION`（`packages/driver-api`）变更时需同步更新所有插件
- `AI_PROTOCOL_VERSION`（`packages/ai-api`）变更时需同步更新所有 AI Provider 插件
- AI 配置加密存储在 `ai_config.enc`，不会出现在日志中
- 连接密码等凭据：AES-256-GCM；**主密钥**默认在系统钥匙串，开发/adhoc 或 `DATAZEN_KEYRING=file` 时用 `{appData}/.key`（见 `store/key_store.rs`）
- Prompt 模板在 `src-tauri/resources/prompts/{lang}/`，支持用户覆盖
- 日志文件位于 `{data_dir}/logs/`
- 主题包与驱动选型独立：`{appData}/themes/` 不由 `resolve-drivers.mjs` 管理；删除主题包不影响 `packages/drivers/`；DataTable 单元格色 token 为 `--dt-*`
