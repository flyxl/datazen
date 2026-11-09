# W8 交接文档：点亮路径两刀（server 执行面绿路径）

> **交接日期**：2026-11-05（第三版刷新）。**交接范围**：Wave8-a 余项（十一端点接线 + seed-example）与 Wave8-b（点亮下半）。
> **当前基线**：main `412b494`（PR #178 合并后）；**W8-a P1（#176）/ P2（#177）/ P3（#178）已全部合并**。
> **权威台账**：[web-ready-plan.md §11](web-ready-plan.md#11-进度回写区)（2026-11-05 Wave8-a P1/P2/P3 三条目含全部设计裁决与实现事实）。
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

### 1.3 W8-a P2 已完成（PR #177 合并，`fb0bb74`）

`crates/datazen-server/src/execution/target_runner.rs`：`ServerTargetRunner { host, lease }` 实现端口 `TargetCommandRunner`（桌面 `DesktopTargetRunner` 的 server 同构投影）。target 分支 = `host.prepare`（WR-F7 闸序）→ `ExecuteAtTarget`（规范化目标 + CAS）→ 25ms 轮询 → Succeeded → `PreparedExecution.artifacts` 端口分块收集；manual_handle 分支 = 租约每调用现取 → `get_session` 三字段校验 → `ExecuteInSession`（context_revision CAS）→ 经 `resource_host::ServerArtifactChunks`（pub(crate)）投影共享收集。授权语：执行面全用 `run()` 真主体。装配：`TableDataState` +registry +runner（与 driverGateway/MCP 同实例）；datazen features 增 `table-browsing`。全量测试绿（lib 193 → P3 后 194）。

### 1.4 W8-a P3 已完成（PR #178 合并，`412b494`）

授权种子（server 局部刀）：`stateless_exec::PrincipalScopedHost`（会话工具 `PerCallHost` 的无状态镜像）——四方法委托共享宿主，`prepare` 返回后覆写 `PreparedExecution.context` 为请求真主体；`ModulesBackend` 无状态分支 `stateless.call(request_context(identity), ...)`。**七工具执行面全链真主体化**；残留（闸序读半边合成主体、driverGateway execute 路径）登记在 `resource_host.rs` 第 2 条（已收窄版）——读半边组织口径正确、仅审计行主体合成，端口签名手术留后续刀。新测试 `principal_scoped_host_overrides_the_prepared_context` 钉住传播。

## 2. 剩余工作

### 2.1 W8-a 余项（点亮上半收尾，两刀）

**接线刀：tableData 十一端点 501 → 真用例接线**（`runner`/`registry` 已在 `TableDataState` 就位，handler 逐条换真实现）

- 读三端点（`getTableData`/`previewTableReadSql`/`estimateTableRows`）：`read_table_page`/`preview_table_read` 用例 + `state.registry.get_sql_driver_by_name(database_type)` + `state.runner`；handler 形态照桌面 `table_target.rs::read/preview`（输入校验 → profile/metadata → driver → `validate_filters` → 调用例）；连接上下文的真主体从 `SessionIdentity.ctx` 取（P3 语义延伸到 HTTP 面）。绑定铸造：`TargetBinding { target: ExecutionTarget{connection_id, namespace(表), object}, revision: Counter(profile.config_revision), manual_handle: None }`（v1 HTTP 面无手动会话句柄）。
- FK 四端点 + 变更四端点：宿主端口依赖（`ForeignKeyNavigationSource`、`TableColumnSource`、会话执行面）逐条评估——能以既有 server 件组合的接，不能的保持 501 并把不可服务原因收窄为逐端口缺失；每接一条删一条 501，`table_data.rs` 模块头 14 方法状态表同步。
- 测试：`table_data_routes.rs` 夹具把占位 `FixtureRunner` 换成真 `ServerTargetRunner` + fake-driver 注册表（`DriverRegistry::with_factories(vec![...])` 静态形与懒形并存）。

**绿路径刀：mcp seed-example + 七工具**：常态 CI 用 fake-driver（桌面 fake-driver 先例），真方言 `#[ignore]`；七工具经 `read_target_command` 闸序（profile → driver → 命令目录 → 执行）不变，P3 后执行面带真主体。

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

1. `git fetch origin && git checkout main`（P1/P2/P3 均已合并，无 WIP 分支遗留）。
2. 本地跑一遍 §3 验证清单（快验证项）确认基线绿：stub 双腿、scripts vitest、`cargo check -p datazen-server`。
3. 开接线刀分支（建议 `w8-a-wire`）：十一端点 501 → 真用例接线（§2.1 第一项；读三端点先行，FK/变更端点按端口依赖逐条评估）。
4. 每完成一段，回写 §11 台账（倒序，历史条目不回改）。
