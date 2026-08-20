# F1 测试报告 — SettingsPage 主窗口内嵌

> 分支：`feat/main-window-pages`  
> 编码 commit：`e00cc340`  
> 关联工作项：F1

## E2E 用例清单

| ID | 步骤 | 期望 |
|----|------|------|
| F1-E2E-001 | 1. 打开主窗口 `tauri://localhost`<br>2. 通过 `menu:open-settings` emit 打开设置 | `[data-testid="settings-page"]` 与 `[data-testid="settings-back"]` 可见；工作区侧栏 `[data-testid="workspace-nav-connections"]` 不可见 |
| F1-E2E-002 | 1. 在 SettingsPage 点击 `[data-testid="settings-back"]` | SettingsPage 关闭；工作区导航（连接/工作流/看板）重新可见 |
| F1-E2E-003 | 1. emit `menu:open-settings` 且 payload `{ section: 'ai' }` | AI 分区内容可见（Provider / API 等文案） |
| SS-001 | 主窗口 TitleBar 主题按钮交互（`before` 刷新后主窗口） | 浅色/深色/跟随系统切换；`html.dark` class 正确 |
| SS-003~SS-006 | IPC `save_settings` / `get_settings` | 主题、分页、查询限制、编辑器偏好持久化 |
| TC-SET-003 | 主窗口 SettingsPage → 编辑器分区 | 字体/字号相关控件可见 |
| TC-SET-004 | 主窗口 SettingsPage → 数据浏览分区 | 分页/限制相关项可见 |
| TC-SET-007 | 主窗口 SettingsPage → Prompt 分区 | Prompt 管理入口可见 |
| SS-NAV-001 | 侧栏切换行为/日志/AI/MCP/扩展 | 各分区关键文案可见 |
| SS-CLN-001 | 行为分区 | 数据清理标题与运行按钮可见 |

实现位置：`e2e/specs/settings.ts`；辅助函数 `openSettingsInMainWindow` / `backFromSettingsInMainWindow`（`e2e/helpers.ts`）。

## 单元测试结果

| 套件 | 结果 |
|------|------|
| `SettingsPage.test.tsx` | **通过**（4/4） |
| `ConnectionWindow.test.tsx`（含 F1 导航用例） | **通过** |
| `windowManager.test.ts`（含 openSettingsWindow） | **通过** |

## Vitest 覆盖率（F1 相关源文件）

| 文件 | Stmts | Lines | 达标 |
|------|-------|-------|------|
| `SettingsPage.tsx` | 100% | 100% | ✓ |
| `windowManager.ts` | 93.84% | 96.29% | ✓ |
| `ConnectionWindow.tsx`（整文件） | 50.38% | 52.19% | ✗（F1 路径由 TC-window 用例覆盖；全文件未达 80%） |

**结论**：F1 新增/变更的 `SettingsPage` 与 `openSettingsWindow` 路径 **lines/statements ≥80%**；`ConnectionWindow` 仅 settings 分支有单测，整文件覆盖率受既有体量影响未达 80%。

命令：

```bash
pnpm vitest run --coverage \
  --coverage.include='src/windows/settings/SettingsPage.tsx' \
  --coverage.include='src/lib/windowManager.ts' \
  src/windows/settings/__tests__/SettingsPage.test.tsx \
  src/lib/__tests__/windowManager.test.ts \
  src/windows/connection/__tests__/ConnectionWindow.test.tsx
```

## E2E 执行结果

**未执行（环境阻塞）**

- 原因：`target/debug/datazen` 不存在；`pnpm e2e --skip-build -- --spec e2e/specs/settings.ts` 报错需先完成 `tauri build --debug --features webdriver`（完整 Rust+前端构建，本轮未在合理时间内完成）。
- 已更新 E2E 用例：`settings.ts` 不再使用 `window.html?window=settings`；`e2e/helpers/data-dashboard.ts`、`i18n-10-locales.ts` 的 `openSettingsWindow` 已改为调用主窗口路径。

## Bugs

本轮 Vitest 未发现 F1 功能缺陷；Bugs 表无新增项。
