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

- Phase: TEST_DONE
- 编码 commit: c428b0c8c
- 测试 commit: 待提交
- 合并 commit: 待合入

## 自验结果（Coder）

| 套件 | 结果 |
|------|------|
| `npx tsc --noEmit` | 无类型报错 ✅ |
| `npx vitest run src/components src/windows/settings src/windows/connection src/windows/data-sync src/windows/schema-diff src/windows/backup` | **100 files / 712 tests** ✅ |

## 复验结果（Tester）

| 套件 | Coder 自报 | Tester 独立实测 |
|------|-----------|----------------|
| `npx tsc --noEmit` | 通过 | 通过 ✅ |
| `npx vitest run`（同上 scope） | 100 / 712 | **101 / 717** ✅（+Badge.test.tsx 5 用例；ExplainPanel severity 断言增强） |

### 代码审查摘要

- **27 文件替换规范**：`focus:border-accent` / `focus:ring-accent/25`、`text-accent`、`bg-accent/*`、Badge `success/warning/danger/accent` 语义 token 一致。
- **Checkbox 焦点**：`SshTunnelFields` 等改用项目既有 `accent-accent` 模式（与 `ConnectionSettingsDialog` 等一致）。
- **McpClientSection**：补全 `focus:ring-2 focus:ring-accent/25`，与 Settings 其他输入框对齐。
- **ExplainPanel 严重度**：high/medium/low → danger/warning/accent，映射合理。
- **遗留 blue 硬编码**：生产代码中 `focus:border-blue-500` / `focus:ring-blue-500` / `bg-blue-500` 已清零；`text-blue-400` 仍存在于 schema 树图标色（本轨范围外，非 accent 强调色场景）。
- **DiffDetail / CompareSummary**：UPDATE 已 accent 化；INSERT/DELETE 仍用 green/red 硬编码（本轨仅覆盖 blue→accent，属预期 partial）。

### 覆盖率（改动核心模块）

| 文件 | 行覆盖 | 说明 |
|------|--------|------|
| `Badge.tsx` | 100% | 新增 `[tester] Badge.test.tsx` 覆盖全部 5 tone |
| `ExplainPanel.tsx` | ~95%+ | 既有测试 + severity token 类断言 |
| 其余 25 文件 | CSS 类替换 | 无逻辑分支；`BackupWindow.test.tsx` 等已有 `bg-accent/20` 断言 |

## 改动摘要

- **输入框焦点**：`IndexesView`、`ImportDialog`、`ConnectionSettingsDialog`、`DocumentConnectionView` 等 `focus:border-accent` / `focus:ring-accent/25`。
- **AI 组件**：`AiChatPanel`、`WorkflowChatPanel` 用户气泡 `bg-accent/20`；`ExplainPanel` 严重度 low → accent、high/medium → danger/warning；`DiagnosisPanel`、`AiCodeBlock` accent 化。
- **连接/备份/工作流**：选中态、Tab 指示条、resize handle、Badge/Tag 统一 accent token。
- **Badge 组件**：success/warning/danger tone 改用语义 token（`bg-success/10` 等）。
- **测试**：`BackupWindow.test.tsx` 选中态断言 `bg-blue-500/20` → `bg-accent/20`；Tester 增补 `Badge.test.tsx`、ExplainPanel severity 断言。

## 心跳

- 2026-09-04 轨道初始化
- 2026-09-04 CODING 完成：commit c428b0c8c，27 文件 accent token 替换，自验通过
- 2026-09-04 TEST_DONE：Tester 独立复验通过，无 Bug 登记
