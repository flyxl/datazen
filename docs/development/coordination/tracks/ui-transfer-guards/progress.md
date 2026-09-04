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

- Phase: READY_FOR_TEST
- 编码 commit: 6815ff687
- 测试 commit: 待测试
- 合并 commit: 待合入

## 心跳

- 2026-09-04 轨道初始化（BOOTSTRAP）
- 2026-09-04 编码接管：P0-2/P0-3 实现与单测补齐，自验通过 → READY_FOR_TEST

## 自验结果

- `npx vitest run src/windows/data-transfer` → exit 0 ✅（11 tests passed）
- `npx tsc --noEmit` → exit 0 ✅

### 实现摘要

- **P0-3**：`canNext` 在 `objects` 步要求 `tables.length > 0`；空表时展示 `data-transfer-objects-empty` 提示与 `data-transfer-reinspect` 重新检测按钮。
- **P0-2**：`preview` 步 `!loading && !preview` 时展示 `data-transfer-preview-error` 错误面板（CopyableError + 重试/返回映射按钮）；`runPreview({ quiet: true })` 从 mapping 进入 preview 时静默失败并留在 preview 步展示内联错误态。

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| objects 空表禁用 Next + 重新检测 | DataTransfer 向导 | 测试代理 |
| preview 失败错误态 + 重试/返回 | DataTransfer 向导 | 测试代理 |
