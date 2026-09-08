# [AGENTS.md](http://AGENTS.md)

> 本文件面向 AI 编程助手，帮助其快速理解项目结构和约定。详细架构设计见 [docs/architecture/](docs/architecture/README.md)。

## 项目概述

DataZen 是一个跨平台桌面数据库管理工具，基于 **Tauri v2**（Rust 后端 + React 前端）构建，集成 AI 辅助功能。

- **框架**：Tauri v2 + React 18 + TypeScript + Tailwind CSS 4
- **包管理**：pnpm（前端）、Cargo workspace（Rust）
- **状态管理**：Zustand
- **设计系统**：`@datazen/ui` 纯 React 基础视图组件库（宿主与插件共享）
- **测试**：Vitest（Host 单元）、驱动 crate 内单测/E2E、WebdriverIO（Host E2E）、手工黑盒（`test/`）
- **AI**：多 Provider（OpenAI / Anthropic / DeepSeek / Ollama / Custom）、MCP Server/Client
- **运行模式**：GUI 桌面应用 或 无头 MCP stdio 服务器（`--mcp-stdio`）
- **发行版本**：Community 社区版（纯开源净室版）/ Pro 增强版（通过特权扩展点注入高级功能）

## 目录结构

```text
datazen/
├── src/                         # React 前端源码
│   ├── components/              # UI 组件（sql-editor/, ai/, chart/, connection/, DataTable/, ui/）
│   ├── windows/                 # 主工作区 *Page + 子窗口 *Window（见 architecture/windows.md）
│   │   └── connection/er/       # ER 图模块（React Flow）
│   ├── stores/                  # Zustand stores（connection / panel / schema / settings / ai 等）
│   ├── commands/                # Tauri IPC 封装
│   ├── lib/                     # 工具库与业务算法
│   ├── hooks/                   # React hooks
│   ├── locales/                 # i18n 领域包与按需加载
│   └── plugins/                 # 自动生成注册文件（generated.ts / generated-locales.ts / generated-pro.ts）
├── src-tauri/                   # Rust 后端
│   ├── src/
│   │   ├── ai/                  # AI Provider 实现 / protocol / context
│   │   ├── commands/            # Tauri IPC 命令
│   │   ├── db/                  # DriverRegistry
│   │   ├── mcp/                 # MCP Server/Client
│   │   ├── workflow/            # YAML Workflow 引擎
│   │   ├── services/            # ConnectionManager, QueryExecutor, DbTools, Transaction
│   │   ├── cache/               # SchemaCache
│   │   ├── store/               # AES-256-GCM 加密持久化
│   │   ├── sql_guard/           # SQL 安全门网与注释剥离扫描
│   │   ├── schema_diff/         # 方言中立 Migration IR 与 DAG 结构比对
│   │   ├── data_sync/           # 同族 Data Synchronization（门闸 / 比较 / ChangeSet / 执行）
│   │   └── data_transfer/       # 异构数据传输引擎与 IR 适配
│   └── resources/               # 菜单翻译、Prompt 模板
├── packages/
│   ├── ui/                      # @datazen/ui: 纯 React 基础组件设计系统（Button, Dialog, Select 等）
│   ├── extension-points/        # @datazen/extension-points: 特权扩展点 (EP) 契约与注册表
│   ├── app-sdk/                 # @datazen/app-sdk: Workspace App 沙箱应用前端 SDK
│   ├── driver-sdk/              # @datazen/driver-sdk: 数据库驱动前端元数据/方言/Command SDK
│   ├── driver-api/              # Rust: DatabaseDriver trait + Command API + inventory 注册
│   ├── ai-api/                  # Rust: AiProvider trait + 工厂 + 消息/调用协议模型
│   ├── drivers/                 # path 驱动 crate（测试严格写在各 crate 内）
│   │   └── <id>/                # Rust `src/` + `tests/`；UI `ui/__tests__/`；E2E `e2e/`
│   ├── wapps/                   # Workspace Apps 运行时应用与主题源码包（安装测试见其 README）
│   ├── pro-extensions/          # Pro 扩展本地存放/挂载目录（gitignored）
│   └── themes/                  # 旧 v1 ThemePack 存档
├── e2e/                         # Host WebdriverIO E2E（通用 UI / IPC；非驱动方言）
├── test/                        # 手工黑盒测试
└── docs/                        # 文档：features/、architecture/、development/
```

## 核心架构模式

### 驱动选型与双版本构建模式（编译时，类似 Caddy 2）

1. `drivers-registry.json` 定义 path 驱动 + git 驱动；Git 可钉 `ref`。
2. `scripts/resolve-drivers.mjs` 构建前执行驱动选型、克隆 Git driver，并生成 `generated.ts`、`driver_init.rs`、`.driver-features.json`（均 gitignored）。
3. `scripts/resolve-pro.mjs` 控制构建版本：默认 `--edition=community`（纯开源 Fallback）；`--edition=pro`（拉取并注入 `@datazen/extension-sql-editor-pro`）。
4. 数据库驱动通过 `inventory` crate 实现链接时自动注册；宿主 `DriverRegistry` 仅走 factories。

```bash
pnpm tauri:dev                         # 默认 basic 驱动，Community 版
pnpm tauri:dev:pro                     # Pro 版开发（注入 SQL Editor Pro）
pnpm tauri:dev --drivers=all           # 全驱动开发
pnpm tauri:build:community             # 构建开源社区版
pnpm tauri:build:pro                   # 构建 Pro 增强版
DATAZEN_DRIVERS=all pnpm tauri:build   # 构建全驱动包
```

### 数据库驱动

- Path 驱动：`packages/drivers/*`（crate 名 `datazen-driver-<id>`），经 optional Cargo feature 注入。
- Git 驱动：克隆到 `packages/drivers/<id>/`（gitignored，非 Cargo workspace member），同样 inventory 注册。
- 前端 `DB_REGISTRY` 合并 `generated.ts` 的 `DRIVER_DB_ENTRIES`。
- 默认 DB 图标来自 `packages/drivers/*/ui/icons/{dbType}.svg`。
- 关键 trait 方法包括 `supports_offset()`、`supports_explain()`、`prompt_overrides()`。
- **驱动专属实现/测试必须落在对应驱动 crate 目录**，禁止加到 Host。

### 四维扩展体系与公共设计系统 (@datazen/ui)

DataZen 确立严格正交的四维可扩展架构，依托独立设计系统 `@datazen/ui` 共享基础视觉：

1. **Driver (数据库驱动)**：编译/链接时注入，基于 `@datazen/driver-sdk` 与 `packages/driver-api`。零 DOM、无 React 强依赖，承载数据库连接、SQL 方言、DDL、Driver Command。
2. **Theme (外观主题)**：纯静态资源包（`manifest.json`, `tokens.css`, `editor.json`, `charts.json`, `icons/`），零代码执行，安全沙箱级最高。
3. **EP (特权扩展点 / Host Extension Points)**：主进程特权插槽，基于 `@datazen/extension-points`。专用于 SQLEditor Pro 增强、高级图表等对键入延迟（<5ms）和 CodeMirror Compartment 深度集成有严苛要求的核心扩展。宿主默认内置纯净基础版（Fallback）。受根目录 `LICENSE` 的 **DataZen Plugin, Driver & Extension Linking Exception** 保护，允许扩展模块使用独立许可证发布，免除 GPL-3.0 传染。
4. **Workspace App (工作区应用)**：独立全屏 iframe 沙箱应用，运行目录 `{appData}/wapps/{publisher}.{name}/`，基于 `@datazen/app-sdk`，通过 `window.postMessage` 受控桥与宿主通信，取数一律走 `execute_driver_command`。
5. **@datazen/ui (公共设计系统)**：`packages/ui/`，纯 React 基础视图组件（Button, Input, Select, Dialog, Tabs, Badge, Label, cn），与宿主业务 Store 和 IPC 严格解耦。宿主 `src/components/ui/` 和驱动 UI 统一复用。

### Driver Command API 与统一执行网关

`packages/driver-api` 提供统一 Command 抽象（`command_definitions()` + `execute_command()`）。SQL 编辑器的 `query`/`execute`、流式 `query_stream`、Schema 对象元数据、Redis 操作、管理命令等全部走 `execute_driver_command`，宿主不按 Driver 类型硬编码。Redis 深度能力集中在 `packages/drivers/redis`，宿主仅为薄 Tab 壳。

### AI 模块与 ai-api

- `packages/ai-api` 是 Rust 层的 AI Provider 抽象契约（类比 `driver-api`），定义核心 `AiProvider` trait、请求/流式响应模型与工厂注册。
- 支持 OpenAI、Anthropic、DeepSeek、Ollama、自定义端点。
- `PromptResolver` 优先级：用户覆盖 → 驱动覆盖 → 资源文件 → 编译时英文嵌入。
- 详情参考 [docs/architecture/backend/ai.md](docs/architecture/backend/ai.md)。

### MCP (Model Context Protocol)

Server 暴露 Tools/Resources/Prompts（DB tools 使用持久化 `connection_id`）；Client 连接外部 MCP Server；`--mcp-stdio` 启动无头模式。详见 [docs/architecture/backend/mcp.md](docs/architecture/backend/mcp.md)。

### Workflows

YAML 驱动的通用执行引擎，GUI、Tauri IPC 和 MCP 共用同一 runtime。Step 通过 Driver Command API 执行；Workflow 默认 connection 可被 Step 继承或覆盖。详见 [docs/architecture/backend/workflow.md](docs/architecture/backend/workflow.md)。

## 前端约定

- **零硬编码**：数据库行为差异全部通过 `DB_REGISTRY` + `DatabaseTypeMeta` 元数据驱动。
- **主工作区 Page**：`main` 内用 `*Page` 导航（`WelcomePage` / `ConnectionPage` / `SettingsPage` 等）；Settings / 新建连接为 main 内嵌；Docs 跳转官网。独立子窗口仅限 `backup` / `data-sync` / `schema-diff` / `data-transfer`。详见 [docs/architecture/windows.md](docs/architecture/windows.md)。
- **IPC 命名**：前端 camelCase，Rust snake_case；Tauri 自动双向映射。
- **右键菜单**：统一使用 Web Context Menu，禁止 Tauri 原生 `Menu.popup()`。
- **主题与色彩**：全站通用 `--color-accent-*` 语义 Token；DataTable 单元格类型色使用 `--dt-*` token + `src/lib/dataTypeColors.ts`。
- **概念区分**：Data Synchronization（仅同族 + 结构/PK 完全一致）≠ Data Transfer（异构 IR 迁移）≠ Schema Diff（结构差异对比与 DDL 迁移生成）。

## ID 术语规范

- `connectionId` = 持久化连接配置 id（原 configId，落盘持久化）。
- `dbSessionId` = 运行时数据库会话 id（内存态，永不落盘）。
- 配置/归属/调度语义必须用 `connectionId`，操作已建立连接会话必须用 `dbSessionId`；禁止双模回退。详见 [docs/architecture/naming.md](docs/architecture/naming.md)。

## 错误处理

`CommandError`（`commands/error.rs`）统一覆盖所有错误类型；`CmdExt` 统一记录日志；全局 IPC 错误 Payload 自动脱敏凭据与绝对路径。

## 关键功能模块

| 功能 | 前端入口 | 后端入口 |
| ---------- | ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| SQL 编辑器 & 风控 | `components/sql-editor/` + `windows/connection/query/` | `sql_guard/` + `services/transaction.rs` + `execute_driver_command` |
| 结构比对 (Schema Diff) | `windows/schema-diff/` | `schema_diff/` + `commands/schema_diff.rs`（Driver `SchemaMigrationRenderer`） |
| 数据传输 (Transfer) | `windows/data-transfer/` | `transfer/` + `data_transfer/` |
| 数据同步 (Data Sync) | `windows/data-sync/` | `data_sync/` + `commands/sync/` |
| 图表可视化 | `components/chart/` + `lib/chart/` | — |
| ER 图 | `windows/connection/ErDiagramView.tsx` + `er/` | `commands/schema.rs → get_er_data` |
| 数据与结构导出 | `DataTable/DataExportDialog.tsx` + `BatchExportDialog.tsx` | `commands/export.rs` |
| AI Chat & 诊断 | `components/ai/AiChatPanel.tsx` + `DiagnosisPanel.tsx` | `commands/ai/` + `ai/` |
| Workflows | `windows/workflow/WorkflowPage.tsx` | `workflow/executor.rs` / `workflow/command_runtime.rs` |
| 权限与管理命令 | `PrivilegeView.tsx` + `Create*Dialog.tsx` | `execute_driver_command` + Driver `admin_commands` |
| Schema 对象树 | 连接树 routines/triggers 等 | `execute_driver_command`（`list_objects` / `get_object_ddl`） |
| Redis 深度运维 | `packages/drivers/redis/ui/*` | `execute_command` / `execute_driver_command` |
| 工作区应用 (Wapp) 与主题 | `windows/wapps/` + `windows/workspace/` + `windows/settings/AppearanceSection.tsx` | `wapps/` + `commands/wapps.rs`（`datazen://` 协议桥接） |

## 开发命令

```bash
pnpm install                           # 安装依赖（自动补齐 codegen）
pnpm dev                               # Vite dev server
pnpm tauri:dev                         # 完整开发（前端 + Rust；默认 basic 驱动，Community 版）
pnpm tauri:dev:pro                     # 启动 Pro 版完整开发
pnpm build                             # 构建前端（缺 codegen 时 --codegen-only）
pnpm tauri:build:community             # 构建 Community 社区版
pnpm tauri:build:pro                   # 构建 Pro 增强版
npx vitest run                         # Host 前端单元测试（不含 packages/drivers）
pnpm test:unit:drivers                 # Path 驱动 UI 单测（packages/drivers/*/ui）
cargo test -p datazen --lib            # Host Rust 单元测试
cargo test -p datazen-ai-api --lib     # AI API 单元测试
cargo test -p datazen-driver-postgres  # 示例：某个驱动 crate 的 Rust 测试
```

多功能需求由主代理协调编码/测试子代理并行开发：三角色模型、bug 流转状态机、worktree 轨道编排与子代理恢复协议见 [docs/development/subagent/](docs/development/subagent/README.md)。

### E2E 测试

- **构建方式**：必须使用 `pnpm tauri:build:webdriver`（或 `pnpm e2e` 自动触发）。**禁止**直接执行裸 `cargo build` 或缺少驱动注入参数的编译。
- **测试落点**：Host 通用 UI 交互路径在 `e2e/specs/`；驱动特定方言 E2E 必须写在 `packages/drivers/<id>/e2e/`。
- **契约矩阵**：`e2e/contract/` 定义统一跨库 journeys，通过 `pnpm e2e:contract:matrix` 运行。

```bash
pnpm e2e                     # 完整构建 + 全部 Host E2E
pnpm e2e:minimal             # DATAZEN_DRIVERS=basic 快速跑
pnpm e2e:skip-build          # 跳过构建（使用已编译的 debug binary）
pnpm e2e:contract:matrix     # Host 契约 × 驱动矩阵
```

## 驱动测试落点

**规则：驱动实现 / 方言 / 专属 UI / 专属 Command 的测试，严格写到该驱动 crate 目录（`packages/drivers/<id>/`），禁止放到 Host。**

| 类型 | 位置 | 运行 |
| -------- | ------------------------------------- | ------------------------------------------------- |
| Rust 单元 | 同文件 `#[cfg(test)]` | `cargo test -p datazen-driver-<id>` |
| Rust 集成 | `packages/drivers/<id>/tests/` | `cargo test -p datazen-driver-<id> --test <name>` |
| 驱动 UI 单测 | `packages/drivers/<id>/ui/__tests__/` | `pnpm test:unit:drivers` |
| 驱动 E2E | `packages/drivers/<id>/e2e/` | 显式脚本，不进默认 `pnpm e2e` |

## i18n 国际化规则

- **开发期间**：只修改 `en.ts`（英文）和可选的 `zh-CN.ts`（中文），不要同时修改其他语言文件。
- **发布前**：使用 `node scripts/i18n-sync-check.mjs` 检查翻译完整性，然后通过 i18n-sync skill 补齐所有语言。
- `en.ts` 是唯一的翻译 source of truth，其他语言文件必须保持相同的 key 集合。
- 采用领域包（Domain Packs）结构，子窗口与深层功能通过 `useLocaleDomains` 按需惰性（Lazy）加载。

## 代码风格

- Rust：`rustfmt` + `thiserror` + `tracing` + `CommandError`。
- **生产路径禁止裸 `unwrap()` / `expect()`**（`#[cfg(test)]` 除外；确需 panic 须注释说明）。详见 [docs/development/panic-policy.md](docs/development/panic-policy.md)。
- TypeScript：严格模式，无 `any`（除 generated 文件），absolute imports。
- CSS：Tailwind utility classes，暗色主题默认。
- **单文件规模与模块拆分**：严格限制单源码文件大小（推荐单文件不超过 800 行，严禁出现超大单文件）；大型模块必须按职责拆分为高内聚、低耦合的子组件与子模块。
- 安全：CSP、AES-256-GCM、路径遍历防护、文件扩展名白名单。

## 交互与补全开发原则（防回归规范）

详细设计与案例复盘见 [docs/development/interaction-and-testing-principles.md](docs/development/interaction-and-testing-principles.md)。

- **状态机思维**：任何上下文识别必须具备完整三要素（进入条件、状态内行为、退出跃迁条件），禁止编写只有进入而无退出的单向死锁逻辑。
- **软排序优先于硬过滤**：优先通过 `boost` 增减权重解决补全排序（期望项加权、干扰项降权）；除非语法 100% 互斥，否则严禁粗暴 `return null` 或过度过滤。
- **连续旅程测试（Journey Test）**：交互与编辑器逻辑禁止只测静态合法语句，必须编写模拟击键全过程的连续状态机测试（涵盖残缺中间态），断言每一步的状态跃迁与退出。
- **数据属性解耦**：点击与选择操作通过 DOM `data-*` 属性直接绑定标识，禁止依赖不可靠的视口几何坐标反查。
- **三维影响度自查**：提交前必须自查（1）是否彻底解决缺陷、（2）是否误伤合法同类、（3）用户下一步操作是否顺畅。

## 重要注意事项

- Path 驱动 Rust crate：`datazen-driver-<id>`；Git 驱动 Rust crate 名以插件仓库为准。
- `Cargo.toml` 中的插件占位段在 git 中应保持为空；`resolve-drivers.mjs` 构建时自动填充。
- 以下文件均为 gitignored 的 codegen 文件，由 `resolve-drivers` / `resolve-pro` 生成，切勿提交：
  - `src/plugins/generated.ts`
  - `src/plugins/generated-locales.ts`
  - `src/plugins/generated-pro.ts`
  - `src-tauri/src/driver_init.rs`
  - `src-tauri/capabilities/default.json`
- **Capabilities 管理**：`src-tauri/capabilities/default.json.host` 是 git 跟踪的 host 权限源文件；需要添加新 host capability 时直接修改该文件。`default.json` 在构建时自动合并生成，**严禁手动编辑或提交**。
- `PROTOCOL_VERSION`（`packages/driver-api`）变更时需同步更新所有插件。
- `AI_PROTOCOL_VERSION`（`packages/ai-api`）变更时需同步更新所有 AI Provider 插件。
- AI 配置加密存储在 `ai_config.enc`，不会出现在日志中。
- 连接密码等凭据：AES-256-GCM；**主密钥**默认在系统钥匙串，开发/adhoc 或 `DATAZEN_KEYRING=file` 时用 `{appData}/.key`。
- 日志文件位于 `{data_dir}/logs/`。

## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
