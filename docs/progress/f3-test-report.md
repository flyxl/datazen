# F3 测试报告 — 左侧 icon rail 底部 Settings 入口

> 分支：`feat/main-window-pages`  
> 编码 commit：`ff09481f`  
> 关联工作项：F3

## 实现核查

| 检查项 | 位置 | 结果 |
|--------|------|------|
| Settings 按钮置于 icon rail 底部（`mt-auto`） | `ConnectionPage.tsx` L962 `<div className="mt-auto">` | ✓ |
| `data-testid="workspace-nav-settings"` | `ConnectionPage.tsx` L967 | ✓ |
| 点击调用 `openSettingsInShell()` | `ConnectionPage.tsx` L969 | ✓ |
| `openSettingsInShell` 保存 `settingsReturnModeRef` 并切 `mainView='settings'` | L672–678 | ✓ |
| `handleSettingsBack` 恢复 `workspaceMode` 并回到 `mainView='workspace'` | L681–685 | ✓ |
| 菜单 `menu:open-settings` 复用同一 `openSettingsInShell` | L690–692 | ✓ |
| i18n `nav.settings` | `en.ts` / `zh-CN.ts` | ✓ |

**结论**：F3 编码实现与需求一致；Settings 入口与 F1 菜单路径共用 `openSettingsInShell` / `handleSettingsBack` 栈。

## E2E 用例清单（F3 新增 / 回归）

| ID | 规格文件（建议） | 步骤 | 期望 |
|----|------------------|------|------|
| F3-E2E-001 | `e2e/specs/settings.ts` 或新建 `unified-main-window.ts` | 1. 打开主窗口<br>2. 点击 `[data-testid="workspace-nav-settings"]` | `[data-testid="settings-page"]` 与 `[data-testid="settings-back"]` 可见；工作区 icon rail（`workspace-nav-connections` 等）不可见 |
| F3-E2E-002 | 同上 | 1. 点击 `workspace-nav-workflow`<br>2. 点击 `workspace-nav-settings`<br>3. 点击 `settings-back` | SettingsPage 关闭；`[data-testid="workflow-workspace"]`（或 workflow 容器）可见 |
| F3-E2E-003 | 同上 | 1. 点击 `workspace-nav-dashboard`<br>2. 点击 `workspace-nav-settings`<br>3. 点击 `settings-back` | SettingsPage 关闭；`[data-testid="dashboard-panel"]` 可见 |
| F3-E2E-004 | `e2e/specs/main-window.ts` 回归 | 主窗口加载后 | `workspace-nav-settings` 存在于 DOM（与 connections/workflow/dashboard 并列） |
| F1-E2E-002 回归 | `e2e/specs/settings.ts` | 经 `menu:open-settings` 打开后返回 | 工作区导航重新可见（F1 行为不受 F3 影响） |

**说明**：当前 E2E 辅助函数 `openSettingsInMainWindow`（`e2e/helpers.ts`）仍通过 `emitCrossWindowEvent('menu:open-settings')` 打开设置，**尚未覆盖 sidebar 点击路径**；上表 F3-E2E-001~003 为待实现/待执行用例。

## 单元测试结果

| 套件 | 结果 |
|------|------|
| `ConnectionPage.test.tsx` | **通过**（15/15，含 F3 新增 2 条） |
| `SettingsPage.test.tsx` | **通过**（4/4） |
| **合计** | **19/19 通过** |

F3 相关 Vitest 用例：

| 用例 ID | 描述 |
|---------|------|
| TC-window: sidebar Settings opens SettingsPage | 默认 connections 模式下点击 `workspace-nav-settings` → SettingsPage |
| TC-window: sidebar Settings from workflow returns to workflow on back | workflow → settings → back → workflow 恢复 |
| TC-window: menu:open-settings shows SettingsPage with section | 菜单路径 + section（F1/F3 共用栈） |
| TC-window: SettingsPage back returns to workspace and restores mode | 菜单打开 + workflow 恢复（F1 已有，F3 回归） |

**缺口**：无 Vitest 覆盖 **dashboard → sidebar settings → back → dashboard**（逻辑与 workflow 对称，未单测）。

命令：

```bash
pnpm vitest run --coverage \
  --coverage.include='src/windows/connection/ConnectionPage.tsx' \
  --coverage.include='src/windows/settings/SettingsPage.tsx' \
  src/windows/connection/__tests__/ConnectionPage.test.tsx \
  src/windows/settings/__tests__/SettingsPage.test.tsx
```

（无 `--coverage` 时同上路径 19/19 通过。）

## Vitest 覆盖率（F3 相关源文件）

| 文件 | Stmts | Lines | F3 路径 | 达标 |
|------|-------|-------|---------|------|
| `SettingsPage.tsx` | 100% | 100% | 返回按钮 / `settings-page` testId | ✓ |
| `ConnectionPage.tsx`（整文件） | 50.47% | 52.29% | — | ✗（整文件门禁） |
| `ConnectionPage.tsx`（F3 变更路径） | — | **≈100%** | `openSettingsInShell`、`handleSettingsBack`、`workspace-nav-settings` 渲染与点击 | ✓ |

**结论**：F3 新增/变更路径（`openSettingsInShell`、`settingsReturnModeRef`、`workspace-nav-settings`、`mt-auto` 容器）均由上述 TC-window 用例执行，**F3 路径 lines ≥80%**；`ConnectionPage.tsx` 整文件未达 80% 为既有体量问题（与 F1/F2 对同文件的判定一致），不阻塞 F3 验收。

> 注：对 `ConnectionPage.tsx` 单独跑 coverage 会因 vitest 全局 threshold（lines 80%）报错退出；F3 路径覆盖结论基于用例命中范围，非整文件门禁。

## E2E 执行结果

**未执行（环境阻塞）**

- 原因：`target/debug/datazen` 不存在；需 `tauri build --debug --features webdriver` 后运行 `pnpm e2e --skip-build`。
- F3 sidebar 点击路径尚无自动化 spec 实现；F1 菜单路径用例已在 `e2e/specs/settings.ts` 文档化。

## Bugs

| Bug ID | 关联 | 标题 | 状态 | 说明 |
|--------|------|------|------|------|
| F3-BUG-001 | F3 | Settings 按钮 `active={mainView === 'settings'}` 永不可达 | 待验证 | `mainView === 'settings'` 时整棵 workspace shell（含 icon rail）被 `SettingsPage` 替换，按钮不在 DOM 中，`active` 分支无法生效；Settings 入口无高亮反馈。 |

**复现步骤（F3-BUG-001）**：

1. 阅读 `ConnectionPage.tsx` L928–971：`mainView === 'settings'` 时渲染 `SettingsPage`，else 分支才含 `workspace-nav-settings`。
2. L968 `active={mainView === 'settings'}` 在按钮可见时 `mainView` 恒为 `'workspace'`，active 样式永不应用。

**测试缺口（非 Bug，不修）**：

- Vitest 缺 dashboard 模式 sidebar settings 返回恢复用例。
- E2E 缺 `workspace-nav-settings` 点击路径（仍走 `menu:open-settings` helper）。
