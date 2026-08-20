# F4 测试报告 — 删除多余窗口壳；保留 Docs

> 分支：`feat/main-window-pages`  
> 编码 commit：`7e60b32c`  
> 关联工作项：F4

## 实现核查

### 已删除文件（编码 commit 确认不存在）

| 文件 | 结果 |
|------|------|
| `src/windows/dashboard/DashboardWindow.tsx` | ✓ 已删除 |
| `src/windows/dashboard/__tests__/DashboardWindow.test.tsx` | ✓ 已删除 |
| `src/windows/settings/SettingsWindow.tsx` | ✓ 已删除 |
| `src/windows/main/ActionPanel.tsx` | ✓ 已删除 |
| `src/windows/main/ConnectionItem.tsx` | ✓ 已删除 |
| `src/windows/main/GroupPanel.tsx` | ✓ 已删除 |
| `src/windows/main/NewConnectionDialog.tsx` | ✓ 已删除 |

### 保留项

| 检查项 | 位置 | 结果 |
|--------|------|------|
| `DocsWindow` 组件 | `src/windows/docs/DocsWindow.tsx` | ✓ 存在 |
| `openDocsWindow(section?)` | `src/lib/windowManager.ts` L180–192 | ✓ 存在；单例 `docs-singleton` |
| `App.tsx` lazy 路由 `case 'docs'` | `src/App.tsx` L43–48, L67–68 | ✓ 仅 Docs 仍为独立子窗口 |
| `MainPage` 薄壳 | `src/windows/main/MainPage.tsx` | ✓ 委托 `ConnectionPage` |
| Settings 单测迁移 | `SettingsWindow.test.tsx` → `SettingsContent.test.tsx` | ✓ R092 重命名 |

**结论**：F4 编码目标（删 legacy 壳、保留 Docs 入口）与文件树一致。

## 全库引用扫描

搜索 `DashboardWindow`、`SettingsWindow`、`ActionPanel`、`GroupPanel`、`ConnectionItem`、`NewConnectionDialog` 及 `from '...'` 指向已删路径。

| 检查项 | 结果 |
|--------|------|
| `src/**/*.{ts,tsx}` 中对已删文件的 `import` | **0 命中** |
| `App.tsx` 无 `DashboardWindow` / `SettingsWindow` lazy | ✓ |
| `openSettingsWindow` / `openDashboardWindow`（`windowManager.ts`） | **预期保留** — 聚焦主窗口 + cross-window emit，非独立 OS 壳 |
| `vitest.config.ts` coverage | `SettingsContent.tsx` 替代 `SettingsWindow`；`dashboard/**` 保留（`DashboardPanel` 等） |
| 架构文档 `docs/architecture/**` | 已标注 legacy 组件移除 / 主工作区路由 |
| PRD `docs/prd/data-dashboard*.md` | 仍引用已删 `DashboardWindow.tsx` / `SettingsWindow.tsx` — 见 Bugs（文档债，非运行时引用） |

**结论**：无编译期坏引用；运行时入口均已迁至主工作区或 `windowManager` emit。

## E2E 用例清单（F4 新增 / 回归）

| ID | 规格文件（建议） | 步骤 | 期望 |
|----|------------------|------|------|
| F4-E2E-001 | `e2e/specs/main-window.ts` 回归 | 打开主窗口 | `workspace-nav-connections` / `workflow` / `dashboard` / `settings` 可见；应用不崩溃 |
| F4-E2E-002 | 新建或 `e2e/specs/docs-window.ts` | 主窗 AI 面板或菜单触发 `openDocsWindow()` | 出现 `docs-singleton` 子窗口；`DocsWindow` 侧栏与正文可见 |
| F4-E2E-003 | 同上 | `window.html?window=docs&section=workflows` | 路由至 Docs；非 MainPage |
| F4-E2E-004 | `e2e/specs/unified-main-window.ts` 回归 | 点击 `workspace-nav-dashboard` | `[data-testid="dashboard-panel"]` 在主窗内可见（非独立 DashboardWindow） |
| F4-E2E-005 | `e2e/specs/settings.ts` 回归 | sidebar / `menu:open-settings` 打开设置 | `[data-testid="settings-page"]` 在主窗内；**无**新 OS 窗口 |
| F4-E2E-006 | 负向 | 直接访问 `window.html?window=settings&section=logging` | **不应**再出现独立 Settings 壳；应为主工作区或需迁移至 `openSettingsInMainWindow`（见 F4-BUG-001） |
| F4-E2E-007 | 负向 | 直接访问 `window.html?window=dashboard` | `windowKind` 别名 → `main`；Dashboard 仅在主窗 panel，无 `DashboardWindow` 组件 |

**说明**：当前仓库 **无** Docs 专用 E2E spec；F4-E2E-002/003 为待实现用例。Settings 主窗路径已在 F1/F3 覆盖；**子窗口 legacy URL** 仍残留在 PIH / hotkeys（见 Bugs）。

## 单元测试结果

| 套件 | 结果 |
|------|------|
| `SettingsContent.test.tsx`（自 `SettingsWindow.test.tsx` 迁移） | **通过**（17/17） |
| `windowManager.test.ts` | **通过**（含 `openDocsWindow` / `openSettingsWindow` / `openDashboardWindow`） |
| `windowKind.test.ts` | **通过**（含 `window=settings` / `window=dashboard` → `main` 别名） |
| `MainPage.test.tsx` | **通过**（1/1） |
| `src/windows/dashboard/__tests__/*` | **通过**（4 文件，与 DashboardWindow 删除无关的 panel 组件单测） |
| **F4 相关合计** | **57/57 通过** |

命令：

```bash
pnpm vitest run --coverage \
  --coverage.include='src/windows/settings/SettingsContent.tsx' \
  --coverage.include='src/lib/windowManager.ts' \
  --coverage.include='src/lib/windowKind.ts' \
  --coverage.include='src/windows/main/MainPage.tsx' \
  src/windows/settings/__tests__/SettingsContent.test.tsx \
  src/lib/__tests__/windowManager.test.ts \
  src/lib/__tests__/windowKind.test.ts \
  src/windows/main/__tests__/MainPage.test.tsx
```

（含 dashboard 子目录单测时共 57/57；dashboard 整包 coverage 未达 80% 为既有 `DashboardPanel` 体量问题，**非 F4 阻塞项**。）

## Vitest 覆盖率（F4 相关源文件）

| 文件 | Stmts | Lines | F4 路径 | 达标 |
|------|-------|-------|---------|------|
| `SettingsContent.tsx` | 85.33% | **87.69%** | 自 SettingsWindow 迁出的设置正文 | ✓ |
| `windowManager.ts` | 93.84% | **96.29%** | `openDocsWindow` 保留；settings/dashboard 改 emit | ✓ |
| `windowKind.ts` | 95.23% | **100%** | legacy 别名 `settings`/`dashboard` → `main` | ✓ |
| `MainPage.tsx` | 100% | **100%** | 薄壳 | ✓ |

**结论**：F4 变更路径 lines **≥80%**；单测全绿。

## E2E 执行结果

**未执行（环境阻塞）**

与 F1–F3 相同：需 `pnpm tauri build --debug --features webdriver` 后跑 Host E2E。F4-BUG-001/002 静态验证已通过（`rg 'window.html\?window=settings' e2e/` → 0；`path-ipc-hardening.ts` / `hotkeys.ts` 使用 `openSettingsInMainWindow`）；PIH-004/005 与 TC-HOTKEY-002 **仍待 webdriver 构建后实跑**。

## Bugs

| Bug ID | 关联 | 标题 | 状态 | 复现 / 说明 |
|--------|------|------|------|-------------|
| F4-BUG-001 | F4 | `path-ipc-hardening.ts` PIH-004/005 legacy settings 子窗口 URL | 已修复 | 11fa9ce2 改 `openSettingsInMainWindow('logging'|'ai')`；静态检查通过（e2e/ 无 legacy URL）。E2E 实跑待 webdriver 构建。 |
| F4-BUG-002 | F4 | `hotkeys.ts` TC-HOTKEY-002 fallback legacy settings URL | 已修复 | 11fa9ce2 改 `openSettingsInMainWindow()`；静态检查通过。E2E 实跑待 webdriver 构建。 |
| F4-BUG-003 | F4 | PRD `docs/prd/data-dashboard*.md` 仍列已删 `DashboardWindow.tsx` / `SettingsWindow.tsx` | 待验证 | 文档债；不影响构建。**留 R1** 文档 sweep（`DashboardPanel` / `SettingsPage` 路径替换）。 |

## 验收结论

| 维度 | 结论 |
|------|------|
| 删文件 / 保留 Docs | **通过** |
| 坏引用扫描 | **通过**（无 import 级断裂） |
| Vitest | **57/57 通过**；F4 路径 coverage ≥80% |
| E2E | **静态验证通过**（legacy URL 已清除）；PIH-004/005、TC-HOTKEY-002 待 webdriver 构建后实跑 |
| 工作项 F4 | **已完成**（Vitest 通过；E2E 阻塞项与已知 bug 已登记） |
