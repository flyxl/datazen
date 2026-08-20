# 窗口管理

> [返回架构总览](README.md)

## 1. 主工作区 Page + 少量子窗口

**连接 / Workflow / Dashboard / Settings** 均在 `main` OS 窗口内以 `*Page` 组件呈现，不再各自开独立 OS 窗口。仍保留用途单一的原生子窗口（`*Window`）：

| 类型 | 组件 / 窗口 | 说明 |
|------|-------------|------|
| **main** | `MainPage` | 路由壳：无连接 → `WelcomePage`；有连接 → `ConnectionPage` |
| Page | `WelcomePage` | 首次安装 / 无保存连接时的功能介绍与「创建第一个连接」引导 |
| Page | `ConnectionPage` | 统一工作区：`ConnectionNavigatorTree`、连接 Tab、Workflow / Dashboard 内嵌导航 |
| Page | `SettingsPage` | 设置（含返回主界面）；sidebar 底部入口或 `openSettingsWindow(section?)` |
| Page | `WorkflowPage` / `DashboardPanel` | 由 `ConnectionPage` 内嵌渲染，非独立 OS 窗口 |
| 子窗口 | `NewConnectionWindow` | 新建/编辑连接（单例） |
| 子窗口 | `BackupWindow` | 备份/恢复（单例） |
| 子窗口 | `DataSyncWindow` | 同族 Data Sync Diff Workspace（单例） |
| 子窗口 | `SchemaDiffWindow` | Schema Diff（单例） |

| 窗口 | Label / `?window=` |
|------|-------------------|
| main | `main`；legacy `connection` / `workflow` / `dashboard` / `settings` / `docs` 别名 → `main` |
| new-connection | `new-connection-singleton` / `new-connection` |
| backup | `backup-singleton` / `backup` |
| data-sync | `data-sync-singleton` / `data-sync` |
| schema-diff | `schema-diff` |

**Settings** 与 **Docs** 均不是子窗口：

- **Settings**：`openSettingsWindow(section?)` 聚焦 `main` 并 emit `menu:open-settings`，渲染 `SettingsPage`。
- **Docs / Help**：`openDocsWindow(section?)` 在系统浏览器打开 GitHub Pages（`src/lib/docsUrls.ts` → `https://flyxl.github.io/datazen/docs.html` 或 `/zh/docs.html`，可选 `#section`）。Rust `open_docs_window` 同样打开该 URL。

`openConnectionWindow()` 聚焦主工作区并通过 `localStorage` + `datazen:open-connection` 投递连接 payload，**不再**创建 `connection-*` 子窗口。

## 2. Rust 端窗口创建

子窗口通过 Rust 命令 `create_sub_window`（`src-tauri/src/commands/window.rs`）创建：

```rust
#[tauri::command]
pub fn create_sub_window(app: AppHandle, options: CreateWindowOptions) -> Result<(), CommandError> {
    let is_mac = cfg!(target_os = "macos");
    let decorations = options.decorations.unwrap_or(false);
    let transparent = options.transparent.unwrap_or(is_mac);

    let mut builder = WebviewWindowBuilder::new(
        &app,
        &options.label,
        WebviewUrl::App(options.url.into()),
    )
    .title(&options.title)
    .inner_size(options.width, options.height)
    .decorations(decorations)
    .transparent(transparent)
    .visible(false)
    .accept_first_mouse(options.accept_first_mouse);
    // ...
}
```

### 2.1 macOS acceptFirstMouse

macOS 默认第一次点击非活跃窗口只会聚焦。Rust 层 `accept_first_mouse(true)` 使首次点击即可操作控件。

## 3. 前端窗口管理器

`src/lib/windowManager.ts` 提供统一 API：

```typescript
function openWindow(label: string, options: OpenWindowOptions) {
  if (isTauri()) {
    void openTauriWindow(label, options); // 调用 Rust create_sub_window
  } else {
    openBrowserWindow(options);           // 浏览器降级: window.open()
  }
}
```

导出函数（摘要）：

- `openNewConnectionWindow(editId?)` — 新建/编辑连接（子窗口）
- `openConnectionWindow(...)` — 聚焦主工作区并打开/追加连接 Tab
- `openSettingsWindow(section?)` — 主工作区内 `SettingsPage`（emit `menu:open-settings`）
- `openDocsWindow(section?)` — 系统浏览器打开官网（`open_path` IPC / `window.open` 降级）
- `openDashboardWindow()` / `openWorkflowWindow()` — 主工作区内导航（emit `menu:dashboard` / `menu:workflow`）
- `openDataSyncWindow()` / `openBackupWindow()` — 对应单例子窗口

## 4. 窗口路由

`src/lib/windowKind.ts` 用 URL 参数 `window` 区分：

```typescript
export type WindowKind =
  | 'main'
  | 'new-connection'
  | 'data-sync'
  | 'schema-diff'
  | 'backup';

/** Legacy sub-window kinds that now route to the unified main shell. */
const LEGACY_MAIN_ALIASES = new Set(['connection', 'workflow', 'dashboard', 'settings', 'docs']);
```

`App.tsx` 按 `getWindowKind()` 懒加载对应页面；legacy 别名一律落到 `main`。

## 5. ErrorBoundary

全局 `ErrorBoundary` 包裹应用，防止未处理 React 错误导致白屏（Dismiss / Reload）。
