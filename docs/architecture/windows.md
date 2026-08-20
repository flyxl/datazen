# 窗口管理

> [返回架构总览](README.md)

## 1. 统一主工作区 + 少量子窗口

自统一连接树改版后，**连接浏览 / Workflow / Dashboard 不再各自开独立 OS 窗口**，而是挂在主窗口（`main`）内的导航与 Tab。React 侧主工作区页面组件以 `*Page` 命名（如 `ConnectionPage`、`WorkflowPage`）；仍保留若干用途单一的原生子窗口（`*Window`）：

| 窗口 | 用途 | Label / `?window=` |
|------|------|-------------------|
| **main**（主工作区） | 连接导航树、连接 Tab、Workflow、Dashboard、查询与 Schema 浏览 | `main`；legacy `connection` / `workflow` / `dashboard` 会别名到 `main` |
| new-connection | 新建/编辑连接（单例） | `new-connection-singleton` / `new-connection` |
| settings | 应用设置（主工作区内 `SettingsPage`；legacy 子窗口路由仍保留） | `settings-singleton` / `settings` |
| docs | 内置使用说明（单例） | `docs-singleton` / `docs` |
| backup | 备份/恢复（单例） | `backup-singleton` / `backup` |
| data-sync | 同族 Data Sync Diff Workspace（单例） | `data-sync-singleton` / `data-sync` |
| schema-diff | Schema Diff（单例） | `schema-diff` |

前端入口：`src/windows/main/MainPage.tsx` → `ConnectionPage.tsx` 在 **main** 工作区内渲染（含 `ConnectionNavigatorTree`、`ConnectionWorkspaceHome`、面板 Tab）。`openConnectionWindow()`（`src/lib/windowManager.ts`）会 `focusMainWindow()`，并通过 `localStorage` + 跨窗口事件 `datazen:open-connection` 投递连接 payload，**不再**创建 `connection-*` 子窗口。

## 2. Rust 端窗口创建

子窗口仍通过 Rust 命令 `create_sub_window`（`src-tauri/src/commands/window.rs`）创建，保证原生属性在 Rust 层设置：

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
- `openConnectionWindow(...)` — **聚焦主工作区**并打开/追加连接 Tab（非新 OS 窗口）
- `openSettingsWindow(section?)` — 主工作区内打开 `SettingsPage`（emit `menu:settings`）
- `openDataSyncWindow()` / `openBackupWindow()` — 对应单例子窗口
- Workflow / Dashboard — 主工作区内导航（`menu:workflow` / `menu:dashboard` 等）

### 3.1 Settings 窗口单例

固定 label `settings-singleton`，已存在则 `setFocus()`。

## 4. 窗口路由

`src/lib/windowKind.ts` 用 URL 参数 `window` 区分：

```typescript
export type WindowKind =
  | 'main'
  | 'new-connection'
  | 'settings'
  | 'data-sync'
  | 'schema-diff'
  | 'backup'
  | 'docs';

/** Legacy sub-window kinds that now route to the unified main shell. */
const LEGACY_MAIN_ALIASES = new Set(['connection', 'workflow', 'dashboard']);
```

`App.tsx` 按 `getWindowKind()` 懒加载对应页面；legacy 别名一律落到 `main`。

## 5. ErrorBoundary

全局 `ErrorBoundary` 包裹应用，防止未处理 React 错误导致白屏（Dismiss / Reload）。
