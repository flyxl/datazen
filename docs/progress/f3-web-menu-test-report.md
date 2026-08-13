# F3 QA 报告：Host `showNativeContextMenu` 切到 Web menu

| 项 | 值 |
|---|---|
| 切片 | F3 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus` |
| 分支 | `feat/web-context-menus` |
| 被测代码 | `src/lib/nativeContextMenu.ts`；Host 调用点：DataTable、SqlConnectionView、QueryPanel、ErDiagramView、DDLView、WorkflowWindow；Redis `RedisWorkbench.tsx` 仅编译传 pos |
| 规格 | Host 全部 `showNativeContextMenu` 从 Tauri 原生 OS menu 改为 Web menu：不再 import `@tauri-apps/api/menu` / `Menu.popup()`；`showNativeContextMenu(items, pos)` 收 client 坐标并打开 `contextMenuStore` / `showWebContextMenu`；各 Host 调用点传 `{x,y}`；二级菜单仍走 F1 portal+flip |
| 测试角色 | **全新独立验收会话**；未修改任何产品代码；未 commit；只写本报告 + `f3-web-menu-coverage.txt` |
| 日期 | 2026-08-13 |
| **总评** | **PASS**（覆盖率套件 39/39；复核套件 78/78；`nativeContextMenu.ts` Lines **100%** ≥80%；E2E 全部 **BLOCKED**；产品缺陷 0） |

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
| webdriver 二进制 | **不存在**（本 worktree 无 `target/` 目录、无 `target/debug/datazen`） |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus
```

覆盖率原始摘要：`docs/progress/f3-web-menu-coverage.txt`。

未改 `docs/progress/web-context-menus.md`，未改任何 `src/`。

### 1.1 桌面应用 / E2E 前置探测

computer-use MCP：

- `list_running_apps`：访达 / Microsoft Edge / Sublime Text / 微信 / iTerm2 / Cursor / 终端。**无 DataZen。**
- `list_windows`：仅 Cursor（`Cursor Agents`）。**无 DataZen 窗。**

本机探测：

- `ps`：无 `datazen` 应用进程（仅 Cursor extension-host 工作区名含 datazen；探测瞬间另有本验收 vitest / rg 进程）
- 本 worktree `target/`：**不存在**
- 本 worktree `target/debug/datazen`：**不存在**
- 本 worktree `target/debug/bundle/macos/DataZen.app`：**不存在**
- 本 worktree `dist/index.html`：**不存在**
- `127.0.0.1:4445`：**未监听**
- `e2e/.env`：**不存在**（仅有 `.env.example`）
- Host `e2e/specs/`：**无** `web-context-menu` / `WebContextMenu` / `E2E-F3` / `data-testid="web-context-menu"` 用例

旁路（**不可用于本切片验收**）：

- `/Applications/DataZen.app` 存在，版本 **0.0.8**，时间戳 2026-08-07，**未运行**。与本分支 0.0.9 / `feat/web-context-menus` 不是同一构建，禁止用旧包冒充 F3 E2E。
- 主仓 `/Users/wuxiaolong/code/rust-projects/datazen/target/debug/datazen` 存在（2026-08-13 14:36），**非本 worktree、非 webdriver 监听**，未启动。

**结论：Host E2E 与 computer-use 黑盒均 BLOCKED，不假装 PASS。**

---

## 2. 范围 / 非范围

### 2.1 本切片范围

- `src/lib/nativeContextMenu.ts` 不再 import `@tauri-apps/api/menu`，不再 `Menu.popup()` / `Menu.new`
- `showNativeContextMenu(items, pos)` 必须接收 client 坐标，并最终打开 `contextMenuStore` / `showWebContextMenu`
- 空菜单 / 仅 separator 规范化后不打开
- `createNativeContextMenuHandler` 传 `clientX` / `clientY`
- 所有 Host 调用点传入 `{x,y}`：
  - DataTable
  - SqlConnectionView（tab + schema tree `payload.x/y`）
  - QueryPanel（editor / favorite / history / history header）
  - ErDiagramView
  - DDLView
  - WorkflowWindow（list + history）
- Redis `RedisWorkbench.tsx` 为编译需要传 pos（深度 Redis UI 单测属 F4，**不因 Redis UI 测试未改成 click web item 而判 F3 FAIL**）
- 二级菜单仍走 F1 portal+flip，不被窗口截断
- `sqlConnectionViewNoWebContextMenu.test.ts` 禁止的是旧 `components/ui/ContextMenu`，不是 `WebContextMenu`；该测试应仍 PASS

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-NCM-01~02 | TS normalizeNativeMenuItems | 保留 disabled；剥空 submenu；折叠首尾/重复 separator | `nativeContextMenu.test.ts` 2 cases |
| UT-NCM-03 | TS nativeEditMenuItems | Cut/Copy/Paste/SelectAll | 1 case |
| UT-NCM-04 | TS showNativeContextMenu | 仅 separator → store 不打开（先开再 no-op 关闭） | `no-ops when only separators remain` |
| UT-NCM-05 | TS showNativeContextMenu | 带坐标打开 store（x/y + item + submenu） | `opens the web menu store at the given client position` |
| UT-NCM-06 | TS showNativeContextMenu | disabled item 保留 | `keeps disabled items` |
| UT-NCM-07 | TS createNativeContextMenuHandler | preventDefault + stopPropagation + clientX/Y | `prevents default… at client coords` |
| UT-NCM-08 | TS createNativeContextMenuHandler | `stopPropagation: false` | `can skip stopPropagation` |
| UT-DT-01~02 | TS DataTable | 单元格右键组菜单；选中行无 cell hit 组菜单 | DataTable 2 contextmenu cases（mock `showNativeContextMenu`） |
| UT-SQL-01~02 | static | 禁止旧 `components/ui/ContextMenu` / `<ContextMenu` | `sqlConnectionViewNoWebContextMenu.test.ts` 2 cases **PASS** |
| ST-HOST-01~11 | static | 各 Host + Redis 调用点第二参数为坐标 | 源码对照（见 §3.3） |
| ST-RG-01~03 | static | 无 `@tauri-apps/api/menu`；无 `Menu.popup`/`Menu.new` | `rg` 无匹配 |
| ST-F1 | static | `WebContextMenu` 仍 portal + `positionSubmenu` | `WebContextMenu.tsx` |
| E2E-F3-01~04 | E2E | schema 树 / DataTable / SQL 编辑器 / Workflow 列表右键 | **BLOCKED** |

### 2.2 非范围

- 不修代码、不改产品逻辑、不 commit
- 不跑 `pnpm tauri build --debug --features webdriver`（无既有本 worktree 二进制则 BLOCKED，不新建构建）
- 不启动 `/Applications/DataZen.app` 0.0.8 旧包 / 主仓 `target/debug/datazen`
- **F4** Redis UI 测试迁移（不因 Redis UI 单测未 click web item 判 FAIL）
- **F5** Data Sync
- **F6** 文档 / AGENTS.md「禁止 Web ContextMenu」条文（仍是旧约定，F6 才改）。**不因 AGENTS.md L229 仍写禁止 Web 菜单而判 F3 FAIL。**
- 不改 `docs/progress/web-context-menus.md`

---

## 3. 单元测试

### 3.1 命令与结果

#### 覆盖率门槛套件

```bash
cd /Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus

npx vitest run \
  src/lib/__tests__/nativeContextMenu.test.ts \
  src/components/DataTable/__tests__/DataTable.test.tsx \
  src/windows/connection/__tests__/sqlConnectionViewNoWebContextMenu.test.ts \
  src/windows/workflow/__tests__/WorkflowWindow.test.tsx \
  --coverage \
  --coverage.include='src/lib/nativeContextMenu.ts' \
  --reporter=verbose
```

| 命令 | 通过 | 失败 | 忽略 | 结果 |
|---|---:|---:|---:|---|
| vitest 4 files + coverage | **39** | 0 | 0 | **PASS**（2.92s） |

分文件：

| 文件 | 通过 | 失败 |
|---|---:|---:|
| `nativeContextMenu.test.ts` | 8 | 0 |
| `DataTable.test.tsx` | 10 | 0 |
| `sqlConnectionViewNoWebContextMenu.test.ts` | 2 | 0 |
| `WorkflowWindow.test.tsx` | 19 | 0 |

#### 复核套件（不进 coverage 门槛）

```bash
npx vitest run \
  src/lib/__tests__/nativeContextMenu.test.ts \
  src/components/DataTable/__tests__/DataTable.test.tsx \
  src/windows/main/__tests__/MainWindow.test.tsx \
  src/windows/workflow/__tests__/WorkflowWindow.test.tsx \
  src/windows/connection/__tests__/sqlConnectionViewNoWebContextMenu.test.ts
```

| 命令 | 通过 | 失败 | 忽略 | 结果 |
|---|---:|---:|---:|---|
| vitest 5 files verbose | **78** | 0 | 0 | **PASS**（2.64s） |

其中 `MainWindow.test.tsx` 39/39（F2 主窗 Web 菜单回归，本切片顺带复核）。

失败详情：无。

### 3.2 已落地单测清单

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-NCM-01 | `keeps disabled items and removes empty submenus` | disabled 保留；空 submenu 删除 | PASS |
| UT-NCM-02 | `collapses duplicate and edge separators` | 首尾/重复 separator 折叠 | PASS |
| UT-NCM-03 | `returns cut/copy/paste/selectAll predefined items` | 标准编辑块 | PASS |
| UT-NCM-04 | `no-ops when only separators remain after normalize` | 仅 separator → `open=false`（先开再关） | PASS |
| UT-NCM-05 | `opens the web menu store at the given client position` | `open=true`；`x=40` `y=80`；含 item + submenu | PASS |
| UT-NCM-06 | `keeps disabled items so the web menu can render them inert` | store 中 `enabled: false` | PASS |
| UT-NCM-07 | `prevents default, stops propagation, and shows menu at client coords` | handler → store `x=15` `y=25` | PASS |
| UT-NCM-08 | `can skip stopPropagation` | `stopPropagation: false` 不调用 | PASS |
| UT-DT-01 | `opens native context menu with TablePlus-style items when a cell is hit` | `contextMenu` + `clientX/Y=10`；组出 copy/export 等 item | PASS |
| UT-DT-02 | `copies selected rows without cell hit` | 容器右键组出 copy-selected-rows | PASS |
| UT-SQL-01 | `does not import components/ui/ContextMenu` | 禁止旧组件 import | PASS |
| UT-SQL-02 | `does not render <ContextMenu` | 禁止旧 `<ContextMenu` | PASS |

WorkflowWindow 19 例覆盖列表/历史/执行面板，**未** fire `contextmenu`、**未** mock `showNativeContextMenu`。调用点坐标由 ST-HOST 静态对照，不记产品缺陷。

### 3.3 静态对照（不改代码）

#### `nativeContextMenu.ts`

- 文件头仅 `import type { MouseEvent as ReactMouseEvent } from 'react'`，**无** `@tauri-apps/api/menu`。
- `showNativeContextMenu(items, pos: ContextMenuPosition)` L68–74：`void import('../stores/contextMenuStore').then(({ showWebContextMenu }) => { showWebContextMenu(items, pos); })`。
- `createNativeContextMenuHandler` L99：`const pos = { x: e.clientX, y: e.clientY }`，再 `showNativeContextMenu(items, pos)`。
- `rg "Menu.popup|Menu.new" src/lib/nativeContextMenu.ts`：**无匹配**（exit 1）。

#### `rg "@tauri-apps/api/menu" src packages`

**无匹配**（exit 1）。`src/` 下无该 import。测试 mock **亦无**残留（无需注明产品代码漏迁；测试侧也干净）。

文档 `docs/progress/f2-web-menu-test-report.md` 有历史提及，不在 `src`/`packages` 产品路径。

#### Host 调用点第二参数均为坐标

| 调用点 | 文件:行 | 第二参数 | 结果 |
|---|---|---|---|
| DataTable | `DataTable.tsx:315` | `{ x: e.clientX, y: e.clientY }` | PASS |
| SqlConnectionView tab | `SqlConnectionView.tsx:504` | `{ x: e.clientX, y: e.clientY }` | PASS |
| SqlConnectionView schema tree | `SqlConnectionView.tsx:721` | `{ x: payload.x, y: payload.y }` | PASS |
| QueryPanel editor | `QueryPanel.tsx:529` | `{ x: e.clientX, y: e.clientY }` | PASS |
| QueryPanel favorite | `QueryPanel.tsx:559` | `{ x: e.clientX, y: e.clientY }` | PASS |
| QueryPanel history | `QueryPanel.tsx:581` | `{ x: e.clientX, y: e.clientY }` | PASS |
| QueryPanel history header | `QueryPanel.tsx:603` | `{ x: e.clientX, y: e.clientY }` | PASS |
| ErDiagramView | `ErDiagramView.tsx:191` | `{ x: event.clientX, y: event.clientY }` | PASS |
| DDLView | `DDLView.tsx:90` | `{ x: e.clientX, y: e.clientY }` | PASS |
| WorkflowWindow list | `WorkflowWindow.tsx:1310` | `{ x: e.clientX, y: e.clientY }` | PASS |
| WorkflowWindow history | `WorkflowWindow.tsx:1410` | `{ x: e.clientX, y: e.clientY }` | PASS |
| Redis Workbench（编译 / 非 F4） | `packages/drivers/redis/ui/RedisWorkbench.tsx:364` | `{ x: e.clientX, y: e.clientY }` | PASS（不测 Redis UI click） |

`createNativeContextMenuHandler` 仅在 `nativeContextMenu.ts` 定义 + 单测使用；Host 业务面直接 `showNativeContextMenu(..., pos)`。

#### F1 二级菜单未回退

`WebContextMenu.tsx` 仍 `createPortal`（L199）+ `positionRootMenu` / `positionSubmenu`（L135、L148）。F3 只改调用入口，不改定位基建。

#### AGENTS.md（不判 FAIL）

`AGENTS.md` L229 仍写：「右键菜单统一使用 Tauri 原生 Menu（`showNativeContextMenu`），禁止新增 Web ContextMenu」。属 **F6** 文档切片，按任务约定不记 F3 FAIL。

#### 旧 ContextMenu 禁令 vs WebContextMenu

`sqlConnectionViewNoWebContextMenu.test.ts` 只断言不 import `components/ui/ContextMenu`、不渲染 `<ContextMenu`。`SqlConnectionView.tsx` 使用 `showNativeContextMenu`（现已是 Web store），**2/2 PASS**，与规格一致。

UT 未击中但代码存在（记测试缺口，不记产品缺陷）：

1. DataTable UT 未断言 `showNativeContextMenu` 第二参数 `{x,y}`（只断言 items；fireEvent 带了 `clientX/Y=10`；实现 L315 已传坐标）。
2. WorkflowWindow / QueryPanel / SqlConnectionView / Er / DDL 无「右键 → store 打开」组件层用例（静态已确认 pos）。
3. `createNativeContextMenuHandler` 无 Host 业务调用点（仅工具函数 + 单测）。
4. 覆盖率 Branch 未击中 L48 / L56 两臂（Lines 已 100%）。

---

## 4. 覆盖率

```bash
npx vitest run \
  src/lib/__tests__/nativeContextMenu.test.ts \
  src/components/DataTable/__tests__/DataTable.test.tsx \
  src/windows/connection/__tests__/sqlConnectionViewNoWebContextMenu.test.ts \
  src/windows/workflow/__tests__/WorkflowWindow.test.tsx \
  --coverage \
  --coverage.include='src/lib/nativeContextMenu.ts' \
  --reporter=verbose
```

| 文件 | Stmts | Branch | Funcs | **Lines（门槛）** |
|---|---|---|---|---|
| `nativeContextMenu.ts` | 100%（38/38） | 91.3%（21/23） | 100%（9/9） | **100%（33/33）** |

**门槛：`nativeContextMenu.ts` Lines ≥ 80% → 100% PASS。**

未覆盖分支：L48（连续 separator `continue`）、L56（尾部分隔符 `lastSep` 的一种判定）。属 normalize 边角，不挡 Lines 门槛。

完整文本：`docs/progress/f3-web-menu-coverage.txt`。

---

## 5. E2E 用例表

现状：**全部 BLOCKED**。原因叠加：

1. computer-use 确认无 DataZen 窗口 / 进程
2. 本 worktree 无 `target/`、无 Tauri webdriver debug 二进制
3. 无 `dist/index.html`、无 `e2e/.env`、4445 未监听
4. Host `e2e/specs/` 无断言 `data-testid="web-context-menu"` 的 F3 spec
5. `/Applications/DataZen.app` 为 0.0.8（2026-08-07），不是本分支构建

未启动应用，未执行任何 WDIO / computer-use 交互断言。**不把单元测试结果记为 E2E PASS。**

应有 E2E（本切片验收清单）：

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F3-01 | 连接窗 schema 树节点右键 | 弹出 `data-testid="web-context-menu"`（非 OS 原生 menu）；坐标在指针附近；可点项 | **BLOCKED**（无本分支应用 / 无 webdriver）。静态：`SqlConnectionView.tsx:721` `{ x: payload.x, y: payload.y }` |
| E2E-F3-02 | DataTable 单元格右键 | 同上；菜单含 copy / export 等 Host 项 | **BLOCKED**。UT-DT-01 覆盖组菜单 items（mock），未渲染 Web host |
| E2E-F3-03 | SQL 编辑器右键 | 同上；含 run / format / favorite 等 | **BLOCKED**。静态：`QueryPanel.tsx:529` `clientX/Y` |
| E2E-F3-04 | Workflow 列表右键 | 同上；含 open / run / delete / copyName | **BLOCKED**。静态：`WorkflowWindow.tsx:1310` `clientX/Y` |

既有 Host spec 备注（**未跑、不记 PASS/FAIL**）：

- `e2e/specs/connection-window.ts` CTX-001~006 对内容区 dispatch `contextmenu`，断言 `copyCell` / `editStructure` 等，并 click `.fixed.z-[9999] button`。这是旧整区 ContextMenu 路径，**不是** `web-context-menu` testid；与 `sqlConnectionViewNoWebContextMenu` 禁令可能冲突。有本分支二进制后需另评，不记本切片缺陷。
- `e2e/specs/homepage-features.ts` / `main-window.ts` 主窗右键属 **F2**。
- 无 Workflow / SQL 编辑器 / schema 树专属 web 菜单 spec。

### 5.1 失败则重现步骤

E2E 未跑到断言，无 FAIL 重现。解除 BLOCKED 的前置：

1. 在本 worktree 执行 `pnpm tauri build --debug --features webdriver`（或等价 `scripts/e2e-tauri-build.mjs`），得到 `target/debug/datazen` 或 macOS `.app` bundle
2. 增加或改写 Host `e2e/specs/`：schema 树 / DataTable / SQL 编辑器 / Workflow 列表右键 → `web-context-menu` 在 `document.body`；点 item；近边缘 submenu 不被截断
3. `pnpm e2e:skip-build -- --spec <该 spec>`
4. 或启动本分支 `pnpm tauri:dev` 后用 computer-use 按 E2E-F3-01~04 手工/自动点选

---

## 6. 缺陷列表

无。本切片 P0（去掉 Tauri Menu.popup；`showNativeContextMenu` 收坐标并打开 web store；Host 调用点传 `{x,y}`；旧 ContextMenu 禁测仍 PASS；Lines ≥80%）在单元测试与静态审查中与规格一致。

**不记缺陷**（测试缺口 / 后续 / 非范围）：

1. DataTable UT 未断言第二参数坐标（实现已传）。
2. QueryPanel / SqlConnectionView / Er / DDL / Workflow 无组件层右键→store 用例。
3. E2E-F3-01~04 全部 BLOCKED；既有 CTX-001~006 可能仍指向旧整区菜单，有应用后需重写。
4. `nativeContextMenu.ts` Branches 91.3%（L48、L56）；Lines 已 100%。
5. AGENTS.md L229 仍禁止 Web ContextMenu → **F6**。
6. Redis UI 单测是否改为 click web item → **F4**。

---

## 7. 总评

| 门槛 | 结果 |
|---|---|
| coverage 套件 `npx vitest run` 4 files | **39 passed / 0 failed** |
| 复核套件 5 files（含 MainWindow） | **78 passed / 0 failed** |
| `nativeContextMenu.ts` Lines ≥80% | **100%**（33/33）；Stmts 100%；Branch 91.3% |
| 无 `@tauri-apps/api/menu`（`src` + `packages`） | **静态确认**（rg 无匹配；测试亦无 mock 残留） |
| 无 `Menu.popup` / `Menu.new` in `nativeContextMenu.ts` | **静态确认** |
| `src/` 无 `@tauri-apps/api/menu` import | **静态确认** |
| Host 调用点第二参数为坐标 | **11/11 + Redis 1** 静态确认 |
| `sqlConnectionViewNoWebContextMenu`（旧 ContextMenu，非 WebContextMenu） | **2/2 PASS** |
| F1 portal+flip 仍在 | **静态确认** `createPortal` + `positionSubmenu` |
| 与 P0 规格冲突的产品缺陷 | **无** |
| E2E-F3-01~04 | 全部 **BLOCKED**（无本分支桌面应用、无 webdriver 二进制） |
| 记入缺陷 | **0** |

**总评：PASS**

F3 Host 右键已从 Tauri 原生 OS menu 切到 Web menu：`showNativeContextMenu` 动态打开 `showWebContextMenu` / `contextMenuStore`，强制 client 坐标；列出的 Host 调用点均传入 `{x,y}`（schema 树用 `payload.x/y`）；`nativeContextMenu.ts` 行覆盖 100%。E2E 因本机无本分支 DataZen / 无 webdriver debug 二进制全部 BLOCKED，按任务约定**不单独导致 FAIL**。AGENTS.md 旧「禁止 Web ContextMenu」条文属 F6，不判本切片 FAIL。有应用后优先补 E2E-F3-01~04（schema 树、DataTable、SQL 编辑器、Workflow 列表）。
