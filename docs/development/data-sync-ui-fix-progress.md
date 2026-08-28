# Data Sync UI 修复进度

> 来源：[data-sync-ui-review.md](./data-sync-ui-review.md)  
> 流程：[subagent-dev-playbook.md](./subagent-dev-playbook.md)

## 功能总览

| 编号 | 功能 | 状态 | 编码 commit | 测试 commit |
|------|------|------|-------------|-------------|
| F1 | 核心状态机（H1/H2/H4/M1/M2/M8/M9/M10/L1/L8 + rolledBack） | 完成 | 4dbe2efd | — |
| F2 | 子组件 UX/a11y（M3–M7/L4/L5/L7 + 只读 banner + tabs） | 完成 | 0fbb120f | — |
| F3 | 死代码清理（H3/L2 删除未接入组件 + i18n prune） | 完成 | 26ae3373 | — |
| F4 | 测试与 E2E（H5/L6 + 可测试性改进） | 完成 | — | （本 commit） |

## Bug 台账

| Bug ID | 所属 | 描述 | 状态 | 记录时间 |
|--------|------|------|------|----------|
| — | — | — | — | — |

## 测试约定

- 功能轮：`npx vitest run src/windows/data-sync` + `npx tsc --noEmit`
- R 阶段：`pnpm e2e:skip-build -- --spec e2e/specs/journeys/data-sync-journey.ts` + edge-cases + 新增只读 case

## E2E 运行记录

- **R 阶段（2026-08-28）**：`CI=true node e2e/run.mjs --minimal-drivers` 重建后
  - `data-sync-edge-cases.ts` — **15 passing**
  - `data-sync-journey.ts` — **3 passing**

---

## F1 — 核心状态机

**范围**：`DataSyncWindow.tsx` — Cancel reset、端点变更 reset、execute done、错误反馈、session 失败、SQL preview fallback、testid/state 属性、rolledBack 文案、copy report 反馈、fallback 提示

**验收**：
- [x] H1 handleCancel 恢复 syncState
- [x] H2 source/target 变更 resetCompareState
- [x] H4 execute 成功 banner
- [x] M1 库加载失败提示
- [x] M2 session 失败 inline/禁用 Compare
- [x] M8 copy report 反馈
- [x] M9 SQL preview empty state
- [x] M10 data-sync-window + data-sync-state
- [x] L1 移除硬编码版本
- [x] L8 apply fallback 用户可见提示
- [x] rolledBack 专用 i18n

## F2 — 子组件 UX/a11y

**范围**：`EndpointsBar` `DiffDetail` `MappingPanel` `OptionsBar` `TableListPanel` `CompareSummary` `DataSyncWindow` tabs

**验收**：
- [x] M3 DiffDetail page reset
- [x] M4 列名 header（或 tooltip）
- [x] M5 incompatible reason 布局
- [x] M6 tablist + aria-pressed filters
- [x] M7 库 Select label
- [x] L4 MappingPanel stable key
- [x] L5 OptionsBar hint
- [x] L7 search aria-label
- [x] 只读目标 EndpointsBar banner

## F3 — 死代码清理

**验收**：
- [x] 删除 SavedTasksBanner / SyncProgressPanel / ResumeSyncDialog
- [x] 清理仅被上述组件使用的 i18n 键
- [x] 保留仍被 IPC/其他模块引用的 sync task 键

## F4 — 测试与 E2E

**验收**：
- [x] H5 只读目标 E2E（DS-EDGE-013）
- [x] execute done E2E 断言（DS-EDGE-014 + journey helper）
- [x] DS-EDGE-012 强化（data-sync-state + Compare 可点）
- [x] 端点变更清除 mapping E2E（DS-EDGE-015）
- [x] DataSyncWindow 单测（cancel/reset/read-only/explain mock）
- [x] filter/tab testid（F2 已落地）
- [x] e2e-coverage.md 更新
