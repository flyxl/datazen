# Track: cap-bridge — Bug 清单

## cap-bridge-BUG-001：新增能力桥模块缺少专属单测（未绑定 throw / bind 转发 / useBoundXxx 订阅路径）

- **状态**: 已修复（Tester round 2 复测通过，commit 3908f64e5）
- **复测记录（Tester round 2）**: 阶段 A 审查 `git diff 4cf33a4c6..3908f64e5` 确认仅新增 4 个
  bridge 测试套件 + `packages/driver-sdk/tsconfig.json` 一行 `"jsx": "react-jsx"`，零运行时改动；
  四套件断言与 bridge 源码逐字核对属实（throw 文案、setState 同引用透传、真实 zustand
  订阅重渲染、confirm 二元组透传、dbSessionId 分支、引用相同跳过），无凑数空断言。
  以 bugs.md 重现命令实测：confirmDialogBridge / connectionStoreBridge /
  settingsStoreBridge / schemaStoreBridge 四项 Stmts/Branch/Funcs/Lines 均 **100%**（≥80% 达标）。
  回归：driver-sdk 36/36（Coder 自报 32/32 系计数口径偏差，实测 7 files/36 tests 全绿：
  29 新增 + 既有 driverSettingsForm 2 / resolveEditorFontFamily 3 / pathItemsCache 2）；
  redis ui 218/218；`npx vitest run src` 402 files / 4194 passed；`tsc --noEmit -p tsconfig.json` 0 错误；
  nativeContextMenu 89.8% Lines / fileCommands 100% 未回退。
- **修复记录（Coder round 2）**: `packages/driver-sdk/__tests__/` 新增四个专属套件
  `settingsStoreBridge.test.tsx`（8 tests）/ `connectionStoreBridge.test.tsx`（5）/
  `confirmDialogBridge.test.tsx`（3）/ `schemaStoreBridge.bound.test.tsx`（13），
  以 `vi.resetModules()` + 动态 import 隔离未绑定态：覆盖三个 throw 文案、
  selector/getState/setState（对象与 updater 同引用透传）转发、真实 zustand
  store 下 `useBoundXxx` 组件内订阅重渲染、confirm 二元组透传，及
  schemaStoreBridge sync 助手（含 dbSessionId 有/无两分支）与
  `subscribeSchemaPathItems` 引用相同跳过分支。实测（bugs.md 重现命令）：
  四个 bridge 行/语句/分支/函数覆盖率均 **100%**。
- **量级**: 中（不阻断运行时行为，阻断本轨覆盖率验收硬标准）
- **描述**:
  `packages/driver-sdk/src/` 下四个 bridge 模块为本轨新增/扩展的核心运行时代码，但全仓没有任何针对它们的专属单测。实测行覆盖率（v8，`--coverage.all`，仅统计现有全部相关测试执行结果）：
  | 模块 | 行覆盖 | 未覆盖关键路径 |
  | ---- | ------ | -------------- |
  | `confirmDialogBridge.ts` | 20% | L33-40：`useBoundConfirmDialog` 未绑定 throw（`'ConfirmDialog has not been bound to driver-sdk yet.'`）与绑定后 hook 转发 |
  | `connectionStoreBridge.ts` | 25% | L28-35：`getStore` 未绑定 throw；L44-46：`useBoundConnectionStore` selector 转发 / `getState` 转发 |
  | `settingsStoreBridge.ts` | 33.3% | L32-35：未绑定 throw；L50-60：`useBoundSettingsStore` 的 selector 调用、`getState`、`setState`（对象与 updater 两种形态）转发 |
  | `schemaStoreBridge.ts`（新增 call signature + `useBoundSchemaStore`） | 42.3% | L46-93：`useBoundSchemaStore` 钩子/`getState` 转发路径 |

  驱动侧 harness（`useRedisGate.test.tsx`、`redisConsoleCompletionJourney.test.tsx`）只在模块顶层 bind 假 store 走通了 happy path，且无法覆盖 throw 分支（bind 后同文件内无法回到未绑定态）。
- **重现步骤**:
  ```bash
  npx vitest run src/lib/__tests__/nativeContextMenu.test.ts src/commands/__tests__/file.test.ts \
    packages/driver-sdk --coverage --coverage.provider=v8 --coverage.all \
    --coverage.include='packages/driver-sdk/src/**/*.ts' --coverage.reporter=text
  ```
  查看四个 bridge 文件的 % Lines 均 <80%。
- **影响范围**: 三个 `useBoundXxx` 访问器与 `useBoundSchemaStore` 是全部驱动 UI 读取宿主 store 的唯一通道；若未绑定错误路径或 zustand 转发语义（setState partial/updater、call signature 订阅）被后续重构破坏，现有测试完全无法拦截。
- **期望**: Coder 在 `packages/driver-sdk/__tests__/` 新增四个 bridge 的专属测试（建议文件名 `settingsStoreBridge.test.ts` / `connectionStoreBridge.test.ts` / `confirmDialogBridge.test.ts` / `schemaStoreBridge.bound.test.ts`），至少覆盖：
  1. 未 bind 时调用访问器/hook throw，断言错误信息文案（用 `vi.resetModules()` 隔离模块态，防止与其他在顶层 bind 的套件互相污染）；
  2. bind zustand 真实 store 后 selector 调用返回正确切片、`getState()` 转发正确、settingsStore 的 `setState` 对象与 updater 两种入参均转发到位；
  3. `useBoundXxx` 在 React 组件内订阅更新路径（rendering component 收到 store 变更重渲染，可用 `@testing-library/react` + 驱动测试同款 harness 形状）；
  4. `useBoundConfirmDialog` bind 假 hook 后返回 `[confirmFn, node]` 二元组并透传调用。
  目标：四个 bridge 模块行覆盖率 ≥80%。

---

## 已裁决非 Bug 事项（Tester 复核确认）

- `redisKeyWebContextMenu.test.tsx` 保留宿主 `WebContextMenuHost` + `contextMenuStore` 夹具（驱动侧宿主集成夹具，归后续组件下沉里程碑）— 协调者已裁决，维持。
- `hideNativeContextMenu` 在 bridge 未绑定时静默 no-op — 复核合理：未绑定则 `showNativeContextMenu` 必然 throw，菜单不可能已打开；该修复保障宿主未加载 contextMenuStore 的测试环境（ConnectionPage mount）不崩，与"懒挂载可取消"基线语义（revision + 微任务 + pointerdown 取消）一致，宿主 `nativeContextMenu.test.ts` lazy-cancel 用例仍绿。
- `confirmDialogBridge.ts` 中 `ConfirmDialogOptions` 与宿主 `useConfirmDialog.tsx` 的 `ConfirmOptions` 结构重复（靠结构类型兼容）— 非本轨缺陷，登记为观察项：后续里程碑建议宿主改为复用 sdk 类型，消除双定义漂移风险。
