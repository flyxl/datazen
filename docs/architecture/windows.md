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
| 对话框 | `NewConnectionDialog` | 新建/编辑连接（主窗内 modal，非 OS 子窗口） |
| 子窗口 | `BackupWindow` | 备份/恢复（单例） |
| 子窗口 | `DataSyncWindow` | 同族 Data Sync Diff Workspace（单例） |
| 子窗口 | `SchemaDiffWindow` | 结构对比（单例；Sync 式双栏 + EndpointsBar） |
| 子窗口 | `DataTransferWindow` | 异构 Data Transfer 6 步向导（单例） |

| 窗口 | Label / `?window=` |
|------|-------------------|
| main | `main`；legacy `connection` / `workflow` / `dashboard` / `settings` / `docs` / `new-connection` 别名 → `main` |
| backup | `backup-singleton` / `backup` |
| data-sync | `data-sync-singleton` / `data-sync` |
| schema-diff | `schema-diff` |
| data-transfer | `data-transfer` |

**Settings** 与 **Docs** 均不是子窗口：

- **Settings**：`openSettingsWindow(section?)` 聚焦 `main` 并 emit `menu:open-settings`，渲染 `SettingsPage`。
- **Docs / Help**：`openDocsWindow(section?)` 在系统浏览器打开 GitHub Pages 使用手册（`src/lib/docsUrls.ts` → `https://flyxl.github.io/datazen/manual.html` 或 `/zh/manual.html`；旧 section id 如 `context` 会 remap 到 manual anchor）。Rust `open_docs_window` 同样打开该 URL。

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

- `openNewConnectionDialog(editId?)` — 主窗内新建/编辑连接对话框
- `openConnectionWindow(...)` — 聚焦主工作区并打开/追加连接 Tab
- `openSettingsWindow(section?)` — 主工作区内 `SettingsPage`（emit `menu:open-settings`）
- `openDocsWindow(section?)` — 系统浏览器打开官网（`open_path` IPC / `window.open` 降级）
- `openDashboardWindow()` / `openWorkflowWindow()` — 主工作区内导航（emit `menu:dashboard` / `menu:workflow`）
- `openDataSyncWindow()` / `openBackupWindow()` — 对应单例子窗口

## 4. 窗口路由

`src/lib/windowKind.ts` 用 URL 参数 `window` 区分：

```typescript
export type WindowKind = 'main' | 'data-sync' | 'schema-diff' | 'backup';

/** Legacy sub-window kinds that now route to the unified main shell. */
const LEGACY_MAIN_ALIASES = new Set([
  'connection', 'workflow', 'dashboard', 'settings', 'docs', 'new-connection',
]);
```

`App.tsx` 按 `getWindowKind()` 懒加载对应页面；legacy 别名一律落到 `main`。

## 5. ErrorBoundary

全局 `ErrorBoundary` 包裹应用，防止未处理 React 错误导致白屏（Dismiss / Reload）。

## 6. 窗口边界与 Store 职责

主工作区（`main`）与子窗口（`*Window`）**各自独立 React 树**，但共享同一 Tauri 后端与持久化 Store。理解边界可避免「在子窗口改状态却期望主窗自动同步」类问题。

### 6.1 窗口 ↔ UI 归属

| 窗口 | 典型 Store / 状态 | 说明 |
|------|-------------------|------|
| **main** | `connectionStore`、`activeConnectionStore`、`panelStore`、`schemaStore`、`tableDataStore`、`settingsStore`、`aiStore`、`dashboardStore`、`workspaceTabsStore` | 连接配置、运行时会话、SQL 面板、表数据、设置、AI、Dashboard 均在此窗口 |
| **backup** | 局部 UI state + IPC | 读写备份任务；通过 `crossWindowBus` 通知主窗刷新 |
| **data-sync** | 局部 endpoint state + IPC | 双端点比较/执行；监听 `datazen:connection-ready` 更新可选连接列表 |
| **schema-diff** | `useSchemaDiffEndpoints` 局部 state | 结构对比双栏；同样监听连接就绪事件 |
| **data-transfer** | 向导步骤局部 state + IPC | 异构 Transfer 六步；不持有主工作区 panel 状态 |

**Settings / Workflow / Dashboard / Docs** 不是子窗口：Settings 与 Workflow/Dashboard 仅在 `main` 内路由切换；Docs 打开系统浏览器。

### 6.2 Store 边界规则

1. **持久化配置 vs 运行时会话**（详见 [naming.md](naming.md)）  
   - `connectionStore`：持久化 `connectionId` 与连接配置列表（Rust `Store` 落盘）。  
   - `activeConnectionStore`：内存态 `dbSessionId` 与连接状态；**永不落盘**。

2. **按连接分区的前端状态**  
   - `schemaStore`、`tableDataStore`：以 `connectionId` 为 key 分区；切换连接 Tab 时切换分区，不跨连接泄漏。  
   - `panelStore`：面板元数据 + 查询执行（`queryExec`）；面板绑定 `{ connectionId, dbSessionId }`；执行 IPC 用 `dbSessionId`，历史/收藏过滤用 `connectionId`。

3. **全局 UI**  
   - `settingsStore`：主题、语言、编辑器偏好；变更经 `emitCrossWindow` 同步到其他窗口。  
   - `uiStore`：侧栏宽度、对话框等纯 UI 壳状态；通常仅 main 使用。

4. **子窗口不拥有主工作区 Panel**  
   Sync / Diff / Transfer / Backup 窗口**不**读写 `panelStore.panels`；需要连接列表时从 `connectionStore` + `activeConnectionStore` 读取，或监听 `datazen:connection-ready`。

### 6.3 跨窗口通信

`src/lib/crossWindowBus.ts`：Tauri 下 `emit`/`listen` 广播到所有 Webview；浏览器 dev 降级为 `BroadcastChannel`。

常用事件（非完整列表）：

| 事件 | 方向 | 用途 |
|------|------|------|
| `datazen:connection-ready` | main → 全体 | `{ connectionId, dbSessionId }`；子窗口更新已连接列表 |
| `datazen:connections-changed` | 任意 → 全体 | 连接配置 CRUD 后刷新列表 |
| `datazen:settings-changed` | settings → 全体 | 主题/语言等即时生效 |
| `menu:open-settings` / `menu:workflow` 等 | Rust 菜单 → main | 主窗内导航，非新 OS 窗口 |

新增跨窗口状态时：优先 **事件 + 最小 payload**，避免在子窗口复制整份 `panelStore`；持久化数据一律经 Rust `Store` IPC，不在窗口间手动 sync 大对象。

### 6.4 相关文档

- [前端状态管理](frontend/state.md) — Store 拆分与 selector 约定  
- [ID 术语规范](naming.md) — `connectionId` vs `dbSessionId`  
- [持久化存储](backend/store.md) — 后端落盘边界
