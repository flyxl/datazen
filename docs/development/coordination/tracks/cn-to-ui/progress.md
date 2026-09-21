# Track: cn-to-ui — 驱动 UI 的 cn 全部改从 @datazen/ui 导入

- 分支: `feature/cn-to-ui`（基准 `feat/driver-decoupling` @ 040e15bde）
- 角色: Coder → Tester

## 背景

`packages/ui/src/index.ts` 已导出 `cn`（@datazen/ui 是宿主与驱动共享的基础组件库）。但各驱动 UI 目前仍直接 import 宿主私有模块 `../../../../../src/lib/cn`（redis 约 16 处，另有 sqlserver 等），违反"驱动不得引用 host 代码"约束。本轨是解耦方案第 1 步（机械替换，冲突面最小）。

## 范围

1. 将 `packages/drivers/*/ui/**` 中所有 `from '.../src/lib/cn'`（任意相对深度形态）改为 `from '@datazen/ui'`。
2. 若某驱动 ui 的测试里有 `vi.mock('...src/lib/cn')` 之类字符串路径，同步更新。
3. 检查 `packages/ui` 的 `cn` 行为与 `src/lib/cn.ts` 完全一致（clsx + tailwind-merge）；若 `src/lib/cn.ts` 有特殊行为差异，停下上报 BLOCKED。

## 禁止事项（防跨轨冲突）

- **不改 `src/lib/cn.ts` 本体、不动宿主 `src/**` 的任何 import**（宿主侧收敛属于后续轨道）。
- 不动 `packages/ui/src/**`。
- 不动 driver-sdk 相关类型（types-to-sdk 轨的范围）。

## 验收标准

- `grep`（Grep 工具）在 `packages/drivers/*/ui` 下检索 `src/lib/cn` 结果为 0。
- `npx tsc --noEmit -p tsconfig.json` 通过。
- `npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui` 维持基线 213 pass / 5 fail（本轨不负责修红）。

## 状态

- [x] Coder 完成 → READY_FOR_TEST
- [x] Tester 复测 → TEST_DONE(PASSED)

## Tester 复测记录（2026-09-20，全新实例）

- **阶段 A 实现审查**：`git diff 040e15bde..HEAD` 共 16 个 redis ui 文件，逐文件核实改动仅限 cn import 行（并入既有 `@datazen/ui` import 或原位替换），无夹带逻辑/JSX/其他 import 变更；`src/lib/cn.ts`、宿主 `src/**`、`packages/ui/src/**` 零改动（diff 文件清单除 progress.md 外仅 `packages/drivers/redis/ui/**`）。行为一致性核实：`packages/ui/src/cn.ts` = clsx + twMerge；`src/lib/cn.ts` 已是 `export { cn } from '@datazen/ui'` 再导出，无差异，无需 BLOCKED。
- **阶段 B 独立复跑**：
  - Grep `packages/drivers` 下 `src/lib/cn` = 0 命中。
  - `npx tsc --noEmit -p tsconfig.json`：0 错误。
  - `npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui`：**213 passed / 5 failed（26 文件：24 pass / 2 fail）**，与基线完全一致。5 红分布：stringKeyValue.test.ts ×3（tryDecompressString gzip/zlib 环境性失败）、redisWorkbench.test.tsx ×2（Tauri invoke 未 mock 报 `Cannot read properties of undefined (reading 'invoke')` + set_string 路由断言漂移）；均与本轨 cn 替换无关，归 fix-redis-tests 轨。
- **阶段 C 覆盖率**：纯 import 替换、零新增逻辑分支，无需补测试；Grep `vi.mock` 含 `lib/cn` 字符串 = 0 命中，无遗漏 mock。
- **阶段 D E2E 登记**：见下方"留待 R 回归"。
- 结论：**TEST_DONE(PASSED)**，待测 commit `009461a17`。

## 留待 R 回归

- 【留待 R 回归】redis 工作区视觉回归：打开 redis 连接工作区，确认带 `cn()` 类名拼接的组件渲染正常——重点 `SearchModeTabs` 激活/非激活 tab 高亮样式、`KeyTreeList` 行 hover 态、`RedisConsole` 补全弹出层（`CompletionPopup`）样式；前置条件：webdriver 构建 + redis 示例库。

## Coder 自验记录（2026-09-20）

- 替换范围：`packages/drivers/redis/ui/**` 共 16 个文件，全部 `from '.../src/lib/cn'` 改为 `from '@datazen/ui'`（已有 `@datazen/ui` import 的合并为大括号导入，否则原位替换）；diff 仅含 import 行。
- `packages/drivers/*/ui` 下 Grep `src/lib/cn` = 0；测试中无 `vi.mock('...src/lib/cn')` 残留（`lib/cn` 全仓库 drivers 下 0 命中）。
- 行为一致性：`packages/ui/src/cn.ts` = clsx + twMerge，且 `src/lib/cn.ts` 本身已是 `export { cn } from '@datazen/ui'` 的再导出，无差异。
- `npx tsc --noEmit -p tsconfig.json`：通过（0 错误）。
- `npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui`：213 passed / 5 failed（与基线一致，5 红为 stringKeyValue/decode 既有失败，属 fix-redis-tests 轨）。
