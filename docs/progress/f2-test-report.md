# F2 测试报告 — 主工作区 Window→Page 重命名

> 分支：`feat/main-window-pages`  
> 编码 commit：`2b91921d`  
> 关联工作项：F2

## 重命名完整性扫描

全库搜索 `ConnectionWindow` / `WorkflowWindow` / `MainWindow`（组件级引用），排除：子窗口组件（`NewConnectionWindow` 等）、`open*Window` API、`focusMainWindow`、E2E helper 函数名、Rust OS 窗口注释、历史 PRD（`docs/prd/**`）、F1 测试报告快照。

| 检查项 | 结果 |
|--------|------|
| `src/**/*.tsx` 中 `\b(ConnectionWindow\|WorkflowWindow\|MainWindow)\b` | **0 命中** |
| 旧文件 `MainWindow.tsx` / `ConnectionWindow.tsx` / `WorkflowWindow.tsx` | **已删除** |
| 新文件 `MainPage.tsx` / `ConnectionPage.tsx` / `WorkflowPage.tsx` | **存在** |
| `App.tsx` lazy import | **MainPage** ✓ |
| 架构文档 / AGENTS.md | **已同步**（编码 commit） |
| E2E 源码路径断言 | **指向 `*Page.tsx`**（`workflow-window.ts`、`i18n-10-locales.ts`、`app-data-backup.ts`） |

**结论**：无应改未改的组件级漏改。

## E2E 用例清单（主窗口导航 / Page 重命名相关）

| ID | 规格文件 | 步骤 | 期望 |
|----|----------|------|------|
| F2-E2E-001 | `e2e/specs/main-window.ts` | 打开主窗口，等待加载 | `[data-testid="workspace-nav-connections"]` / `workflow` / `dashboard` 均可见 |
| F2-E2E-002 | `e2e/specs/unified-main-window.ts` | 点击 `workspace-nav-workflow` | `[data-testid="workflow-workspace"]` 可见 |
| F2-E2E-003 | `e2e/specs/unified-main-window.ts` | 点击 `workspace-nav-dashboard` | `[data-testid="dashboard-panel"]` 可见 |
| F2-E2E-004 | `e2e/specs/unified-main-window.ts` | 点击 `workspace-nav-connections` | `[data-testid="connection-workspace-home"]` 可见 |
| F2-E2E-005 | `e2e/specs/homepage-features.ts` HOME-001 | 主窗口加载后 | 三栏工作区导航均可见 |
| F2-E2E-006 | `e2e/specs/settings.ts` F1-E2E-002 回归 | Settings 返回后 | 工作区导航（连接/工作流/看板）重新可见 |
| F2-E2E-007 | `e2e/specs/workflow-window.ts` | 读 `WorkflowPage.tsx` 源码 | 含 `openWorkflowsDir` 或 `open_workflows_dir` 接线 |
| F2-E2E-008 | `e2e/specs/i18n-10-locales.ts` | 读 `ConnectionPage.tsx` 源码 | 路径为 `src/windows/connection/ConnectionPage.tsx` |
| F2-E2E-009 | `e2e/specs/app-data-backup.ts` | 读连接页源码路径 | 解析至 `ConnectionPage.tsx` |

## 单元测试结果

| 套件 | 结果 |
|------|------|
| `MainPage.test.tsx` | **通过**（1/1） |
| `ConnectionPage.test.tsx` | **通过**（15/15） |
| `WorkflowPage.test.tsx` | **通过**（17/17） |
| `pathIpcWiring.test.ts` | **失败**（1/6）— 见 Bugs，**非 F2 范围** |

命令：

```bash
pnpm vitest run --coverage \
  --coverage.include='src/windows/main/MainPage.tsx' \
  --coverage.include='src/windows/connection/ConnectionPage.tsx' \
  --coverage.include='src/windows/workflow/WorkflowPage.tsx' \
  src/windows/main/__tests__/MainPage.test.tsx \
  src/windows/connection/__tests__/ConnectionPage.test.tsx \
  src/windows/workflow/__tests__/WorkflowPage.test.tsx
```

## Vitest 覆盖率（F2 相关源文件）

| 文件 | Stmts | Lines | 达标 |
|------|-------|-------|------|
| `MainPage.tsx` | 100% | 100% | ✓ |
| `ConnectionPage.tsx`（整文件） | 50.38% | 52.19% | ✗（F2 仅重命名，覆盖率与 F1 前 `ConnectionWindow` 相同；非 F2 回归） |
| `WorkflowPage.tsx`（整文件） | 76.16% | 76.13% | ✗（略低于 80% 门禁；非 F2 新增逻辑） |

**结论**：F2 变更的 `MainPage` **≥80%**；`ConnectionPage` / `WorkflowPage` 整文件未达 80% 为既有体量问题，与 F1 对 `ConnectionWindow` 的判定一致，不阻塞 F2 重命名验收。

## E2E 执行结果

**未执行（环境阻塞）**

- 原因：无 `target/debug/datazen` 二进制；需 `tauri build --debug --features webdriver` 后运行 `pnpm e2e --skip-build`。
- 用例已核对：`main-window.ts`、`unified-main-window.ts`、`homepage-features.ts` 等均通过 `data-testid` 断言主窗口内导航，不依赖旧 `*Window` 组件名；源码路径断言已指向 `*Page.tsx`（编码 commit 已更新）。

## Bugs

| Bug ID | 关联 | 标题 | 状态 | 说明 |
|--------|------|------|------|------|
| F1-BUG-001 | F1 | `pathIpcWiring.test.ts` 仍断言 `SettingsWindow.tsx` 含 `openLogDir` | 验证不通过 | F1 将目录打开逻辑迁至 `SettingsContent.tsx` 后，`SettingsWindow` 仅为 legacy shell，单测未同步。**非 F2 范围**。 |

**复现步骤（F1-BUG-001）**：

```bash
pnpm vitest run src/commands/__tests__/pathIpcWiring.test.ts
# 失败：settings open helpers use dedicated open_* commands
# expect(SettingsWindow.tsx).toContain('openLogDir') — 实际文件无 openLogDir
```

**测试侧修正（F2 本轮）**：`ConnectionPage.test.tsx` 中误将 mock 键名改为 `openNewConnectionPage`（产品 API 仍为 `openNewConnectionWindow`），已改回。
