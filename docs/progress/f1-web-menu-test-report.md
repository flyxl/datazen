# F1 QA 报告：Web 右键菜单基建（二级菜单窗口边缘不截断）

| 项 | 值 |
|---|---|
| 切片 | F1 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus` |
| 分支 | `feat/web-context-menus` |
| 被测代码 | `src/lib/contextMenuPosition.ts`、`src/lib/runPredefinedEdit.ts`、`src/stores/contextMenuStore.ts`、`src/components/ui/WebContextMenu.tsx`、`src/App.tsx`（挂 `WebContextMenuHost`） |
| 规格 | Web 右键菜单基建：portal 到 `document.body`；二级菜单在右/下边缘完整可见（`left+width ≤ innerWidth`，`top+height ≤ innerHeight`）；点菜单项执行 action 并关闭；Escape / 点空白关闭 |
| 测试角色 | **全新独立验收会话**；未修改任何产品代码；未 commit；只写本报告 + `f1-web-menu-coverage.txt` |
| 日期 | 2026-08-13 |
| **总评** | **PASS**（单测 17/17；覆盖率 include Lines **87.5%** ≥80%；`WebContextMenu.tsx` Lines **80.88%** ≥80%；E2E 全部 **BLOCKED**；产品缺陷 0） |

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

覆盖率原始摘要：`docs/progress/f1-web-menu-coverage.txt`。

### 1.1 桌面应用 / E2E 前置探测

computer-use MCP：

- `list_running_apps`：访达 / Microsoft Edge / Sublime Text / 微信 / iTerm2 / Cursor / 终端。**无 DataZen。**
- `list_windows`：仅 Cursor（`Cursor Agents`）。**无 DataZen 窗。**

本机探测：

- `ps`：无 `datazen` 应用进程（仅 Cursor extension-host 工作区名含 datazen；探测瞬间另有本验收 vitest 进程）
- 本 worktree `target/debug/datazen`：**不存在**
- 本 worktree `target/debug/bundle/macos/DataZen.app`：**不存在**
- 本 worktree `dist/index.html`：**不存在**
- `127.0.0.1:4445`：**未监听**
- `e2e/.env`：**不存在**（仅有 `.env.example`）
- Host `e2e/specs/`：**无** `web-context-menu` / `WebContextMenu` / `E2E-F1` 用例

旁路（**不可用于本切片验收**）：

- `/Applications/DataZen.app` 存在，版本 **0.0.8**，时间戳 2026-08-07，**未运行**。与本分支 0.0.9 / `feat/web-context-menus` 不是同一构建，禁止用旧包冒充 F1 E2E。
- 主仓 `/Users/wuxiaolong/code/rust-projects/datazen/target/debug/datazen` 存在（2026-08-13 14:36），**非本 worktree、非 webdriver 监听**，未启动。

**结论：Host E2E 与 computer-use 黑盒均 BLOCKED，不假装 PASS。**

---

## 2. 范围 / 非范围

### 2.1 本切片范围

- `positionRootMenu` / `positionSubmenu`：根菜单与二级菜单在视口右/下边缘翻转或 clamp，面板不被窗口裁切
- `runPredefinedEdit`：Cut/Copy/Paste/SelectAll/Undo/Redo → `document.execCommand`；Separator 忽略
- `contextMenuStore` / `showWebContextMenu`：空菜单不打开；规范化后打开；hide 清空
- `WebContextMenuHost`：`createPortal(..., document.body)`；`data-testid="web-context-menu"` / `web-context-submenu`；点 item 执行 action 并关闭；Escape 关闭
- `App.tsx` 挂载 `<WebContextMenuHost />`（覆盖率 include 不含此文件；静态对照）

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-POS-01~04 | TS contextMenuPosition | 根菜单：有空间 / 右翻 / 上翻 / pad clamp | `positionRootMenu` 4 cases |
| UT-POS-05~08 | TS contextMenuPosition | 二级：右侧 / 左翻 / 上移 / 矮窗高菜单 | `positionSubmenu` 4 cases |
| UT-EDIT-01~02 | TS runPredefinedEdit | 6 个 predefined → execCommand；Separator 忽略 | 2 cases |
| UT-STORE-01~03 | TS contextMenuStore | 空菜单忽略；打开+规范化；hide | 3 cases |
| UT-WCM-01 | TS WebContextMenu | 关闭时不渲染 | `renders nothing when closed` |
| UT-WCM-02 | TS WebContextMenu | 点 item → action + 关闭 | `shows items and runs actions` |
| UT-WCM-03 | TS WebContextMenu | hover 开 submenu；右边缘 `left+width ≤ innerWidth`；点子项执行 | `opens a submenu on hover and does not clip near the right edge` |
| UT-WCM-04 | TS WebContextMenu | Escape 关闭 | `closes on Escape` |
| E2E-F1-01~04 | E2E | portal / 边缘 submenu / 点项 / Escape·点空白 | **BLOCKED**（无本分支应用 / 无 webdriver；无对应 Host spec） |

### 2.2 非范围

- 不修代码、不改产品逻辑、不 commit
- 不跑 `pnpm tauri build --debug --features webdriver`（无既有本 worktree 二进制则 BLOCKED，不新建构建）
- 不启动 `/Applications/DataZen.app` 0.0.8 旧包
- 各业务菜单组装（schema 树 / SQL 编辑器 / 表数据等）属后续切片
- Host `e2e/specs/` 新增 web 菜单用例（本切片无 spec 可跑）

---

## 3. 单元测试

### 3.1 命令与结果

```bash
cd /Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus

npx vitest run \
  src/lib/__tests__/contextMenuPosition.test.ts \
  src/lib/__tests__/runPredefinedEdit.test.ts \
  src/stores/__tests__/contextMenuStore.test.ts \
  src/components/ui/__tests__/WebContextMenu.test.tsx
```

| 命令 | 通过 | 失败 | 忽略 | 结果 |
|---|---:|---:|---:|---|
| vitest 4 files（verbose 复核） | **17** | 0 | 0 | **PASS**（0.996s） |

分文件：

| 文件 | 通过 | 失败 |
|---|---:|---:|
| `contextMenuPosition.test.ts` | 8 | 0 |
| `runPredefinedEdit.test.ts` | 2 | 0 |
| `contextMenuStore.test.ts` | 3 | 0 |
| `WebContextMenu.test.tsx` | 4 | 0 |

失败详情：无。

### 3.2 已落地单测清单

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-POS-01 | `places at the cursor when there is room` | 根菜单 left/top = cursor | PASS |
| UT-POS-02 | `flips left when the right edge would clip` | `left + width ≤ viewport - pad` | PASS |
| UT-POS-03 | `flips up when the bottom edge would clip` | `top + height ≤ viewport - pad` | PASS |
| UT-POS-04 | `clamps to padding when flipping would leave the viewport` | 超大菜单 clamp 到 pad=8 | PASS |
| UT-POS-05 | `opens to the right when there is room` | submenu `side=right`，left=itemRight | PASS |
| UT-POS-06 | `flips to the left when the right side would clip` | `side=left`；left+width 仍在视口内 | PASS |
| UT-POS-07 | `shifts up when the submenu would clip the bottom` | `top+height ≤ viewport - pad` | PASS |
| UT-POS-08 | `keeps a tall submenu inside a short window` | 高菜单 top=pad | PASS |
| UT-EDIT-01 | `maps predefined items to execCommand` | Copy/Cut/Paste/SelectAll/Undo/Redo | PASS |
| UT-EDIT-02 | `ignores separators` | Separator 不调 execCommand | PASS |
| UT-STORE-01 | `ignores menus that normalize to empty` | 仅 separator → `open=false` | PASS |
| UT-STORE-02 | `opens with normalized items and position` | 剥 separator；x/y 保留 | PASS |
| UT-STORE-03 | `hides the menu` | hide → open=false、items=[] | PASS |
| UT-WCM-01 | `renders nothing when closed` | 无 `web-context-menu` | PASS |
| UT-WCM-02 | `shows items and runs actions` | 点 `web-context-item-run` → action 1 次；菜单关闭；含 separator + disabled 项 | PASS |
| UT-WCM-03 | `opens a submenu on hover and does not clip near the right edge` | 视口 400×300、cursor x=360；hover `web-context-submenu-trigger-more`；`web-context-submenu` 的 `left+width ≤ 400`；点子项执行 | PASS |
| UT-WCM-04 | `closes on Escape` | keyDown Escape → 菜单消失 | PASS |

### 3.3 静态对照（不改代码）

`WebContextMenuHost`：`createPortal(..., document.body)`，根面板 `data-testid="web-context-menu"`，二级 `data-testid="web-context-submenu"`，`position: fixed` + `z-[10000]`，不依赖父 overflow。

定位：`useLayoutEffect` 测 `offsetWidth/Height` 后调 `positionRootMenu` / `positionSubmenu`，视口取 `window.innerWidth/innerHeight`。二级优先右侧，右侧不够则左侧；垂直 clamp。

关闭：item click → `hide()` 再 `action()` / `runPredefinedEdit`；`keydown Escape` → `hide()`；`mousedown` 且 target 不在 root/sub 内 → `hide()`。

`App.tsx` L108：`<WebContextMenuHost />` 与窗口内容并列，在 `ErrorBoundary` 内、`Suspense` 外，各 windowKind 共用同一 host。

UT 未击中但代码存在（记测试缺口，不记产品缺陷）：

1. 点空白关闭（`mousedown` outside）— 对应 E2E-F1-04 后半
2. predefined 项走 `runPredefinedEdit` 的 Host 路径（`runPredefinedEdit.ts` 自身已测）
3. 组件层未断言 submenu `top+height ≤ innerHeight`（纯函数 UT-POS-07/08 已覆盖算法）
4. 未显式断言 portal parent === `document.body`（实现为 `createPortal(..., document.body)`）

---

## 4. 覆盖率

```bash
npx vitest run \
  src/lib/__tests__/contextMenuPosition.test.ts \
  src/lib/__tests__/runPredefinedEdit.test.ts \
  src/stores/__tests__/contextMenuStore.test.ts \
  src/components/ui/__tests__/WebContextMenu.test.tsx \
  --coverage \
  --coverage.include='src/lib/contextMenuPosition.ts' \
  --coverage.include='src/lib/runPredefinedEdit.ts' \
  --coverage.include='src/stores/contextMenuStore.ts' \
  --coverage.include='src/components/ui/WebContextMenu.tsx'
```

| 文件 | Stmts | Branch | Funcs | **Lines（门槛）** |
|---|---|---|---|---|
| `contextMenuPosition.ts` | 100%（24/24） | 100%（16/16） | 100%（3/3） | **100%（23/23）** |
| `runPredefinedEdit.ts` | 85.71%（6/7） | 75%（3/4） | 100%（1/1） | **100%（5/5）** |
| `contextMenuStore.ts` | 100%（9/9） | 100%（2/2） | 100%（4/4） | **100%（8/8）** |
| `WebContextMenu.tsx` | 79.01%（64/81） | 64.55%（51/79） | 84.21%（16/19） | **80.88%（55/68）** |
| **合计（include）** | **85.12%（103/121）** | 71.28%（72/101） | 88.88%（24/27） | **87.5%（91/104）** |

**门槛：上述 include 的 Lines ≥ 80% → 合计 87.5% PASS；`WebContextMenu.tsx` Lines 80.88% 亦 ≥80% → PASS。**

说明：`WebContextMenu.tsx` **Statements 79.01%** 略低于 80%，**不是本切片门槛**（门槛为 Lines）。缺口见 `f1-web-menu-coverage.txt`（点空白、predefined Host 路径、itemLabel 未用臂、onFocus）。

`App.tsx` 不在 include 内。

---

## 5. E2E 用例表

现状：**全部 BLOCKED**。原因叠加：

1. computer-use 确认无 DataZen 窗口 / 进程
2. 本 worktree 无 Tauri webdriver debug 二进制
3. 无 `dist/index.html`、无 `e2e/.env`、4445 未监听
4. Host `e2e/specs/` 无 F1 web 菜单 spec（无法 `pnpm e2e:skip-build -- --spec …` 对口跑）
5. `/Applications/DataZen.app` 为 0.0.8（2026-08-07），不是本分支构建

未启动应用，未执行任何 WDIO / computer-use 交互断言。**不把单元测试结果记为 E2E PASS。**

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F1-01 | 打开任意窗，右键弹出 web 菜单 | `data-testid="web-context-menu"` 在 body portal，不被父 overflow 裁切 | **BLOCKED**（无本分支应用 / 无 webdriver）。UT-WCM-02 覆盖「菜单出现」；portal 由静态审查 `createPortal(..., document.body)` + `fixed` 对照，不记 E2E PASS |
| E2E-F1-02 | 靠近右/下边缘打开带 submenu 的菜单 | `data-testid="web-context-submenu"` 完整可见（`left+width ≤ innerWidth`，`top+height ≤ innerHeight`） | **BLOCKED**。UT-WCM-03 覆盖右边缘 `left+width ≤ 400`；下边缘算法由 UT-POS-07/08 覆盖，组件层未测 bottom |
| E2E-F1-03 | 点菜单项 | action 执行且菜单关闭 | **BLOCKED**。UT-WCM-02 / UT-WCM-03 覆盖同断言（组件层） |
| E2E-F1-04 | Escape / 点空白 | 菜单关闭 | **BLOCKED**。Escape：UT-WCM-04 PASS。点空白：实现有 `mousedown` outside → `hide()`，**无 UT、无 E2E** |

### 5.1 失败则重现步骤

E2E 未跑到断言，无 FAIL 重现。解除 BLOCKED 的前置：

1. 在本 worktree 执行 `pnpm tauri build --debug --features webdriver`（或等价 `scripts/e2e-tauri-build.mjs`），得到 `target/debug/datazen` 或 macOS `.app` bundle
2. 增加 Host `e2e/specs/` 用例：右键 → `web-context-menu` 在 `document.body`；近右/下缘 submenu 几何；click item；Escape 与 mousedown 空白
3. `pnpm e2e:skip-build -- --spec <该 spec>`
4. 或启动本分支 `pnpm tauri:dev` 后用 computer-use 按 E2E-F1-01~04 手工/自动点选

---

## 6. 缺陷列表

无。本切片 P0（portal 根菜单、二级边缘翻转/clamp、点项执行并关闭、Escape 关闭）在单元测试与静态审查中与规格一致。

**不记缺陷**（测试缺口 / 后续）：

1. 点空白关闭无 UT（`WebContextMenu.tsx` L162–165 未击中）；E2E-F1-04 后半 BLOCKED。属覆盖缺口，不是观察到的错误行为。
2. Host 未测 predefined 项点击（`runPredefinedEdit` 单测已覆盖映射）。
3. 组件层未覆盖下边缘 submenu 几何（纯函数已覆盖）。
4. `WebContextMenu.tsx` Statements 79.01% / Branches 64.55% 偏低；Lines 已过 80%。
5. Host E2E 无对应 spec，有应用后仍须补用例，否则 E2E 表会长期 BLOCKED。

---

## 7. 总评

| 门槛 | 结果 |
|---|---|
| `npx vitest run` 指定 4 files | **17 passed / 0 failed** |
| include 合计 Lines ≥80% | **87.5%**（91/104）；Stmts **85.12%**（103/121） |
| `WebContextMenu.tsx` Lines 尽量 ≥80% | **80.88%**（55/68）；Stmts 79.01%（非门槛） |
| `contextMenuPosition.ts` Lines | **100%**（23/23） |
| `runPredefinedEdit.ts` Lines | **100%**（5/5） |
| `contextMenuStore.ts` Lines | **100%**（8/8） |
| `App.tsx` 挂载 Host | 静态确认 L108 `<WebContextMenuHost />` |
| 与 P0 规格冲突的产品缺陷 | **无** |
| E2E E2E-F1-01~04 | 全部 **BLOCKED**（无本分支桌面应用、无 webdriver 二进制、无 Host spec） |
| 记入缺陷 | **0** |

**总评：PASS**

F1 Web 右键菜单基建满足：定位算法在右/下边缘翻转或 clamp；Host portal 到 `document.body` 且 `fixed`；点 item 执行 action 并关闭；Escape 关闭。include 四文件行覆盖合计 87.5%，`WebContextMenu.tsx` 行覆盖 80.88%。E2E 因本机无本分支 DataZen / 无 webdriver debug 二进制全部 BLOCKED，按任务约定**不单独导致 FAIL**。有应用后优先补 Host spec 并验证 E2E-F1-01 portal、E2E-F1-02 下边缘、E2E-F1-04 点空白。
