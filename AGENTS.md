# [AGENTS.md](http://AGENTS.md)

> 本文件面向 AI 编程助手，帮助其快速理解项目结构和约定。详细架构设计见 [docs/architecture/](docs/architecture/README.md)。

## 项目概述

DataZen 是一个跨平台桌面数据库管理工具，基于 **Tauri v2**（Rust 后端 + React 前端）构建，集成 AI 辅助功能。

- **框架**：Tauri v2 + React 18 + TypeScript + Tailwind CSS 4
- **包管理**：pnpm（前端）、Cargo workspace（Rust）
- **状态管理**：Zustand
- **设计系统**：`@datazen/ui` 纯 React 基础视图组件库（宿主与插件共享）
- **测试**：Vitest（Host 单元）、驱动 crate 内单测/E2E、WebdriverIO（Host E2E）
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
│   └── extensions/              # 自动生成注册文件（generated.ts / generated-locales.ts / generated-pro.ts）
├── src-tauri/                   # Rust 后端
│   ├── src/
│   │   ├── ai/                  # AI 对外 re-export 表面 + 宿主 Runtime/SchemaCache/profile 适配
│   │   ├── bootstrap/           # AppState、启动与关停装配
│   │   ├── commands/            # Tauri IPC 命令
│   │   ├── dashboard/           # Dashboard 宿主存储与执行适配
│   │   ├── db/                  # DriverRegistry 门面（re-export → packages/runtime）
│   │   ├── mcp/                 # MCP Server/Client 宿主传输与权限
│   │   ├── monitor/             # MonitorEngine、授权与 occurrence
│   │   ├── platform/            # PlatformAdapter、DesktopSessionBackend、身份/策略/仓储
│   │   ├── services/            # QueryExecutor、Schema metadata、DbTools、JobRegistry
│   │   ├── store/               # AES-256-GCM 加密持久化
│   │   ├── tunnel/ + ssh_tunnel.rs  # 隧道装配
│   │   ├── wapps/               # Wapp 协议桥与管理器
│   │   └── workflow/            # YAML Workflow 宿主适配与历史
│   └── resources/               # 菜单翻译、内置 Workflow 模板、原生工具探针
├── packages/
│   ├── ui/                      # @datazen/ui: 纯 React 基础组件设计系统（Button, Dialog, Select 等）
│   ├── extension-points/        # @datazen/extension-points: 特权扩展点 (EP) 契约与注册表
│   ├── wapp-sdk/                 # @datazen/wapp-sdk: Workspace App 沙箱应用前端 SDK
│   ├── driver-sdk/              # @datazen/driver-sdk: 数据库驱动前端元数据/方言/Command SDK
│   ├── driver-api/              # Rust: DatabaseDriver trait + Command API + inventory 注册
│   ├── platform-api/             # Rust: 跨形态端口与契约（ProfileRepository / RequestContext / FrontendBridge）
│   ├── platform-infra/           # Rust: 形态中立的跨形态基础设施原语（脱敏 / 路径安全），零 datazen-* 依赖
│   ├── application/              # Rust: 应用层用例与 ApplicationServices（不依赖宿主形态）
│   ├── runtime/                  # Rust: 共享内核（session / resource / job / gateway / schema_cache / sql_guard）
│   ├── feature-core/             # Rust: 共享 feature（Workflow / AI / MCP / Dashboard / 导出 / 备份恢复）
│   ├── schema-diff/ data-sync/ data-transfer/ migration-common/  # 迁移三件套与共享迁移边界
│   ├── workspace-apps/           # Rust: Wapp manifest / 安装 / 存储
│   ├── backend-client/           # JS: 远端 / 浏览器侧 BackendClient 端口实现
│   ├── ai-api/                  # Rust: AiProvider trait + 工厂 + 消息/调用协议模型
│   ├── ai-providers/            # Rust: OpenAI/Anthropic/DeepSeek/Ollama/Custom provider 适配 + wire protocol 实现（纯客户端，不依赖 Tauri）
│   ├── drivers/                 # path 驱动 crate（测试严格写在各 crate 内）
│   │   └── <id>/                # Rust `src/` + `tests/`；UI `ui/__tests__/`；E2E `e2e/`
│   ├── wapps/                   # Workspace Apps 运行时应用与主题源码包（安装测试见其 README）
│   └── pro-extensions/          # Pro 扩展本地存放/挂载目录（gitignored）；每个子包是独立 git 仓库，需独立 commit/pull/push
├── e2e/                         # Host WebdriverIO E2E（通用 UI / IPC；非驱动方言）
├── docs/                        # 入库文档：features/、architecture/、development/、blogs/、bugs/
└── posts/                       # 推广与发布文案本地存档（gitignored，不提交）
```

## 核心架构模式

### 驱动选型与双版本构建模式（编译时，类似 Caddy 2）

1. `drivers-registry.json` 定义 path 驱动 + git 驱动；Git 可钉 `ref`。
2. `scripts/resolve-drivers.mjs` 构建前执行驱动选型、克隆 Git driver，并生成 `generated.ts`、`driver_init.rs`、`.driver-features.json`（均 gitignored）。
3. `scripts/resolve-pro.mjs` 控制构建版本：默认 `--edition=community`（纯开源 Fallback）；`--edition=pro`（打包并 stage `@datazen/extension-sql-editor-pro` 到 `src-tauri/resources/builtin-ep/`，运行时经签名验签后动态加载）。
4. 数据库驱动通过 `inventory` crate 实现链接时自动注册；宿主 `DriverRegistry` 仅走 factories。

注入前托管清单文件的干净副本会存进 gitignored 的 `.driver-file-stash/`；该 stash 带 `.stash-meta.json` 旁证，复用前判新鲜度，**陈旧时 `resolve-drivers.mjs` 以退出码 1 硬失败并给出恢复命令，而不是静默回滚合并后的清单文件**。此时**不要**跑 `--restore`（它会用合并前的旧副本覆盖当前结果）。详见 [docs/development/worktree-isolation.md](docs/development/worktree-isolation.md)。

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
4. **Workspace App (工作区应用)**：独立全屏 iframe 沙箱应用，运行目录 `{appData}/wapps/{publisher}.{name}/`，基于 `@datazen/wapp-sdk`，通过 `window.postMessage` 受控桥与宿主通信，取数一律走 `execute_driver_command`。

`@datazen/ui`（`packages/ui/`）不是第五维扩展，而是四维共享的纯 React 基础视图组件库（Button, Input, Select, Dialog, Tabs, Badge, Label, cn），与宿主业务 Store 和 IPC 严格解耦；宿主 `src/components/ui/` 和驱动 UI 统一复用。

### Driver Command API 与统一执行网关

`packages/driver-api` 提供统一 Command 抽象（`command_definitions()` + `execute_command()`）。SQL 编辑器的 `query`/`execute`、流式 `query_stream`、Schema 对象元数据、Redis 操作、管理命令等全部走 `execute_driver_command`，宿主不按 Driver 类型硬编码。Redis 深度能力集中在 `packages/drivers/redis`，宿主仅为薄 Tab 壳。

### AI 模块与 ai-api

- `packages/ai-api` 是 Rust 层的 AI Provider 抽象契约（类比 `driver-api`），定义核心 `AiProvider` trait、请求/流式响应模型与工厂注册，并统一 active → default → first 的 AI Profile 选择回退规则。
- `packages/ai-providers` 承载具体 provider 适配与 wire protocol 实现：`protocol/{openai_chat,openai_responses,anthropic}.rs` 实现 OpenAI Chat/Responses 与 Anthropic 协议，`{openai,anthropic,deepseek,ollama,custom}.rs` 实现 OpenAI / Anthropic / DeepSeek / Ollama / 自定义端点适配，`registry.rs` 负责注册与配置恢复，`metadata.rs` 提供现有 Provider 的默认 endpoint/protocol 元数据。`packages/feature-core/src/ai/` 承载 Provider-neutral 的 Prompt、Schema、Chat/NL2SQL 请求组装、诊断/分析/过滤/文档生成、安全、预算、取消、工具循环与 ownership policy（包括 grant database 绑定和结果 attribution）；`src-tauri/src/ai/` 仅保留 `mod.rs` 的对外 re-export 表面与 Runtime/SchemaCache/profile 元数据适配，`commands/ai/` 保留 Tauri IPC、Provider/Profile 配置、仓储/会话读取、输入脱敏、context 文件、MCP/Runtime 工具宿主、事件流和取消接线。
- `PromptResolver` 优先级：用户覆盖 → 驱动覆盖 → 资源文件 → 编译时英文嵌入。
- 详情参考 [docs/architecture/backend/ai.md](docs/architecture/backend/ai.md)。

### MCP (Model Context Protocol)

Server 暴露 Tools/Resources/Prompts（DB tools 使用持久化 `connection_id`）；Client 连接外部 MCP Server；`--mcp-stdio` 启动无头模式。详见 [docs/architecture/backend/mcp.md](docs/architecture/backend/mcp.md)。

### Workflows

YAML 驱动的通用执行引擎，GUI、Tauri IPC 和 MCP 共用同一 runtime。Step 通过 Driver Command API 执行；Workflow 默认 connection 可被 Step 继承或覆盖。详见 [docs/architecture/backend/workflow.md](docs/architecture/backend/workflow.md)。

## 前端约定

- **零硬编码**：数据库行为差异全部通过 `DB_REGISTRY` + `DatabaseTypeMeta` 元数据驱动。
- **主工作区 Page**：`main` 内用 `*Page` 导航（`WelcomePage` / `ConnectionPage` / `SettingsPage` 等）；Settings / 新建连接为 main 内嵌；Docs 跳转官网。独立子窗口仅限 `onboarding` / `backup` / `data-sync` / `schema-diff` / `data-transfer`。详见 [docs/architecture/windows.md](docs/architecture/windows.md)。
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
| SQL 编辑器 & 风控 | `components/sql-editor/` + `windows/connection/query/` | `packages/runtime/src/sql_guard/` + Runtime session transaction + `execute_driver_command` |
| 结构比对 (Schema Diff) | `windows/schema-diff/` | `packages/schema-diff/` + `commands/schema_diff.rs`（Driver `MigrationRenderer`） |
| 数据传输 (Transfer) | `windows/data-transfer/` | `packages/data-transfer/` + `commands/data_transfer/` |
| 数据同步 (Data Sync) | `windows/data-sync/` | `packages/data-sync/` + `commands/sync/` |
| 图表可视化 | `components/chart/` + `lib/chart/` | — |
| ER 图 | `windows/connection/ErDiagramView.tsx` + `er/` | `commands/schema.rs → get_er_data` |
| 数据与结构导出 | `DataTable/DataExportDialog.tsx` + `BatchExportDialog.tsx` | `commands/export.rs` |
| AI Chat & 诊断 | `components/ai/AiChatPanel.tsx` + `DiagnosisPanel.tsx` | `commands/ai/` + `packages/feature-core/src/ai/` |
| Workflows | `windows/workflow/WorkflowPage.tsx` | `packages/feature-core/src/workflow/` + `workflow/` 宿主适配 |
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
pnpm typecheck                         # tsc --noEmit，覆盖 src + packages，**含测试文件**
cargo test -p datazen --lib            # Host Rust 单元测试
cargo test -p datazen-ai-api --lib     # AI API 单元测试
cargo test -p datazen-driver-postgres  # 示例：某个驱动 crate 的 Rust 测试
```

> **测试命令的长输出必须落临时文件再读**：测试/门禁命令（`vitest`、`cargo test`、`typecheck`、
> `check-*.mjs`、`pnpm build`）的输出动辄上千行，直接回显会被工具截断，而**结论恰恰在末尾**
> （`Test Files` / `Tests` / `test result:` / 编译错误列表 / 退出码）——被截掉就等于没跑，
> 且极易据此误判"通过"。一律重定向到**系统临时目录**，再用 `tail` / `grep` 取需要的片段：
>
> ```bash
> LOG="$(mktemp -t datazen-test).log"
> npx vitest run > "$LOG" 2>&1; echo "EXIT=$?"     # 退出码必须单独打印并如实记录
> tail -n 40 "$LOG"                                # 结论行
> grep -nE 'FAIL|✗|test result:|error\[|error TS' "$LOG" | head -60   # 失败清单
> ```
>
> 注意：临时文件**只放系统 temp 目录**，不得落在仓库内（会污染 `git status`）；
> 完整日志留在文件里备查，不要为了"看全"而把整个文件回显进对话。

> **测试文件参与类型检查**：`tsconfig.json` 不再排除 `__tests__/` 与 `*.test.ts(x)`，
> 新增或修改测试后必须保证 `pnpm typecheck` 干净。历史上测试被排除在外，导致 mock
> 与真实类型长期漂移而不被发现（例如 `TableInfo.rowCount` 实收 `null` 却声明为 `?: number`）。
> 若某个 mock 确实只实现子集，请用精确断言（`as unknown as X`、`satisfies`、`Pick<>`）显式说明，
> 禁止用 `any` 绕过；也不要为了让类型通过而删测试或删断言。

**同一棵工作树不得同时被提交方和验证方使用。** 验证方做变异、提交方跑门禁或 `git add`，必须用各自独立的工作树：`git worktree add --detach <path> <commit>`。实测代价：同一棵树上「提交方 `git add`」收进去的是验证方变异后的残留，于是出现一个 message 与内容完全相反的提交，且 `rev-parse` / `is-ancestor` 全部放过——旧提交本身合法，只有内容是毒的。任何门禁重跑都要**首尾各记录一次 HEAD 与工作区 sha**，证明运行期间没人动过。

并行开发用子代理时，主代理直接派发独立子任务并在汇总时验收。

**进度台账：开发期间允许，交付即销毁。** 多轨并行时进度、门禁实测、待裁定项散落在对话上下文里，主代理一死就全丢，且新接手的实例无法判断前一个实例做完了什么。因此：

- **允许**在各轨分支根目录记 `progress.md`（自证已完成/进行中/未开始、门禁命令与逐字结论行、遗留与待裁定项），以及在集成分支根目录记 `hub.md` 作为协调者的跨轨台账。这两个文件**可以提交入库**，随分支流转。
- **禁止**把它们当交付物：轨道通过验收并合并后，`progress.md` 与 `hub.md` **必须删除**，不得存活在 `main` 上。删除是合并流程的收尾步骤之一，与清理 worktree、删除 feature 分支同级。
- **缺陷清单不单独建文件**：缺陷结论记在台账里当下游输入，最终必须变成代码、测试和 `docs/` 正式文档里的事实。台账消失后，结论必须仍然能从代码和测试读出来——否则说明结论只存在于台账里，是没做完。

**请求用户决策时，必须在同一轮内交付足以决策的上下文。** 只抛结论、把选择丢回去，等于让用户
在你没有给出依据的情况下替你承担判断责任。实测代价：门禁盘点阶段把「R3 规则要不要转阻断」
和「restore-guards 要不要留在 typecheck 链里」两条标进「需你决定」，却各只给一句
「这是特性级重构」「建议原样保留」——没有数字、没有范围、没有代价、没有替代方案，
用户无法判断，只能照单接受或拒绝，两条都成了无效征求意见。写「需你决定」之前先自查：

- **具体位置**：哪条规则、哪个文件、哪一行、当前实测命中几条（本例：R3 全仓仅 1 处，
  `src/windows/connection/DocumentConnectionView.tsx` 单行 import 引入 9 个符号）。
- **实测数字**：耗时占比、当前是否已红、翻成阻断会让多少个 PR 变红
  （本例：38ms / 13808ms typecheck = 0.28%；翻阻断则每个 PR 都红）。
- **修复的工作量与边界**：改哪几个包、动多少个调用点、是否要扩展共享契约
  （本例：9 个纯函数要经 `DatabaseTypeMeta` 暴露，而该契约当前零函数槽位）。
- **各选项的代价**：包含「什么都不做」这一项，以及选项之间的可逆性。
- **实测与推断要分开**：凡是推断，标明依据；凡是实测，给出命令与退出码。

同样地，**对已有结论补上下文时，先核对结论本身是否经得起复查**——本次正是靠复查发现
「136 行纯逻辑函数被说成特性级重构」这个定级错误：数字补齐后，改成契约扩展而非改写文件。

推广与发布文案只写本地 `posts/`（gitignored）。

### 文档维护纪律

- 仓库只维护三类长期文档：功能使用（`docs/features/`）、代码架构（`docs/architecture/`）、开发流程（`docs/development/`）；另有公开发布的 `docs/blogs/`，以及仅记录**已核实复现、本轮明确不修**缺陷的 `docs/bugs/`。索引见 [docs/README.md](docs/README.md)。
- **不新增** PRD、设计稿、原型图、实施规范、开发进度、Bug List、评审记录类文档；这些是一次性过程产物，已交付即失效。缺陷结论优先变成代码、测试与正式文档；只有核实复现且明确不修的缺陷才按 `docs/bugs/` 的单文件规则登记。唯一例外是并行开发期间的进度台账（分支根目录 `progress.md` / 集成分支根目录 `hub.md`）——见上文「进度台账：开发期间允许，交付即销毁」：开发期间它存在于分支而非 `docs/`，**验收合并时必须删除**，因而同样满足「已交付即失效」。
- 用户明确要求并已入库的 [桌面 / Web 平台演进设计](docs/README.md) 是本纪律的窄范围例外；其中标为「目标设计」的概要、详细设计和开发计划只在该明确授权范围内维护，落地后逐节改写为已实现事实。该例外不授权新增其他方案、评审记录或进度文档。
- 需要长期留存的设计结论，**改写为「已实现」的事实**并入对应架构文档，而不是保留方案原文。
- 引用文件路径、命令、IPC 名称、驱动数量等事实前先核对代码；删除或重构代码时同步删除失效引用。
- 写完文档自查链接可达：`pnpm exec` 下的脚本与 CI 不会校验 Markdown 链接，失效引用只能靠人工/代理检查。

### E2E 测试

- **构建方式**：必须使用 `pnpm tauri:build:webdriver`（或 `pnpm e2e` 自动触发）。**禁止**直接执行裸 `cargo build` 或缺少驱动注入参数的编译。
- **数据迁移三件套的 DMG 打包边界**：Schema Diff、Data Sync、Data Transfer 的 WDIO 构建如果已成功生成可运行的 `DataZen.app`，但随后仅在 macOS DMG 打包阶段失败，该 DMG 问题不属于本功能的阻塞项；不要为此中断迁移功能开发或专门修复打包脚本。继续用生成的 `.app` 运行 WDIO，并以应用启动和 WDIO 结果判断功能验证状态。
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

- **开发期间**：只修改 `en.ts`（英文），不要同时修改其他语言文件。
- **发布前（硬阻断）**：`node scripts/i18n-sync-check.mjs` 已在 `.github/workflows/release.yml`
  的 `pre-release-guards` 中作为**发布硬门禁**——翻译不完整，发布构建根本不启动。
  **发布出去的版本不允许只有英文**；`bump-version` skill 触发重建时必须一并跑该检查。
  缺失的 key 用 i18n-sync skill 补齐（只补翻译，不改 `en.ts`）。
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
  - `src/extensions/generated.ts`
  - `src/extensions/generated-locales.ts`
  - `src/extensions/generated-pro.ts`
  - `src-tauri/src/driver_init.rs`
  - `src-tauri/capabilities/default.json`
- **Capabilities 管理**：`src-tauri/capabilities/default.json.host` 是 git 跟踪的 host 权限源文件；需要添加新 host capability 时直接修改该文件。`default.json` 在构建时自动合并生成，**严禁手动编辑或提交**。
- `PROTOCOL_VERSION`（`packages/driver-api`）变更时需同步更新所有插件。
- `AI_PROTOCOL_VERSION`（`packages/ai-api`）变更时需同步更新所有 AI Provider 插件。
- AI 配置加密存储在 `ai_config.enc`，不会出现在日志中。
- **本地环境变量文件保护**：任何 Agent（含子代理）不得打开、读取、解析、source 或打印仓库及其 worktree 中 `.env` / `.env.test` 文件的**内容**；不得将凭据写入提示、日志或报告。只允许检查文件是否存在及 Git 忽略状态。
  - **测试程序读 `.env` 是合法的**，不得因此禁止运行测试或驱动 live 套件——集成测试本就需要其中的 `DATABASE_URL` 等配置。被禁的是「把内容读进上下文」，不是「程序读文件」。误把二者混同会逼出「不跑测试」的假禁令，等于放走真实缺陷。
  - **真正的泄漏路径是输出回显**：命令的 stdout / panic 消息 / 断言失败文本里若回显了凭据，内容照样会随对话进入模型服务商。运行任何命令前先确认其输出路径不含凭据；一旦怀疑输出可能含密钥，只摘录结构性结论（哪个文件、哪条断言、退出码、测试计数），**不要转发原始输出**，并优先用上面的「长输出落临时文件」模式取片段。
  - 审阅「读 env 文件」的测试代码时，看**源码**（是否自造合成探针、默认值是否拒绝读文件），不看 `.env` 本身。
- 连接密码等凭据：AES-256-GCM；**主密钥**默认在系统钥匙串，开发/adhoc 或 `DATAZEN_KEYRING=file` 时用 `{appData}/.key`。
- 日志文件位于 `{data_dir}/logs/`。

## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
