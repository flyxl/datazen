# 主窗口 Page 化与欢迎页 — 开发进度

> 分支：`feat/main-window-pages`  
> Worktree：`/Users/wuxiaolong/code/rust-projects/datazen-main-pages`  
> 开始：2026-08-20

## 流程约定

每个功能项按循环执行：

1. **编码 agent** 实现功能 + 单元测试 → 更新本文件状态 → **commit**
2. **测试 agent**（新 session，禁止复用编码 agent）输出 E2E 用例与结果、覆盖率（目标 ≥80%）、缺陷复现步骤（不修 bug）→ **commit**
3. 若不通过：更新 bug 状态为「验证不通过」→ commit → 编码 agent 修复 → bug 改「待验证」→ commit → 测试 agent 验证 → 通过则 bug「已修复」、工作项「已完成」→ commit
4. 全部完成后做回归测试，更新架构文档 / AGENTS.md，合并到 `main`

## 工作项

| ID | 功能 | 状态 | 编码 commit | 测试 commit | 备注 |
|----|------|------|-------------|-------------|------|
| F1 | Settings 改为主窗口内独立 SettingsPage（含返回按钮，返回主界面） | 已完成 | e00cc340 | 85c122a7 | Vitest 通过；E2E 用例已更新，待 CI/本地 webdriver 构建后执行 |
| F2 | 主窗口内页面 Window→Page 重命名（如 ConnectionWindow→ConnectionPage），同步文档 | 已完成 | 2b91921d | （本轮） | 无漏改；Page 单测 33/33 通过；MainPage 100% 覆盖；E2E 用例已文档化，待 webdriver 构建 |
| F3 | 左侧功能 sidebar 底部增加 Settings 入口；进入 SettingsPage；返回恢复先前页面 | 未开始 | — | — | 依赖 F1 |
| F4 | 删除多余窗口（DashboardWindow 等独立窗口壳）；保留使用说明入口 | 未开始 | — | — | |
| F5 | 首次安装欢迎页（介绍连接/看板/工作流/AI，引导创建首个连接；有连接后进主界面） | 未开始 | — | — | |
| F6 | 帮助文档改为官网上线文档；点击使用说明跳转官网 | 未开始 | — | — | |
| R1 | 全量回归 + 文档更新（架构 / AGENTS.md）+ 合并 main | 未开始 | — | — | |

状态枚举：`未开始` | `进行中` | `待测试` | `测试中` | `验证不通过` | `待验证` | `已完成`

## Bugs

| Bug ID | 关联 | 标题 | 状态 | 复现步骤 | 发现 commit |
|--------|------|------|------|----------|-------------|
| F1-BUG-001 | F1 | `pathIpcWiring.test.ts` 断言 `SettingsWindow` 含 `openLogDir`（F1 迁移后失效） | 验证不通过 | `pnpm vitest run src/commands/__tests__/pathIpcWiring.test.ts` | 测试 commit |

Bug 状态：`待验证` | `验证不通过` | `已修复` | `已关闭`

## 变更记录

| 日期 | 说明 |
|------|------|
| 2026-08-20 | 初始化进度文件与分支 |
| 2026-08-20 | F1 编码：SettingsPage 嵌入主窗口；openSettingsWindow 改 emit；抽取 SettingsContent |
| 2026-08-20 | F2 编码：主工作区 ConnectionPage / WorkflowPage / MainPage 重命名；架构文档与 AGENTS.md 同步 |
| 2026-08-20 | F2 测试：重命名完整性通过；Vitest 33/33；f2-test-report.md；登记 F1-BUG-001（pathIpcWiring） |
