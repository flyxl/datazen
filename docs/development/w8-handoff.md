# W8 交接文档：点亮路径两刀（server 执行面绿路径）

> **交接日期**：2026-11-05。**交接范围**：Wave8-a（点亮上半）与 Wave8-b（点亮下半）的全部剩余工作。
> **当前基线**：main `7fad412`（PR #175 合并后）；W8-a P1 已完成并推送至分支 `w8-a`。
> **权威台账**：[web-ready-plan.md §11](web-ready-plan.md#11-进度回写区)（2026-11-05 Wave8-a P1 条目含全部设计裁决）。
> 本文是接手人视角的操作手册：先读「现状」，再按「剩余工作」逐刀执行，「环境须知」与「执行纪律」两节是本沙箱/本仓的硬约束。

## 1. 现状（截至交接时）

### 1.1 已完成（全部合并进 main，CI 4/4 绿）

| 波次 | 内容 | PR |
| --- | --- | --- |
| Wave6 全部 | WR-F7 目标元数据、C2 driverGateway 过渡域、C3 tableData 域路由（14 方法 × 状态表）、A3 P2 构建期手术面等 | #168–#172 |
| W7-A | crates/packages 结构重构（纯移动零行为变更，`modules → crates/datazen` 等，包名不动） | #173 |
| W7-B | `drivers` 提入 desktop default，脚本化构建 `--no-default-features`，SKU 特征集零变化 | #174 |
| W7-C | rust job 磁盘加固（DEBUG=0 + INCREMENTAL=0 + runner 预装清理） | #175 |

### 1.2 W8-a P1 已完成（分支 `w8-a`，待开 PR）

**内容**：server 驱动工厂点亮——`driver-*` per-driver feature（16 个）+ `deploy-drivers` 聚合，缺省不链接任何驱动（注册表空表，fail-closed 语义不变）。

**改动清单**（8 个文件）：

1. `Cargo.toml`（根）：`[workspace.dependencies]` 补其余 12 个驱动 path 条目。
2. `crates/datazen-server/Cargo.toml`：16 个 `optional = true` workspace 依赖 + 16 个 `driver-*` feature + `deploy-drivers` 聚合。
3. `crates/datazen-server/src/driver_link.rs`（新文件）：15 个 `extern crate` 链接闭包（每个挂同名 feature）。
4. `crates/datazen-server/src/lib.rs`：挂 `pub mod driver_link`。
5. `crates/datazen-server/src/bootstrap.rs`：`DriverRegistry::with_factories([])` → `DriverRegistry::new()`（inventory 懒形）。
6. `scripts/check-managed-stubs.mjs`：`--check-static-drivers` 增 server 腿（`serverDriverDeclarationProblems`）。
7. `scripts/__tests__/check-static-drivers.test.ts`：server 腿 5 用例。
8. `Cargo.lock`：无变化（缺省形态零依赖边变化）。

**机制链**（与桌面宿主同源）：驱动 crate 内 `datazen_driver_api::inventory::submit!`（如 `crates/drivers/postgres/src/sync_adapter.rs:14`）→ driver-api `factory.rs` 的 `inventory::collect!(&'static dyn DatabaseDriverFactory)` → server 的 `driver_link.rs` extern crate 强制链接 → `DriverRegistry::new()` 懒加载。mysql crate 自带同源方言工厂（MariaDB/Doris/Starrocks/Manticore/OceanBase-Oracle）、postgres 自带 QuestDB 别名工厂，链接即随 crate 入表。

**验证状态**：
- ✅ `node scripts/check-managed-stubs.mjs --check-static-drivers` 双腿绿（桌面 15 path / 2 git + server 15 path / 2 git）
- ✅ `corepack pnpm vitest run scripts/__tests__/check-static-drivers.test.ts` 12/12
- ✅ `cargo check -p datazen-server`（缺省形态）绿（5m57s，冷构建）
- ⚠️ `cargo check -p datazen-server --features deploy-drivers`（聚合形态，15 驱动全链接）编译至驱动 crate 层无错后被沙箱重置中断——**push 后以 CI 复验为准**
- git 驱动（kiwi/superset）不在此列（维持现状裁决：私有仓约束，构建期注入，静态声明会炸干净检出的 `cargo metadata`）

## 2. 剩余工作

### 2.1 W8-a 余项（点亮上半，接 P2/P3/接线）

**P2：`TargetCommandRunner` server 实现**（设计已裁决入账，直接照做）

- 落点：`crates/datazen-server/src/execution/target_runner.rs`（新文件）。
- 结构：`pub struct ServerTargetRunner { host: Arc<dyn ResourceExecutionHost>, lease: Arc<dyn RuntimeLeaseSource> }`。
- 端口：`datazen::table_browsing::api::TargetCommandRunner`（`api.rs:196`，单方法 `run(&self, context, binding, command, input) -> Result<Vec<Json>, ApiError>`）；`TargetBinding = ExecutionTarget + revision: Counter + manual_handle: Option<SessionHandle>`。
- 参考实现（桌面）：`crates/datazen-desktop/src/commands/table_target.rs`（`DesktopTargetRunner`，57–154 行）。
- **target 分支**：`host.prepare(&connection_id, revision.get(), command, target)`（复用 WR-F7 闸序，返回 `PreparedExecution { use_cases, artifacts, context, target }`）→ `prepared.use_cases.issue_submission_token(context, ExecuteAtTarget, None)` → `execute_at_target`（`ExecuteAtTargetRequest { target: prepared.target, expected_config_revision: binding.revision, call, idempotency_key }`）→ 轮询 `get_execution`（25ms 间隔）到 terminal → 要求 `Succeeded` → 逐 artifact 分块收集（`prepared.artifacts.read_chunk`，首块读 `published_chunk_count` 后循环取块）。
- **manual_handle 分支**：`lease.resolve()` **每调用现取**（租约纪律：禁止缓存 `RuntimeConnectionUseCases`）→ `get_session` → 三字段校验（`connection_id`/`config_revision`/`observed_context.namespace` 与 binding 不符 → `InvalidArgument "Manual table session target changed"`）→ `issue_submission_token(ExecuteInSession, Some(handle))` → `execute_in_session`（带 `expected_context_revision: Some(session.context_revision)`）→ 轮询收集（用 `runtime.read_artifact_chunk` 固有方法——`ConnectionUseCases` trait 无此方法，target 分支因此用 `PreparedExecution.artifacts` 端口）。
- **授权语**：执行面（token/execute/poll/collect）全用 `run()` 的 `RequestContext`（请求真主体）；`prepare` 内部 profile 读取沿用 host 合成主体 `server-mcp-tool`（P3 收口点）。
- **错误映射**：`WorkflowError` → `ApiError` 沿用 `driver_gateway.rs:139` 既有裁决（`Driver`/`ConnectionFailed` → `ServiceUnavailable`；其余 → `InvalidArgument`）。
- **装配**：bootstrap `TableDataState` 扩展（+`registry` +`runner`），与 driverGateway/MCP 共享 host/lease/registry 同一实例（bootstrap 既有纪律）。

**P3：授权种子**——把 `resource_host.rs` 的合成服务主体（`server-mcp-tool`）替换/收口为请求真主体的传播机制（`stateless_exec` 模块头同款登记，见 `resource_host.rs` 模块头登记第 2 条）。

**tableData 十一端点换真用例接线**（501 → 真实现，逐条按 `table_data.rs` 模块头状态表）：

- 读三端点（`getTableData`/`previewTableReadSql`/`estimateTableRows`）：`read_table_page`/`preview_table_read` 用例 + `registry.get_sql_driver_by_name(database_type)` + runner；handler 形态照桌面 `table_target.rs::read/preview`（输入校验 → profile/metadata → driver → `validate_filters` → 调用例）。
- FK 四端点 + 变更四端点：各有宿主端口依赖（`ForeignKeyNavigationSource`、`TableColumnSource`、会话执行面），依赖清单与不可服务原因逐条登记在 `crates/datazen-server/src/http/domains/table_data.rs` 模块头 14 方法状态表——接一条删一条 501，状态表同步。
- 测试：`crates/datazen-server/tests/table_data_routes.rs` 扩容（fixture 已有真存储/登录会话/CSRF 通道；驱动 mock 用 `DriverRegistry::with_factories([mock])` 自装配——registry 懒形与静态形并存，测试可用静态形）。

**mcp seed-example + 七工具绿路径**：常态 CI 用 fake-driver（桌面 fake-driver 先例），真方言 `#[ignore]`。七工具经 `read_target_command` 的闸序（profile → driver → 命令目录 → 执行）不变。

### 2.2 W8-b（点亮下半）

1. **stream 投递桥**：`QueryStreamEvent` → 既有 events 端点 SSE 词汇（复用 WR-D5 生命周期）；`execute_driver_command_stream` 的闸后 501 → 真建流。
2. **driverType 直取**：`execute_driver_command` 的 driverType 路径（内联临时连接 → 内存 ephemeral 会话句柄 TTL + principal 归属）。
3. **命令历史落 audit_events**（v6 既有表）：`recordHistory` 透传从「接受但无半边」变真落账。
4. **driver_gateway_routes 测试 17 扩容** + 契约腿同步 + 两域 501 状态表清零。

### 2.3 环境侧余项（不随 W8，长期等环境）

WR-A4 live 验收（`#[ignore]` + `run-live-contract.sh`）、WR-G2 真浏览器故障面、WR-G3 `:8443` TLS 与镜像演练、`pnpm e2e` 实测回写——代码与夹具已就位，入口在案（Wave6 收口条目，§11 2026-11-04（五））。

## 3. 执行纪律（每刀必守）

1. **每刀独立 PR**：W8-a（P2/P3/接线可按体量再拆 2–3 个 PR）→ W8-b；刀内改动一次 PR 收口。
2. **每刀落地立即 push**——W8-a 首版代码因沙箱重置 + 未 push 全失过一次，教训在案。
3. **本地验证清单**（push 前）：
   - `node scripts/check-managed-stubs.mjs --check-static-drivers`（双腿）
   - `corepack pnpm vitest run scripts/__tests__/check-static-drivers.test.ts`
   - `cargo check -p datazen-server`（缺省）+ `--features deploy-drivers`（聚合，改动驱动面时）
   - server 集成测试全量（`cargo test -p datazen-server`，本地可跑；desktop 的 GTK 面照例交 CI）
   - 前端测试：`corepack pnpm vitest run`（根配置）；drivers 前端测试必须 `--config vitest.drivers.config.ts`
4. **CI 4/4 绿才合并**（frontend / rust / desktop / ci 聚合闸）；合并方式沿既有惯例（merge commit）。
5. **台账**：每刀完成后在 `web-ready-plan.md` §11 顶部加条目（倒序），历史条目路径不回改。

## 4. 环境须知（本沙箱硬约束）

### 4.1 网络（用户规则：tuna 直连不挂代理；git 必须走代理）

- **cargo**：`~/.cargo/config.toml`（或 `$CARGO_HOME/config.toml`）已配 TUNA sparse 镜像（`sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/`）——**不要给它挂代理**。
- **pnpm**：`corepack pnpm install --registry=https://registry.npmmirror.com`（npmmirror 对 optional 平台包更全；tuna npm 镜像对 `@esbuild/*` 等会 404）。
- **xlsx 依赖**（`cdn.sheetjs.com`，非 npm 包）：直连与默认超时都拉不动，需挂代理且实际耗时 >126s：`HTTPS_PROXY=http://176.111.37.216:39811 corepack pnpm install --frozen-lockfile --prefer-offline --fetch-timeout 300000`（其余包已缓存时唯一网络流量就是它）。
- **git**：全局配置必须带（沙箱重置会丢）：
  ```bash
  git config --global http.version HTTP/1.1
  git config --global http.postBuffer 524288000
  git config --global http.proxy http://176.111.37.216:39811
  git config --global https.proxy http://176.111.37.216:39811
  ```
  远程 URL 需 PAT 内嵌（既有配置）；PAT 见交接沟通渠道，不写入文档。
- **GitHub API**：`curl -H "Authorization: Bearer $PAT" https://api.github.com/repos/flyxl/datazen/...`。

### 4.2 Rust 工具链（沙箱重置会清空，重建流程）

`/home/z/my-project/{cargo,rustup}-home` 会被重置清空。重建：

```bash
export PATH="/home/z/my-project/cargo-home/bin:$PATH" RUSTUP_HOME=/home/z/my-project/rustup-home CARGO_HOME=/home/z/my-project/cargo-home
# rustup 经 TUNA 镜像安装 stable（不走代理）
export RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup
export RUSTUP_UPDATE_ROOT=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup
```

### 4.3 冷构建纪律（磁盘防满）

```bash
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo check -j 3
```

- 冷构建全程约 25 分钟（多轮续传：被工具超时掐掉后重跑同一命令即可，cargo 缓存保留进度）。
- **后台进程不可用**：`nohup`/`setsid` 都会被会话清理——一律前台跑，单轮 ≤580s。
- 磁盘清理工具：`python3 /home/z/my-project/scripts/prune-stale-target.py`（清 target 陈旧产物）。

### 4.4 git 提交

`.husky/pre-commit` 需要 cargo 在 PATH——**commit 前必须带 4.2 的环境三要素**，否则钩子炸。

## 5. 接手第一步（建议顺序）

1. `git fetch origin && git checkout w8-a`（P1 代码在此）。
2. 本地跑一遍 §3 验证清单（快验证项），确认 P1 状态。
3. 开 PR（W8-a P1），等 CI 4/4——CI 的 rust job 会同时验证聚合形态所需的全部驱动 crate 编译。
4. 合并后从 main 拉新分支做 P2（§2.1 设计已裁决，含桌面参考实现的文件行号）。
5. 每完成一段，回写 §11 台账。
