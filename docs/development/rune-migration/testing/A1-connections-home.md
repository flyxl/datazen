# A1 连接首页/树逐页对齐（阶段一：首页）验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 任务：A1 连接首页/树逐页对齐（本提交覆盖连接首页：首页 Hero + 连接卡列表 + 最近查询区块的结构与样式对齐；连接树层级 pinned/最近/分组/值搜索后续继续）。
- 被测提交：见 tasks.md A1 行（feat）；本报告随 docs 提交落库。
- 改动面：`crates/datazen-ui/src/lib.rs`（`connection_list`/Connections 主页重排、测试）。不改 `packages/frontend`、`packages/web`、宿主与内核契约。

## 结论

通过。Connections 主页由扁平列表重构为对齐 React 首页的信息结构：`connections-home-hero`（标题与连接数/引擎类型数摘要）、`connections-card-list`（“Your connections”分组标题与卡片式连接项，保留 `connection-{id}` 交互键与既有消息/加载/表单/导入能力）、`connections-recent-queries`（“Recent queries”区块；当前无历史数据源时显示空态）。既有连接表单保存、导入、桌面适配器消息与连接打开行为保持兼容。

## 实现面

- **Hero**：`connections-home-hero`/`connections-home-title`/`connections-home-subtitle`，基于当前可见连接列表统计数量与引擎种类。
- **卡片列表**：`connections-card-list` + `connections-section-title`，每项保留 `key("connection-{id}")` 与点击打开行为，展示名称/引擎与 Connect 视觉按钮。
- **最近查询**：`connections-recent-queries` + `connections-recent-title` + `connections-recent-empty`（结构占位，待历史数据源接入后可填充）。
- **兼容**：保留 `connection-list` 根键、`desktop-adapter-message`、`connect-loading`、`connection-form*`/输入与保存键、导入相关键。

## 测试

| 测试 | 位置 | 覆盖 |
|---|---|---|
| `connections_home_shows_hero_cards_and_recent_queries_section` | `crates/datazen-ui/src/lib.rs` | hero/卡片区/最近查询区块与关键文案/键存在 |
| 既有连接相关测试（8 项） | 同上 | 连接页交互、表单、导入、适配器消息、打开连接等兼容 |

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR2 | `CARGO_TARGET_DIR=$PWD/target cargo test -p datazen-ui --offline --lib -- --test-threads=1` | **160 passed / 0 failed** |
| GR4 | `CARGO_TARGET_DIR=$PWD/target cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| GR1 | 沿用（未改 ui-api） | 56 passed |
| GR3 | 沿用（未改宿主） | 59+3 环境失败 |

## 备注

- A1 全量目标还包含连接树层级（pinned/最近/分组/值搜索）的逐页对齐，本报告记录首页部分；后续提交继续推进树侧对齐。
- 仅改 Rune UI 与文档；React/内核契约未动。


## 增量：导航器过滤（树侧）

- **提交**：`14d021f5e`（feat），`655f7e8d9`（docs）
- **改动**：`navigator_panel` 增加 `navigator-filter-input`；`object_tree` 支持按名称过滤数据库/模式/表并在无匹配时显示 `object-tree-empty`（“No objects”/“No matches”）。
- **测试**：`navigator_filter_filters_object_tree_nodes`；GR2 **161 passed / 0 failed**。

## 增量：导航器连接树层级（Recent / All，进行中）

- **提交**：feat `73261fa10`，docs 本次提交
- **改动**：`navigator_panel` 增加 `navigator-connections` 连接树（A1）：`nav-group-recent`（会话内最近打开，`AppState::recent_connection_ids`，UI 状态、非持久化/无契约变更）与 `nav-group-all`（全部可见连接）两级分组；`navigator-filter-input` 同时过滤连接名/引擎/id；点击 `nav-conn-*` / `nav-conn-recent-*` 打开连接并记录 recent。保留既有 `object-tree` 与对象过滤行为。
- **测试**：`navigator_connections_tree_shows_recent_and_all_groups_and_filters`；既有 `navigator_filter_filters_object_tree_nodes`、`the_navigator_panel_holds_the_tree_and_persists_width` 通过。
- **门禁**：GR2 **162 passed / 0 failed**（`--test-threads=1`）；GR4 `cargo check -p datazen-rune-host --offline` 通过。

说明：A1 目标中的 pinned/用户分组仍依赖 `ConnectionProfile`/公开契约未承载的元数据，未在本增量臆造；已在契约允许范围内提供“最近（会话态）/全部”树层级与值过滤，并持续记录差距。

## 增量：首页像素对齐（空态 + 落地页 + 壳层溢出修复）

- **提交**：feat `729eae712`，docs 本次提交
- **取证**：headless 截图对照（`datazen-native`/`datazen-rune` `--snap/--tree --size 1440x816`）。改版前首页为裸 hero；对照 React `ConnectionWorkspaceHome`/`HomeHero`/`ConnectionCardList`/`RecentQueriesList`/`ShortcutFooter` 重排后，`--tree` 逐元素核对 bounds。
- **壳层修复（影响全部页）**：`app.rs` 的 `datazen-body` 原为 `h_full`（=整窗 816），叠在 40pt 自绘标题栏下整体越界 40pt（导航/内容列 bottom 856、状态栏文本 804.5..819.5 被 816 裁掉 descender）；改为 `grow(1.0)+min_h(0.0)` 后 `datazen-body` 40..816(776)，`content-status-bar` 752..792、文本 764.5..779.5 完整入窗。
- **空态（React State 1）**：`connections-empty` 居中卡（图标 48 + "No connections yet" + 提示 + Import ghost + Create first primary），`import-connections`/`connections-create-first` 键保留；导入条（密码输入+提示）以 12pt 间距居中挂在卡片正下（`--tree`：卡片中心 x=918.5，导入条中心 x=918.5 完全对齐）。
- **落地页（React State 3）**：`connections-home-hero`（"Select a connection" 17pt/700 + 加粗计数 rich 文本 + 右侧引擎徽章簇，引擎 hue 稳定映射 postgres=256/sqlite=152/mysql=58 等）、`connections-header`（"YOUR CONNECTIONS" 大写靠左 + 筛选输入 + Import + New connection 靠右，justify-between）、`connections-card-list` 边框圆角卡（引擎徽章 34、名称+小 chip、PostgreSQL mono 显示名、`conn-status-{id}` 状态 pill Connected/Connecting…/Offline、`conn-open-{id}` Connect/Open 按钮、>6 条 `connections-showing`+`connections-show-all` 分页、无匹配 `connections-no-match`）、`connections-recent-queries` 空态卡（⏱ No recent queries，诚实占位）、`connections-shortcut-footer`（⌘↩ Run query）。内容列居中 max-w 1008 + scroll。
- **状态判定**：`connection_is_open`（desktop 连接 id 或 workspace 会话匹配）驱动 Connected/Open；Connecting… pill 仅在 `connect_loading` 且 recent 首位即该连接时显示。
- **诚实占位**：Recent queries 数据源（host 未暴露 query history）与 shortcut footer 仅列出真实接线的 ⌘↩。
- **测试**：新增 3 项——`connections_home_without_connections_centers_the_empty_state`、`connections_home_filters_and_pages_the_card_list`（筛选软过滤、no-match、6 条分页/Show all）、`connection_cards_show_the_status_pill_and_the_connected_open_button`；既有 176 项（导入、表单、适配器横幅、连接打开等）保持兼容。
- **门禁**：GR2 **179 passed / 0 failed**（`--test-threads=1`）；GR4 `cargo check -p datazen-rune-host --offline` 通过（8 个既有 warnings）。
- **截图**：改版后空态 `rune-home-empty-v2.png`、落地页 `rune-home-landing-v2.png`（1440x816）已与 React 基准逐块比对：居中/两端对齐/卡列表结构一致；差异保留项——Recent queries 无数据、footer 仅一项快捷键（契约限制，诚实展示）。

## 增量：侧栏 rail 对齐与真实弹窗层

- **提交**：feat `0aa7fa445`，docs 本次提交
- **侧栏（React `WorkspaceModeSidebar` 对齐）**：顶部分组顺序改为 Connections / Workflows / Dashboards / Workspace / Tools / Wapps；Settings 与折叠按钮移入底部组（`rail-bottom`，上方弹性占位），折叠按钮改为 lucide panel-left-close/open（展开态显示 "Collapse sidebar" 并可截断），图标与尺寸对齐 React 语义（16pt）。激活项改为 accent/20 底色 + accent 前景；展开态标签可截断。
- **真实弹窗层**：新增 `views/modals.rs` 的 root modal layer（`modal_layer` 挂在 `datazen-root` 尾部）：全窗 scrim（`oklcha(0,0,256,0.5)`）+ 居中面板 + `.modal(false, dismiss)`，背景 rail/navigator/page inert，Escape 关闭。导入弹窗（460pt）承载格式提示、选文件、密码、预览/确认/结果；连接表单弹窗（700pt）承载分组字段与表单自带操作行。`connections_home` 移除内联导入条，按钮改为打开导入弹窗；`connection_list` 不再内联表单，改由 modal layer 呈现。
- **状态与测试适配**：`AppState` 增加 `import_dialog_open`/`import_file`；`open_import_dialog`/`close_import_dialog` 管理导入旅程；`io_tools::the_connections_page_imports_a_previewed_file` 适配“先开弹窗再选文件”流程（导入确认后 notice 仍保持可见，兼容既有断言）。
- **新增测试**（`crates/datazen-ui/src/tests/rail_modals.rs`）：`the_rail_orders_sections_like_the_react_sidebar`、`the_rail_uses_lucide_panel_icons`、`the_import_dialog_is_a_modal_over_the_page`、`the_connection_dialog_is_a_modal_over_the_page`（含背景 inert 与 Escape/Cancel 关闭，弹窗入场后 settle 再指针交互）。
- **门禁**：GR1 `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` → **56 + 183 passed, 0 failed**；GR2/GR4 `cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。G1–G4 不适用（未改 `packages/frontend` / `packages/web`）。
- **视觉取证**：release `datazen-rune --snap/--tree --size 1440x816`（含 `--script` 点击打开弹窗）核对 rail 顺序/底部组与导入/连接浮层居中及遮罩。

## 增量：导航器基础组件与 ConnectionNavigatorTree 集成

- **提交**：feat `c0da43bd0`，docs `e390028b4`；修复（分组图标与收起）见本次提交（代码与文档同提交）。
- **目标**：按“先基础 UI 组件、再复杂组件”的方向，修复连接导航器样式并将重构后的 ConnectionNavigatorTree 集成到页面（`navigator_panel` / 主壳页面）。
- **基础组件层（Rune 原语薄封装）**：新增 `crates/datazen-ui/src/ui/{mod,theme,nav}.rs`
  - `theme`：导航器统一度量与状态色映射（行高/缩进/图标/圆角、hover/selected/前景层级，基于 `tok::Theme`）。
  - `nav`：`search_field`（带搜索图标、保留 `navigator-filter-input` 键）、`icon_button_action`、`nav_section_header`（含语义图标）、`nav_empty_row`、`nav_connection_row`、`nav_branch_row`、`nav_leaf_row` 等树/工具栏原语。
  - `icons` 补齐 lucide 几何图标：`chevron-right/down`、`folder/folder-open`、`table`、`refresh-cw`、`layout` 等，供导航与操作行复用。
- **ConnectionNavigatorTree / 导航器面板**（`views/navigation.rs`）
  - 顶部改为 Databases 标题栏 + “New query” 图标按钮（保留 `open-query` 键）+ 带图标搜索框。
  - 连接树保留并样式化 `Recent` / `All connections` 分组（`nav-group-recent` / `nav-group-all`）：分组头带 chevron + 语义图标（Recent=clock，All=database）+ 计数，**可点击收起/展开**（UI 折叠态复用 `AppState.collapsed`，`nav:recent`/`nav:all`；过滤时自动展开）；空态与过滤行为保留，连接项为图标 + 名称/引擎双行样式并支持当前连接高亮（仍使用 `nav-conn-*` / `nav-conn-recent-*` 键与原交互）。
  - 对象树改为分支/叶子行原语（数据库/模式展开 chevron 与图标、表图标、缩进、计数标签），保留 `object-tree` / `tree-db-*` / `tree-schema-*` / `tree-table-*` / `object-tree-empty` 及展开/打开行为；底部操作行（刷新/ER/断开）统一为图标+文本样式。
- **兼容性**：未改 `packages/frontend`/`packages/web`、内核契约与 `datazen-ui-api`；保留既有 navigator 相关 key 与测试契约。
- **门禁**：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` → **56 + 183 passed, 0 failed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。G1–G4 不适用（未改 `packages/frontend` / `packages/web`）。
