# Track `ui-transfer-guards` — progress

> Initiative: UI/UX Enhancement
> Plan: `docs/development/coordination/ui-enhancement-plan.md`（Wave 1）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/ui-enhancement`

## 范围

P0-2, P0-3：DataTransferWindow 预览错误态与空表禁用守卫
- P0-2：当 preview 失败或 preview 为 null 时，预览步骤不要只留空白容器，必须展示错误状态、说明并提供“重新生成预览”与“返回映射步骤”的按钮。
- P0-3：在 `objects` 步骤，如果 `tables.length === 0`，必须禁止点击“下一步”继续前进，并提供明确的空表提示（如无可用表或未能获取到表，建议检查源端连接与数据库）。同时如果用户尝试继续时应有明确守卫。

## 状态

- Phase: PASSED
- 编码 commit: 6815ff687
- 测试 commit: （见下方测试复验提交）
- 合并 commit: 待合入

## 心跳

- 2026-09-04 轨道初始化（BOOTSTRAP）
- 2026-09-04 编码接管：P0-2/P0-3 实现与单测补齐，自验通过 → READY_FOR_TEST
- 2026-09-04 测试复验：独立重跑 vitest/tsc、代码审查、覆盖率 ≥80% → PASSED

## 自验结果（编码代理）

- `npx vitest run src/windows/data-transfer` → exit 0 ✅（11 tests passed）
- `npx tsc --noEmit` → exit 0 ✅

## 复验结果（测试代理）

- `node scripts/generate-builtin-locales.mjs` → exit 0 ✅
- `npx vitest run src/windows/data-transfer` → exit 0 ✅（11 tests passed，独立实测与编码自报一致）
- `npx tsc --noEmit` → exit 0 ✅
- `DataTransferWindow.tsx` 行覆盖率：**80.44%**（268/348 stmts，251/312 lines）✅

### 实现摘要

- **P0-3**：`canNext` 在 `objects` 步要求 `tables.length > 0`；空表时展示 `data-transfer-objects-empty` 提示与 `data-transfer-reinspect` 重新检测按钮。
- **P0-2**：`preview` 步 `!loading && !preview` 时展示 `data-transfer-preview-error` 错误面板（CopyableError + 重试/返回映射按钮）；`runPreview({ quiet: true })` 从 mapping 进入 preview 时静默失败并留在 preview 步展示内联错误态。

### 代码审查摘要

- P0-3：`canNext` `objects` 分支由 `tables.length === 0 || …` 改为 `tables.length > 0 && tables.some(…)`，空表时 Next 禁用；空态 UI 与 re-inspect 回调正确。
- P0-2：`runPreview` 增加 `quiet` 选项与 `previewError` 状态；mapping→preview 始终进入 preview 步并在失败时展示内联错误态；重试/返回映射按钮行为与单测一致。
- 未覆盖路径（非 P0 阻断）：`runPreview` 非 quiet 失败弹窗、objects 加载 spinner、全部表 disabled 时 Next 禁用（已有逻辑，非本次 Bug 范围）。

## E2E 登记

| 用例 | 前置 | 执行点 | 状态 |
|------|------|--------|------|
| objects 空表禁用 Next + 重新检测 | DataTransfer 向导 | 单元测试覆盖 `[tester]` | ✅ 单测已覆盖 |
| preview 失败错误态 + 重试/返回 | DataTransfer 向导 | 单元测试覆盖 `[tester]` | ✅ 单测已覆盖 |

> Host E2E 未新增：DataTransfer 为独立子窗口，现有 Host E2E 矩阵未包含该路径；P0 守卫已由 `DataTransferWindow.test.tsx` 11 项单测覆盖，留待 R 回归时可补 `e2e/specs/` 旅程。
