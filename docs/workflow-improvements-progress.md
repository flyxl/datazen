# Workflow Improvements Progress

## Branch: `feat/workflow-improvements`

## Tasks

### Bug Fixes

| ID | Description | Status | Notes |
|----|------------|--------|-------|
| FIX-001 | 去掉 workflow 窗口中切换主题的按钮 | done | 移除 ThemeToggle import 和 TitleBar rightContent |
| FIX-002 | workflow 中点击打开文件夹按钮无效 | done | 用 Tauri invoke `open_in_explorer` 替代不存在的 plugin-shell |
| FIX-003 | 去掉图表预览弹窗的关闭按钮（与还原按钮重复） | done | 移除 ChartExpandOverlay 中的 X 按钮 |

### New Features

| ID | Description | Status | Notes |
|----|------------|--------|-------|
| FEAT-001 | workflow 窗口中添加 tab 页，每个 workflow 执行结果在独立 tab 中显示 | pending | |

### Documentation

| ID | Description | Status | Notes |
|----|------------|--------|-------|
| DOC-001 | 更新系统架构文档 | pending | |
| DOC-002 | 更新 AGENTS.md | pending | |

## Test Results

### Bug Fixes E2E Test (bugfix-verification.ts)

| Test Case | Result |
|-----------|--------|
| FIX-001: Workflow 窗口不应包含主题切换按钮 | ✅ PASS |
| FIX-002: Workflow 窗口打开文件夹按钮应存在 | ✅ PASS |
| FIX-003: 图表预览弹窗不应有关闭(X)按钮 | ✅ PASS |

**Summary**: 3 passing (18.1s), 0 failing
