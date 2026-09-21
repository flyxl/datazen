# Track: cap-bridge — 驱动宿主值依赖收敛（能力注入 + 纯函数/IPC 下沉）

- 分支: `feature/cap-bridge`（基准 `feat/driver-decoupling` @ c3058fdd0）
- 角色: Coder → Tester
- Wave 2 并行轨道之一（另一轨 i18n-core 负责 useI18n / PathInput，本轨**不碰**）

## 范围

消灭驱动 UI 对宿主**非 i18n 值**的直接 import。已盘点落点（基准上 Grep 复核为准）：

### A. 纯函数 / IPC 封装下沉 @datazen/driver-sdk（移动实现，宿主原位置改为从 driver-sdk import 使用）
1. `src/lib/driverSettings.ts`（`readBooleanField`、`applySchemaDefaults`，71 行，纯函数）→ `packages/driver-sdk/src/driverSettings.ts`
2. `src/commands/driver.ts`（`driverCommands`，62 行 Tauri IPC 封装）→ `packages/driver-sdk/src/ipc/driverCommands.ts`；宿主 `commands/driver.ts` 保留再从导出（宿主自身仍用 `driverCommands` 原符号）。driver-sdk/index.ts 已有 re-export，改指本地。
3. `src/commands/file.ts` 中被 redis ImportExport 用到的部分（先读文件确认，仅下沉被驱动消费的函数，其余留宿主）
4. `src/lib/nativeContextMenu.ts`（123 行：`showNativeContextMenu` 等；类型 Wave 1 已下沉）→ 实现下沉 driver-sdk；宿主消费点（src/windows 等）改从 driver-sdk 或经薄宿主文件再导出，**只留一份实现**。
5. `src/lib/resolveEditorFontFamily.ts`（仅 RedisConsole 1 处，先读实现确认无 store 依赖；有则 BLOCKED 上报，无则下沉 driver-sdk）

### B. 能力注入（沿用 schemaStoreBridge 先例，driver-sdk 新增 bind 模块，宿主启动时注入真实实现）
6. `useSettingsStore`（驱动 5 文件消费）→ driver-sdk `settingsStoreBridge.ts`：`bindSettingsStore(store)` + 暴露 `useBoundSettingsStore` hook 访问器；驱动改从 driver-sdk import。宿主 `src/stores/settingsStore.ts` 在 store 定义处调 `bindSettingsStore`（同 schemaStore.ts:694 模式）。
7. `useConnectionStore`（ClusterNodePicker 1 文件）→ 同模式 connectionStoreBridge。
8. `useConfirmDialog`（RedisWorkbench、useRedisGate）→ confirmDialogBridge：`bindConfirmDialog(hook)` + 驱动经访问器使用。
9. `showNativeContextMenu`（RedisWorkbench + redisKeyContextMenu 消费）：下沉后直接 import，无需 bind（若 A4 遇宿主 store 依赖再评估）。

### C. 驱动侧 import 换源
以上落点涉及的驱动 ui 文件（约 12 个 + 测试 `__tests__/useRedisGate.test.tsx`、`__tests__/settings.test.ts`、`__tests__/redisKeyWebContextMenu.test.tsx` 的 import/mock 路径）；`vitest.drivers.config.ts` 如需 alias 补齐则补。

## 禁止事项（防跨轨冲突）
- **不动任何 `src/hooks/useI18n` import、不动 PathInput、不动 packages/ui**（i18n-core 轨范围）。
- 不动 Wave 1 已下沉的类型文件（`driver-sdk/src/types/*`）。
- 宿主对下沉模块的存量 import 全部保持可用。

## 验收标准
- Grep `packages/drivers/*/ui`（排除 useI18n 行）：宿主值 import 仅剩 `src/hooks/useI18n` 与 `src/components/ui/PathInput` 两类。
- 全仓 `npx tsc --noEmit -p tsconfig.json` 0 错误。
- redis ui：`npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui` **218 pass / 0 fail**（Wave 1 后新基线）。
- 宿主 Rust 不涉；宿主 vitest 定向跑下沉模块相关测试（nativeContextMenu、driverSettings、commands）零回归。

## 状态

- [x] Coder 完成 → READY_FOR_TEST
- [x] Tester 复测 → **FAILED（见 bugs.md，待 Coder 补 bridge 单测后复测）**
- [x] Coder round 2 修复 cap-bridge-BUG-001 → READY_FOR_TEST（见下方记录）
- [x] Tester 复测 BUG-001 → **TEST_DONE(PASSED)**（Round 2，见下方记录，全轨关账）

## Tester 复测记录（Round 2，待测 3908f64e5，上轮 docs 4cf33a4c6）

### 阶段 A 修复审查（通过）
- `git diff 4cf33a4c6..3908f64e5`：仅新增 `packages/driver-sdk/__tests__/` 四套件
  （settingsStoreBridge 8 / connectionStoreBridge 5 / confirmDialogBridge 3 /
  schemaStoreBridge.bound 13）+ `packages/driver-sdk/tsconfig.json` 一行 `"jsx": "react-jsx"`
  + 两份协调文档；`packages/driver-sdk/src`、`src/`、`packages/drivers` **零运行时改动**。
- 断言真实性逐文件核对：四处未绑定 throw 文案与源码逐字一致（`vi.resetModules()` +
  动态 import 隔离，不与驱动侧顶层 bind 套件互污）；setState 对象/updater 两形态
  `toBe(patch)` 同引用透传断言；`useBoundSettingsStore`/`useBoundConnectionStore`/
  `useBoundSchemaStore` 用真实 zustand `create` store 组件内订阅→act→重渲染；
  confirm 二元组引用透传 + options 原样入参（confirm/cancel 两态）；schemaStoreBridge
  sync 助手 dbSessionId 有/无分支、`subscribeSchemaPathItems` 引用相同跳过 + 退订断言。
  无为覆盖率凑数的空断言。

### 阶段 B 完整复跑（实测 = 全绿）
- `npx vitest run packages/driver-sdk`：**36 passed / 0 failed**（7 files；Coder 自报 32/32
  系计数口径偏差，实测多 4 个既有用例、方向为全绿，非缺陷）。
- 覆盖率重现命令（bugs.md 口径，v8 + `--coverage.all`）：四个 bridge
  **Stmts/Branch/Funcs/Lines 均 100%**（Coder 自报 100% 属实）。
- redis ui：`npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui`：
  **218 passed / 0 failed**（基线保持）。
- `npx vitest run src`：**4194 passed / 402 files / 0 fail**。
- `npx tsc --noEmit -p tsconfig.json`：**0 错误**。
- 上轮 B 结论抽查：`nativeContextMenu.ts` 89.8% Lines / 80% Branch、`fileCommands.ts`
  100%，均未因重构回退。

### 阶段 C 覆盖率收口（c3058fdd0..HEAD 全 diff 核心模块）
- 本轨新增/下沉 driver-sdk 模块：四 bridge 100%、resolveEditorFontFamily 100%、
  fileCommands 100%、nativeContextMenu 89.8% — 全部 ≥80%。
- 豁免维持上轮口径：`driverSettings.ts` 21%、`ipc/driverCommands.ts` 14%（薄封装，
  与迁移前基线持平）；`index.ts` 为纯 re-export barrel 无逻辑。
- redis ui 改动行均为 import 换源（baf0bb0e4 diff 复核非 import 行仅测试 harness bind），
  由 218 套件全量执行。BUG-001 置"已修复"，**全轨验收关账**。

### 判定
- **TEST_DONE(PASSED)**。留待 R 回归的 2 项真实环境 E2E 见上方登记表，不变。

## Tester 复测记录（Round 1，基准 c3058fdd0，待测 92a039383 + baf0bb0e4）

### 阶段 A 实现审查（全部通过）
- **单实现约束**：`git diff -M` 确认 driverSettings.ts（97%）/ resolveEditorFontFamily.ts（100%）/ 两个测试文件为 rename；`nativeContextMenu`/`driverCommands`/`fileCommands` 宿主侧仅剩薄再导出（或 spread 合并），全仓 Grep 函数体各仅一份。
- **bind 完备性**：`bindSettingsStore`（settingsStore.ts 末尾）、`bindConnectionStore`（connectionStore.ts 末尾）、`bindConfirmDialog`（useConfirmDialog.tsx 定义处）、`bindContextMenuBridge`（contextMenuStore.ts 末尾）均模块加载即绑定；宿主运行时 `App.tsx` 恒挂载 `WebContextMenuHost` → 桥必绑定。四处未绑定 throw 文案清晰（`<X> has not been bound to driver-sdk yet.`）。
- **fileCommands 拆分**：仅下沉驱动消费的 3 函数，宿主流式/会话命令保留，无重复实现；宿主 `file.test.ts` 经再导出对象仍 100% 覆盖 3 下沉函数转发（sdk 实现体被驱动测试执行到）。
- **hideNativeContextMenu 回归修复复核**：微任务延后 + request revision + pointerdown 可取消语义与基线逐行等价；未绑定 no-op 分支逻辑自洽（未绑定 ⇒ show 必 throw ⇒ 菜单不可能已开）。宿主 lazy-cancel 用例仍绿。
- **resolve-drivers.mjs**：改动位于生成模板字符串（`DriverSettingsContribution` import 源），非生成物；`node scripts/resolve-drivers.mjs --codegen-only --drivers=all` 重跑后 generated.ts 与现场逐字节一致（含 sdk import、无旧 `../lib/driverSettings` 引用），tsc 0 错误，工作区干净。
- **越界检查**：diff 不含 useI18n / PathInput / packages/ui。
- **验收 Grep**：`packages/drivers/*/ui` 宿主值 import 仅剩 useI18n、PathInput 与已裁决的 redisKeyWebContextMenu 夹具两行。

### 阶段 B 独立复验（实测 = 自报，全部通过）
- `npx tsc --noEmit -p tsconfig.json`：**0 错误**。
- redis ui：`npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui`：**218 passed / 0 failed**（基线保持）。
- 宿主全量：`npx vitest run`：**4470 passed / 433 files**（与 Coder 自报一致；`npx vitest run src` 过滤子集为 4194/4194，差异系 packages 下 sdk/ui/extension-points 测试不在 src 过滤器内）。
- driver-sdk 新套件存在且绿：`packages/driver-sdk/__tests__/`（3 files / 7 tests）。

### 阶段 C 覆盖率：不达标 → BUG-001
被改动 driver-sdk 核心模块行覆盖：nativeContextMenu 89.8% / resolveEditorFontFamily 100% / fileCommands 100% / driverSettings 21%（与迁移前基线持平，薄封装按配置豁免）/ **四个 bridge 仅 20%–44%，未绑定 throw、bind 转发、useBoundXxx 组件内订阅更新路径零专属测试** → 登记 `cap-bridge-BUG-001`（待修复）。

## 留待 R 回归

- [E2E] redis 键树右键菜单真实弹出：连接真实 Redis → key-browser 右键 → Web context menu 在光标处弹出（经 `showNativeContextMenu` → `bindContextMenuBridge` 新链路）；菜单弹出后立即移动/按下指针可取消（懒挂载取消语义）；Esc/点击外部关闭。前置：完整构建 + 真实 Redis 连接。
- [E2E] redis 危险操作确认对话框真实渲染：Safe Mode 开启时执行写命令 → 确认对话框（`useBoundConfirmDialog` → 宿主 `useConfirmDialog`）可见、确认/取消按钮行为正确、gate 拦截与放行结果正确。前置：完整构建 + `allowUnsafeFileOperations`/Safe Mode 两种态。

## Coder 编码记录（READY_FOR_TEST）

### A 下沉（移动实现，全仓单份）
1. `src/lib/driverSettings.ts` → `packages/driver-sdk/src/driverSettings.ts`（**移动**，纯函数；宿主消费点 JsonSchemaSettingsForm / DriverSettingsSection 改为直接 import sdk；`src/lib/__tests__/driverSettingsForm.test.ts` 迁移至 `packages/driver-sdk/__tests__/`）。
2. `src/commands/driver.ts` → `packages/driver-sdk/src/ipc/driverCommands.ts`（**移动**；宿主 `commands/driver.ts` 保留薄再导出，任务书明确要求，宿主 10+ 消费点零改动）。
3. `src/commands/file.ts`：仅下沉驱动消费的 3 个原生对话框函数（saveTextWithDialog / saveBase64WithDialog / openBase64WithDialog）→ `packages/driver-sdk/src/ipc/fileCommands.ts`；宿主 `fileCommands = { ...sdk, openTextWithDialog, beginSave..., exportTablesStream }`，流式/会话命令留宿主（**移动+合并**，单实现）。
4. `src/lib/nativeContextMenu.ts` → `packages/driver-sdk/src/nativeContextMenu.ts`（**移动 + bind**：实现依赖宿主 `contextMenuStore.showWebContextMenu/hide`，按任务书"遇 store 依赖改走 bind"——新增 `bindContextMenuBridge`；动态 import 的"懒挂载可取消"语义以微任务延后 + request revision 原样保留（宿主 `nativeContextMenu.test.ts` 的 lazy-cancel 用例仍绿）。宿主 `src/lib/nativeContextMenu.ts` 改为薄再导出（任务书允许，15 个宿主消费点零改动、零跨轨冲突面）；绑定发生在 `contextMenuStore.ts` 模块加载时。
5. `src/lib/resolveEditorFontFamily.ts` → `packages/driver-sdk/src/resolveEditorFontFamily.ts`（**移动**，无 store 依赖；宿主 2 消费点 settingsStore / editorExtensions 改 import sdk；测试迁移至 `packages/driver-sdk/__tests__/`）。

### B 能力注入（复刻 schemaStoreBridge 模式）
6. `settingsStoreBridge.ts`：`bindSettingsStore` + `useBoundSettingsStore`（callable + getState/setState，zustand 形状）；宿主 `src/stores/settingsStore.ts` 文件末尾 bind。
7. `connectionStoreBridge.ts`：`bindConnectionStore` + `useBoundConnectionStore`；宿主 `connectionStore.ts` 末尾 bind。
8. `confirmDialogBridge.ts`：`bindConfirmDialog` + `useBoundConfirmDialog`；宿主 `useConfirmDialog.tsx` 定义处 bind。
9. `showNativeContextMenu`：见 A4（下沉 + `bindContextMenuBridge`，驱动直接 import sdk 符号，store 侧 bind）。
10. 超出清单但 Grep 复核发现：`RedisWorkbench` 直接 import 宿主 `useSchemaStore` → 扩展 `schemaStoreBridge`：`BoundSchemaStore` 增加 call signature，新增导出 `useBoundSchemaStore`（宿主已有 `bindSchemaStore(useSchemaStore)` 启动注入，零新增宿主改动）。

### C 驱动侧换源
redis ui 11 个运行时代码文件（redisInvoke / ImportExport / RedisConsole / useRedisGate / SafeModeBadge / MonitorPanel / ClusterNodePicker / RedisWorkbench）全部改 `@datazen/driver-sdk` import；测试 `settings.test.ts`、`useRedisGate.test.tsx`（改为测试内 bindSettingsStore/bindConfirmDialog 注入 harness store，去除宿主 store/hook 直接依赖）、`redisKeyWebContextMenu.test.tsx`（showNativeContextMenu 换源）、`redisConsoleCompletionJourney.test.tsx`（补 harness bind）。`vitest.drivers.config.ts` alias 已存在，无需改。

### 自验
- `npx tsc --noEmit -p tsconfig.json`：**0 错误**。
- redis ui：`npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui`：**218 pass / 0 fail**（基线保持）。
- 宿主定向 vitest（nativeContextMenu / settingsStore / commands / DataTable / ExecutionStrategySelect / driver-sdk / ConnectionPage×2）全绿；**全量宿主 vitest：4470 passed / 0 failed**。
  - 修复记录：`hideNativeContextMenu` 在 bridge 未绑定时静默失效（menu 必然未打开），避免宿主单测中未加载 contextMenuStore 时的 ConnectionPage mount 崩溃（原实现靠动态 import 隐式加载）。
- Grep 残留：`packages/drivers/*/ui` 宿主值 import 仅剩 useI18n、PathInput（i18n-core 轨）+ `redisKeyWebContextMenu.test.tsx` 两处宿主集成夹具（WebContextMenuHost 渲染器 + contextMenuStore 断言/store 访问，属宿主侧 web 菜单本体，非本轨下沉清单范围，需 Tester/协调人裁决）。

## Coder round 2 修复记录（cap-bridge-BUG-001 → 待复测）

仅补单测，零运行时代码改动：

- 新增 `packages/driver-sdk/__tests__/settingsStoreBridge.test.tsx`（8）、`connectionStoreBridge.test.tsx`（5）、`confirmDialogBridge.test.tsx`（3）、`schemaStoreBridge.bound.test.tsx`（13），共 29 个用例；`packages/driver-sdk/tsconfig.json` 增补 `"jsx": "react-jsx"`（该包 `__tests__` 新增 .tsx 所需，仅类型配置）。
- 覆盖路径：三处未绑定 throw 文案断言（`vi.resetModules()` + 动态 import 隔离，与既有顶层 bind 套件互不影响）；bind 后 selector / getState 转发、settingsStore `setState` 对象与 updater 两种入参**同引用透传**断言；真实 zustand store 下 `useBoundSettingsStore` / `useBoundConnectionStore` / `useBoundSchemaStore` 组件内订阅→`act(setState)`→重渲染；`useBoundConfirmDialog` 二元组 `[confirmFn, node]` 转发与 options 原样透传（confirm/cancel 两态）；`syncSchemaTables` / `syncSchemaNamespace` / `registerPathAliases` 的 dbSessionId 有/无分支与 action 委托；`getCachedPathItems` 未绑定 optional-chain；`subscribeSchemaPathItems` 引用相同跳过 + 退订后不再通知。
- 实测覆盖率（bugs.md 重现命令，v8 + `--coverage.all`，含 nativeContextMenu/file 宿主测试 + driver-sdk 全量 53 tests）：

  | 模块 | 修复前行覆盖 | 修复后（rows / branch / stmts / funcs） |
  | ---- | ---- | ---- |
  | `confirmDialogBridge.ts` | 20% | **100% / 100% / 100% / 100%** |
  | `connectionStoreBridge.ts` | 25% | **100% / 100% / 100% / 100%** |
  | `settingsStoreBridge.ts` | 33.3% | **100% / 100% / 100% / 100%** |
  | `schemaStoreBridge.ts` | 42.3% | **100% / 100% / 100% / 100%** |

  （聚合 "All files" 仍受本轨豁免模块拖累：driverSettings 21%（薄封装、Tester 已确认与迁移前持平豁免）、`ipc/driverCommands` 14%（invoke 薄封装，同上口径）——非 BUG-001 验收项。）
- 回归验证：`npx vitest run packages/driver-sdk` 32/32 绿；`npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui` **218 pass / 0 fail**（基线保持）；`npx tsc --noEmit -p tsconfig.json` **0 错误**；宿主全量 `npx vitest run` **4499 passed / 437 files**（基线 4470/433 + 本修复 29 tests/4 files，零回归）。