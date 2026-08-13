# F2 QA 报告：MainWindow 连接列表 Web 右键菜单

| 项 | 值 |
|---|---|
| 切片 | F2 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus` |
| 分支 | `feat/web-context-menus` |
| 被测代码 | `src/lib/mainWindowContextMenu.ts`；`src/windows/main/MainWindow.tsx` 三个 context menu handler；`src/lib/__tests__/mainWindowContextMenu.test.ts`；`src/windows/main/__tests__/MainWindow.test.tsx` |
| 规格 | 主窗连接列表系统原生 Tauri context menu → Web context menu；空白/分组/连接右键；移到分组走 F1 portal + `positionSubmenu`；必须 `showWebContextMenu(items, {x: clientX, y: clientY})`；测试点真实 `web-context-item-*` / `web-context-submenu-trigger-*` |
| 测试角色 | **全新独立验收会话**；未修改任何产品代码；未 commit；只写本报告 + `f2-web-menu-coverage.txt` |
| 日期 | 2026-08-13 |
| **总评** | **PASS**（单测 **42/42**；`mainWindowContextMenu.ts` Lines **100%** ≥80%；`MainWindow.tsx` Lines **96.25%**；E2E 全部 **BLOCKED**；产品缺陷 0） |

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

覆盖率原始摘要：`docs/progress/f2-web-menu-coverage.txt`。

### 1.1 桌面应用 / E2E 前置探测

computer-use MCP：

- `list_running_apps`：访达 / Microsoft Edge / Sublime Text / 微信 / iTerm2 / Cursor / 终端。**无 DataZen。**
- `list_windows`：仅 Edge（`deepseek-harness/...`）。**无 DataZen 窗。**

本机探测：

- `ps`：无 `datazen` 应用进程（仅 Cursor extension-host 工作区名含 datazen；探测瞬间另有本验收 vitest 进程）
- 本 worktree `target/debug/datazen`：**不存在**
- 本 worktree `target/debug/bundle/macos/DataZen.app`：**不存在**
- 本 worktree `dist/index.html`：**不存在**
- `127.0.0.1:4445`：**未监听**
- `e2e/.env`：**不存在**（仅有 `.env.example`）
- Host `e2e/specs/`：**无** 针对 F2 Web 菜单的 `web-context-item-*` / `web-context-submenu` 交互 spec

旁路（**不可用于本切片验收**）：

- `/Applications/DataZen.app` 存在，版本 **0.0.8**，时间戳 2026-08-07，**未运行**。与本分支 0.0.9 / `feat/web-context-menus` 不是同一构建，禁止用旧包冒充 F2 E2E。
- 主仓 `/Users/wuxiaolong/code/rust-projects/datazen/target/debug/datazen` 存在（2026-08-13 14:36），**非本 worktree、非 webdriver 监听**，未启动。

**结论：Host E2E 与 computer-use 黑盒均 BLOCKED，不假装 PASS。**

---

## 2. 范围 / 非范围

### 2.1 本切片范围

- 空白区域右键：新建分组、新建连接
- 分组标题右键：新建分组；已命名分组还有重命名/删除；未分组只有新建分组
- 连接右键：打开/断开、编辑、复制、移到分组（二级 submenu）、移出分组、删除（确认对话框）
- 三个 handler 调用 `showWebContextMenu(..., { x: e.clientX, y: e.clientY })`，不得再用 `@tauri-apps/api/menu` 的 `Menu.popup()`
- 二级「移到分组」走 F1 `WebContextMenuHost` portal + `positionSubmenu`
- MainWindow 测试必须点真实 `web-context-item-*` / `web-context-submenu-trigger-*`，不得 mock Tauri `menuItemActions`

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-BLANK-01 | TS builder | 空白菜单 = new-group + new-connection | `builds blank-area items` |
| UT-GROUP-01 | TS builder | 未分组 header 仅 new-group | `omits rename/delete for the ungrouped header` |
| UT-GROUP-02 | TS builder | 已命名分组含 rename/delete | 同上 case 的 grouped 臂 |
| UT-CONN-01 | TS builder | 移到分组为 submenu；含目标 + 移出；action 传 groupId | `puts move targets in a submenu so they can flip at the window edge` |
| UT-MW-BLANK-01 | TS MainWindow | 空白右键 → 点 `web-context-item-new-group` → 对话框加组 | `new group dialog adds group` / `new group Enter key submits` |
| UT-MW-BLANK-02 | TS MainWindow | 空白右键 → 点 `web-context-item-new-connection` | `blank context menu on list area` |
| UT-MW-BLANK-03 | TS MainWindow | 右键连接项不弹出空白菜单 | `blank context menu skips conn item target` |
| UT-MW-GROUP-01 | TS MainWindow | 已命名分组：rename / delete | `group context menu rename and delete` |
| UT-MW-GROUP-02 | TS MainWindow | 未分组：仅 new-group，无 rename/delete | `ungrouped context menu has no rename/delete` |
| UT-MW-GROUP-03 | TS MainWindow | 重命名 Escape 取消 | `rename escape cancels inline edit` |
| UT-MW-CONN-01 | TS MainWindow | 未连接：open / edit / duplicate / delete（ask=true） | `connection context menu open, edit, duplicate, delete` |
| UT-MW-CONN-02 | TS MainWindow | 已连接：点 `web-context-item-disconnect` | `connection context menu disconnect when connected` |
| UT-MW-CONN-03 | TS MainWindow | hover `web-context-submenu-trigger-move-to-group` 后点目标组 / 移出 | `connection context menu move to group` |
| E2E-F2-01~05 | E2E | HOME 空白/分组/连接右键 + 二级贴右边缘 | **BLOCKED**（无本分支应用 / 无 webdriver；无对口 spec） |

### 2.2 非范围

- 不修代码、不改产品逻辑、不 commit
- 不改 `docs/progress/web-context-menus.md`
- 不跑 `pnpm tauri build --debug --features webdriver`（无既有本 worktree 二进制则 BLOCKED，不新建构建）
- 不启动 `/Applications/DataZen.app` 0.0.8 旧包
- DataTable / QueryPanel / Schema / Workflow / Redis / Data Sync 仍用 native menu 属 F3–F5；**不得因此判 F2 FAIL**

---

## 3. 单元测试

### 3.1 命令与结果

```bash
cd /Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus

npx vitest run \
  src/lib/__tests__/mainWindowContextMenu.test.ts \
  src/windows/main/__tests__/MainWindow.test.tsx \
  --coverage \
  --coverage.include='src/lib/mainWindowContextMenu.ts' \
  --coverage.include='src/windows/main/MainWindow.tsx' \
  --reporter=verbose
```

| 命令 | 通过 | 失败 | 忽略 | 结果 |
|---|---:|---:|---:|---|
| vitest 2 files（verbose） | **42** | 0 | 0 | **PASS**（1.86s） |

分文件：

| 文件 | 通过 | 失败 |
|---|---:|---:|
| `mainWindowContextMenu.test.ts` | 3 | 0 |
| `MainWindow.test.tsx` | 39 | 0 |

失败详情：无。

### 3.2 已落地单测清单（对照规格）

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-BLANK-01 | `builds blank-area items` | ids = `new-group`, `new-connection` | PASS |
| UT-GROUP-01 | `omits rename/delete for the ungrouped header` | `isUngrouped: true` → length 1 | PASS |
| UT-GROUP-02 | 同上（grouped 臂） | 已命名分组含 `rename-group` | PASS |
| UT-CONN-01 | `puts move targets in a submenu so they can flip at the window edge` | submenu id=`move-to-group`；含 `move-group-Prod` + `remove-from-group`；action('Prod') | PASS |
| UT-MW-BLANK-01 | `TC-main: new group dialog adds group` | 空白右键 → `web-context-item-new-group` → OK → `addGroup('Staging')` | PASS |
| UT-MW-BLANK-01b | `TC-main: new group Enter key submits` | 同上路径 Enter → `addGroup('QA')` | PASS |
| UT-MW-BLANK-02 | `TC-main: blank context menu on list area` | 空白右键 → `web-context-item-new-connection` | PASS |
| UT-MW-BLANK-03 | `TC-main: blank context menu skips conn item target` | 连接右键出现 edit，不出现 new-connection | PASS |
| UT-MW-GROUP-01 | `TC-main: group context menu rename and delete` | `web-context-item-rename-group` / `delete-group` | PASS |
| UT-MW-GROUP-02 | `TC-main: ungrouped context menu has no rename/delete` | 仅 `web-context-item-new-group` | PASS |
| UT-MW-GROUP-03 | `TC-main: rename escape cancels inline edit` | Escape 不调用 `renameGroup` | PASS |
| UT-MW-CONN-01 | `TC-main: connection context menu open, edit, duplicate, delete` | 点真实 item；delete 走 `ask` 确认后 `deleteConnection` | PASS |
| UT-MW-CONN-02 | `TC-main: connection context menu disconnect when connected` | 已连接 → `web-context-item-disconnect` → `disconnect('c1')` | PASS |
| UT-MW-CONN-03 | `TC-main: connection context menu move to group` | hover `web-context-submenu-trigger-move-to-group` → 点子项 Prod / remove-from-group | PASS |

其余 26 条 `MainWindow.test.tsx` 用例覆盖主窗非菜单路径（init / 状态栏 / 拖拽 / 导入导出等），全部 PASS，不作为 F2 行为门槛，但计入本命令的 42/42。

### 3.3 静态对照（不改代码）

在 worktree 执行（exit 1 = 无匹配，符合「必须无匹配」）：

| 检查 | 结果 |
|---|---|
| `rg "@tauri-apps/api/menu" src/windows/main/MainWindow.tsx` | **无匹配**（exit 1） |
| `rg "showWebContextMenu" src/windows/main/MainWindow.tsx` | **有匹配**：L3 import；L326 分组；L359 连接；L682 空白 |
| `rg "menuItemActions\|@tauri-apps/api/menu" src/windows/main/__tests__/MainWindow.test.tsx` | **无匹配**（exit 1） |
| `rg "web-context-item-" src/windows/main/__tests__/MainWindow.test.tsx` | **有匹配**（L188 helper + L926–928 / L958–959） |
| `rg "web-context-submenu-trigger-" …MainWindow.test.tsx` | **有匹配**（L192 `hoverSubmenu`） |

源码对照：

- 三个 handler 均 `showWebContextMenu(items, { x: e.clientX, y: e.clientY })`（L342、L390、L691），未使用 `Menu.popup()`。
- `src/windows/main/` 无 `@tauri-apps/api/menu` / `showNativeContextMenu` / `nativeContextMenu` import。
- 测试挂载 `<MainWindow />` + `<WebContextMenuHost />`，经 store 打开真实 Web 菜单。
- F1 Host：`WebContextMenu.tsx` `createPortal(..., document.body)` + `positionSubmenu`（L148）；二级菜单边缘翻转由 F1 基建承担。本切片连接菜单把「移到分组」做成 `kind: 'submenu'`，hover trigger 后出现 `web-context-submenu`。

实现细节（不记缺陷）：规格用顿号并列「移到分组（二级 submenu）、移出分组」；实现把 `remove-from-group` **放进** `move-to-group` submenu（`mainWindowContextMenu.ts` L90–97）。能力存在，UT-CONN-01 / UT-MW-CONN-03 均覆盖点击。

UT 未击中但代码存在（记测试缺口，不记产品缺陷）：

1. `grouped=false && moveTargets.length>0`（未分组连接 + 其它分组）→ submenu 无「移出分组」。builder 未单测该组合（branch 83.33% 缺口）。
2. 删除连接 `ask` 返回 false 时不调用 `deleteConnection`（默认 mock 为 false，但无显式 cancel 用例）。
3. 已命名分组右键点「新建分组」的交互（builder 含该项；交互只从空白区域点过）。
4. MainWindow 交互未断言 submenu 几何 `left+width ≤ innerWidth`（F1 UT-WCM-03 / UT-POS-05~08 覆盖算法；本切片只验证 hover 后可点子项）。

---

## 4. 覆盖率

见 `docs/progress/f2-web-menu-coverage.txt`（含完整 verbose 输出）。

| 文件 | Stmts | Branch | Funcs | **Lines（门槛）** |
|---|---|---|---|---|
| `mainWindowContextMenu.ts` | 100%（16/16） | 83.33%（10/12） | 100%（5/5） | **100%（15/15）** |
| `MainWindow.tsx` | 89.67%（408/455） | 65.02%（132/203） | 91.89%（136/148） | **96.25%（360/374）** |
| **合计（include）** | **90.02%（424/471）** | 66.04%（142/215） | 92.15%（141/153） | **96.4%（375/389）** |

**门槛：**

- `mainWindowContextMenu.ts` Lines **必须 ≥80%** → **100% PASS**。
- `MainWindow.tsx` 是大文件；任务约定整文件可能 <80% 时**不要单独判 FAIL**。本次实测 **96.25% ≥80%**，无需「整文件低于门槛，切片新模块达标」备注。
- 全部 F2 行为用例 PASS；源码不再 import `@tauri-apps/api/menu`。

---

## 5. E2E 用例表

现状：**全部 BLOCKED**。原因叠加：

1. computer-use 确认无 DataZen 窗口 / 进程
2. 本 worktree 无 Tauri webdriver debug 二进制
3. 无 `dist/index.html`、无 `e2e/.env`、4445 未监听
4. Host `e2e/specs/` 无 F2 对口 spec（无 `web-context-menu` / `web-context-item-*` 交互）
5. `/Applications/DataZen.app` 为 0.0.8（2026-08-07），不是本分支构建

未启动应用，未执行任何 WDIO / computer-use 交互断言。**不把单元测试结果记为 E2E PASS。**

既有相关 spec（**仍 BLOCKED，且内容过时**，未跑）：

- `e2e/specs/main-window.ts` L112–128：只断言连接项/分组头 DOM 存在，注释写明 native menu 会阻塞 WebDriver
- `e2e/specs/homepage-features.ts` HOME-020/030/031：同样只查 handler 容器；**HOME-021** 标题为「页面不包含自定义菜单覆盖层（已使用原生菜单）」，有应用后需改写，否则会与 F2 Web 菜单冲突

应有（本切片）E2E 用例如下，全部标 BLOCKED：

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F2-01 | HOME 主窗连接列表**空白区域**右键 | `data-testid="web-context-menu"` portal；项为新建分组 / 新建连接；点项执行 | **BLOCKED**（无本分支应用 / 无 webdriver）。UT-MW-BLANK-01/02 覆盖同路径（组件层） |
| E2E-F2-02 | **已命名分组**标题右键 | 新建分组 + 重命名 + 删除；点重命名进入行内编辑 | **BLOCKED**。UT-MW-GROUP-01 PASS |
| E2E-F2-03 | **未分组**标题右键 | 仅新建分组；无重命名/删除 | **BLOCKED**。UT-MW-GROUP-02 PASS |
| E2E-F2-04 | **连接**右键（未连接 / 已连接） | 打开或断开、编辑、复制、移到分组 submenu、移出、删除确认 | **BLOCKED**。UT-MW-CONN-01/02/03 PASS |
| E2E-F2-05 | 连接右键后 hover「移到分组」，菜单贴近**窗口右边缘** | `data-testid="web-context-submenu"` 完整可见（`left+width ≤ innerWidth`，不被截断） | **BLOCKED**。交互 hover 由 UT-MW-CONN-03 覆盖；几何由 F1 UT-WCM-03 / `positionSubmenu` 覆盖，**未在本切片组件层对 MainWindow 菜单测右边缘** |

### 5.1 失败则重现步骤

E2E 未跑到断言，无 FAIL 重现。解除 BLOCKED 的前置：

1. 在本 worktree 执行 `pnpm tauri build --debug --features webdriver`，得到 `target/debug/datazen` 或 macOS `.app` bundle
2. 增加 / 改写 Host `e2e/specs/`（建议 `homepage-features.ts` HOME-020~031 与 `main-window.ts` 右键段）：右键空白/分组/连接 → 断言 `web-context-item-*`；hover `web-context-submenu-trigger-move-to-group`；近右缘 submenu 几何；删除确认
3. 删除或改写 HOME-021「无自定义覆盖层」断言
4. `pnpm e2e:skip-build -- --spec e2e/specs/homepage-features.ts`（或新 spec）
5. 或启动本分支 `pnpm tauri:dev` 后用 computer-use 按 E2E-F2-01~05 点选

---

## 6. 缺陷列表

无。本切片 P0（空白/分组/连接 Web 菜单、已连接 disconnect、移到分组二级菜单 hover 后点击、删除确认、`showWebContextMenu` + 真实 `web-context-item-*`）在单元测试与静态审查中与规格一致。

**不记缺陷**（测试缺口 / 后续 / 非范围）：

1. builder 未覆盖 `grouped=false && moveTargets.length>0`（branch 83.33%；Lines 已 100%）。
2. 删除确认取消无显式 UT。
3. E2E-F2-01~05 全部 BLOCKED；既有 HOME-021 仍按 native menu 编写，有应用后必须改。
4. DataTable / QueryPanel / Schema / Workflow / Redis / Data Sync 仍用 native menu → **F3–F5，不判 F2 FAIL**。
5. MainWindow 交互未测 submenu 贴右边缘几何（F1 已测基建）。

---

## 7. 总评

| 门槛 | 结果 |
|---|---|
| `npx vitest run` 指定 2 files | **42 passed / 0 failed** |
| `mainWindowContextMenu.ts` Lines ≥80% | **100%**（15/15） |
| `MainWindow.tsx` 整文件 Lines | **96.25%**（360/374）；不因整文件判 FAIL；本次亦 ≥80% |
| include 合计 Lines | **96.4%**（375/389） |
| F2 行为用例（空白/未分组/已命名分组/打开/断开/编辑/复制/二级移组/移出/删除确认） | **全部 PASS** |
| `MainWindow.tsx` 无 `@tauri-apps/api/menu` | 静态确认 |
| `MainWindow.tsx` 调用 `showWebContextMenu` | L3 / L326 / L359 / L682 |
| `MainWindow.test.tsx` 无 `menuItemActions` / `@tauri-apps/api/menu` | 静态确认 |
| `MainWindow.test.tsx` 点 `web-context-item-*` / `web-context-submenu-trigger-*` | 静态 + 交互确认 |
| 与 P0 规格冲突的产品缺陷 | **无** |
| E2E E2E-F2-01~05 | 全部 **BLOCKED**（无本分支桌面应用、无 webdriver 二进制、无对口 Host spec） |
| 记入缺陷 | **0** |

**总评：PASS**

F2 将主窗连接列表右键从 Tauri native menu 改为 Web 菜单：三个 handler 均 `showWebContextMenu(..., {x: clientX, y: clientY})`；空白/分组（含未分组空态）/连接（含已连接 disconnect）/二级「移到分组」hover 后点击均有真实 `web-context-item-*` 交互覆盖。`mainWindowContextMenu.ts` 行覆盖 100%。E2E 因本机无本分支 DataZen / 无 webdriver debug 二进制全部 BLOCKED，按任务约定**不单独导致 FAIL**。有应用后优先补 HOME 空白/分组/连接右键与右边缘 submenu，并改写 HOME-021。
