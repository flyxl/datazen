# cr-p1-workflow-ui — 进度

**轨 ID：** cr-p1-workflow-ui | **分支：** feature/cr-p1-workflow-ui | **状态：** 已完成

## 范围
WorkflowPanel 复用 WorkflowForm + 共享组件；删除 ~500 行重复。

## 完成项

- [x] `WorkflowPanel` 改用 `windows/workflow/WorkflowForm`（`variant="compact"`），删除内嵌 Form/StepEditor/DatabasePicker（1425 → 407 行）
- [x] 抽取共享模块：
  - `workflowDraftConvert.ts` — definition ↔ draft 转换
  - `workflowStepResultUtils.tsx` — step 结果工具 + `StepStatusIcon`
  - `WorkflowExecutionResultPanel.tsx` — 折叠式执行结果（Panel + history 详情）
  - `WorkflowHistorySection.tsx` — `WorkflowHistoryList`（Page）+ `WorkflowHistoryTab`（Panel）
- [x] `WorkflowPage` 复用上述共享组件/工具（1445 → 1290 行）
- [x] `WorkflowForm` 增加 `variant: 'page' | 'compact'` 以适配 AI 面板
- [x] vitest：`WorkflowPanel.test.tsx` 16 项 + `WorkflowForm` / `WorkflowPage` 回归 45 项全绿

## 验收

```bash
npx vitest run src/components/ai/__tests__/WorkflowPanel.test.tsx \
  src/windows/workflow/__tests__/WorkflowForm.test.tsx \
  src/windows/workflow/__tests__/WorkflowPage.test.tsx
```
