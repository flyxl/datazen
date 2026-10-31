# DataZen Agent Guide

本文件是仓库级快速约定；详细架构以 `docs/architecture/`、开发流程以 `docs/development/` 为准。动手前先读本文件、相关目录说明和目标功能文档，并检查当前 branch、worktree、HEAD 与未提交差异。

## 项目与目录

DataZen 是 Tauri v2 + React 18 + TypeScript + Rust 的跨平台数据库管理工具，使用 pnpm 与 Cargo workspace。

```text
apps/desktop/            桌面壳：入口、platform/desktop 适配、shell、以及 src-tauri 宿主
apps/desktop/src-tauri/  Desktop bootstrap、commands、platform、store、宿主适配（crate `datazen_desktop`）
packages/frontend/       共享 React UI（`@datazen/frontend`）：stores、组件、hooks、i18n、平台端口
packages/                driver-api、infra、UI/SDK（backend-client、driver-sdk、wapp-sdk、extension-points、ui）、wapps、drivers/*
modules/                 单一业务 crate `datazen`（feature 门控的领域与用例）
server/                  团队 Web 服务宿主（crate `datazen-server`，web-ready WR-D1 起骨架：HTTP 中间件链 + /healthz /readyz；域路由随 WR-D2+ 逐域点亮）
e2e/                     Host WebdriverIO journeys
docs/                    features、architecture、development、blogs、已核实且明确不修的 bugs
```

数据库驱动实现与专属测试必须在 `packages/drivers/<id>/`；Host 不得硬编码驱动方言。`packages/ui` 是纯 React 基础组件库，不能依赖宿主 Store 或 IPC。

## DDD 重构目标

权威方案：[DDD 模块化重构](docs/development/ddd-refactor-plan.md)（总纲；实施细节按主题在同目录 `ddd-refactor/` 下，细节文件保留原 §2–§14 编号）。该方案区分当前事实与目标设计，不能把目标目录写成已实现。

- 最终以一个 `modules/` crate 按功能组织 `application` / `domain`；目标 package 为 `datazen`（已落地：`modules/` 即 `datazen` crate）。
- `apps/desktop/src-tauri` 是 Desktop 宿主（crate `datazen_desktop`）；`server/` 是团队 Web 服务宿主（crate `datazen-server`，WR-D1 骨架已落地）；目标浏览器宿主 `apps/web/`（尚未落地，不要写成已实现；Web 形态已移出 DDD 重构范围，D8 在桌面单宿主内收尾）。两个已落地宿主互不依赖，都只依赖 `modules` 内核；server 的依赖闭包由 CI 门禁 `pnpm test:server-deps` 强制无 tauri/UI。
- Repository trait 由所属 domain 定义，Desktop/Web infrastructure 实现；不建立全局仓储全集。
- Controller 只做上下文、DTO、应用服务调用和错误映射，不直接访问 Store、AppDb、Driver、Runtime internals。
- `execution::runtime` 承载 actor、lease、队列、等待、资源回收；`execution::application` 编排用例；Job 归 `jobs`。
- 保持 `connectionId`、`dbSessionId`、`executionId` 语义不混用，以及 owner、scope、epoch、revision、事务、取消、幂等、artifact、恢复语义。
- 每个迁移 PR 必须有真实消费者、行为测试和依赖门禁；不提交空壳 Service、`todo!()`、DummyRepository 或重复状态。
- 当前阶段按 D0→D8 推进；临时兼容导出必须写明消费者和删除阶段，D8 清零。

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
- 生成文件不得提交：`packages/frontend/src/extensions/generated*.ts`、`apps/desktop/src-tauri/src/driver_init.rs`、`apps/desktop/src-tauri/capabilities/default.json`。
- 新 Host capability 改 `apps/desktop/src-tauri/capabilities/default.json.host`，不要手改生成的 `default.json`。
- `PROTOCOL_VERSION` 或 `AI_PROTOCOL_VERSION` 变化时同步检查全部插件/Provider。
- 四维扩展保持正交：Driver、静态 Theme、特权 EP、沙箱 Workspace App；扩展不绕过统一 Driver Command 和权限边界。

## 测试与构建

常用命令：

```bash
pnpm build
pnpm typecheck                       # 默认单引擎门禁，包含测试文件
pnpm typecheck:tsc                   # 权威 TypeScript 编译器
npx vitest run                       # Host 前端单测
cargo fmt --all -- --check
cargo test -p datazen_desktop --lib
node scripts/check-ddd-layers.mjs
node scripts/check-platform-crate-boundaries.mjs
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

- 长期文档只维护 `docs/features/`、`docs/architecture/`、`docs/development/`，以及公开 `docs/blogs/` 和明确不修的 `docs/bugs/`。
- 不新增 PRD、方案评审、Bug List、进度报告类长期文件；已实现结论改写进对应架构文档。
- 修改路径、IPC、命令、驱动数量或测试事实前先核对代码；文档链接必须人工检查可达。
- CodeGraph 存在时优先 `codegraph explore`；否则使用专用搜索/读取工具，编辑前先读当前文件。
