# DataZen E2E 测试指南（WebdriverIO）

> 面向 AI Agent / 开发者：如何**正确**编译并跑通 WebDriver E2E。  
> 简要入口见 [AGENTS.md](../../AGENTS.md)「E2E 测试」小节。

## 1. 硬性要求（必读）

| 规则 | 说明 |
|------|------|
| **必须用 Tauri CLI 构建** | 经 `e2e/run.mjs` → `scripts/e2e-tauri-build.mjs`（`--debug` + `webdriver` + `.driver-features.json` 驱动 feature） |
| **禁止裸 `cargo build`** | `cargo build -p datazen_desktop --features webdriver` 常导致运行时报 `asset not found: index.html`（未走 `beforeBuildCommand` / 资源嵌入流程） |
| **必须开 `webdriver` feature** | 否则 4445 端口不会监听，WDIO 连不上 |
| **必须启用驱动 Cargo feature** | 仅 `--features webdriver` 不会链接 path 驱动；inventory 注册依赖 `-f driver-postgres,...`（由 `.driver-features.json` 提供） |
| **前端产物 `packages/desktop/dist/`** | 由 `pnpm build`（Tauri `beforeBuildCommand`）生成；`frontendDist` 为 `../../packages/desktop/dist` |

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
  → beforeBuildCommand: pnpm build   # 生成 packages/desktop/dist/index.html 等
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
| **驱动边界** | 驱动方言 / 专属 Command / 专属 UI → `crates/drivers/<id>/e2e/`（或插件仓），不进 Host `e2e/specs/`。 |
| **例外登记** | 自动化无法稳定覆盖的路径，必须在 [e2e-coverage.md](./e2e-coverage.md) 登记原因、替代测试与手工项。 |

覆盖矩阵与缺口跟踪：[docs/e2e-coverage.md](./e2e-coverage.md)。

## 2. 一键跑通（推荐）

### 2.0 全量 E2E 默认并行（分阶段、安全隔离）

```bash
pnpm e2e                    # 完整构建 + 全量 E2E（默认并行，最多 4 实例）
pnpm e2e:skip-build         # 已有 webdriver 构建时：同上，跳过构建
pnpm e2e:serial             # 强制串行隔离全量跑（调试/排查串扰时使用）
pnpm e2e:serial:skip-build  # 同上，跳过构建
pnpm e2e:skip-build -- --spec e2e/specs/x.ts   # 单 spec
pnpm e2e:skip-build -- --suite <name>          # 指定 suite
```

**默认全量行为（不传 `--spec` / `--suite`）**：

- runner 将全量 specs 分为两阶段执行，避免共享夹具冲突：
  1. **并行阶段**：除 data-sync / data-transfer / schema-diff 共享夹具套件外的 spec，按多实例并行执行（`--instances` 默认自动选择，**上限 4**）。
  2. **串行阶段**：`data-sync`、`data-transfer`、`schema-diff` 套件相关 specs 串行隔离执行（每个 spec 独立应用进程 + 共享夹具重置），避免并发操作同一批共享 fixture 库（`datazen_sync_src` / `datazen_sync_tgt` 等）。
- 每个并行 worker 使用独立 `DATAZEN_DATA_DIR`、独立 WebDriver 端口与独立 worker PostgreSQL 数据库，降低跨 worker 干扰。
- 可用 `--instances N` 指定并发度（全量跑会自动 **cap 到 4**）；需要完全串行排查时使用 `pnpm e2e:serial[:skip-build]` 或 `--serial`。

> 说明：`data-sync` / `data-transfer` / `schema-diff` 依赖共享夹具库与特定 setup 脚本，单独运行时仍建议使用对应命令（如 `pnpm e2e:data-sync`、`pnpm e2e:data-transfer`、`pnpm e2e:schema-diff`）。


```bash
# 首次 / 改过 Rust 或前端后：完整构建 + 跑全部 E2E（默认并行，最多 4 实例）
pnpm e2e

# 更快构建：仅 basic 四核心驱动（跳过 Git / 其余 path 驱动）（仅内置驱动；多数 UI/路径 IPC spec 足够）
pnpm e2e:minimal
# 等价：DATAZEN_DRIVERS=basic pnpm e2e

# 已有正确的 webdriver debug 构建后：跳过构建，只跑 WDIO
pnpm e2e:skip-build

# 只跑某个 spec / suite
pnpm e2e:skip-build -- --spec e2e/specs/path-ipc-hardening.ts
pnpm e2e:skip-build -- --suite <name>

# 分组快捷方式（均默认 --skip-build，前提是已做过 webdriver 构建）
# 分组清单统一定义为 WDIO suites（e2e/wdio.conf.ts 的 suites 字段），单一事实来源
pnpm e2e:core
pnpm e2e:db
pnpm e2e:ai
pnpm e2e:redis          # 显式：crates/drivers/redis/e2e/（不进默认 pnpm e2e）
pnpm e2e:schema-tree-objects  # MySQL/PostgreSQL Schema Tree 对象 journey（驱动 crate 内用例）
pnpm e2e:i18n-backup
pnpm e2e:path-ipc
pnpm e2e:dashboard      # data-dashboard*.ts（同样 skip-build）
pnpm e2e:data-transfer  # 数据传输专用：preflight 清理 + 全量 transfer suite
pnpm e2e:data-transfer:build
pnpm e2e:schema-diff    # 结构对比专用：preflight 清理 + 全量 schema-diff suite
pnpm e2e:schema-diff:build
pnpm e2e:journeys
pnpm e2e:journeys:edge
pnpm e2e:connection:edge
pnpm e2e:contract:matrix
pnpm e2e:contract:pg
pnpm test:unit:e2e-contract:coverage
```

> 脚本化封装：受限运行环境（应用数据目录写入受限、需 HOME 沙箱）可用
> [`scripts/run-regression.sh`](../../scripts/run-regression.sh)（全量回归门禁）与
> [`scripts/run-e2e-minimal.sh`](../../scripts/run-e2e-minimal.sh)（minimal 集 + `E2E_ENV_FILE`/主检出 `.env` 回退解析）。

## Schema Tree 手工 fixtures

MySQL 和 PostgreSQL 的长期手测对象与 E2E 临时对象分开保存。安装脚本会读取现有 E2E 环境配置并确保数据库和对象存在；手工测试时连接到以下数据库：

| 驱动 | 数据库 | 对象范围 |
|---|---|---|
| MySQL | `datazen_manual_schema_tree` | 表、外键/索引、视图、函数、存储过程和触发器；[安装器与 SQL](../../crates/drivers/mysql/e2e/install-manual-schema-tree-fixtures.sh)。 |
| PostgreSQL | `datazen_manual_schema_tree`，schema `manual_schema_tree` | 表、外键/索引、视图、函数、存储过程、触发器、序列、enum 和 domain；[安装器与 SQL](../../crates/drivers/postgres/e2e/install-manual-schema-tree.sh)。 |

首次安装或需要重建 fixture 时，在仓库根目录运行：

```bash
bash crates/drivers/mysql/e2e/install-manual-schema-tree-fixtures.sh
bash crates/drivers/postgres/e2e/install-manual-schema-tree.sh
```

`pnpm e2e:schema-tree-objects` 则在每次 journey 中创建带随机后缀的临时数据库/schema，断言对象树、对象定义和数据操作，并在 finally/after 中仅清理本次成功创建的对象；这些临时对象与上面的长期手测 fixtures 相互隔离。

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
E2E_WD_PORT=4445 pnpm e2e:core

# Job 2: DB specs
E2E_WD_PORT=4446 pnpm e2e:db

# Job 3: Feature specs
E2E_WD_PORT=4447 pnpm e2e:skip-build -- --suite ai --suite i18n-backup --suite path-ipc --suite dashboard --suite journeys --suite journey-edge --suite connection-edge
```

`e2e/run.mjs` 会将 `E2E_WD_PORT`（或 `--port <number>`）同步到 Tauri WebDriver 插件（`TAURI_WEBDRIVER_PORT`）与 WDIO（`e2e/wdio.conf.ts`）。默认端口仍为 4445，不影响 `pnpm e2e` 现有行为。

注意：需要为每个 job 构建独立的 webdriver binary，或共享同一个 debug build。

### 单机多实例并行（同一机器）

`e2e/run.mjs` 支持多实例并行：为每个 worker 启动独立 app 进程、WebDriver 端口与 `DATAZEN_DATA_DIR`，并分配不同 spec 子集。

```bash
# 全量（默认并行，自动 cap 4）
pnpm e2e:skip-build

# 指定并发度（全量跑仍会 cap 到 4）
node e2e/run.mjs --skip-build --instances 3

# 指定 suite 并行（按目标场景评估是否适合并行）
node e2e/run.mjs --skip-build --instances 3 -- --suite smoke
node e2e/run.mjs --skip-build --instances 2 -- --suite core
```

`--instances N` 行为（简化）：

| N | app 进程 | app-data 目录 | WebDriver 端口 |
|---|----------|---------------|----------------|
| 1 | 1 | `e2e/.app-data` | `E2E_WD_PORT` 或 4445 |
| N>1 | N | `e2e/.app-data-0` … `e2e/.app-data-(N-1)` | `port`, `port+1`, … |

**并行安全说明**：

- 并行 worker 各自拥有独立 worker PostgreSQL 数据库与 app-data，适合大多数 UI/表格/连接类 spec。
- `data-sync` / `data-transfer` / `schema-diff` 套件依赖共享夹具库（如 `datazen_sync_src` / `datazen_sync_tgt`），**全量跑会自动放入串行阶段**，避免跨 worker 并发冲突；单独跑这些套件时建议按 suite 串行执行（使用对应命令）。
- 并发度过高会导致 CPU/内存争用与 WebDriver/DB 超时；全量跑默认并行并**限制为最多 4**。

## 插件自有测试（Host 默认不拉）

**驱动相关 E2E / 单测写在对应驱动 crate，不要往 `e2e/specs/` 加驱动方言或专属 Command 用例。** 见 [AGENTS.md](../../AGENTS.md)「驱动测试落点」。

| 类型 | 命令 / 位置 |
|------|-------------|
| Path 驱动 UI 单测 | `pnpm test:unit:drivers`（**不是** `pnpm test:unit`）→ `crates/drivers/<id>/ui/__tests__/` |
| Path 驱动 Rust | `cargo test -p datazen-driver-<id>`（**不是** `-p datazen`） |
| Redis E2E | `pnpm e2e:redis` → `crates/drivers/redis/e2e/` |
| Kiwi E2E | 在 `datazen-driver-kiwi` 执行 `pnpm e2e:kiwi`（定位 Host 后跑本仓 spec） |

**Agent 推荐流程：**

1. 若不确定本地二进制是否合格 → 直接 `pnpm e2e -- --spec <spec>`（**不要**加 `--skip-build`），或先执行构建命令再 skip-build。  
2. 仅当本会话刚成功跑过 `e2e-tauri-build.mjs`（含驱动 feature）时，才用 `pnpm e2e:skip-build`。


## 2.3 截图与录屏（输出到 `e2e/screenshots/`，按需同步到站点）

**统一原则**：
- 截图/录屏产物默认输出到 `e2e/screenshots/`（不直接写入 `site/assets/`）。
- 同步到 `site/assets/*` 是**后续操作**，基于 `e2e/screenshots/` 中已有文件完成，不应通过重新截图/重录实现。
- 截图采集默认复用已构建的 webdriver 包（`--skip-build`）。

### 截图采集（site/marketing）
```bash
# 采集默认站点截图集合（输出到 e2e/screenshots/site/）
pnpm e2e:shots

# 指定 spec 生成对应截图（推荐：按需生成）
node e2e/run.mjs --skip-build --capture -- --spec e2e/specs/wizard-screenshot.ts
node e2e/run.mjs --skip-build --capture -- --spec e2e/specs/dashboard-screenshot.ts

# Pro 截图采集（输出同目录约定）
pnpm e2e:shots:pro

# 将 e2e/screenshots/site/ 同步到 site/assets/screenshots/（后处理，不重采集）
pnpm e2e:shots:sync
# 或同步全部截图/博客/演示资源
pnpm e2e:assets:sync
```

> `--capture` 会放开默认排除的截图/录屏 spec（`*screenshot*`、`zz-screenshots.ts`、`demo-recording.ts` 等）。采集内容应通过 `--spec` / `--suite` 精确指定，避免误跑。

### 演示录屏
```bash
# 默认复用已有 webdriver 构建，输出到 e2e/screenshots/demo/
pnpm e2e:demo

# 需要时显式构建
pnpm e2e:demo:build

# 将 e2e/screenshots/demo/demo-recording.mp4 同步到 site/assets/video/（后处理，不重录）
pnpm e2e:demo:sync
```

### 调试截图留痕（trace）
`--screenshot` / `E2E_SCREENSHOT=1` 用于测试过程留痕（输出到 `e2e/screenshots/<spec>/...`），与“发布素材采集”是不同用途：
```bash
node e2e/run.mjs --skip-build --screenshot -- --spec e2e/specs/table-data.ts
```


## 3. 手工分步（调试用）

```bash
# 1) 构建（唯一合法的 E2E 二进制来源；含驱动 feature）
node scripts/generate-menu-labels.mjs && node scripts/with-driver-inject.mjs --drivers=basic -- node scripts/e2e-tauri-build.mjs

# 2) 确认产物
ls packages/desktop/dist/index.html
ls target/debug/datazen_desktop
# macOS 另有：
ls target/debug/bundle/macos/DataZen.app/Contents/MacOS/datazen_desktop

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
| **每个 spec 前** | 清空共享夹具库 `datazen_sync_src` / `datazen_sync_tgt` / `datazen_sync_mysql_src` / `datazen_sync_mysql_tgt`，并补回只读角色在 `datazen_sync_tgt` 上的 SELECT | `e2e/reset-sync-dbs.sh`（隔离模式由 `run.mjs` 起应用前调用；串跑模式由 `e2e/services/shared-fixture-reset.ts` 在每个 spec 文件的首个 suite 前调用） |
| **跑前** | 清空 `e2e/.app-data/`（隔离的应用数据目录） | `e2e/run.mjs`（`--keep-app-data` 保留） |
| **WDIO before** | upsert 默认 PG 连接 `conn_e2e_pg` | `e2e/wdio.conf.ts` → `seedDefaultPgConnection` |
| **WDIO onComplete** | 删除 `e2e-*` / `E2E-*` 连接、`e2e-*` workflow、清空 query history | `e2e/lib/testDataLifecycle.ts` |
| **跑后** | DROP 名称含 `e2e` / 前缀 `sync_` / 遗留 `ds_j_` / `ds_edge_` 的表，重 seed `product` | `e2e/teardown-e2e-env.sh`（`run.mjs` 自动调用） |

跳过 DB teardown：`E2E_SKIP_TEARDOWN=1 pnpm e2e:skip-build`。各 spec 内的 `after` hook 仍应清理本用例创建的连接/表（双保险）。

`datazen_sync_*` 这四个库是 data-sync / data-transfer / schema-diff 三个家族共用的夹具库。它们**每个 spec 文件开始前都会清空**（`e2e/reset-sync-dbs.sh`，实测干净时约 0.15s、有表时约 0.9s），因为此前只在整套开始/结束时各清一次：同一个 spec 单跑绿、整包红，红点还会漂到别的 spec 头上——「前面跑过谁」决定了「现在还剩什么表」。重置只动这四个夹具库，不碰种子库 `datazen_e2e` / `datazen_test` 和每个会话自己的 `e2e_w*` 工作库；PG 侧保留 `product` 以维持既有语义。跳过：`E2E_SKIP_SHARED_FIXTURE_RESET=1`（排查顺序相关问题时偶尔需要）。

#### Data Sync 夹具约定

| 层级 | 数据量 / 类型 | 清理 |
|------|--------------|------|
| **UI Journey**（`journeys/data-sync-journey.ts`） | 单表 5+3 行；`int` PK + `text`/`varchar`；INSERT+UPDATE+DELETE（Execute 确认） | 表名 `e2e_ds_j_*`；`after` + teardown |
| **Edge**（`data-sync-edge-cases.ts`） | 同上 | 表名 `e2e_ds_edge_*` |
| **IPC**（`data-sync-real.ts`） | 多表；含 `sync_pg_types` 宽类型（numeric/bool/timestamptz/uuid 等） | 表前缀 `sync_*`；spec `after` |

Journey 用小数据集保证 UI 路径稳定；类型与 apply 闭环见 `SYNC-REAL-*`（`SYNC-REAL-024` 覆盖 numeric/bool/double/uuid/timestamptz 的 PG apply）。修改前端/Rust 后应用 `pnpm e2e:minimal` 重建 webdriver binary，避免 `e2e:skip-build` 跑旧嵌入资源。

### 应用数据隔离（DATAZEN_DATA_DIR）

`e2e/run.mjs` 启动 webdriver 应用时会注入 `DATAZEN_DATA_DIR=<repo>/e2e/.app-data`（gitignored），
宿主 `DesktopRepository::default_app_data_dir()` / `DesktopRepository::init()` 优先读取该变量。**没有它，E2E 会直接读写
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

无数据库时，仅 UI/设置类 spec（如 `settings.ts`、`i18n-*`、部分 `path-ipc-hardening`）仍可能通过；依赖真实连接的 suite 会失败。需要 Kiwi / Superset 等插件驱动的 spec 必须用默认 `pnpm e2e`（全部插件）构建。

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
| Redis E2E（插件包，非默认） | `crates/drivers/redis/e2e/redis.ts`, `redis-topology.ts` — `pnpm e2e:redis` |
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
- [ ] `packages/desktop/dist/index.html` 存在
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
| `e2e/reset-sync-dbs.sh` | 清空 `datazen_sync_*` 四个共享夹具库（每个 spec 前调用） |
| `e2e/teardown-e2e-env.sh` | 跑后清理临时表、重置 seed |
| `e2e/lib/testDataLifecycle.ts` | WDIO 全局 IPC setup/teardown |
| `src-tauri/Cargo.toml` → `webdriver` feature | 启用 `tauri-plugin-webdriver` |

## Rune 原生桌面 E2E

旧 `pnpm e2e` 仍验证 React/Tauri。Rune 的测试从
[`e2e/rune/run.py`](../../e2e/rune/run.py) 启动实际的
`datazen-native`，使用 Rune 的原生运行时回放按键和点击（按元素 key 定位），
从渲染树验证可见结果，并用独立 SQLite 连接核验数据库写入。每个用例使用私有的
临时目录，不读取本机已保存的连接，不用 mock driver。这是无窗口回放，不覆盖
操作系统窗口、托盘交互或安装包启动。默认套件只使用本地临时数据；
真实 PostgreSQL/MySQL 需要显式启用独立服务配置，Redis 使用驱动自己的测试入口。

Linux 需有 Vulkan（无物理 GPU 可用 Mesa lavapipe）和 `libxkbcommon` 开发包。
从包含 `vendor/rune` 子模块的检出运行：

```bash
cargo build --locked -p datazen-rune-host --bin datazen-native
python3 e2e/rune/run.py target/debug/datazen-native
rustup component add llvm-tools-preview
python3 e2e/rune/coverage.py --output target/rune-e2e-report
```

覆盖率门禁只合并上述 E2E 进程生成的 LLVM profile，不混入构建脚本、单测或
其它集成测试；分母为本平台原生二进制中三个 Rune crate（`datazen-ui`、
`datazen-ui-api`、`datazen-rune-host`）的 LLVM 映射源码行，不含 vendor、共享
内核和数据库驱动。报告显示三个 crate 各自及合并的已覆盖行数/总行数，不按
crate 百分比取平均；用例失败、没有有效 profile 或精确行覆盖率低于 90%
均为失败。GitHub Actions 的 Rune 覆盖率工作流只保留验证产物，不上传发布包。

### 离线用例迁移范围

`e2e/rune/run.py` 包含连接、基础表浏览、DDL 及查询的原生冒烟。
[`e2e/rune/offline.py`](../../e2e/rune/offline.py) 补充以下旧版旅程在 Rune
原生入口上的对应覆盖。每个用例自建 SQLite 库或只用本地配置文件，不属于
依赖外部数据库的测试组。

| 旧套件 | Rune 离线覆盖 |
| --- | --- |
| `settings.ts` | 主题、分页大小和查询行数设置持久化；非法页大小不落盘；本地 SQL 片段新增、重启读取、删除 |
| `connection-search-group.ts` / `connection-validation.ts` | 连接搜索忽略大小写、无结果提示、取消新建不保存 |
| `table-filter.ts` / `sqlite.ts` | 单列筛选和清空、三态排序、翻页、列/索引/外键、查询 SQLite 视图 |
| `sql-query.ts` / `chart-views.ts` | 结果查找与下一处命中、数值结果图表与表格切换、只读连接不写库 |
| `sql-query.ts` 选区和光标执行 | SQLite 仅执行所选文本或光标所在语句，保留字符串中的 UTF-8 与分号；拒绝多语句选区和未闭合的 SQL，独立核对邻近未选中的写语句没有生效 |
| `sqlite.ts` 查询结果分批 | 单条真实 SQLite 查询跨 500 行批次保留 600 行，查询结果定位到最后一行并与独立查询核对 |
| `export-import.ts` / `backup-database.ts` | CSV 与 JSON 真落盘并与数据库核对、取消不落盘、SQLite 备份在独立库中恢复 |
| `table-dirty-cells.ts` | 真实 SQLite 网格暂存三格，恢复其中一格后待提交数由 3 降为 2，独立确认提交前数据不变且提交仅落地剩余两格 |
| `export-import.ts` CSV 导入 | 真实 SQLite 现有表，按同名表头预览并确认追加；独立核对 UTF-8、引号、逗号、NULL 与空串、默认值及原有记录；取消、只读、格式错误、重复表头、目标触发器和后续行键冲突均不写入 |
| `sql-query.ts` 收藏 | 从设置保存当前 SQL 到真实收藏文件；重启加载但不自动执行，显式执行并与 SQLite 比对；改名不改身份，删除保留回收站文件 |
| `query-history.ts` | 真 SQL 成功/失败、重启持久化、连接隔离、加载历史语句后手动重新执行 |
| `workflow.ts` / `workflow-window.ts` | YAML 错误校验、保存后重启、执行真实的 SQLite 写入并用第二个连接读回 |
| `data-dashboard-sql-add.ts` | 创建并保存真实 SQL widget；独立 SQLite 连接修改数据后，重启并手动刷新，验证图表变化和隐藏工作流未出现在普通列表 |
| `data-transfer-window.ts` / `journeys/data-transfer-journey.ts` | 原生工具从真实 SQLite 文件向另一文件的现有表映射列并追加数据；独立读回区分 NULL 和空串、确认原行和默认值；另外验证执行前取消、只读目标、同文件别名和键冲突均不写入数据；另验证选择非默认表、修复重复映射后仅写入所选表 |
| `schema-diff-window.ts` | 原生工具以只读方式比较两个真实 SQLite 文件的表列，验证源列缺失、目标额外列、类型变化、可空性与默认值变化、无差异和同文件别名拒绝；独立读取核对两端结构、数据及文件未改变 |
| `data-sync-real.ts` / `data-sync-edge-cases.ts` | 只读比较两个真实 SQLite 文件的现有表，由独立查询核对待插入、待更新、未变化、仅目标端保留的行数；验证复合主键的 1,000 行边界，拒绝无匹配主键、超过 1,000 行或同文件别名；切换工具清除旧预览，两个文件不改变 |

覆盖率命令可加 `--jobs 3` 分片。每个用例仍使用独立临时目录；
默认所有用例通过且合并后的覆盖率达到阈值才算通过。`--report-only`
只关闭百分比门禁，不忽略失败、跳过的用例或 LLVM 错误。真实 PostgreSQL 和
MySQL 的 opt-in 结构用例用独立命令行连接核对列、索引顺序/唯一性和复合外键；
MySQL 逐行核对建表 DDL；PostgreSQL 独立核对带引号的非默认命名空间、默认值、主键、检查约束、外键及二级索引，同名表在多个命名空间出现时不显示任一表的 DDL。详细的报告口径、
真实 SQL fixture 参数及剩余迁移范围见 [Rune E2E 指南](../../e2e/rune/README.md)。

这不是旧套件的全量迁移。CSV 导入仅支持 UTF-8、现有无触发器的 SQLite 普通表、同名列、最多 256 KiB / 100 行 / 20 列；任意列映射、建表、其他格式和跨驱动导入仍未覆盖。
多条件筛选、历史检索/全部作用域、平台窗口和发布包验证、同步写入及跨驱动同步
以及结构对比的生成计划与执行未被上述用例覆盖。SQLite 数据传输使用真实宿主和共享引擎；
暂不支持跨引擎、结构建表、破坏性写入或 Job 续跑。内存演示不计为生产传输覆盖。
