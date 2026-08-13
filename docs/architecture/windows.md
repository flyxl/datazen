# 窗口管理

> [返回架构总览](README.md)

## 1. 多窗口架构

DataZen 采用 Tauri 的多窗口模型，每个主要功能区域在独立的原生窗口中运行：

| 窗口 | 用途 | Label 模式 |
|------|------|-----------|
| main | 连接管理主页 | `main` |
| new-connection | 新建/编辑连接（单例） | `new-connection-singleton` |
| connection | 数据库浏览/编辑 | `connection-{ts}-{n}` |
| settings | 应用设置（单例） | `settings-singleton` |
| docs | 内置使用说明（单例） | `docs-singleton` |
| backup | 备份/恢复（单例） | `backup-singleton` |
| data-sync | 数据同步 Diff Workspace（单例；Apply 未接线） | `data-sync-singleton` |
| workflow | Workflow 管理（单例） | `workflow-singleton` |

## 2. Rust 端窗口创建

所有子窗口通过 Rust 命令 `create_sub_window` 创建（`src-tauri/src/commands/window.rs`），确保原生窗口属性在 Rust 层正确设置：

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

macOS 默认行为是第一次点击非活跃窗口只会聚焦该窗口，不会触发控件事件。通过在 Rust 层设置 `accept_first_mouse(true)`，确保用户第一次点击即可直接操作控件。

## 3. 前端窗口管理器

`src/lib/windowManager.ts` 提供统一的窗口创建 API：

```typescript
function openWindow(label: string, options: OpenWindowOptions) {
  if (isTauri()) {
    void openTauriWindow(label, options); // 调用 Rust create_sub_window
  } else {
    openBrowserWindow(options);           // 浏览器降级: window.open()
  }
}
```

导出函数：
- `openNewConnectionWindow(editId?)` — 新建/编辑连接
- `openConnectionWindow(connectionId, connectionName, database?, databaseType?)` — 数据库浏览（含 SQL 查询面板）
- `openSettingsWindow(section?)` — 设置窗口（单例模式）
- `openDataSyncWindow()` — 数据同步
- `openBackupWindow()` — 备份/恢复

### 3.1 Settings 窗口单例

Settings 窗口使用固定 label `settings-singleton`，打开时先检查是否已存在：

```typescript
const existing = await WebviewWindow.getByLabel(SETTINGS_LABEL);
if (existing) {
  await existing.setFocus();
  return;
}
```

## 4. 窗口路由

前端使用 URL query parameter `window` 区分窗口类型：

```typescript
function getWindowKind(): string {
  const params = new URLSearchParams(window.location.search);
  return params.get('window') ?? 'main';
}
```

`App.tsx` 根据 `windowKind` 懒加载对应的窗口组件。

## 5. ErrorBoundary

全局 `ErrorBoundary` 组件包裹整个应用，防止未处理的 React 错误导致白屏：

- 捕获 `getDerivedStateFromError` + `componentDidCatch`
- 显示错误信息 + Dismiss / Reload 按钮
- 错误日志输出到 console
