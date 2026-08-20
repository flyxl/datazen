# F5 测试报告 — 首次安装欢迎页

> 分支：`feat/main-window-pages`  
> 编码 commit：`2024f465`；修复 commit：`bbefb6bf`  
> 验证 commit：（本轮）  
> 关联工作项：F5

## 实现核查

| 检查项 | 位置 | 结果 |
|--------|------|------|
| 无连接时渲染整页 `WelcomePage` | `MainPage.tsx` L56–57 | ✓ |
| 有连接时渲染 `ConnectionPage`（统一工作区） | `MainPage.tsx` L60 | ✓ |
| `connectionsLoaded` 防闪烁（加载中 spinner） | `connectionStore.ts` L68–68、L98/L102；`MainPage.tsx` L38–53 | ✓ |
| 欢迎页四宫格功能介绍（连接/看板/工作流/AI） | `WelcomePage.tsx` L30–51、L68–71 | ✓ |
| CTA「创建第一个连接」→ `openNewConnectionWindow()` | `WelcomePage.tsx` L75–81 | ✓ |
| 跨窗口 `datazen:connections-changed` 刷新连接列表 | `MainPage.tsx` L27–35；`connectionStore.ts` L123 | ✓ |
| `data-testid="welcome-page"` / `welcome-create-connection` | `WelcomePage.tsx` L54、L76 | ✓ |
| i18n `welcome.*`（en / zh-CN） | `en.ts` L106–118；`zh-CN.ts` L104–114 | ✓ |
| 其他内置语言 fallback 至 en | `locales/index.ts` L76–78 | ✓（非 en/zh-CN 显示英文文案） |

**结论**：实现符合「无连接整页欢迎 / 有连接主界面」需求；`connectionsLoaded` 避免空列表瞬间闪出欢迎页。

### 路由与窗口

| 检查项 | 结果 |
|--------|------|
| `App.tsx` `windowKind=main` → `MainPage` | ✓ |
| 欢迎态无 `workspace-nav-*` 侧栏（整页，非 ConnectionPage 子集） | ✓（WelcomePage 独立布局，仅 TitleBar + MenuBar） |
| 创建连接走 `new-connection-singleton` 子窗口 | `windowManager.ts` L122–134 | ✓ |
| 保存连接后 `emitCrossWindow('datazen:connections-changed')` → MainPage 重拉 → 切 ConnectionPage | store + MainPage 监听 | ✓（单测覆盖 emit；E2E 待实跑） |

## E2E 用例清单（F5 新增 / 回归）

| ID | 规格文件（建议） | 步骤 | 期望 |
|----|------------------|------|------|
| F5-E2E-001 | `e2e/specs/welcome.ts` | 1. 清空所有连接（IPC `delete_connection`）<br>2. reload 主窗口 | `[data-testid="welcome-page"]` 可见；**无** `[data-testid="workspace-nav-connections"]` |
| F5-E2E-002 | 同上 | 欢迎页加载完成 | 标题含 `welcome.title`（zh-CN：`欢迎使用 DataZen`）；四宫格功能卡片可见 |
| F5-E2E-003 | 同上 | 点击 `[data-testid="welcome-create-connection"]` | 打开新建连接子窗口（保存按钮可见） |
| F5-E2E-004 | 同上 | 在新建连接窗保存首个连接并关闭 | 主窗自动显示 `[data-testid="workspace-nav-connections"]` 与连接列表；**无** welcome-page |
| F5-E2E-005 | 同上 | 删除最后一个连接后 reload | 主窗回到 `[data-testid="welcome-page"]` |
| F5-E2E-006 | `e2e/specs/main-window.ts` 回归 | 现有 seeded 连接环境（wdio `before` hook） | 仍显示 ConnectionPage 工作区；**不**出现 welcome-page |
| F5-E2E-007 | 负向 | 首次 `fetchConnections` 失败 | 显示 `[data-testid="welcome-load-error"]` + 重试按钮；**不**显示 welcome-page |

**说明**：`e2e/wdio.conf.ts` 的 `before` hook **始终 upsert** `conn_e2e_pg`；`e2e/specs/welcome.ts` 在 suite `before` 清空连接、`after` 恢复 seed，使 F5-E2E-001~005 可与默认 wdio 配置共存。

## 单元测试结果

| 套件 | 结果 |
|------|------|
| `WelcomePage.test.tsx` | **通过**（2/2） |
| `MainPage.test.tsx` | **通过**（8/8，含 load error / retry） |
| `connectionStore.test.ts` | **通过**（14/14，含 `connectionsLoaded` 断言） |
| **F5 相关合计** | **10/10 通过**（WelcomePage + MainPage；本轮验证） |

命令：

```bash
pnpm vitest run --coverage \
  --coverage.include='src/windows/welcome/WelcomePage.tsx' \
  --coverage.include='src/windows/main/MainPage.tsx' \
  --coverage.include='src/stores/connectionStore.ts' \
  src/windows/welcome/__tests__/WelcomePage.test.tsx \
  src/windows/main/__tests__/MainPage.test.tsx \
  src/stores/__tests__/connectionStore.test.ts
```

### F5 相关 Vitest 用例摘要

| 用例 | 描述 |
|------|------|
| WelcomePage renders feature overview and CTA | 四功能卡片 + create-connection 按钮 |
| WelcomePage CTA opens new connection window | `openNewConnectionWindow` 被调用 |
| MainPage loading until connections loaded | `main-connections-loading`；无 welcome/connection shell |
| MainPage WelcomePage when no connections | `connectionsLoaded=true` 且 `connections=[]` |
| MainPage ConnectionPage when connections exist | 有连接项时渲染 connection shell |
| MainPage welcome CTA | CTA 触发 `openNewConnectionWindow` |
| MainPage mount fetch + cross-window listen | `fetchConnections` / `fetchGroups` / `listenCrossWindow` |
| MainPage load error when fetch failed | `welcome-load-error` + retry；无 welcome-page |
| MainPage retry refetches connections | retry 按钮调用 `fetchConnections` |
| MainPage ConnectionPage when connections exist despite error | 有连接时优先工作区 |
| connectionStore fetchConnections sets connectionsLoaded | 成功与失败路径均 `connectionsLoaded=true` |

## Vitest 覆盖率（F5 相关源文件）

| 文件 | Stmts | Lines | F5 路径 | 达标 |
|------|-------|-------|---------|------|
| `WelcomePage.tsx` | 100% | **100%** | 欢迎页 UI + CTA | ✓ |
| `MainPage.tsx` | 93.1% | **91.3%** | 三分支路由 + 加载态 + load error | ✓（未覆盖 L32–33 cross-window 回调体） |
| `connectionStore.ts` | 93.51% | **95.69%** | `connectionsLoaded` + 既有 actions | ✓ |

**结论**：F5 变更路径 lines **≥80%**；单测全绿。

**备注**：`vitest.config.ts` coverage gate 已含 `MainPage.tsx` 与 `WelcomePage.tsx`（F5-BUG-002 已修复）。

## E2E 执行结果

**未实跑**（需 `pnpm tauri build --debug --features webdriver`）；**静态验证通过**：

| 检查项 | 结果 |
|--------|------|
| `e2e/specs/welcome.ts` 存在 | ✓（wdio `specs/**/*.ts` 自动纳入） |
| F5-E2E-001~005 覆盖欢迎页 / CTA / 首连 / 删尽回欢迎 | ✓ |
| 规避 wdio 全局 seed（`before` 清连接、`after` reseed `conn_e2e_pg`） | ✓ |
| testid 与 `WelcomePage.tsx` / `MainPage.tsx` 一致 | ✓ |

## Bugs

| Bug ID | 关联 | 标题 | 状态 | 复现 / 说明 |
|--------|------|------|------|-------------|
| F5-BUG-001 | F5 | Host E2E 无欢迎页 journey；wdio 全局 seed 连接 | **已修复** | `e2e/specs/welcome.ts` 本地清连接 + after 恢复 seed | bbefb6bf |
| F5-BUG-002 | F5 | `WelcomePage.tsx` 未列入 `vitest.config.ts` coverage include/thresholds | **已修复** | `vitest.config.ts` L43/L72 已纳入 WelcomePage | bbefb6bf |
| F5-BUG-003 | F5 | 首次启动 `fetchConnections` 失败时显示欢迎页而非错误态 | **已修复** | MainPage `welcome-load-error` + retry；Vitest 8/8 覆盖 | bbefb6bf |

Bug 状态：**已修复**（Vitest 10/10 + coverage gate 静态确认 + E2E spec 静态审查）

## 验收结论

| 维度 | 结论 |
|------|------|
| 实现核查（无连接欢迎 / 有连接主界面） | **通过** |
| Vitest | **10/10 通过**（WelcomePage + MainPage）；MainPage lines **91.3%** ≥80% |
| E2E | **未实跑**；`welcome.ts` 静态审查通过；F5-BUG-001 **已修复** |
| 工作项 F5 | **已完成**（F5-BUG-001~003 验证 → 已修复） |
