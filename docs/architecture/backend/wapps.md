# 运行时工作区应用系统（UI 页面 + 主题 Workspace App / Wapp）

> [返回架构总览](../README.md) · 源码包与安装测试：[packages/wapps/](../../../packages/wapps/)

统一运行时扩展机制：一份 `manifest.json` 声明应用贡献了什么（工作区页面 / 主题），宿主提供壳与受控 postMessage 桥，应用在沙箱 iframe 中运行。与**编译时数据库驱动**（`packages/drivers/*`，inventory 注册）及**特权扩展点 EP**（`packages/extension-points`）正交并存、互不替代；与主题包的关系见下文「主题」。

## 总体架构

```text
┌──────────────────────────── Main Window (React) ────────────────────────────┐
│ aside(workspace/wapps)      │ WorkspaceNavigator │ WorkspaceTabBar              │
│ wappStore / workspaceTabsStore │ WappPageShell ────── postMessage(wapp)────────┐│
└──────────────────────────┬──────────────────────────────┼──────────────────┘│
                           │ Tauri IPC                    │ datazen://{id}/…  │
┌──────────────────────────▼──────────────────────────────▼──────────────┐    │
│ src-tauri: commands/wapps.rs → wapps/{mod, protocol}                   │    │
│ 内核 packages/workspace-apps = datazen-workspace-apps                   │    │
│ register_uri_scheme_protocol("datazen") 资产服务 + open 深链             │    │
│ execute_driver_command（复用，零改动）                                    │    │
└─────────────────────────────────────────────────────────────────────────┘    │
        ▲ sandbox iframe（opaque origin，@datazen/wapp-sdk 或裸 JS）◄───────────┘
```

## 内核下沉（Wave-2 track B1）

后端按 **形态中立 / 形态相关** 切成两层，宿主侧保留一层 re-export 门面：

| 层 | 位置 | 内容 |
|---|---|---|
| 内核（形态中立） | `packages/workspace-apps`（crate `datazen-workspace-apps`） | `manifest`（schema + 规则 1–7）、`install`（ZIP / 目录安装）、`storage`（按 wapp 隔离的键值存储）。zip 条目路径守卫不在本层，见 `packages/platform-infra` 的 `path_safety`，本层 `install` 改为调用它 |
| 宿主（形态相关） | `src-tauri/src/wapps/` | `protocol`（`datazen://` 协议桥 + IPC）、`WappManager`（启动期扫描 + `.enabled` 标记）、`mod.rs`（门面） |
| 宿主集成测试 | `src-tauri/src/wapps/integration_tests.rs` | 每个用例都驱动宿主命令层（`commands::wapps::*_impl` + `TestAppState` + `WappManager`），故留在宿主 |

关键约定：

- **存储根参数化**：内核不读取任何 ambient 路径、不持有连接、不发网络请求。桌面端传 `{appData}/wapps`，将来的服务端适配器传自己的存储根，**内核公开函数签名保持不变**（`install_from_zip(zip_path, wapps_dir)` / `install_from_dir(src_dir, wapps_dir)` / `storage_*(wapps_dir, wapp_id, key[, value])`）。
- **零 HTTP、零桌面壳依赖**：由 `scripts/check-platform-crate-boundaries.mjs` 的 F-09 / F-10 强制。F-10 只禁服务端框架，HTTP *客户端* 是 `packages/ai-providers` 的职责，本内核两者都不引入。
- 门面在 `src-tauri/src/wapps/mod.rs` 顶层 `pub use datazen_workspace_apps::{install, manifest, storage}` 及扁平名，宿主所有 `crate::wapps::*` 调用点一行未改。
- 随 wapp installer 下沉而失去唯一调用方的 `src-tauri/src/util/theme_zip.rs` 已按死代码纪律删除；其追加的三条守卫（空名 / NUL / ` -> `）改由 crate 内 `empty_nul_and_symlink_names_are_rejected` 钉住。宿主 `app_data_archive::validate_zip_entry_path` 保留（备份 / 还原归档仍用）。

## 目录与安装

安装根：`{appData}/wapps/{publisher}.{name}/`（目录名 == manifest.id）。该根**不由内核自行推导**：`install_from_zip` / `install_from_dir` / `storage_get` / `storage_set` / `storage_remove` 一律把 `wapps_dir` 作为入参，由桌面端适配器传入 `{appData}/wapps`（详见上文「内核下沉」）。zip/目录两态安装均先进入 `.datazen-staging-*` 临时目录跑完整规则校验再原子改名；同 id 重装自动备份 `{id}.old.bak`。卸载删除整目录——**含 `.storage.json` 用户数据**（确认弹窗已明示）。

源码包位于 [`packages/wapps/`](../../../packages/wapps/)（`community.slate-blue` 纯主题示例、`datazen.playground` 全功能示例），由内核 crate 的 `fixture_tests` 与 vitest `wappThemes.test.ts` 双向守护。

## Manifest（apiVersion = 2）

`packages/workspace-apps/src/manifest.rs` 为唯一权威 schema（serde camelCase + deny_unknown_fields）。要点：

- id：`^[a-z0-9][a-z0-9-]{0,30}\.[a-z][a-z0-9-]{1,31}$`
- `contributes.pages[]`：`{id,title,icon?,showIn?="workspace"}`
- `contributes.themes[]`：`{id,name,tokensCss,modes,previewImage?,editorJson?,chartsJson?,iconsDir?}` —— 后三项对齐旧 ThemePack 能力（CodeMirror overlay / 图表调色板 / 语义图标覆盖目录），应用管线软失败
- `permissions[]`：`context:connections | command:invoke | storage:local | ui:notify`（deny-by-default）
- `backend` 字段 v1 必须为空（P2 后端插件预留位）
- 规则 5–7：声明路径防穿越/白名单扩展/SVG 内容扫描/≤50MB·≤2000 文件

## datazen:// 协议

`register_uri_scheme_protocol("datazen")`：

- `datazen://{wappId}/{path}` 资产服务（enabled 校验→MIME 白名单→路径安全）；Windows/WebView2 映射为 `http(s)://datazen.<host>/…`
- 响应头固定注入 CSP `default-src 'self' datazen:; script-src 'self' datazen:; …; connect-src 'none'` 与 nosniff。**必须双源**：macOS WebKit 对自定义 scheme 文档不匹配 `'self'`（BUG-F9-04），Windows 映射形态只有 `'self'` 能匹配
- `datazen://{wappId}/open?page=…&params=…` 深链 → 宿主事件 `wapps:open-page`

## 桥接协议（src/lib/wappBridge.ts）

信封 `{ch:'datazen-wapp', type, reqId, target}`（兼容 `datazen-extension`）；握手 `wapp.ready → host.ready(apiVersion/locale/dark/tokens)`；响应 `${type}.ok|.err` 回显 reqId。路由表 own-property 查找（原型链键一律 E_NOT_FOUND，BUG-F6-01）。

| API | 权限 | 备注 |
|-----|------|------|
| context.getConnections / getActiveConnection | context:connections | 只读摘要 |
| command.invoke | command:invoke | 转发 execute_driver_command；审计落盘 |
| storage.get/set/remove | storage:local | `{appData}/wapps/{id}/.storage.json`，≤1MB |
| ui.notify | —（无需声明） | ≥5s 冷却 |
| i18n.getString | — | 应用自带 `locales/<locale>.json` 查表，en 兜底 |

限流 ≤20 并发（E_RATE_LIMIT）、单请求超时 30s。**主题推送**：`host.ready` 携带首次 `{dark, tokens}` 快照；宿主在 `<html>` class 变化与 `datazen:theme-pack-changed` 事件时对每个挂载桥推送 `theme.apply`（`WappPageShell` → `pushThemeSnapshot`），页面侧义务见 [packages/wapps/README.md](../../../packages/wapps/README.md)「主题一致性规范」。**审计**：command.invoke 写 webview console（`[wapp:{id}]`）并同时经 `wapp_audit_log` 命令进 tracing 文件 sink（`{dataDir}/logs/datazen.log`，target `extension_audit`）；detail 仅含命令名+连接 id，参数内容永不入日志（前端构造式白名单 + Rust 双端截断）。

## 主题应用

Settings「外观」仅列出已启用应用的 themes 贡献（packId 形如 `wapp:{wappId}:{themeId}`）。`themePackApply.applyThemePack` 经 `read_wapp_file` 读 tokens.css 并 blob 重写 `url()` 相对资产；icons/editor/charts 三类可选资产随后应用，失败只降级对应切片。切换/清除由 `resetPackState()` 统一回收。旧 `{appData}/themes/` 运行时入口已移除。

## 首屏背景缓存（surface-bg）

Wapp 主题在 IPC 之后才注入 CSS，首屏无法读取 pack token。`syncWebviewBackgroundFromTokens` 把当前 `--c-surface` 经 `set_surface_background` 写入 `{appData}/surface-bg.json`，子窗口 native `backgroundColor` 同样读这份缓存。缺失时回退 `#0f172a`。

| 模块 | 路径 | 职责 |
|------|------|------|
| 缓存 | `src-tauri/src/theme/surface_bg.rs` | 读写 `{appData}/surface-bg.json` |
| IPC | `src-tauri/src/commands/theme.rs` | `set_surface_background` |
| 前端 | `src/lib/surfaceBgCache.ts` | 解析 `--c-surface` 并 invoke IPC |

详见 [持久化存储](store.md)。

## 事件广播

- 状态变更：`wapps:changed`（安装、卸载、启用、禁用时广播，通知各窗口与 Store 刷新）
- 页面深链：`wapps:open-page`（`datazen://{wappId}/open?page=...` 被触发时派发）

## 测试分层

- Rust 内核：`cargo test -p datazen-workspace-apps`（manifest 规则 / install 真实路径 / 存储隔离 / fixture 与 packages/wapps 双向守护 / zip 路径守卫）
- Rust 宿主：`cargo test -p datazen --lib wapps`（`datazen://` 协议与协议安全 45 条、命令层 / bootstrap / 安装集成 19 条）
- Host 单测：桥接语义+安全（wappBridge*.test）、主题管线（themePackApply.wappTheme.test）、壳集成（WappPageShell.bridge）、store、管理页/工作区组件
- E2E：`e2e/specs/wapps.spec.ts` J1–J5（装→列表→Tab→桥探针→外观→停用卸载），探针经桥 `storage.set` 落盘后磁盘对账（不依赖帧内 WebDriver 自动化）

---

## 扩展双轨架构：沙箱应用（Wapp） vs 特权扩展点（Host Extension Points）

DataZen 扩展体系按照安全与性能需求划分为双轨机制：

| 维度 | 沙箱工作区应用（Workspace App / Wapp） | 特权扩展点（Host Extension Points / EP） |
|---|---|---|
| **运行形态** | 独立沙箱 `<iframe>`，基于 `datazen://` 协议 | 宿主主进程内存执行（In-Process） |
| **隔离级别** | 严格进程/源隔离，仅受控 postMessage 通信 | 零 IPC 损耗，直接访问宿主内部状态与 DOM |
| **适用场景** | 独立工作区页面（Pages）、外观主题（Themes） | 核心编辑器增强（SQLEditor Pro）、高级图表、深度分析 |
| **通信机制** | JSON-RPC 信封（`wappBridge.ts` / `@datazen/wapp-sdk`） | 类型化 TypeScript 契约（`ExtensionPoint<T>`） |
| **许可证豁免** | 臂长通信（Arms-length），无 GPL 传染 | 受 **DataZen Plugin, Driver & Extension Linking Exception** 保护 |
| **准入与安全** | 通用第三方应用（受严格权限声明管控） | 官方签名/可信特权模块，防止任意代码执行 |

### 特权扩展点（Host Extension Points）机制

特权扩展点专为对性能、深度交互有极致要求（例如 CodeMirror 键入期分析 <5ms、Compartment 动态重配置）的核心增强功能设计。

1. **契约解耦**：宿主定义抽象扩展点契约（如 `SqlEditorProExtensionPoint`），并内置开箱即用的轻量基础版（Fallback），确保没有安装特权扩展时宿主功能依然自洽可用。
2. **动态插拔**：通过统一的 `extensionRegistry` 进行注册与退订，支持运行时或构建时无缝切换基础体验与增强体验。
3. **法律合规**：根目录 `LICENSE` 声明了明确的 **GPL Linking Exception**，允许特权扩展使用独立许可证（包括商业闭源或宽松开源许可）发布与分发，免受宿主 GPL-3.0 协议传染。

