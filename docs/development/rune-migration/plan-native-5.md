# Rune 原生宿主第五阶段：布局对齐 React 版

前四阶段交付了演示界面、活宿主、宿主能力分发与 basic 驱动。本阶段把 Rune UI 的**布局骨架**对齐 Tauri/React 版：自绘标题栏、两级侧栏、工具栏、面板标签条、状态栏、可拖拽分割与布局持久化，并把网格/编辑器的硬编码暗色换成主题 token（亮色正确）。

React 继续保留，规则与前几阶段相同。本阶段不改 `packages/frontend`、`packages/web` 与内核 `crates/datazen` 的公开契约（settings 走既有无类型 JSON 文档，不碰 `SharedSettings`）。

## 差距（写于立项时，两侧行号均为勘察实测）

| 框架要素 | React 版（基准） | Rune 版现状 | 差距 |
|---|---|---|---|
| 标题栏 | 自绘 `TitleBar` h-10（`desktop/TitleBar.tsx:164`）：左 MenuBar，右 ThemeToggle；macOS Overlay 拖拽 | 无自绘标题栏；header 48 右侧只有 DARK/LIGHT 文字 | 缺整个 chrome 层 |
| 侧栏 | 两级：图标 rail `w-10`/展开 `w-28` 可折叠持久化（`WorkspaceModeSidebar.tsx:99`）+ navigator 树 280（min 200/max 500，`useResizable` 持久化 `connection-sidebar-width`） | 单个固定 224 侧栏混排导航/树/按钮 | 结构性不同 |
| 工具栏 | `ContentToolbar` h-12（`ContentToolbar.tsx:132`） | 无 | 缺 |
| 面板标签 | `PanelTabBar` h-12：tablist + Arrow/Home/End 键盘 + active 下划线（`PanelTabBar.tsx:60-64`） | tab strip 32，仅 Table/Query，无键盘导航 | 高度与能力 |
| 状态栏 | `ContentStatusBar` h-10：连接状态 + 库/表/列/行计数 + 快捷键提示（`ContentStatusBar.tsx:67-69`） | 底部 "Migration preview" 占位横幅，非状态栏 | 缺 |
| 可调整性 | 分割区全程可拖 + `resize:<key>` localStorage 持久化（`useResizable.ts:36`；uiStore 布局字段） | 0 个分割条、0 个持久化布局状态，高度全为常量 | 根本性 |
| 主题 | 全量主题 token（Tailwind），light/dark/system + theme pack | 网格表头/单元格/ER/补全为硬编码暗色 oklch（`data_grid.rs:905-915` 等），亮色下网格仍暗 | 亮色不正确 |
| 图标 | lucide 图标族 | 零图标资源，纯文字 | rail 无法折叠成形 |

## 调查结论（写于立项时）

- **rune 框架能力足够**：
  - 自绘标题栏是内置能力：`Titlebar::new(height)`（`vendor/rune/.../window/titlebar.rs:32`）+ `App::titlebar`（`app.rs:189`）。rune 移除原生标题栏；macOS 红绿灯进带内（`Chrome::DEFAULT.full_size_content = true`，`Chrome::traffic_lights` 可定位）；Windows 由 rune 画 caption 按钮（带内尾缘）；应用内容用 `.window_drag()` 标记拖拽区，带内控件照常可点；`Cx::titlebar_inset` 报告窗口控件占用的前缘宽度。
  - 连续拖拽手势现成：`Drag { phase, x, y, start_x, start_y, dx, dy, data, clicks }`（`el/handlers.rs:117`），`data` 跨相位保持起抓值——分割条只需 `on_drag` 里按 `dx/dy` 换算新尺寸。`on_drag_end(DragOutcome)` 可处理取消。
  - `AppSpec.min_size` 现成（对齐 React 主窗 960×640）。
  - 多窗口（`WindowSpec`/`run_windows`）存在，但 React 的五个独立窗口本阶段**维持页内折叠形态**，多窗化决策另立任务（不阻塞布局对齐）。
- **持久化载体已定**：宿主写 settings 走无类型 JSON 文档（`live.rs:2147` `save_app_settings(document)`），`"ui"` 键下自由键值（`themeDark/tablePageSize/queryMaxRows`）。布局尺寸新增 `Setting::Layout { key, value }` 骑同一条已测链路，`ui.layout` 子对象存储；**不动内核 `SharedSettings` 契约**。`Setting` 是 `Eq` 派生，值用整数 pt（React 所有布局值都是整数；拖拽的分数在持久化前四舍五入）。
- **React 基准数字**（对齐目标）：标题栏 40；rail 折叠 40/展开 112；navigator 280（min 200/max 500）；ContentToolbar 48；PanelTabBar 48；状态栏 40；查询编辑器默认高 320（`query-editor-height` 持久化）；结果 pane 高上限 50%。Rune 侧对齐值在各自任务里落常量与默认。
- **图标**：rune 依赖已开 `svg` feature（workspace `Cargo.toml:68`）。rail 图标用内联 SVG 描边路径对齐 lucide 语义（ISC 许可），不引外部资源文件。

## 不变量（承接前四阶段，新增三条）

1. 承接前四阶段全部不变量（plan.md、plan-native*.md）。
2. **布局尺寸一律经 `Setting::Layout` 持久化**：读走 `ui_settings()` 的缓存路径（memory-speed），写走 `write_ui_setting`；拖拽中只在内存暂存，拖完才落设置。语义设置（主题/页大小/最大行数）不挪键。
3. **无图标不阻塞结构**：rail 图标用内联 SVG（lucide 语义），不引位图、不引外部文件；亮色正确性以 theme token 为准，禁止新增硬编码 oklch。

## 门禁

沿用 GR1–GR4；G1–G4（前端）不适用。本阶段无驱动改动，GR5 不涉及。

## 任务

### 框架对齐（L 系列）

| 任务 | 一天内做完的事 | 完成标准 |
|---|---|---|
| L1 | `Setting::Layout { key, value: u32 }` 端到端：ui-api 枚举 + 共享校验（key 形状 + 值范围）+ `setting_kind` 按键去重 + MemoryWorkspace 校验；宿主 live.rs 读写映射（`ui.layout` 子对象）+ 单测；ui 侧 `layout_size`/`set_layout_size` helper | 测：宿主存取往返与非法值拒绝；memory 端口同键覆盖 |
| L2 | `splitter.rs` 分割条组件：Drag 手势 → 尺寸 clamp → `Setting::Layout` 持久化；垂直/水平两向；`role="separator"` + a11y 值；双击复位默认；箭头键微调 | 测：drag 处理纯函数（clamp/复位/增量/取消回滚）单测 |
| L3 | 自绘标题栏：`App::titlebar(40)` + `.window_drag()` 带 + 左侧 section 标题 + 右侧主题切换按钮（对齐 ThemeToggle 位置）+ `min_size` 960×640 + `Cx::titlebar_inset` 让位红绿灯；demo 与宿主两个入口同改 | 测：view 树断言标题栏元素与拖拽区；两个入口的构建单测 |
| L4 | 两级侧栏：40pt 图标 rail（7 个内联 SVG，lucide 语义）+ 展开 112pt 文字 + 折叠持久化；对象树挪入内容区 navigator 面板（280pt 可拖拽 min 200/max 500，持久化） | 测：折叠/展开切换 + 宽度持久化往返 + 树照常渲染 |
| L5 | `ContentToolbar` 48pt（按 section 出动作行）+ `PanelTabBar` 48pt（tablist 键盘 Arrow/Home/End、active 下划线、关闭钮） | 测：tab 条键盘导航与关闭行为单测 |
| L6 | `ContentStatusBar` 40pt：连接状态 + 库/表/列/行计数 + 快捷键提示；替换 "Migration preview" 横幅 | 测：状态文案随状态变化单测 |
| L7 | 主题修正：`data_grid.rs`/`sql_editor.rs`/ER 的硬编码 oklch → theme token；亮色模式下网格/补全/ER 与界面一致 | 测：亮色渲染断言（token 取值随主题翻转） |

### 逐页对齐（A 系列，框架落地后开）

| 任务 | 一天内做完的事 | 完成标准 |
|---|---|---|
| A1 | 连接首页（Hero + 连接卡列表 + 最近查询）与连接树层级（pinned/最近/分组/值搜索） | 测：首页元素与树层级渲染断言 |
| A2 | 连接表单对话框化（分组标准字段 + SSH 隧道族，对齐 NewConnectionDialog） | 测：必填校验与字段分组渲染 |
| A3 | 查询面板对齐：编辑器高度可拖持久化 + 工具栏动作群（执行/格式化/explain/片段/事务）+ 收藏/历史侧栏 + 结果多 tab（pin） | 测：分割持久化 + 多结果 tab 切换 |
| A4 | 表浏览对齐：行号 gutter、列宽按表持久化、FK 面包屑、状态横幅、待提交条 | 测：行号渲染与列宽持久化往返 |
| A5 | 结果工作区对齐：表格/图表 toggle + 状态条 + FindBar | 测：find 命中行为 |
| A6 | 设置导航分组（12 section × 2 组 + 深链形态） | 测：导航切换与 section 渲染 |
