# Track `ui-accent-tokens` — progress

> Initiative: UI/UX Enhancement
> Plan: `docs/development/coordination/ui-enhancement-plan.md`（Wave 2）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/ui-enhancement`

## 范围

统一强调色到 `accent` token：
- 清理 `focus:border-blue-500`、`focus:ring-blue-500`、`text-blue-500` 等硬编码为 `focus:border-accent` / `focus:ring-accent` / `text-accent`。
- 危险色与 Badge 统一走 `--c-danger` / `--c-warning` / `--c-success` 等语义 token。
- 确保换肤和暗色模式下焦点和高亮视觉统一。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: c428b0c8c
- 测试 commit: 待测试
- 合并 commit: 待合入

## 自验结果（Coder）

| 套件 | 结果 |
|------|------|
| `npx tsc --noEmit` | 无类型报错 ✅ |
| `npx vitest run src/components src/windows/settings src/windows/connection src/windows/data-sync src/windows/schema-diff src/windows/backup` | **100 files / 712 tests** ✅ |

## 改动摘要

- **输入框焦点**：`IndexesView`、`ImportDialog`、`ConnectionSettingsDialog`、`DocumentConnectionView` 等 `focus:border-accent` / `focus:ring-accent/25`。
- **AI 组件**：`AiChatPanel`、`WorkflowChatPanel` 用户气泡 `bg-accent/20`；`ExplainPanel` 严重度 low → accent、high/medium → danger/warning；`DiagnosisPanel`、`AiCodeBlock` accent 化。
- **连接/备份/工作流**：选中态、Tab 指示条、resize handle、Badge/Tag 统一 accent token。
- **Badge 组件**：success/warning/danger tone 改用语义 token（`bg-success/10` 等）。
- **测试**：`BackupWindow.test.tsx` 选中态断言 `bg-blue-500/20` → `bg-accent/20`。

## 心跳

- 2026-09-04 轨道初始化
- 2026-09-04 CODING 完成：commit c428b0c8c，27 文件 accent token 替换，自验通过，待 Tester 复测
