# R1 回归测试报告 — Host Vitest 全量

> 分支：`feat/main-window-pages`  
> 基线：`main`  
> 验证日期：2026-08-20  
> 关联工作项：R1（F1–F6 回归）

## 范围

本轮回归覆盖 `feat/main-window-pages` 相对 `main` 的全部 Host 前端改动（F1–F6）：

| ID | 变更摘要 |
|----|----------|
| F1 | Settings 嵌入主窗口 `SettingsPage`；`openSettingsWindow` 改 emit |
| F2 | 主工作区 `Window` → `Page` 重命名（ConnectionPage / WorkflowPage / MainPage 等） |
| F3 | 左侧 sidebar 底部 Settings 入口；返回恢复先前页面 |
| F4 | 删除 DashboardWindow / SettingsWindow 等独立窗口壳；保留 Docs 入口 |
| F5 | 首次安装欢迎页 `WelcomePage`；无连接时展示 |
| F6 | 帮助文档改跳转官网 `docs.html`；删除 in-app Docs 子窗口 |

## 测试命令

### 全量 Host Vitest（本轮执行）

```bash
npx vitest run
```

### F1–F6 重点路径（补充执行，全绿）

```bash
npx vitest run \
  src/windows/ \
  src/lib/ \
  src/stores/__tests__/connectionStore.test.ts \
  src/commands/__tests__/pathIpcWiring.test.ts
```

## 单元测试结果

| 套件 | 文件数 | 用例数 | 结果 |
|------|--------|--------|------|
| **全量 Host Vitest** | **187** | **1466** | **全部通过** |
| F1–F6 重点路径（windows / lib / connectionStore / pathIpcWiring） | 125 | 942 | 全部通过 |

执行耗时：全量约 31s；重点路径约 21s。

### F1–F6 相关套件明细

| 关联 | 套件 | 结果 |
|------|------|------|
| F1 | `src/windows/settings/__tests__/SettingsPage.test.tsx` | 通过 |
| F1 | `src/commands/__tests__/pathIpcWiring.test.ts`（SettingsContent / openLogDir） | 通过 |
| F2 | `src/windows/main/__tests__/MainPage.test.tsx` | 通过 |
| F2 | `src/windows/connection/__tests__/ConnectionPage.test.tsx` | 通过 |
| F2 | `src/windows/workflow/__tests__/WorkflowPage.test.tsx` | 通过 |
| F3 | `src/windows/main/__tests__/MainPage.test.tsx`（sidebar Settings 导航） | 通过 |
| F4 | `src/lib/__tests__/windowManager.test.ts`（无 legacy 子窗口） | 通过 |
| F4 | `src/lib/__tests__/windowKind.test.ts` | 通过 |
| F5 | `src/windows/welcome/__tests__/WelcomePage.test.tsx` | 通过 |
| F5 | `src/windows/main/__tests__/MainPage.test.tsx`（welcome 路由 / load error） | 通过 |
| F6 | `src/lib/__tests__/docsUrls.test.ts` | 通过 |
| F6 | `src/lib/__tests__/windowManager.test.ts`（openDocsWindow） | 通过 |
| F6 | `src/commands/__tests__/pathIpcWiring.test.ts`（无 DocsWindow / docs-singleton） | 通过 |

## 失败列表

**无。** 1466/1466 用例全部通过；未发现本轮 Vitest 回归缺陷。

## 回归 Bugs

本轮 **未登记新 regression bug**。

### 已知遗留（非 Vitest 回归；见 `main-window-pages.md`）

| Bug ID | 关联 | 标题 | 状态 | 说明 |
|--------|------|------|------|------|
| F4-BUG-003 | F4 | PRD 仍引用已删 DashboardWindow / SettingsWindow 路径 | 已修复 | R1 已更新 `docs/prd/data-dashboard*.md` 为 DashboardPanel / SettingsPage |
| F6-BUG-001 | F6 | GitHub Pages `docs.html` 返回 404 | 待部署验证 | merge 并部署 Pages 后 smoke |

## 未覆盖项（本轮 R1 范围外）

| 维度 | 状态 | 说明 |
|------|------|------|
| Host E2E（webdriver） | 未执行 | 需 `pnpm tauri build --debug --features webdriver`；F1–F6 用例已文档化 |
| Rust 单测 `cargo test -p datazen` | 未执行 | R1 后续步骤 |
| 架构文档 / AGENTS.md 更新 | **已完成** | R1 文档 commit |
| 合并 `main` | **禁止**（本轮） | 由编排者合并 |

## 验收结论

| 维度 | 结论 |
|------|------|
| Host Vitest 全量回归 | **通过**（187 文件 / 1466 用例） |
| F1–F6 相关路径 | **通过**（无失败、无新 regression bug） |
| R1 整体 | **Vitest 阶段通过**；架构/AGENTS/PRD 已同步；待 E2E 实跑、F6-BUG-001 部署验证后合并 main |
