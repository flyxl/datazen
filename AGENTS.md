# DataZen Agent Guide

本文件是仓库级快速约定；详细架构以 `docs/architecture/`、开发流程以 `docs/development/` 为准。动手前先读本文件、相关目录说明和目标功能文档，并检查当前 branch、worktree、HEAD 与未提交差异。

## 项目与目录

DataZen 是 Tauri v2 + React 18 + TypeScript + Rust 的跨平台数据库管理工具，使用 pnpm 与 Cargo workspace。

```text
packages/desktop/            桌面壳：入口、platform/desktop 适配、shell
packages/web/            浏览器壳（`src/platform/web/`：HTTP transport、SSE、路由分发、不支持名单）
crates/datazen-desktop/  Desktop bootstrap、commands、platform、store、宿主适配（crate `datazen_desktop`，tauri.conf.json 在此）
packages/frontend/       共享 React UI（`@datazen/frontend`）：stores、组件、hooks、i18n、平台端口
packages/                driver-api、infra、UI/SDK（backend-client、driver-sdk、wapp-sdk、extension-points、ui）、wapps、drivers/*
crates/datazen/          单一业务内核 crate `datazen`（feature 门控的领域与用例；shared/application/execution/jobs/migration/各域）
crates/datazen-server/   团队 Web 服务宿主（crate `datazen-server`）：axum 中间件链 + `/healthz` `/readyz` + `/api/v1` 十五域全亮（112 域客户端方法 / 108 路由，契约 v1.10.1）+ wapp 静态面；管理库首版 SQLite（WR-D2 裁决）
e2e/                     Host WebdriverIO journeys
docs/                    features、architecture、development、blogs、bugs（已核实不修）、archive（过程文档归档）
deploy/                  服务端部署：Dockerfile / docker-compose 与 dev/ 一键开发栈（dex OIDC + seed）
```

数据库驱动实现与专属测试必须在 `crates/drivers/<id>/`；Host 不得硬编码驱动方言。`packages/ui` 是纯 React 基础组件库，不能依赖宿主 Store 或 IPC。

## DDD 内核结构（重构已落地）

重构方案已归档：[DDD 模块化重构](docs/archive/development/ddd-refactor-plan.md)（总纲 + 同目录 `ddd-refactor/` 细节）。D0–D8 已全部完成（旧 `packages/kernel` crate 已于 D8 退役删除；命令层薄壳化于 2026-11-04 核实收口：`_impl` 115 → 21，全部为文档化共享入口/测试夹具，无 1:1 转发残留）。当前不变量：

- `crates/datazen/` 是唯一业务内核 crate（package `datazen`），按功能组织 `application` / `domain`；共享契约在 `src/shared/`。
- `crates/datazen-desktop` 是 Desktop 宿主（crate `datazen_desktop`）；`crates/datazen-server/` 是团队 Web 服务宿主（crate `datazen-server`，W8 已交付十五域）；`packages/web/` 是浏览器壳（已落地，`src/platform/web/` 只做 HTTP/SSE 传输适配）。两个宿主互不依赖，都只依赖内核；server 的依赖闭包由 CI 门禁 `pnpm test:server-deps` 强制无 tauri/UI。
- Repository trait 由所属 domain 定义，Desktop/Server infrastructure 实现；不建立全局仓储全集。
- 宿主边界（command 与 HTTP handler）只做请求接线、DTO 与错误映射，不直接访问 Store、AppDb、Driver、Runtime internals；同一用例在 Tauri command 与 HTTP handler 是两个薄绑定，契约唯一来源是 `packages/backend-client` 的域客户端（`docs/architecture/platform/http-api-contract.md`）。
- `execution::runtime` 承载 actor、lease、队列、等待、资源回收；`execution::application` 编排用例；Job 归 `jobs`。
- 保持 `connectionId`、`dbSessionId`、`executionId` 语义不混用，以及 owner、scope、epoch、revision、事务、取消、幂等、artifact、恢复语义。
- 不提交空壳 Service、`todo!()`、DummyRepository 或重复状态；新增临时兼容导出必须写明消费者和删除阶段。

## 架构与安全不变量

- Rust 生产路径禁止裸 `unwrap()` / `expect()`；使用明确 `Result`、`thiserror`、`tracing` 和现有 `CommandError`。
- 组织作用域使用 `OrganizationScope::from_context(&ctx)` 或 `state.organization_scope()`；不得在宿主生产代码直接读取安装级组织常量。确有桌面专用例外时使用相邻 `// scope-gate: <id>` 并登记白名单。
- 公开摘要、错误、日志和 AI/MCP payload 不得包含凭据、绝对敏感路径或原始连接串；凭据使用 AES-256-GCM / 系统钥匙串（开发文件后端为显式配置）。
- 不读取、打印或 source `.env` / `.env.test` 内容；测试程序可以按项目约定读取，但输出不得泄漏凭据。
- 不以放宽权限、宽泛 `allow`、删除断言或跳过测试解决问题。
- 前端严格 TypeScript、禁止 `any`（generated 文件除外）；CSS 使用 Tailwind utility；IPC 前端 camelCase、Rust snake_case。
- `@datazen/frontend` 只经公开入口消费：`.`（平台端口与能力位）、`./app`（startSharedApp 启动链）、`./shell`（壳层切片）、`./styles.css`；`./testing/*` 仅测试文件可用。禁止 `@datazen/frontend/src/` 深导入（`scripts/check-frontend-public-entry.mjs` 门禁）。
- 数据库行为差异走 `DB_REGISTRY` / `DatabaseTypeMeta`；不要把某个数据库的判断散落到 Host。
- 交互/补全必须测试完整连续旅程，优先软排序，不用不可逆硬过滤；点击选择通过 DOM `data-*` 标识而不是视口坐标。

## 驱动、扩展与生成文件

- `scripts/resolve-drivers.mjs` 根据 `drivers-registry.json` 注入 path/git 驱动；`resolve-pro.mjs` 处理 Community/Pro。
- 生成文件不得提交：`packages/frontend/src/extensions/generated*.ts`、`crates/datazen-desktop/src/driver_init.rs`、`crates/datazen-desktop/capabilities/default.json`。
- 新 Host capability 改 `crates/datazen-desktop/capabilities/default.json.host`，不要手改生成的 `default.json`。
- `PROTOCOL_VERSION` 或 `AI_PROTOCOL_VERSION` 变化时同步检查全部插件/Provider。
- 四维扩展保持正交：Driver、静态 Theme、特权 EP、沙箱 Workspace App；扩展不绕过统一 Driver Command 和权限边界。

## 测试与构建

常用命令：

```bash
pnpm build
pnpm typecheck                       # 默认单引擎门禁，包含测试文件
pnpm typecheck:tsc                   # 权威 TypeScript 编译器
npx vitest run                       # Host 前端单测（含 backend-client）
pnpm --filter datazen-web test       # web 形态内核（platform/web）单测
cargo fmt --all -- --check
cargo test -p datazen_desktop --lib
cargo test -p datazen-server --lib   # 服务端宿主单测
node scripts/check-ddd-layers.mjs
node scripts/check-platform-crate-boundaries.mjs
pnpm test:server-deps                # server 闭包无 tauri/UI 门禁
pnpm e2e                             # 构建并运行 Host E2E
pnpm e2e:contract:matrix
```

测试输出较长时写入系统临时目录，单独记录退出码，再读取结论行；不能把超时、OOM 或环境缺失误报成代码通过或回归。Rust 链接资源不足可用 `CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'` 重跑并如实记录。

E2E 必须使用 `pnpm tauri:build:webdriver` 或 `pnpm e2e` 的构建流程，禁止用裸 `cargo build` 代替。驱动专属 Rust/UI/E2E 测试必须落在对应驱动目录；Host 通用旅程放 `e2e/`。

## 并行开发与 Git

- 并行开发使用独立 worktree；主检出只用于合流和清理，不在同一 worktree 同时提交与验证。
- 使用 `scripts/new-feature-worktree.sh <track> <base>` 创建轨道；不要 `pnpm install` 破坏软链。并行 Cargo 轨道设置独立 `CARGO_TARGET_DIR`。
- 轨道期间可提交 `progress.md`，协调分支可提交 `hub.md`；功能验收合并后必须删除，不能成为交付物。
- 提交前检查 `git status`、`git diff --check`、实际测试和 diff；只提交本任务文件。提交信息使用 `feat:` / `fix:` / `refactor:` / `docs:` / `test:` / `chore:`。
- 只在用户明确要求时 push、创建 PR、部署、发布或执行外部有副作用操作；PR 前检查全部提交、base/head、diff、checks。

## 文档与实现纪律

- 长期文档只维护 `docs/features/`、`docs/architecture/`、`docs/development/`，以及公开 `docs/blogs/` 和明确不修的 `docs/bugs/`；过程文档（PRD、设计稿、台账、worklog、评审）交付后移入 `docs/archive/`，不留在活跃目录。
- 不新增 PRD、方案评审、Bug List、进度报告类长期文件；已实现结论改写进对应架构文档。
- 修改路径、IPC、命令、驱动数量或测试事实前先核对代码；文档链接必须人工检查可达。
- CodeGraph 存在时优先 `codegraph explore`；否则使用专用搜索/读取工具，编辑前先读当前文件。
