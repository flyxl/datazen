# Data Transfer UI 重构 — 进度台账

> 协调者维护总览与 bug 台账；各轨道代理只写本功能小节。  
> 规格：[data-transfer-ui-redesign.zh-CN.md](../features/data-transfer-ui-redesign.zh-CN.md)

## 1. 功能总览

| 编号 | 功能 | 轨道 | 状态 | 编码 commit | 测试 commit |
|------|------|------|------|-------------|-------------|
| F1 | 能力限制弹窗（打开窗口 + 不再提示） | transfer-ui-window | 编码完成 | ba1ffb8a | — |
| F2 | 6 步向导 UI 重构（token / stepper / Preview+Execute） | transfer-ui-window | 编码完成 | ba1ffb8a | — |
| F3 | Mapping 子组件 token 对齐 | transfer-ui-mapping | 编码完成 | fc869f28 | — |
| F4 | Host 单元测试适配 | transfer-ui-tests | 测试中 | — | — |
| F5 | Host E2E 适配 | transfer-ui-e2e | 编码完成 | — | — |

## 2. Bug 台账

| Bug ID | 所属 | 描述 | 状态 | 记录时间 |
|--------|------|------|------|----------|
| — | — | — | — | — |

## 3. 测试约定

- Host 单测：`npx vitest run src/windows/data-transfer`
- 相关 lib：`npx vitest run src/lib/transferLimitationsPrefs`
- E2E：`e2e/specs/data-transfer-window.ts`、`data-transfer-diverse-types.ts` 及引用旧 step id 的 spec
- 步骤 id 迁移：`mode`/`options`/`execute` → `setup`；Execute 在 Preview 底栏

## 4. 功能小节

### F1+F2 transfer-ui-window

- **范围**：`DataTransferWindow.tsx`、`TransferLimitationsDialog.tsx`、`transferLimitationKeys.ts`、`transferLimitationsPrefs.ts`、`en.ts`/`zh-CN.ts`（transfer.* keys）
- **验收**：
  - [ ] 打开窗口时弹 limitations Dialog（含关闭 + 不再提示）
  - [ ] 未勾选不再提示时，下次打开仍弹窗
  - [ ] 6 步：endpoints / setup / objects / mapping / preview / result
  - [ ] Preview 底栏 Execute，无空 Execute 页
  - [ ] `bg-surface` / `border-edge`，stepper 居中
  - [ ] 保留/迁移 `data-testid`（含 `data-transfer-step-setup`）

### F3 transfer-ui-mapping

- **范围**：`TransferMappingStep.tsx`、`ColumnMappingEditor.tsx` — 仅 token（border-edge / bg-surface）
- **验收**：Transfer 目录内无 `border-border` / `bg-bg` 残留（mapping 文件）

### F4 transfer-ui-tests

- **范围**：`DataTransferWindow.test.tsx`、可选 `transferLimitationsPrefs` 单测
- **验收**：vitest 全绿；覆盖 limitations dialog + setup 步路径

### F5 transfer-ui-e2e

- **范围**：`e2e/helpers.ts`（dismiss dialog）、`data-transfer-window.ts`、`data-transfer-diverse-types.ts`、journey specs
- **验收**：step id 与 dialog 断言更新；wizard 闭环路径仍可达 execute
- **回归**：【留待 R 回归】本机未跑完整 `pnpm e2e`
