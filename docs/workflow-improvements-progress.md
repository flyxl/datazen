# Workflow Improvements Progress

## Branch: `feat/workflow-improvements`

## Features

| ID | Description | Status | Unit Tests | E2E Tests | Notes |
|----|------------|--------|------------|-----------|-------|
| FEAT-001 | WorkflowWindow 统一 tab 系统（参考 ConnectionWindow） | ✅ 已完成 | ✅ 通过 | ✅ 通过 (9/9) | ExecutionTab/HistoryTab 类型，ConnectionWindow 风格 tab bar，执行失败也创建 error tab |
| FEAT-002 | E2E 测试自包含（创建测试数据库和 workflow） | ✅ 已完成 | N/A | ✅ 通过 (8/8) | chart-expand 用实际表+清理，workflow-window 用 IPC 创建测试 workflow |

## Bug Fixes

| ID | Description | Status | Notes |
|----|------------|--------|-------|
| FIX-001 | 去掉 workflow 窗口中切换主题的按钮 | ✅ 已修复 | 移除 ThemeToggle import 和 TitleBar rightContent |
| FIX-002 | workflow 中点击打开文件夹按钮无效 | ✅ 已修复 | 用 Tauri invoke `open_in_explorer` 替代不存在的 plugin-shell |
| FIX-003 | 去掉图表预览弹窗的关闭按钮（与还原按钮重复） | ✅ 已修复 | 移除 ChartExpandOverlay 中的 X 按钮 |
| FIX-004 | Workflow 执行失败时右侧面板无反馈 | ✅ 已修复 | handleExecute 在 workflowError 时也创建 error tab 显示错误信息 |
| FIX-005 | 删除旧的 workflow-tabs.ts E2E 测试文件 | ✅ 已完成 | 使用了已删除的 i18n.ts 和不存在的 DOM 选择器 |

## Documentation

| ID | Description | Status |
|----|------------|--------|
| DOC-001 | 更新系统架构文档 | 🔄 待完成 |
| DOC-002 | 更新 AGENTS.md | 🔄 待完成 |

## Unit Test Results

- 日期: 2026-08-04
- 总计: 165 passed / 19 test files
- 全部通过 ✅

## E2E Test Results

### workflow-window.ts (2026-08-04)

| Test Case | Result | Duration |
|-----------|--------|----------|
| 主窗口应包含工作流入口按钮 | ✅ PASS | — |
| 点击工作流按钮应打开新窗口 | ✅ PASS | — |
| Workflow 窗口应显示 Workflows 和执行记录标签 | ✅ PASS | — |
| 应显示测试 workflow 在列表中 | ✅ PASS | — |
| 选择 workflow 后应显示执行按钮 | ✅ PASS | — |
| 执行后应在右侧打开结果 tab | ✅ PASS | — |
| 结果 tab 应显示步骤子导航 | ✅ PASS | — |
| 关闭结果 tab 应回到空状态 | ✅ PASS | — |
| 切换到执行记录标签应可查看历史 | ✅ PASS | — |

**Summary**: 9 passing (35.9s), 0 failing

### chart-expand.ts (2026-08-04)

| Test Case | Result | Duration |
|-----------|--------|----------|
| 应能切换到图表视图 | ✅ PASS | — |
| 应显示放大按钮 | ✅ PASS | — |
| 点击放大按钮应打开全屏图表覆盖层 | ✅ PASS | — |
| 放大视图应包含图表画布 | ✅ PASS | — |
| 放大视图应有导出按钮 | ✅ PASS | — |
| 按 ESC 应关闭放大视图 | ✅ PASS | — |
| 点击关闭按钮也应关闭放大视图 | ✅ PASS | — |
| 切回表格视图后放大按钮不可见 | ✅ PASS | — |

**Summary**: 8 passing (15.8s), 0 failing

## Bug List (E2E 发现)

暂无
