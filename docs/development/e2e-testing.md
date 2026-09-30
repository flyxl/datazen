# DataZen E2E 测试指南（WebdriverIO）

> 面向 AI Agent / 开发者：如何**正确**编译并跑通 WebDriver E2E。  
> 简要入口见 [AGENTS.md](../../AGENTS.md)「E2E 测试」小节。

## 1. 硬性要求（必读）

| 规则 | 说明 |
|------|------|
| **必须用 Tauri CLI 构建** | 经 `e2e/run.mjs` → `scripts/e2e-tauri-build.mjs`（`--debug` + `webdriver` + `.driver-features.json` 驱动 feature） |
| **禁止裸 `cargo build`** | `cargo build -p datazen --features webdriver` 常导致运行时报 `asset not found: index.html`（未走 `beforeBuildCommand` / 资源嵌入流程） |
| **必须开 `webdriver` feature** | 否则 4445 端口不会监听，WDIO 连不上 |
| **必须启用驱动 Cargo feature** | 仅 `--features webdriver` 不会链接 path 驱动；inventory 注册依赖 `-f driver-postgres,...`（由 `.driver-features.json` 提供） |
| **前端产物 `dist/`** | 由 `pnpm build`（Tauri `beforeBuildCommand`）生成；`frontendDist` 为 `../dist` |

正确构建链路与快捷命令：

```bash
# 一键编译带全部活跃驱动的 WebDriver 版应用
pnpm tauri:build:webdriver

# 或编译 minimal 驱动集合
pnpm tauri:build:webdriver:minimal
```

其背后的执行链路：
```
node scripts/with-driver-inject.mjs [--drivers=basic] -- node scripts/e2e-tauri-build.mjs
  → resolve-drivers → .driver-features.json
  → pnpm tauri build --debug -f webdriver,driver-postgres,...
  → beforeBuildCommand: pnpm build   # 生成 dist/index.html 等
  → cargo 嵌入 dist + WebDriver + 驱动
  → （macOS）产出 target/debug/bundle/macos/DataZen.app
```

入口脚本：`e2e/run.mjs`（`pnpm e2e` 会调用它）。

## 1.1 Host UI / 路径覆盖规则（硬性）

在 Host 范围内（驱动专属 E2E 仍见「插件自有测试」与 [AGENTS.md](../../AGENTS.md)「驱动测试落点」）：

| 规则 | 要求 |
|------|------|
| **UI 交互全覆盖** | 所有用户可操作的 Host UI 控件/对话框，须有 `e2e/specs/` 走到该交互，并断言可见结果；仅「文案出现」不算覆盖。 |
| **用户路径全覆盖** | 所有用户可走到的交互路径（入口 → 操作 → 结果/错误态）须有 E2E；含关键空态/失败态（例：未完成筛选不得出现「加载表数据失败」）。 |
| **同 PR 更新** | 新增或变更 Host UI / 用户路径时，必须同 PR 补齐或更新 E2E。 |
| **驱动边界** | 驱动方言 / 专属 Command / 专属 UI → `packages/drivers/<id>/e2e/`（或插件仓），不进 Host `e2e/specs/`。 |
| **例外登记** | 自动化无法稳定覆盖的路径，必须在 [e2e-coverage.md](./e2e-coverage.md) 登记原因、替代测试与手工项。 |

覆盖矩阵与缺口跟踪：[docs/e2e-coverage.md](./e2e-coverage.md)。

## 2. 一键跑通（推荐）

```bash
# 首次 / 改过 Rust 或前端后：完整构建 + 跑全部 E2E
pnpm e2e

# 更快构建：仅 basic 四核心驱动（跳过 Git / 其余 path 驱动）（仅内置驱动；多数 UI/路径 IPC spec 足够）
pnpm e2e:minimal
# 等价：DATAZEN_DRIVERS=basic pnpm e2e

# 已有正确的 webdriver debug 构建后：跳过构建，只跑 WDIO
pnpm e2e:skip-build

# 只跑某个 spec
pnpm e2e:skip-build -- --spec e2e/specs/path-ipc-hardening.ts

# 分组快捷方式（均默认 --skip-build，前提是已做过 webdriver 构建）
# 分组清单统一定义为 WDIO suites（e2e/wdio.conf.ts 的 suites 字段），单一事实来源；
# 临时跑某个分组也可直接：pnpm e2e:skip-build -- --suite <name>
pnpm e2e:core
pnpm e2e:db
pnpm e2e:ai
pnpm e2e:redis          # 显式：packages/drivers/redis/e2e/（不进默认 pnpm e2e）
pnpm e2e:i18n-backup
pnpm e2e:path-ipc
pnpm e2e:dashboard      # data-dashboard*.ts（同样 skip-build）
pnpm e2e:data-transfer  # 数据传输专用：preflight 清理 + 全量 transfer suite（25000 行宽类型）
pnpm e2e:data-transfer:build  # 同上，但会先完整 webdriver 构建
pnpm e2e:schema-diff  # 结构对比专用：preflight 清理 + 全量 schema-diff suite（19 列宽类型 + 跨方言）
pnpm e2e:schema-diff:build  # 同上，但会先完整 webdriver 构建
pnpm e2e:journeys  # 跨模块连续用户旅程（Welcome/Query/Navigator/Chart/迁移）+ 截图留痕
pnpm e2e:journeys:edge  # 首次安装、连接和查询的异常恢复/状态边界 journeys
pnpm e2e:connection:edge  # 连接校验、生命周期和重试等模块级边界测试
pnpm e2e:contract:matrix          # Host UI/IPC × PG/MySQL/SQLite 连接窗
pnpm e2e:contract:pg              # 仅 PostgreSQL 契约冒烟
pnpm test:unit:e2e-contract:coverage  # 契约纯逻辑单测 ≥80%
# Kiwi：在 datazen-driver-kiwi 仓 `pnpm e2e:kiwi`（Host 同名脚本会 exit 1）
```

> 脚本化封装：受限运行环境（应用数据目录写入受限、需 HOME 沙箱）可用
> [`scripts/run-regression.sh`](../../scripts/run-regression.sh)（全量回归门禁）与
> [`scripts/run-e2e-minimal.sh`](../../scripts/run-e2e-minimal.sh)（minimal 集 + `E2E_ENV_FILE`/主检出 `.env` 回退解析）。

## 2.1 CI 并行策略

为缩短 CI 墙钟时间，可将 E2E 拆为 3 个独立 job 并行运行：

| Job | Suite | 预计耗时 | 依赖 |
|-----|-------|----------|------|
| core | `--suite core` | ~10 min | 无外部 DB |
| db | `--suite db` | ~25 min | PostgreSQL + MySQL |
| features | `--suite ai --suite i18n-backup --suite path-ipc --suite dashboard --suite journeys --suite journey-edge --suite connection-edge` | ~20 min | 按需 |

每个 job 使用独立端口和隔离的 app-data 目录：

```bash
# Job 1: Core (无 DB)
E2E_WD_PORT=4445 pnpm e2e:ci:core

# Job 2: DB specs
E2E_WD_PORT=4446 pnpm e2e:ci:db

# Job 3: Feature specs
E2E_WD_PORT=4447 pnpm e2e:ci:features
```

`e2e/run.mjs` 会将 `E2E_WD_PORT`（或 `--port <number>`）同步到 Tauri WebDriver 插件（`TAURI_WEBDRIVER_PORT`）与 WDIO（`e2e/wdio.conf.ts`）。默认端口仍为 4445，不影响 `pnpm e2e` 现有行为。

注意：需要为每个 job 构建独立的 webdriver binary，或共享同一个 debug build。

### 单机多实例并行（同一机器）

在同一台机器上启动多个 DataZen 实例，WDIO 通过多 capability 并行分配 spec：

```bash
# 3 个 app 实例（端口 4445–4447），跳过构建
pnpm e2e:parallel

# 含完整构建
pnpm e2e:parallel:build

# 自定义实例数 / 起始端口 / suite
node e2e/run.mjs --skip-build --instances 3 --port 4445 -- --suite smoke
```

`--instances N`（默认 1，向后兼容）行为：

| N | app 进程 | app-data 目录 | WebDriver 端口 |
|---|----------|---------------|----------------|
| 1 | 1 | `e2e/.app-data` | `E2E_WD_PORT` 或 4445 |
| N>1 | N | `e2e/.app-data-0` … `e2e/.app-data-(N-1)` | `port`, `port+1`, … |

runner 向 WDIO 注入 `E2E_INSTANCES`、`E2E_WD_PORTS`、`E2E_DATA_DIRS`；`e2e/wdio.conf.ts` 为每个端口创建 capability，各 worker 在 `before` 中独立 seed 连接与语言设置。

**限制**：多实例共享同一 PostgreSQL/MySQL 测试库，DB 写入类 spec 可能偶发冲突；优先用于 core/smoke 等读多写少 suite。全量并行前建议先跑 `pnpm e2e:parallel -- --suite core` 验证稳定性。

## 插件自有测试（Host 默认不拉）

**驱动相关 E2E / 单测写在对应驱动 crate，不要往 `e2e/specs/` 加驱动方言或专属 Command 用例。** 见 [AGENTS.md](../../AGENTS.md)「驱动测试落点」。

| 类型 | 命令 / 位置 |
|------|-------------|
| Path 驱动 UI 单测 | `pnpm test:unit:drivers`（**不是** `pnpm test:unit`）→ `packages/drivers/<id>/ui/__tests__/` |
| Path 驱动 Rust | `cargo test -p datazen-driver-<id>`（**不是** `-p datazen`） |
| Redis E2E | `pnpm e2e:redis` → `packages/drivers/redis/e2e/` |
| Kiwi E2E | 在 `datazen-driver-kiwi` 执行 `pnpm e2e:kiwi`（定位 Host 后跑本仓 spec） |

**Agent 推荐流程：**

1. 若不确定本地二进制是否合格 → 直接 `pnpm e2e -- --spec <spec>`（**不要**加 `--skip-build`），或先执行构建命令再 skip-build。  
2. 仅当本会话刚成功跑过 `e2e-tauri-build.mjs`（含驱动 feature）时，才用 `pnpm e2e:skip-build`。

## 3. 手工分步（调试用）

```bash
# 1) 构建（唯一合法的 E2E 二进制来源；含驱动 feature）
node scripts/generate-menu-labels.mjs && node scripts/with-driver-inject.mjs --drivers=basic -- node scripts/e2e-tauri-build.mjs

# 2) 确认产物
ls dist/index.html
ls target/debug/datazen
# macOS 另有：
ls target/debug/bundle/macos/DataZen.app/Contents/MacOS/datazen

# 3) 跑指定用例
pnpm e2e:skip-build -- --spec e2e/specs/main-window.ts
```

`e2e/run.mjs` 在 macOS 上会优先选用 **较新的** `.app` 包内二进制，避免被裸 `cargo build` 覆盖的 `target/debug/datazen` 抢走启动权。

## 4. 环境变量

复制并填写：

```bash
cp e2e/.env.example e2e/.env
# Creates datazen_e2e + product seed, sync DBs, and RO users (idempotent)
bash e2e/setup-e2e-env.sh
```

`e2e/run.mjs` 在启动 WDIO 前也会调用 `setup-e2e-env.sh`；失败只警告，不中止整套 UI spec。
跑完后会自动调用 `teardown-e2e-env.sh` 清理测试库中的临时表并重置 `product` seed。

### 测试数据生命周期

| 阶段 | 动作 | 位置 |
|------|------|------|
| **跑前** | 创建/重置 PG `datazen_e2e`、MySQL `datazen_test`、sync 库、`product` seed | `e2e/setup-e2e-env.sh`（`run.mjs` 自动调用） |
| **跑前** | 清空 `e2e/.app-data/`（隔离的应用数据目录） | `e2e/run.mjs`（`--keep-app-data` 保留） |
| **WDIO before** | upsert 默认 PG 连接 `conn_e2e_pg` | `e2e/wdio.conf.ts` → `seedDefaultPgConnection` |
| **WDIO onComplete** | 删除 `e2e-*` / `E2E-*` 连接、`e2e-*` workflow、清空 query history | `e2e/lib/testDataLifecycle.ts` |
| **跑后** | DROP 名称含 `e2e` / 前缀 `sync_` / 遗留 `ds_j_` / `ds_edge_` 的表，重 seed `product` | `e2e/teardown-e2e-env.sh`（`run.mjs` 自动调用） |

跳过 DB teardown：`E2E_SKIP_TEARDOWN=1 pnpm e2e:skip-build`。各 spec 内的 `after` hook 仍应清理本用例创建的连接/表（双保险）。

#### Data Sync 夹具约定

| 层级 | 数据量 / 类型 | 清理 |
|------|--------------|------|
| **UI Journey**（`journeys/data-sync-journey.ts`） | 单表 5+3 行；`int` PK + `text`/`varchar`；INSERT+UPDATE+DELETE（Execute 确认） | 表名 `e2e_ds_j_*`；`after` + teardown |
| **Edge**（`data-sync-edge-cases.ts`） | 同上 | 表名 `e2e_ds_edge_*` |
| **IPC**（`data-sync-real.ts`） | 多表；含 `sync_pg_types` 宽类型（numeric/bool/timestamptz/uuid 等） | 表前缀 `sync_*`；spec `after` |

Journey 用小数据集保证 UI 路径稳定；类型与 apply 闭环见 `SYNC-REAL-*`（`SYNC-REAL-024` 覆盖 numeric/bool/double/uuid/timestamptz 的 PG apply）。修改前端/Rust 后应用 `pnpm e2e:minimal` 重建 webdriver binary，避免 `e2e:skip-build` 跑旧嵌入资源。

### 应用数据隔离（DATAZEN_DATA_DIR）

`e2e/run.mjs` 启动 webdriver 应用时会注入 `DATAZEN_DATA_DIR=<repo>/e2e/.app-data`（gitignored），
宿主 `Store::default_app_data_dir()` / `Store::init()` 优先读取该变量。**没有它，E2E 会直接读写
真实生产数据**（`~/Library/Application Support/com.tbeasy.datazen`），清连接类 spec 会删掉真实
连接列表。注意：

- 该隔离只对**包含此支持的二进制**生效（2026-08 之后的构建）；旧二进制忽略该变量
- spec 层仍应遵循 zz-screenshots 的「备份 → 清理 → 恢复」模式作为双保险
- 手工启动 webdriver 二进制调试时，如需隔离请自行 `DATAZEN_DATA_DIR=...` 前缀

| 变量前缀 | 用途 |
|----------|------|
| `DATAZEN_DATA_DIR` | 应用数据目录覆盖（E2E 隔离用；未设置时行为不变） |
| `E2E_PG_*` / `PG_*` | PostgreSQL。`E2E_PG_DB` 默认 `datazen_e2e`（由 setup 创建并锁定到 Host 连接） |
| `E2E_MYSQL_*` | MySQL |
| `E2E_REDIS_*` | Redis Standalone（`redis.ts`）：`HOST` / `PORT` / `PASSWORD` |
| `E2E_REDIS_CLUSTER_*` | 可选 Cluster（`redis-topology.ts`）：`CLUSTER_NODES`、`CLUSTER_PASSWORD`；未设置则跳过 |
| `E2E_REDIS_SENTINEL_*` | 可选 Sentinel（`redis-topology.ts`）：`SENTINEL_NODES`、`SENTINEL_MASTER_NAME`、密码等；未设置则跳过 |
| `E2E_KIWI_*` | Kiwi 插件 E2E（在 kiwi 仓 `pnpm e2e:kiwi`；可写 kiwi `e2e/.env`） |
| `E2E_AI_*` | AI 功能 E2E |
| `DATAZEN_DRIVERS=basic` | E2E 构建时仅 basic 四核心驱动（跳过 Git / 其余 path 驱动）（见 `pnpm e2e:minimal`） |
| `DATAZEN_E2E_QUIET` | 静默模式：macOS 上应用不激活、窗口透明，跑 E2E 时不抢本机键盘焦点。`e2e/run.mjs` 默认注入 `1`，截图/录屏链路除外（见下节；仅 `webdriver` 构建生效） |
| `DATAZEN_E2E_QUIET_CONCEAL` | 设为 `0` 时保留可见窗口，只拦焦点抢占（见下节） |
| `DATAZEN_KEYRING` | 主密钥后端（`file` / `keyring`）。`e2e/run.mjs` 默认注入 `file`，见下方「主密钥与系统钥匙串」 |

#### 主密钥与系统钥匙串

主密钥 `master-key` 在 macOS 正式签名版里存登录钥匙串，开发/adhoc 构建则回落到
`security` CLI。E2E 每次都用全新的 `DATAZEN_DATA_DIR`，本地没有 `.key` 兜底，因此
每次启动都必然走一次 `security add-generic-password`。一旦钥匙串搜索列表异常
（`security list-keychains` 报 `Module Directory Service error has occurred`），macOS 会弹出
「找不到用于储存 master-key 的钥匙串 login / 还原为默认」模态框，**阻塞** `security` 子进程
直到人工点掉，E2E 会整个挂死。

`e2e/run.mjs` 因此默认给应用注入 `DATAZEN_KEYRING=file`，主密钥改落该次 data dir 的 `.key`；
E2E 本就不该读写开发者真实钥匙串。需要显式覆盖时设 `DATAZEN_KEYRING=keyring` 退回原行为。

无数据库时，仅 UI/设置类 spec（如 `settings.ts`、`i18n-*`、部分 `path-ipc-hardening`）仍可能通过；依赖真实连接的 suite 会失败。需要 Kiwi / OLAP 等插件驱动的 spec 必须用默认 `pnpm e2e`（全部插件）构建。

### 静默模式（`DATAZEN_E2E_QUIET`）

macOS 上 DataZen 会把自己顶成前台应用，跑 E2E 的人的编辑器就被抢走焦点。静默模式让
整套 E2E 安静地跑，并且**默认开启**——`e2e/run.mjs` 启动应用时会注入
`DATAZEN_E2E_QUIET=1`，不需要手动设置：

```bash
pnpm e2e:minimal                              # 静默跑（默认）
pnpm e2e:skip-build                           # 已有 webdriver 构建时静默跑
DATAZEN_E2E_QUIET=0 pnpm e2e:skip-build       # 关掉：窗口照旧可见、可抢焦点
DATAZEN_E2E_QUIET_CONCEAL=0 pnpm e2e:skip-build  # 窗口可见，但不抢焦点
```

- 只在 `webdriver` 构建里生效，实现在 `src-tauri/src/e2e_quiet.rs`；不带该 feature 的构建里
  `enabled()` 恒为 `false`，shim 代码根本不会被编译进二进制。
- 进程共有四处会把前台抢走，后两处由 E2E 打开子窗口时触发（`commands/window.rs` 里有
  8 处 `set_focus()`）：

  | #   | 触发点                                                                                                                  | 靠什么拦                                          |
  | --- | ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
  | 1   | tao `applicationDidFinishLaunching`（`AppState::launched`）里的 `window_activation_hack` + `activateIgnoringOtherApps:` | 换选择器                                          |
  | 2   | wry 每建一个 webview 时的 `-[NSApplication activate]`（macOS 14+）                                                      | 换选择器                                          |
  | 3   | 窗口 `show()` 走 `orderFront:`                                                                                          | 换选择器 + conceal                                |
  | 4   | 窗口 `set_focus()` 走 `makeKeyAndOrderFront:`，AppKit 在窗口 becomeKey 时走**内部**激活路径                             | conceal（只 `orderFrontRegardless:`，不 makeKey） |

  前两处发生在**第一个窗口之前**——Tauri 的 setup 钩子比它们晚，所以拦截必须放在
  `bootstrap/run.rs` 构造 `tauri::Builder` **之前**，否则后面怎么调都没用。

- 生效方式（`e2e_quiet::install()`，两件事必须一起做）：
  1. 用 objc2 把本进程 `NSApplication` **基类**上的 `activate` / `activateIgnoringOtherApps:`
     替换成空实现，并把激活策略降级为 `NSApplicationActivationPolicy::Accessory`（没有
     Dock 图标、不进 Cmd-Tab）。此刻 tao 的 `TaoApp` 子类还没建出来（`object_setClass`
     在事件循环里），而它并不自己实现这两个选择器，消息仍会落到基类。ObjC 方法替换是
     进程内的，系统里其它应用不受影响。
  2. 把 `NSWindow` 的 `orderFront:` / `makeKeyAndOrderFront:` 换成「透明窗口 + 只
     `orderFrontRegardless:`」：`alphaValue = 0`、`ignoresMouseEvents = YES`、窗口级别
     `NSFloatingWindowLevel`、`collectionBehavior` 加入所有 Space 且不打断全屏。
     （`makeOrderedFront:` 不必管——macOS 15 上 `NSWindow` 并没有这个方法，运行时
     `instance_method` 取不到。）
- **第 2 条才是根治点。** 只做第 1 条时启动确实安静了，但 E2E 一打开子窗口，第 4 条路径
  又把前台抢走。原因是「窗口 becomeKey → AppKit 内部激活应用」这条路径既不经过
  `activate` 也不经过 `activateIgnoringOtherApps:`，换选择器拦不到；窗口不做 key 就没有
  触发条件，进程也就永远拿不到前台。
- 同一台机器、同一组 spec（`data-transfer-window.ts` + `settings.ts`，约 90 秒），开着
  显示器断言、期间人一直在 Code / ChatGPT 之间切应用，采样 `lsappinfo front`：

  | `DATAZEN_E2E_QUIET_CONCEAL` | 用例       | DataZen 抢到前台的次数                                                  |
  | --------------------------- | ---------- | ----------------------------------------------------------------------- |
  | `1`（默认）                 | 29/29 通过 | **0**                                                                   |
  | `0`                         | 3 例失败   | **8**（t+1.9s / 13.4s / 47.3s / 59.0s / 62.4s / 65.1s / 73.1s / 90.8s） |

  开着 conceal 时应用日志里有 68~90 条 `key request declined`（窗口号 + 选择器），说明
  `set_focus()` 确实一路走到 `makeKeyAndOrderFront:`、确实被这条 shim 拦下，不是「本来
  就没触发」。

- 换成 `orderFrontRegardless:` 而不是完全不排序，是为了让合成器继续为窗口取帧——
  `WKWebView` 的 `takeSnapshot` 才有内容。窗口停在 `NSFloatingWindowLevel`（普通窗口
  之上、全屏之下）让「透明但仍在渲染」成立；alpha 0 保证用户看不见，浮在最上层保证它
  不会因为被别的窗口盖住而不出画。
- 静默模式下 WebView 里 `document.visibilityState === "visible"`、
  `document.hasFocus() === false`。WebDriver 的 `click` / `keys` 都是 JS 合成
  （`el.click()`、native setter + `InputEvent`），`saveScreenshot` 走 webview 快照，
  都不依赖窗口可见或进程激活，因此点击、输入、视口断言、截图均不受影响。
- 截图 / 录屏链路（`--capture`、`--screenshot`、`e2e:shots`、`e2e:demo`）需要窗口处在
  系统正常的前台与合成状态，`e2e/run.mjs` 对这些运行默认**不**注入静默变量（判定看的是
  命令行参数，`e2e:demo` 走的 `--spec demo-recording.ts` 同样能识别）。需要时用
  `DATAZEN_E2E_QUIET=1` 显式覆盖。
- 排查时可以盯着应用日志里的 `key request declined`：只要它还在涨而前台没被抢走，就说明
  拦截生效；前台仍被抢走则说明还有第五条激活路径，需要拿它的时刻去对齐
  `lsappinfo front` 的时间轴。
- 跑 E2E 时建议 `caffeinate -dimsu`：屏幕熄灭或自动锁定时 WebKit 会节流 webview，
  `data-transfer-window.ts` 里的「数据传输真实迁移」用例会成片地以
  `operation was aborted due to timeout` / `element still not displayed after 15000ms`
  失败。这与静默模式无关，两种配置都会中。

### Journey 截图留痕（`--screenshot`）

为调试或文档留痕，可在 E2E 运行时保存 **UI 状态发生变化** 的全屏截图：

```bash
pnpm e2e:skip-build -- --screenshot --spec e2e/specs/main-window.ts
```

- `e2e/run.mjs` 设置 `E2E_SCREENSHOT=1` 并 `mkdir -p e2e/screenshots/`（目录 gitignored，不存在时会自动创建）
- **仅 spec 内**在断言通过、UI 达到目标状态后调用 `captureJourneyStep(label, 0, true)`；helpers **不再**自动截图（避免 connect / openTab 链产生重复帧）
- 同一 spec 文件内跨 `it()` 的 **相同 PNG** 会自动去重（`fail` 帧除外）；同一 `it()` 内连续相同帧也会丢弃；被去重时 **不会** 留下空目录
- 输出：`e2e/screenshots/<spec>/<test-title>/01_<label>.png` …

纯 IPC 用例（无 UI 操作）通常 **零截图**，属预期行为。营销固定场景仍见 `e2e/specs/zz-screenshots.ts`。

## 5. 架构说明

```
e2e/run.mjs
  ├─ (可选) with-driver-inject → e2e-tauri-build.mjs  # webdriver + 驱动 features
  ├─ 启动 target/debug/.../datazen   # 插件监听 127.0.0.1:4445
  └─ npx wdio run e2e/wdio.conf.ts [--spec ...]

e2e/wdio.conf.ts
  ├─ hostname/port: 127.0.0.1:4445
  ├─ before: 强制 language=zh-CN，必要时 seed PostgreSQL 连接
  ├─ specs: e2e/specs/**/*.ts
  └─ suites: 分组清单（core/db/contract/redis/ai/i18n-backup/path-ipc/dashboard/data-transfer/journeys/journey-edge/connection-edge），供 --suite 选择
```

- Spec 写法：通过 `browser.executeAsync` + `__TAURI_INTERNALS__.invoke` 调后端；UI 用 WebdriverIO `$` / `expect`。  
- 纯文件读写 IPC（`write_file` / `write_file_base64` / `read_file`）已删除（IPC 重构决策 4）：E2E 的 fixture 准备直接用 Node.js `fs`（E2E 进程本身即 Node），不再经后端写读文件。  
- 路径类 IPC：生产构建一律走对话框系 `*_with_dialog`；webdriver 构建仅保留少量受 `require_webdriver_path_ipc` 门控的直连变体（连接/app-data 导入导出、`backup_database` / `restore_database` / `execute_sql_file`）供 E2E 驱动真实落盘链路，后续按决策 3 以 `override_path` 参数收敛。  
- 原生系统对话框（另存为）在自动化里难以点选 → E2E 用上述门控路径 IPC 或 mock `invoke`；fixture 文件一律 Node fs。

## 6. Spec 索引（节选）

| 领域 | Spec |
|------|------|
| 核心 UI | `main-window.ts`, `homepage-features.ts`, `settings.ts`, `i18n-menu.ts` |
| 连接 | `new-connection.ts`, `edit-delete-connection.ts`, `connection-window.ts` |
| SQL / 表 | `sql-query.ts`, `table-data.ts`, `table-filter.ts`, `table-indexes.ts`, `table-edit.ts`, `export-import.ts`, `object-browser.ts` |
| 连续用户旅程 | `journeys/README.md`；首次安装、连接创建/浏览/查询、异常恢复、图表，以及迁移类 journeys |
| 连接模块边界 | `connection-validation.ts`, `connection-edge-cases.ts`, `connection-navigator-expansion.ts` |
| 路径 IPC / 备份 | `path-ipc-hardening.ts`, `app-data-backup.ts`, `backup-database.ts`, `backup-window.ts`, `schema-diff-window.ts` |
| i18n | `i18n-10-locales.ts`, `system-locale.ts`, `i18n-menu.ts` |
| AI / Workflow | `ai-features.ts`, `ai-context.ts`, `workflow.ts`, `workflow-window.ts`, `driver-commands.ts` |
| 驱动（Host） | `sqlite.ts`, `mysql.ts`（及其他 SQL Host specs） |
| Redis E2E（插件包，非默认） | `packages/drivers/redis/e2e/redis.ts`, `redis-topology.ts` — `pnpm e2e:redis` |
| Kiwi E2E（插件仓，非默认） | `datazen-driver-kiwi`：`pnpm e2e:kiwi` |

覆盖矩阵（UI 交互 / 用户路径）：[e2e-coverage.md](./e2e-coverage.md)。

完整列表与分层测试见 [architecture/testing.md](../architecture/testing.md)。

## 7. 常见错误与修复

### `asset not found: index.html`

| 原因 | 修复 |
|------|------|
| 用了 `cargo build --features webdriver` | 改用 `pnpm tauri build --debug --features webdriver` |
| `dist/` 在编译时不存在 / 过期 | 同上（会跑 `pnpm build`） |
| `--skip-build` 用了被 cargo 覆盖的旧二进制 | 重新 Tauri 构建，再 skip-build |

### Port `4445` not ready

| 原因 | 修复 |
|------|------|
| 未启用 `webdriver` feature | 构建时加 `--features webdriver` |
| 端口被占用 | `lsof -i :4445` 后杀掉旧 DataZen / e2e 进程 |
| 启动即崩溃（资源缺失） | 见上一节 |

### `invoke` spy / mock 失败（`unconfigurable property`）

Tauri 2 会冻结 `__TAURI_INTERNALS__.invoke`，**无法**在浏览器里 `defineProperty` 替换。E2E 应：

- 用源码断言验证 UI 调用了哪个 command wrapper
- 用 `invokeBackend('open_log_dir')` 等直接测 IPC
- 不要依赖 mock `invoke` 的点击劫持


### `invoke` 参数命名

Tauri 2 前端传参为 **camelCase**（如 `defaultFileName`），不要用 snake_case。

## 8. Agent 检查清单

在声称「E2E 已跑通」之前：

- [ ] 使用的是 `pnpm e2e` 或先 `pnpm tauri build --debug --features webdriver`
- [ ] **没有**仅用 `cargo build` / `cargo test` 冒充 E2E 二进制
- [ ] `dist/index.html` 存在
- [ ] 启动日志无 `asset not found: index.html`
- [ ] 4445 端口就绪
- [ ] WDIO 退出码为 0（或如实报告失败用例）
- [ ] 本次改动的 Host UI / 用户路径已有对应 `e2e/specs/`（见 §1.1 与 [e2e-coverage.md](./e2e-coverage.md)）
- [ ] 驱动专属路径未误写入 Host `e2e/specs/`

## 9. 相关文件

| 路径 | 作用 |
|------|------|
| `e2e/run.mjs` | 构建 / 启动 / WDIO 编排 |
| `e2e/wdio.conf.ts` | WDIO 配置与全局 before hook |
| `e2e/helpers.ts` | 公共 UI/窗口助手 |
| `e2e/specs/` | 用例 |
| `e2e/.env.example` | 环境变量模板 |
| `e2e/setup-e2e-env.sh` | 创建 E2E 库、seed `product`、RO 用户 |
| `e2e/teardown-e2e-env.sh` | 跑后清理临时表、重置 seed |
| `e2e/lib/testDataLifecycle.ts` | WDIO 全局 IPC setup/teardown |
| `src-tauri/Cargo.toml` → `webdriver` feature | 启用 `tauri-plugin-webdriver` |
