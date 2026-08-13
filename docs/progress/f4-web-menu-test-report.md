# F4 QA 报告：Redis 驱动 key 列表 Web 右键菜单

| 项 | 值 |
|---|---|
| 切片 | F4 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus` |
| 分支 | `feat/web-context-menus` |
| 被测代码 | `packages/drivers/redis/ui/redisKeyContextMenu.ts`；`packages/drivers/redis/ui/RedisWorkbench.tsx`（`handleKeyContextMenu` + `KeyTable` 行 `onContextMenu`）；`packages/drivers/redis/ui/__tests__/redisKeyContextMenu.test.ts`；`packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx` |
| 规格 | Redis 驱动 key 列表右键改为 Web 菜单：`RedisWorkbench.tsx` 调用 `showNativeContextMenu(..., { x: e.clientX, y: e.clientY })`，不使用 `@tauri-apps/api/menu`；`buildRedisKeyContextMenuItems` 产出 copy-key / set-ttl / rename / delete；二级菜单防截断走 F1 Host portal；单测含 builder + 真实 `WebContextMenuHost` 点击 `web-context-item-*` |
| 测试角色 | **全新独立验收会话**；未修改任何产品代码；未 commit；只写本报告 + `f4-web-menu-coverage.txt` |
| 日期 | 2026-08-13 |
| **总评** | **PASS**（单测 **7/7**；`redisKeyContextMenu.ts` Lines **100%** ≥80%；静态 `clientX`/`clientY` 且无 `@tauri-apps/api/menu`；E2E 全部 **BLOCKED**；产品缺陷 0） |

---

## 1. 测试环境

| 工具 | 版本 |
|---|---|
| OS | macOS darwin 24.6.0 (arm64) |
| rustc | 1.90.0 (1159e78c4 2025-09-14) |
| cargo | 1.90.0 (840b83a10 2025-07-30) |
| Node | v22.20.0 |
| vitest | 4.1.10 |
| crate / app | `datazen` 0.0.9（`package.json` / `src-tauri/tauri.conf.json`） |
| 桌面应用（本分支） | **未运行 / 未构建** |
| webdriver 二进制 | **不存在**（本 worktree 无 `target/debug/datazen`） |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus
```

覆盖率原始摘要：`docs/progress/f4-web-menu-coverage.txt`。

### 1.1 桌面应用 / E2E 前置探测

computer-use MCP：

- `list_running_apps`：访达 / Microsoft Edge / Sublime Text / 微信 / iTerm2 / Cursor / 终端。**无 DataZen。**
- `list_windows`：仅 Cursor（`Cursor Agents`）。**无 DataZen 窗。**

本机探测：

- `ps`：无 `datazen` 应用进程（仅 Cursor extension-host 工作区名含 datazen）
- 本 worktree `target/debug/datazen`：**不存在**
- 本 worktree `target/debug/bundle/macos/DataZen.app`：**不存在**
- 本 worktree `dist/index.html`：**不存在**
- `127.0.0.1:4445`：**未监听**
- `e2e/.env`：**不存在**（仅有 `.env.example`）
- `packages/drivers/redis/e2e/`：**无** key 列表右键 / `web-context-item-*` / `web-context-menu` 用例（`redis.ts` / `redis-topology.ts` 均无 contextmenu）

旁路（**不可用于本切片验收**）：

- `/Applications/DataZen.app` 存在，版本 **0.0.8**，时间戳 2026-08-07，**未运行**。与本分支 0.0.9 / `feat/web-context-menus` 不是同一构建，禁止用旧包冒充 F4 E2E。
- 主仓 `/Users/wuxiaolong/code/rust-projects/datazen/target/debug/datazen` 存在（2026-08-13 14:36），**非本 worktree、非 webdriver 监听**，未启动。

实际执行 `pnpm e2e:redis`（`node e2e/run.mjs --skip-build -- --spec packages/drivers/redis/e2e/redis.ts,packages/drivers/redis/e2e/redis-topology.ts`）：

```text
[e2e-runner] Skipping build (--skip-build). Binary MUST come from a prior Tauri webdriver build.
[e2e-runner] E2E binary not found: .../web-context-menus/target/debug/datazen
```

exit 1。未启动任何 DataZen 进程，未用旧 `.app`。

**结论：Redis E2E 与 computer-use 黑盒均 BLOCKED，不假装 PASS。**

---

## 2. 范围 / 非范围

### 2.1 本切片范围

- `buildRedisKeyContextMenuItems`：有 handler 时产出 `copy-key` / `set-ttl` / `rename` / `delete`；缺 handler 则省略对应项；标签由调用方传入
- `RedisWorkbench.handleKeyContextMenu`：`preventDefault` + `stopPropagation` 后 `showNativeContextMenu(items, { x: e.clientX, y: e.clientY })`
- `KeyTable` 行 `onContextMenu={(e) => onKeyContextMenu(e, entry.key)}`（`RedisWorkbench.tsx` L1014）
- 走 Host `showNativeContextMenu` → `showWebContextMenu` → `App` 挂载的 `WebContextMenuHost` portal（F1）；本菜单为扁平四项，无 submenu
- 单测：builder 纯函数 + 真实 `WebContextMenuHost` 点击 `web-context-item-*`

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-RK-01 | TS builder | 四 handler → copy-key / set-ttl / rename / delete，action 各 1 次 | `builds copy / set-ttl / rename / delete when handlers are set` |
| UT-RK-02 | TS builder | 仅 copy + delete → 只出这两项 | `skips items whose handlers are missing` |
| UT-RK-03 | TS builder | 无 handler → `[]` | `returns empty when no handlers` |
| UT-RK-04 | TS builder | 调用方中文 label 原样使用 | `uses caller-supplied labels` |
| UT-RK-05 | TS builder | 缺 set-ttl/delete → 仅 copy-key + rename | `omits only set-ttl and rename when those handlers are absent` |
| UT-RK-WEB-01 | TS WebContextMenuHost | `showNativeContextMenu` + 点 `web-context-item-copy-key` / `web-context-item-delete` | `opens a web menu at client coordinates and runs key actions` |
| UT-RK-WEB-02 | 静态源码 | `RedisWorkbench.tsx` 含 `showNativeContextMenu` + `{ x: e.clientX, y: e.clientY }`，不含 `@tauri-apps/api/menu` | `wires RedisWorkbench key rows to showNativeContextMenu with client coords` |
| E2E-F4-01~05 | E2E Redis | 键行右键 web 菜单 + 四项点击 | **BLOCKED**（无本分支 webdriver 二进制；现有 redis E2E 无右键 spec） |

### 2.2 非范围

- 不修代码、不改产品逻辑、不 commit
- 不跑 `pnpm tauri build --debug --features webdriver`（无既有本 worktree 二进制则 BLOCKED，不新建构建）
- 不启动 `/Applications/DataZen.app` 0.0.8 旧包
- Host DataTable / MainWindow（F2/F3）
- Data Sync（F5）
- 文档 / `web-context-menus.md`（F6）
- Redis Console / Monitor / Pub/Sub 右键（本切片仅 key 列表）

---

## 3. 单元测试

### 3.1 命令与结果

```bash
cd /Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus

npx vitest run --config vitest.drivers.config.ts \
  packages/drivers/redis/ui/__tests__/redisKeyContextMenu.test.ts \
  packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx \
  --coverage \
  --coverage.include='packages/drivers/redis/ui/redisKeyContextMenu.ts' \
  --reporter=verbose
```

| 命令 | 通过 | 失败 | 忽略 | 结果 |
|---|---:|---:|---:|---|
| vitest 2 files（verbose） | **7** | 0 | 0 | **PASS**（0.999s） |

分文件：

| 文件 | 通过 | 失败 |
|---|---:|---:|
| `redisKeyContextMenu.test.ts` | 5 | 0 |
| `redisKeyWebContextMenu.test.tsx` | 2 | 0 |

失败详情：无。

### 3.2 已落地单测清单

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-RK-01 | `builds copy / set-ttl / rename / delete when handlers are set` | ids = `['copy-key','set-ttl','rename','delete']`；四 action 各 1 次 | PASS |
| UT-RK-02 | `skips items whose handlers are missing` | 仅 copy+delete → `['copy-key','delete']` | PASS |
| UT-RK-03 | `returns empty when no handlers` | `[]` | PASS |
| UT-RK-04 | `uses caller-supplied labels` | 中文四标签原样 | PASS |
| UT-RK-05 | `omits only set-ttl and rename when those handlers are absent` | `['copy-key','rename']`（用例名写 set-ttl and rename，实际省略的是 set-ttl **与 delete**；断言与 handlers 一致，见 §6 缺口） | PASS |
| UT-RK-WEB-01 | `opens a web menu at client coordinates and runs key actions` | render `<WebContextMenuHost />`；`showNativeContextMenu(..., {x:120,y:40})`；`findByTestId('web-context-menu')`；点 `web-context-item-copy-key` → `onCopyKey` 1 次；再开菜单点 `web-context-item-delete` → `onDelete` 1 次 | PASS |
| UT-RK-WEB-02 | `wires RedisWorkbench key rows to showNativeContextMenu with client coords` | 读 `RedisWorkbench.tsx` 源码字符串：含 `showNativeContextMenu`、`{ x: e.clientX, y: e.clientY }`，不含 `@tauri-apps/api/menu` | PASS |

### 3.3 静态对照（不改代码）

`RedisWorkbench.tsx`：

- L23：`import { showNativeContextMenu } from '../../../../src/lib/nativeContextMenu'`
- L32：`import { buildRedisKeyContextMenuItems } from './redisKeyContextMenu'`
- L332–368：`handleKeyContextMenu` 调 `showNativeContextMenu(buildRedisKeyContextMenuItems({ labels, handlers }), { x: e.clientX, y: e.clientY })`
- L608：`<KeyTable ... onKeyContextMenu={handleKeyContextMenu} />`
- L1014：行 `onContextMenu={(e) => onKeyContextMenu(e, entry.key)}`
- **不含** `@tauri-apps/api/menu`（全文件 rg 无匹配）
- 含 `clientX` / `clientY`：L364 菜单坐标；L963 另为列宽 `onResizeStart(ci, e.clientX)`，与菜单无关但不违反「含 clientX/clientY」

`showNativeContextMenu`（`src/lib/nativeContextMenu.ts` L68–75）：动态 `import` `showWebContextMenu`，打开 F1 store；**无** `@tauri-apps/api/menu` / `Menu.popup()`。

`App.tsx` L108：`<WebContextMenuHost />` 与 `WindowContent` 并列，连接窗（含 Redis Workbench）与主窗共用同一 Host portal。

`buildRedisKeyContextMenuItems` 四项均为 `kind: 'item'`，无 submenu。F1 portal + `positionSubmenu` 防截断对本菜单无直接几何需求；根菜单仍走 Host `positionRootMenu`。

UT 未击中但代码存在（记测试缺口，不记产品缺陷）：

1. 未渲染真实 `RedisWorkbench` / `KeyTable` 后 `fireEvent.contextMenu`（接线仅源码字符串断言）
2. Web Host 未点击 `web-context-item-set-ttl` / `web-context-item-rename`（builder UT-RK-01 已调 action；Workbench 对话框 L776–874 未在本切片组件测中打开）
3. Workbench 调用点始终传入四个 handler，builder 的「缺 handler 省略」臂只被纯函数测覆盖

---

## 4. 覆盖率

命令见 §3.1。门槛：`redisKeyContextMenu.ts` Lines ≥ 80%。

| 文件 | Stmts | Branch | Funcs | **Lines（门槛）** |
|---|---|---|---|---|
| `redisKeyContextMenu.ts` | 100%（7/7） | 100%（2/2） | 100%（4/4） | **100%（5/5）** |
| **合计（include）** | **100%（7/7）** | **100%（2/2）** | **100%（4/4）** | **100%（5/5）** |

**门槛：`redisKeyContextMenu.ts` Lines ≥ 80% → 100% PASS。**

说明：v8 text 表 File 行为空（include-only）；`coverage/coverage-summary.json` 明确该文件 5/5 lines。可执行行即 `item` / `push` / `buildRedisKeyContextMenuItems` 函数体（类型 export 不计）。无未覆盖行。

`RedisWorkbench.tsx` 不在 include 内。

---

## 5. E2E 用例表

现状：**全部 BLOCKED**。原因叠加：

1. computer-use 确认无 DataZen 窗口 / 进程
2. 本 worktree 无 Tauri webdriver debug 二进制
3. 无 `dist/index.html`、无 `e2e/.env`、4445 未监听
4. `pnpm e2e:redis` 以 `--skip-build` 运行，runner 在 `target/debug/datazen` 缺失处 exit 1
5. `packages/drivers/redis/e2e/redis.ts` 现有 RD-001~RD-024（浏览 / Console / Monitor / Pub/Sub / 工作台 CRUD / 批量删除），**无** 键行 `contextmenu` / `web-context-menu` / copy-key / set-ttl / rename 右键路径
6. `/Applications/DataZen.app` 为 0.0.8（2026-08-07），不是本分支构建

未启动应用，未执行任何 WDIO / computer-use 交互断言。**不把单元测试结果记为 E2E PASS。**

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F4-01 | 打开 Redis 连接窗 Workbench，对 key 行右键 | `data-testid="web-context-menu"` 出现（body portal）；含 `web-context-item-copy-key` / `set-ttl` / `rename` / `delete`；非 OS 原生菜单 | **BLOCKED**（无本分支 webdriver）。UT-RK-WEB-01/02 + 静态 L336–364 / L1014 对照，不记 E2E PASS |
| E2E-F4-02 | 点 `web-context-item-copy-key` | 剪贴板为该 key 名；菜单关闭 | **BLOCKED**。UT-RK-WEB-01 点 copy-key 调 handler；Workbench `navigator.clipboard.writeText(key)` 未在 E2E/组件测验证 |
| E2E-F4-03 | 点 `web-context-item-set-ttl` | 打开 TTL Dialog（`t('redis.setTtl')`）；菜单关闭 | **BLOCKED**。无 UT 点击 set-ttl；对话框实现 L776–814 |
| E2E-F4-04 | 点 `web-context-item-rename` | 打开重命名 Dialog（`t('redis.renameKey')`）；菜单关闭 | **BLOCKED**。无 UT 点击 rename；对话框实现 L816–850 |
| E2E-F4-05 | 点 `web-context-item-delete` | 打开删除确认 Dialog；菜单关闭 | **BLOCKED**。UT-RK-WEB-01 点 delete 调 handler；Workbench 确认框 L852–874 未在 E2E 验证 |

现有 Redis E2E（RD-005 键表列、RD-007 单击详情、RD-018~021 创建/编辑/批量删除）**不能**替代上述右键路径。

### 5.1 失败则重现步骤

E2E 未跑到断言，无 FAIL 重现。解除 BLOCKED 的前置：

1. 在本 worktree 执行 `pnpm tauri build --debug --features webdriver`（或 `scripts/e2e-tauri-build.mjs`），得到 `target/debug/datazen` 或 macOS `.app` bundle
2. 配置 `e2e/.env`（`E2E_REDIS_*`），确保 Redis 可达
3. 在 `packages/drivers/redis/e2e/` 增加 key 行右键 spec：右键 → `web-context-menu` 四项；分别点 copy / TTL / rename / delete 并断言对话框或剪贴板
4. `pnpm e2e:skip-build -- --spec packages/drivers/redis/e2e/<该 spec>`（或完整 `pnpm e2e:redis`）
5. 或启动本分支 `pnpm tauri:dev --drivers=redis`（或 `all`）后用 computer-use 按 E2E-F4-01~05 点选

---

## 6. 缺陷列表

无。本切片 P0（builder 四项、Workbench 走 `showNativeContextMenu` + client 坐标、无 `@tauri-apps/api/menu`、Host portal 点击 copy/delete）在单元测试与静态审查中与规格一致。

**不记缺陷**（测试缺口 / 后续）：

1. UT-RK-WEB 未点 `web-context-item-set-ttl` / `web-context-item-rename`；未对真实 `KeyTable` 行发 `contextmenu`。属覆盖缺口。
2. UT-RK-05 测试名写 “omits only set-ttl and rename”，handlers 实为 `{ onCopyKey, onRename }`，省略的是 set-ttl **与 delete**；断言 `['copy-key','rename']` 正确。命名不准，不是产品错误。
3. Redis E2E 无 key 右键 spec；有应用后仍须补 E2E-F4-01~05，否则 E2E 表会长期 BLOCKED。
4. `redisWorkbench.test.tsx` 只测 invoke helpers，不含右键（本切片已有专用文件，不重复记缺）。

---

## 7. 总评

| 门槛 | 结果 |
|---|---|
| `npx vitest run` 指定 2 files | **7 passed / 0 failed** |
| `redisKeyContextMenu.ts` Lines ≥80% | **100%**（5/5）；Stmts **100%**（7/7） |
| `RedisWorkbench.tsx` 含 `clientX`/`clientY` | **是**（L364 `{ x: e.clientX, y: e.clientY }`） |
| `RedisWorkbench.tsx` 不含 `@tauri-apps/api/menu` | **是**（rg 无匹配；走 `showNativeContextMenu`） |
| `buildRedisKeyContextMenuItems` 产出 copy-key / set-ttl / rename / delete | **是**（UT-RK-01） |
| 真实 `WebContextMenuHost` + 点击 `web-context-item-*` | **是**（UT-RK-WEB-01：copy-key、delete） |
| 与 P0 规格冲突的产品缺陷 | **无** |
| E2E E2E-F4-01~05 / `pnpm e2e:redis` | 全部 **BLOCKED**（无本分支 webdriver 二进制；现有 redis spec 无右键） |
| 记入缺陷 | **0** |

**总评：PASS**

F4 Redis key 列表右键满足：builder 产出 copy-key / set-ttl / rename / delete；Workbench 以 `showNativeContextMenu(..., { x: e.clientX, y: e.clientY })` 打开 F1 Web Host portal，不依赖 `@tauri-apps/api/menu`。`redisKeyContextMenu.ts` 行覆盖 100%。E2E 因本机无本分支 DataZen / 无 webdriver debug 二进制全部 BLOCKED，按任务约定**不单独导致 FAIL**。有应用后优先在 `packages/drivers/redis/e2e/` 补 E2E-F4-01~05（键行右键四项与 TTL/重命名/删除对话框）。
