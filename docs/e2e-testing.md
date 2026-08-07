# DataZen E2E 测试指南（WebdriverIO）

> 面向 AI Agent / 开发者：如何**正确**编译并跑通 WebDriver E2E。  
> 简要入口见 [AGENTS.md](../AGENTS.md)「E2E 测试」小节。

## 1. 硬性要求（必读）

| 规则 | 说明 |
|------|------|
| **必须用 Tauri CLI 构建** | `pnpm tauri build --debug --features webdriver` |
| **禁止裸 `cargo build`** | `cargo build -p datazen --features webdriver` 常导致运行时报 `asset not found: index.html`（未走 `beforeBuildCommand` / 资源嵌入流程） |
| **必须开 `webdriver` feature** | 否则 4445 端口不会监听，WDIO 连不上 |
| **前端产物 `dist/`** | 由 `pnpm build`（Tauri `beforeBuildCommand`）生成；`frontendDist` 为 `../dist` |

正确构建链路：

```
pnpm tauri build --debug --features webdriver
  → beforeBuildCommand: pnpm build   # 生成 dist/index.html 等
  → cargo build --features webdriver # 嵌入 dist + 启用 WebDriver 插件
  → （macOS）产出 target/debug/bundle/macos/DataZen.app
```

入口脚本：`e2e/run.mjs`（`pnpm e2e` 会调用它）。

## 2. 一键跑通（推荐）

```bash
# 首次 / 改过 Rust 或前端后：完整构建 + 跑全部 E2E
pnpm e2e

# 更快构建：跳过 Git 插件（仅内置驱动；多数 UI/路径 IPC spec 足够）
pnpm e2e:minimal
# 等价：DATAZEN_PLUGINS=none pnpm e2e

# 已有正确的 webdriver debug 构建后：跳过构建，只跑 WDIO
pnpm e2e:skip-build

# 只跑某个 spec
pnpm e2e:skip-build -- --spec e2e/specs/path-ipc-hardening.ts

# 分组快捷方式（均默认 --skip-build，前提是已做过 webdriver 构建）
pnpm e2e:core
pnpm e2e:db
pnpm e2e:ai
pnpm e2e:kiwi
pnpm e2e:i18n-backup
pnpm e2e:path-ipc
```

**Agent 推荐流程：**

1. 若不确定本地二进制是否合格 → 直接 `pnpm e2e -- --spec <spec>`（**不要**加 `--skip-build`），或先执行构建命令再 skip-build。  
2. 仅当本会话刚成功跑过 `pnpm tauri build --debug --features webdriver` 时，才用 `pnpm e2e:skip-build`。

## 3. 手工分步（调试用）

```bash
# 1) 构建（唯一合法的 E2E 二进制来源）
pnpm tauri build --debug --features webdriver

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
```

| 变量前缀 | 用途 |
|----------|------|
| `E2E_PG_*` / `PG_*` | PostgreSQL（多数核心 / DB spec 需要） |
| `E2E_MYSQL_*` | MySQL |
| `E2E_REDIS_*` | Redis |
| `E2E_KIWI_*` | Kiwi 插件 |
| `E2E_AI_*` | AI 功能 E2E |
| `DATAZEN_PLUGINS=none` | E2E 构建时跳过 Git 插件（见 `pnpm e2e:minimal`） |

无数据库时，仅 UI/设置类 spec（如 `settings.ts`、`i18n-*`、部分 `path-ipc-hardening`）仍可能通过；依赖真实连接的 suite 会失败。需要 Kiwi / OLAP 等插件驱动的 spec 必须用默认 `pnpm e2e`（全部插件）构建。

## 5. 架构说明

```
e2e/run.mjs
  ├─ (可选) pnpm tauri build --debug --features webdriver
  ├─ 启动 target/debug/.../datazen   # 插件监听 127.0.0.1:4445
  └─ npx wdio run e2e/wdio.conf.ts [--spec ...]

e2e/wdio.conf.ts
  ├─ hostname/port: 127.0.0.1:4445
  ├─ before: 强制 language=zh-CN，必要时 seed PostgreSQL 连接
  └─ specs: e2e/specs/**/*.ts
```

- Spec 写法：通过 `browser.executeAsync` + `__TAURI_INTERNALS__.invoke` 调后端；UI 用 WebdriverIO `$` / `expect`。  
- 路径类 IPC：生产构建走 `*_with_dialog`；**webdriver 构建**保留 `write_file` / `export_app_data(path)` 等路径 API 供 E2E 使用。  
- 原生系统对话框（另存为）在自动化里难以点选 → E2E 用路径 IPC 或 mock `invoke`。

## 6. Spec 索引（节选）

| 领域 | Spec |
|------|------|
| 核心 UI | `main-window.ts`, `homepage-features.ts`, `settings.ts`, `i18n-menu.ts` |
| 连接 | `new-connection.ts`, `edit-delete-connection.ts`, `connection-window.ts` |
| SQL / 表 | `sql-query.ts`, `table-data.ts`, `table-edit.ts`, `export-import.ts` |
| 路径 IPC / 备份 | `path-ipc-hardening.ts`, `app-data-backup.ts`, `backup-database.ts` |
| i18n | `i18n-10-locales.ts`, `system-locale.ts` |
| AI / Workflow | `ai-features.ts`, `ai-context.ts`, `workflow.ts`, `workflow-window.ts` |
| 驱动 | `sqlite.ts`, `mysql.ts`, `redis.ts`, `kiwi.ts` |

完整列表与分层测试见 [architecture/testing.md](./architecture/testing.md)。

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

## 9. 相关文件

| 路径 | 作用 |
|------|------|
| `e2e/run.mjs` | 构建 / 启动 / WDIO 编排 |
| `e2e/wdio.conf.ts` | WDIO 配置与全局 before hook |
| `e2e/helpers.ts` | 公共 UI/窗口助手 |
| `e2e/specs/` | 用例 |
| `e2e/.env.example` | 环境变量模板 |
| `src-tauri/Cargo.toml` → `webdriver` feature | 启用 `tauri-plugin-webdriver` |
