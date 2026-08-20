# UX / MCP 打磨 — 开发进度

> 分支：`feat/ux-mcp-polish`  
> 开始：2026-08-20

## 工作项

| ID | 功能 | 状态 | 备注 |
|----|------|------|------|
| P1 | Settings 返回按钮移到左侧导航栏最上方 | 已完成 | settings-nav 顶部；TitleBar 不再放返回 |
| P2 | 欢迎页增加导入连接入口（TablePlus/Navicat 等） | 已完成 | MainPage 挂载 ConnectionShareDialogHost；菜单监听上提 |
| P3 | 创建/编辑连接改为页面内弹窗，不再开子窗口 | 已完成 | openNewConnectionDialog + ConnectionEditorDialogHost |
| P4 | 使用说明在系统浏览器打开官网（确认/补齐，去子窗口残留） | 已完成 | 已确认：`openDocsWindow` / `open_docs_window` 开官网，无 docs-singleton |
| P5 | 欢迎页展示 app icon | 已完成 | `./logo.png`，data-testid=welcome-app-icon |
| P6 | 清理不用的窗口声明/权限/测试 | 已完成 | 移除 settings-singleton、new-connection-*；更新 docs/测试 |
| P7 | MCP Server 开关立即生效；设置开启则 GUI 启动时一并启动 | 已完成 | duplex keepalive + mcp_reload 热重载 |
| P8 | MCP tools 调用出错返回足够说明（含 help 风格输出） | 已完成 | tool_help 模块 + call_tool 错误增强 |
| P9 | Settings 日志路径改为路径选择（PathInput） | 已完成 | PathInput directory picker |
| P10 | 生产代码原生 `<select>` → 封装 Select | 已完成 | 已审计：生产代码无原生 select；`noNativeSelect.test.ts` 回归 |

状态：`未开始` | `进行中` | `已完成`

## Bugs

| Bug ID | 关联 | 标题 | 状态 | 复现 |
|--------|------|------|------|------|
| — | — | — | — | — |

## 变更记录

| 日期 | 说明 |
|------|------|
| 2026-08-20 | 初始化进度与分支 |
| 2026-08-20 | P7/P8：embedded MCP duplex keepalive、热重载、mcp tool help 错误 |
| 2026-08-20 | P2/P5：欢迎页 app icon、导入连接入口；ConnectionShareDialogHost 上提 MainPage |
| 2026-08-20 | P4：已确认 Docs 走系统浏览器（F6）；无 docs-singleton 残留 |
| 2026-08-20 | P6：清理 default.json.host（settings/new-connection）；对齐 codegen；更新 windows.md / AGENTS.md / 测试 |
| 2026-08-20 | P10：全库审计生产代码无 `<select>`；新增 noNativeSelect 回归测试 |
| 2026-08-20 | P1/P9：Settings 返回移至 nav 顶部；日志路径改 PathInput |
| 2026-08-20 | P3：NewConnectionDialog 替代子窗口；E2E/单测已对齐 |
