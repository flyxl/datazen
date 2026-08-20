# F6 测试报告 — 帮助文档改官网上线；浏览器打开

> 分支：`feat/main-window-pages`  
> 编码 commit：`96c7a3be`  
> 验证 commit：（本轮）  
> 关联工作项：F6

## 实现核查

### 已删除（编码 commit 确认不存在）

| 文件 / 目录 | 结果 |
|-------------|------|
| `src/windows/docs/DocsWindow.tsx` | ✓ 已删除 |
| `src/windows/docs/content.ts` | ✓ 已删除 |
| `src/windows/docs/renderMarkdown.ts` | ✓ 已删除 |
| `src/windows/docs/renderMarkdown.test.ts` | ✓ 已删除 |
| `src/windows/docs/` 目录 | ✓ 不存在 |

### 官网文档页

| 检查项 | 位置 | 结果 |
|--------|------|------|
| 英文 `site/docs.html` | 仓库 | ✓ 存在（2480 行） |
| 中文 `site/zh/docs.html` | 仓库 | ✓ 存在（2554 行） |
| 侧栏 TOC 锚点 | 两版 L56–64 | ✓ 7 个 section 链接 |
| Section `id` 与 `docsUrls.ts` 一致 | 见下表 | ✓ |

| Section ID | `site/docs.html` | `site/zh/docs.html` | `docsUrls.ts` / Rust `DOCS_SECTIONS` |
|------------|------------------|---------------------|--------------------------------------|
| `overview` | ✓ L67 | ✓ L67 | ✓ |
| `features` | ✓ L100 | ✓ L101 | ✓ |
| `ai` | ✓ L149 | ✓ L149 | ✓ |
| `context` | ✓ L172 | ✓ L186 | ✓ |
| `workflows` | ✓ L202 | ✓ L259 | ✓ |
| `opsDashboard` | ✓ L1662 | ✓ L1688 | ✓ |
| `schemaDiff` | ✓ L2218 | ✓ L2280 | ✓ |

| 检查项 | 位置 | 结果 |
|--------|------|------|
| 深链滚动偏移 | `site.css` `.docs-section { scroll-margin-top: 88px }` | ✓ 固定顶栏不挡锚点 |
| `sitemap.xml` 含 docs EN/ZH | `site/sitemap.xml` L77–86 | ✓ |
| 站点导航链至 docs | `site/assets/js/site.js` nav/footer | ✓ |

### 应用内入口（浏览器，非子窗口）

| 检查项 | 位置 | 结果 |
|--------|------|------|
| `openDocsWindow(section?)` | `windowManager.ts` L179–190 | ✓ Tauri → `settingsCommands.openPath`；浏览器 → `window.open` |
| `buildDocsUrl` / 常量 | `docsUrls.ts` | ✓ EN/ZH base + 合法 section hash |
| Rust `open_docs_window` | `commands/window.rs` L104–108 | ✓ `open::that(url)`，无 `create_sub_window` |
| macOS Help 菜单 | `lib.rs` `MenuAction::OpenDocs` | ✓ spawn `open_docs_window(app, None)` |
| Web MenuBar Help | `MenuBar.tsx` `help-docs` | ✓ 动态 import → `openDocsWindow()` |
| 各 UI 深链入口 | WorkflowPage / AiChatPanel / ContentToolbar / DashboardPanel / SchemaDiffWindow | ✓ 传对应 section id |
| `App.tsx` 无 `DocsWindow` / `case 'docs'` | 静态扫描 | ✓ |
| `windowKind` legacy `?window=docs` | `windowKind.ts` L6–18 | ✓ 别名 → `main`（非独立 docs 壳） |

**结论**：F6 编码目标（删 in-app Docs 子窗口、跳转 GitHub Pages 官方文档）与源码 / 站点文件一致。

## 全库引用扫描（docs 子窗口残留）

| 模式 | 命中 | 说明 |
|------|------|------|
| `docs-singleton` | 2 | 仅 **负向断言**（`windowManager.test.ts`、`pathIpcWiring.test.ts`） |
| `openSingletonWindow('docs-singleton'` | 1 | 仅 `pathIpcWiring.test.ts` 断言 **不存在** |
| `create_sub_window` + docs | **0** | Rust/TS 均无 docs 标签创建 |
| `DocsWindow` | 3 | `pathIpcWiring.test.ts` 负向断言；`f4-test-report.md` 历史描述（文档债） |
| `?window=docs` | 1 | `windowKind.test.ts` legacy 别名测试 |
| `src/windows/docs/` | **0** | 目录已清空 |

**结论**：无运行时 docs 子窗口残留；仅测试负向断言与 F4 测试报告历史文案需 R1  sweep。

## E2E 用例清单（F6 新增 / 回归）

| ID | 规格文件（建议） | 步骤 | 期望 |
|----|------------------|------|------|
| F6-E2E-001 | 新建 `e2e/specs/help-docs.ts` | macOS：Help → User Guide；Win/Linux：`MenuBar` Help → 使用说明 | **系统浏览器**打开 `https://flyxl.github.io/datazen/docs.html`（或 zh 版）；**无** `docs-singleton` Tauri 子窗口 |
| F6-E2E-002 | 同上 | 设置语言 `zh-CN` 后重复 F6-E2E-001 | URL 为 `.../zh/docs.html` |
| F6-E2E-003 | 同上 | 主窗 Workflow 页点击帮助（`?`） | 浏览器打开 `...#workflows`；页面滚动至 Workflows section |
| F6-E2E-004 | 同上 | AI Chat 侧栏 Context 帮助 | 浏览器打开 `...#context` |
| F6-E2E-005 | 同上 | Connection SQL 工具栏 AI 帮助 | 浏览器打开 `...#ai` |
| F6-E2E-006 | 同上 | Dashboard panel Ops 帮助 | 浏览器打开 `...#opsDashboard` |
| F6-E2E-007 | 负向 | 直接访问 `window.html?window=docs&section=workflows` | `windowKind=main`；渲染主工作区；**不**出现 in-app Docs 侧栏 |
| F6-E2E-008 | 回归 | F1–F5 主窗 / settings / welcome 路径 | 不回归 |

**说明**：当前仓库 **无** Help→浏览器 E2E spec（见 Bugs F6-BUG-002）。Webdriver 环境需 mock 或 OS 级断言 `open_path` / 默认浏览器 URL（可参考 settings `openLogDir` 模式）。

## 单元测试结果

| 套件 | 结果 |
|------|------|
| `docsUrls.test.ts` | **通过**（4/4） |
| `windowManager.test.ts` | **通过**（10/10，含 browser + Tauri `openDocsWindow`） |
| `windowKind.test.ts` | **通过**（4/4，含 `?window=docs` → `main`） |
| `pathIpcWiring.test.ts`（F6 相关用例） | **通过**（含无 `DocsWindow` / 无 `docs-singleton`） |
| **F6 相关合计** | **18/18 通过**（三套件）；含 pathIpc **24/24** |

命令：

```bash
pnpm vitest run --coverage \
  --coverage.include='src/lib/docsUrls.ts' \
  --coverage.include='src/lib/windowManager.ts' \
  --coverage.include='src/lib/windowKind.ts' \
  src/lib/__tests__/docsUrls.test.ts \
  src/lib/__tests__/windowManager.test.ts \
  src/lib/__tests__/windowKind.test.ts
```

### F6 相关 Vitest 用例摘要

| 用例 | 描述 |
|------|------|
| `buildDocsUrl` EN/ZH base | 非中文 → EN；`zh-CN`/`zh-TW` → ZH |
| `buildDocsUrl` unknown section | 非法 id 忽略 hash |
| `buildDocsUrl` all section anchors | 7 个 id 双语言 |
| `openDocsWindow` browser mode | `window.open(DOCS_BASE_EN#workflows)`；无 sub-window |
| `openDocsWindow` unknown section | 仅 base URL |
| `openDocsWindow` Tauri mode | `openPath(DOCS_BASE_ZH#ai)`；**不**调用 `create_sub_window` |
| `WINDOW_CAPABILITY_LABEL_SAMPLES` | 不含 `docs-singleton` |
| `getWindowKind` `?window=docs` | → `main` |
| pathIpc overlay chrome | 无 `DocsWindow`；`buildDocsUrl` + 官网 URL |

## Vitest 覆盖率（F6 相关源文件）

| 文件 | Stmts | Lines | F6 路径 | 达标 |
|------|-------|-------|---------|------|
| `docsUrls.ts` | （含于合计） | **100%**（4 用例全覆盖） | `buildDocsUrl` / 常量 | ✓ |
| `windowManager.ts` | 92.53% | **94.73%** | `openDocsWindow` L179–190 | ✓ |
| `windowKind.ts` | 94.73% | **100%** | legacy `docs` 别名 | ✓ |
| **合计** | 93.68% | **96.2%** | — | ✓ |

未覆盖：`windowManager.ts` L185（Tauri `openPath` 失败 fallback `window.open`）、L147/L200（非 F6 路径）。

**结论**：F6 变更路径 lines **≥80%**；单测全绿。

## E2E 执行结果

**未实跑**（需 `pnpm tauri build --debug --features webdriver` + 浏览器 URL 断言能力）。

**线上 smoke**（2026-08-20）：

| URL | HTTP | 说明 |
|-----|------|------|
| `https://flyxl.github.io/datazen/` | 200 | 站点首页正常 |
| `https://flyxl.github.io/datazen/docs.html` | **404** | 编码已合入 `site/docs.html`，**Pages 尚未发布** — 见 F6-BUG-001 |

## Bugs

| Bug ID | 关联 | 标题 | 状态 | 复现 / 说明 |
|--------|------|------|------|-------------|
| F6-BUG-001 | F6 | GitHub Pages `docs.html` / `zh/docs.html` 返回 404 | 待验证 | 浏览器访问 `https://flyxl.github.io/datazen/docs.html` → 404；仓库 `site/docs.html` 已存在，需 merge/deploy |
| F6-BUG-002 | F6 | Host E2E 无 Help→浏览器 / section 深链 journey | 待验证 | `e2e/specs/` 无 `help-docs` 或等效 spec；F6-E2E-001~007 待实现 |
| F6-BUG-003 | F6 | `f4-test-report.md` 仍描述 in-app `DocsWindow` / `docs-singleton` | 待验证 | 文档债；**留 R1** sweep（F4 报告历史快照，非运行时） |

## 验收结论

| 维度 | 结论 |
|------|------|
| 删 Docs 子窗口 / 官网 HTML | **通过**（仓库内） |
| 残留引用扫描 | **通过**（无运行时 docs 子窗口） |
| Vitest | **18/18 通过**；F6 路径 lines **≥80%** |
| E2E | **未实跑**；用例已文档化；线上 docs **404**（F6-BUG-001） |
| 工作项 F6 | **已完成**（实现与单测通过；部署 / E2E 债登记 Bugs） |
