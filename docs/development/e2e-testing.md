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

正确构建链路：

```
node scripts/with-plugin-inject.mjs [--drivers=basic] -- node scripts/e2e-tauri-build.mjs
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
# 分组清单统一定义为 WDIO suites（e2e/wdio.conf.ts 的 suite 字段），单一事实来源；
# 临时跑某个分组也可直接：pnpm e2e:skip-build -- --suite <name>
pnpm e2e:core
pnpm e2e:db
pnpm e2e:ai
pnpm e2e:redis          # 显式：packages/drivers/redis/e2e/（不进默认 pnpm e2e）
pnpm e2e:i18n-backup
pnpm e2e:path-ipc
pnpm e2e:dashboard      # data-dashboard*.ts（同样 skip-build）
pnpm e2e:contract:matrix          # Host UI/IPC × PG/MySQL/SQLite 连接窗
pnpm e2e:contract:pg              # 仅 PostgreSQL 契约冒烟
pnpm test:unit:e2e-contract:coverage  # 契约纯逻辑单测 ≥80%
# Kiwi：在 datazen-driver-kiwi 仓 `pnpm e2e:kiwi`（Host 同名脚本会 exit 1）
```

> 脚本化封装：受限运行环境（应用数据目录写入受限、需 HOME 沙箱）可用
> [`scripts/run-regression.sh`](../../scripts/run-regression.sh)（全量回归门禁）与
> [`scripts/run-e2e-minimal.sh`](../../scripts/run-e2e-minimal.sh)（minimal 集 + `E2E_ENV_FILE`/主检出 `.env` 回退解析）。

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
node scripts/generate-menu-labels.mjs && node scripts/with-plugin-inject.mjs --drivers=basic -- node scripts/e2e-tauri-build.mjs

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

无数据库时，仅 UI/设置类 spec（如 `settings.ts`、`i18n-*`、部分 `path-ipc-hardening`）仍可能通过；依赖真实连接的 suite 会失败。需要 Kiwi / OLAP 等插件驱动的 spec 必须用默认 `pnpm e2e`（全部插件）构建。

## 5. 架构说明

```
e2e/run.mjs
  ├─ (可选) with-plugin-inject → e2e-tauri-build.mjs  # webdriver + 驱动 features
  ├─ 启动 target/debug/.../datazen   # 插件监听 127.0.0.1:4445
  └─ npx wdio run e2e/wdio.conf.ts [--spec ...]

e2e/wdio.conf.ts
  ├─ hostname/port: 127.0.0.1:4445
  ├─ before: 强制 language=zh-CN，必要时 seed PostgreSQL 连接
  ├─ specs: e2e/specs/**/*.ts
  └─ suite: 分组清单（core/db/contract/redis/ai/i18n-backup/path-ipc/dashboard），供 --suite 选择
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
| 路径 IPC / 备份 | `path-ipc-hardening.ts`, `app-data-backup.ts`, `backup-database.ts`, `backup-window.ts`, `schema-diff-window.ts` |
| i18n | `i18n-10-locales.ts`, `system-locale.ts` |
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
| `src-tauri/Cargo.toml` → `webdriver` feature | 启用 `tauri-plugin-webdriver` |
