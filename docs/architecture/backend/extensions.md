# 运行时插件系统（UI 页面 + 主题 Extension）

> [返回架构总览](../README.md) · 源码包与安装测试：[packages/extensions/](../../../packages/extensions/)

统一运行时扩展机制：一份 `manifest.json` 声明插件贡献了什么（工作区页面 / 主题），宿主提供壳与受控 postMessage 桥，插件在沙箱 iframe 中运行。与**编译时驱动插件**（`packages/drivers/*`，inventory 注册）长期并存、互不替代；与主题包的关系见下文「主题」。

## 总体架构

```text
┌──────────────────────────── Main Window (React) ────────────────────────────┐
│ aside(workspace/extensions) │ WorkspaceNavigator │ WorkspaceTabBar              │
│ extensionStore/workspaceTabsStore │ ExtensionPageShell ── postMessage(extension)──┐│
└──────────────────────────┬──────────────────────────────┼──────────────────┘│
                           │ Tauri IPC                    │ datazen://{id}/…  │
┌──────────────────────────▼──────────────────────────────▼──────────────┐    │
│ src-tauri: commands/extensions.rs → extensions/{mod,manifest,install,storage} │    │
│ register_uri_scheme_protocol("datazen") 资产服务 + open 深链             │    │
│ execute_driver_command（复用，零改动）                                    │    │
└─────────────────────────────────────────────────────────────────────────┘    │
        ▲ sandbox iframe（opaque origin，@datazen/extension-sdk 或裸 JS）◄──────┘
```

## 目录与安装

安装根：`{appData}/plugins/{publisher}.{name}/`（目录名 == manifest.id）。zip/目录两态安装均先进入 `.datazen-staging-*` 临时目录跑完整规则校验再原子改名；同 id 重装自动备份 `{id}.old.bak`。卸载删除整目录——**含 `.storage.json` 用户数据**（确认弹窗已明示）。

源码包位于 [`packages/extensions/`](../../../packages/extensions/)（`community.slate-blue` 纯主题示例、`datazen.playground` 全功能示例），由 Rust `fixture_tests` 与 vitest `extensionThemes.test.ts` 双向守护。

## Manifest（apiVersion = 2）

`src-tauri/src/extensions/manifest.rs` 为唯一权威 schema（serde camelCase + deny_unknown_fields）。要点：

- id：`^[a-z0-9][a-z0-9-]{0,30}\.[a-z][a-z0-9-]{1,31}$`
- `contributes.pages[]`：`{id,title,icon?,showIn?="workspace"}`
- `contributes.themes[]`：`{id,name,tokensCss,modes,previewImage?,editorJson?,chartsJson?,iconsDir?}` —— 后三项对齐旧 ThemePack 能力（CodeMirror overlay / 图表调色板 / 语义图标覆盖目录），应用管线软失败
- `permissions[]`：`context:connections | command:invoke | storage:local | ui:notify`（deny-by-default）
- `backend` 字段 v1 必须为空（P2 后端插件预留位）
- 规则 5–7：声明路径防穿越/白名单扩展/SVG 内容扫描/≤50MB·≤2000 文件

## datazen:// 协议

`register_uri_scheme_protocol("datazen")`：

- `datazen://{pluginId}/{path}` 资产服务（enabled 校验→MIME 白名单→路径安全）；Windows/WebView2 映射为 `http(s)://datazen.<host>/…`
- 响应头固定注入 CSP `default-src 'self' datazen:; script-src 'self' datazen:; …; connect-src 'none'` 与 nosniff。**必须双源**：macOS WebKit 对自定义 scheme 文档不匹配 `'self'`（BUG-F9-04），Windows 映射形态只有 `'self'` 能匹配
- `datazen://{pluginId}/open?page=…&params=…` 深链 → 宿主事件 `plugins:open-page`

## 桥接协议（src/lib/extensionBridge.ts）

信封 `{ch:'datazen-extension', type, reqId, target}`；握手 `plugin.ready → host.ready(apiVersion/locale/dark/tokens)`；响应 `${type}.ok|.err` 回显 reqId。路由表 own-property 查找（原型链键一律 E_NOT_FOUND，BUG-F6-01）。

| API | 权限 | 备注 |
|-----|------|------|
| context.getConnections / getActiveConnection | context:connections | 只读摘要 |
| command.invoke | command:invoke | 转发 execute_driver_command；审计落盘 |
| storage.get/set/remove | storage:local | `{appData}/plugins/{id}/.storage.json`，≤1MB |
| ui.notify | —（无需声明） | ≥5s 冷却 |
| i18n.getString | — | 插件自带 `locales/<locale>.json` 查表，en 兜底 |

限流 ≤20 并发（E_RATE_LIMIT）、单请求超时 30s。**主题推送**：`host.ready` 携带首次 `{dark, tokens}` 快照；宿主在 `<html>` class 变化与 `datazen:theme-pack-changed` 事件时对每个挂载桥推送 `theme.apply`（`PluginPageShell` → `pushThemeSnapshot`），页面侧义务见 [packages/extensions/README.md](../../../packages/extensions/README.md)「主题一致性规范」。**审计**：command.invoke 写 webview console（`[extension:{id}]`）并同时经 `extension_audit_log` 命令进 tracing 文件 sink（`{dataDir}/logs/datazen.log`，target `extension_audit`）；detail 仅含命令名+连接 id，参数内容永不入日志（前端构造式白名单 + Rust 双端截断）。

## 主题应用

Settings「外观」仅列出已启用插件的 themes 贡献（packId 形如 `plugin:{pluginId}:{themeId}`）。`themePackApply.applyPluginTheme` 经 `read_extension_file` 读 tokens.css 并 blob 重写 `url()` 相对资产；icons/editor/charts 三类可选资产随后应用，失败只降级对应切片。切换/清除由 `resetPackState()` 统一回收。旧 `{appData}/themes/` 运行时入口已移除。

## 测试分层

- Rust：`cargo test -p datazen --lib plugins`（manifest 规则/协议安全/存储隔离/install 真实路径/fixture 与 packages/extensions 守护）
- Host 单测：桥接语义+安全（extensionBridge*.test）、主题管线（themePackApply.pluginTheme.test）、壳集成（PluginPageShell.bridge）、store、管理页/工作区组件
- E2E：`e2e/specs/plugins.spec.ts` J1–J5（装→列表→Tab→桥探针→外观→停用卸载），探针经桥 `storage.set` 落盘后磁盘对账（不依赖帧内 WebDriver 自动化）
