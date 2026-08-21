# 插件系统开发进度管理

> 流程：编码 agent 开发 + 单测 → commit → 新测试 agent 输出 E2E 用例与结果（覆盖率 ≥80%，只报不修）→ commit → bug 循环（验证不通过→修复中→待验证→已修复）。
> 分支：`feature/ui-plugins`（worktree：`../datazen-ui-plugins`）。PRD：[ui-plugins.md](./ui-plugins.md) v0.5；技术方案：[ui-plugins-implementation.md](./ui-plugins-implementation.md)。

## 功能工作项

| # | 功能 | 范围摘要 | 状态 | 开发 commit | 测试 commit |
|---|------|---------|------|------------|------------|
| F1 | Rust 插件基座 | plugins/{mod,manifest,install,storage}.rs、IPC 命令组、AppState、单测（capabilities 走既有 ACL 豁免，见测试记录） | 待测试 | 900b9330 | — |
| F2 | datazen:// 协议 | register_uri_scheme_protocol：path 资产服务 + open 深链 + CSP/403/404 | 未开始 | — | — |
| F3 | 前端状态与 IPC 封装 | types/plugin.ts、pluginStore、workspaceTabsStore、commands/plugins.ts | 未开始 | — | — |
| F4 | 主窗口集成 | WorkspaceMode 扩展、aside 两按钮、Workspace 导航栏/默认卡片/独立 Tab 条/页面壳（静态） | 未开始 | — | — |
| F5 | 插件管理页 | PluginManagementPage + InstallPluginDialog（卡片/过滤/安装/启停/卸载） | 未开始 | — | — |
| F6 | RPC 桥 | uiPluginBridge：信封路由、权限判定、限流超时、token 快照推送 | 未开始 | — | — |
| F7 | Settings 外观 | settings.appearance 菜单项 + AppearanceSection 主题切换器 | 未开始 | — | — |
| F8 | SDK 包 | packages/ui-plugin-sdk（bridge/theme/theme.css/useTheme） | 未开始 | — | — |
| F9 | 示例插件与 E2E | e2e/fixtures/sample-plugin + e2e/specs/plugins.spec.ts journeys 1-5 | 未开始 | — | — |

## Bug 跟踪

| ID | 功能 | 描述 | 重现步骤 | 状态 |
|----|------|------|---------|------|
| （暂无） | | | | |

Bug 状态流转：`新建 → 验证不通过(修复中) → 待验证 → 已修复`

## 测试记录

（每个功能测试完成后在此追加小节：用例清单、结果、覆盖率、bug 链接）

## 回归测试

- [ ] 全量回归（cargo test -p datazen --lib + npx vitest run）
- [ ] 文档更新（架构文档 docs/architecture/backend/plugins.md、AGENTS.md 精简增补）
- [ ] 合并 main
