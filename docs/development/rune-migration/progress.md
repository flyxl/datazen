## Current Task
- **任务**: A1 收口与全局视觉/交互对齐验证（plan-native-5）— A2–A6 已合流主轨，待统一门禁、契约内收尾与逐页视觉一致性校验
- **负责人**: Coder Agent
- **状态**: L1–L7 已完成；A1 进行中（首页与导航器 Recent/All/过滤已落地；单行输入框统一由 `ui/field.rs::TextField` 组件（rune-kit 形状的 builder）构建并垂直居中，覆盖首页搜索、设置、仪表盘、网格与 SQL 编辑器查找栏；pinned/用户分组受契约限制）。A2 连接表单对话框化、A3 查询面板对齐、A4 表浏览对齐、A5 结果工作区对齐、A6 设置导航分组已合流主轨。第五阶段计划见 [plan-native-5.md](plan-native-5.md)。逐页 bounds 扫描（headless `--snap/--tree`，1440×816 与 960×640、dark/light）已在本次收口；A1 仍余契约受限的 pinned/用户分组。

## Completed Work (recent)
- [x] 新建/导入连接窗口布局重构（本次提交）：`views/modals.rs`、`views/connections.rs` 重构为 React 对齐布局。新建连接：左侧数据库类型选择栏 + 右侧标题/可滚动表单/底部操作区，表单按两列网格排列基础与连接字段并保留高级/隧道区块结构；导入连接：对齐分享对话框导入模式的提示、文件选择、密码与预览/结果区域及底部操作。保持既有键与行为兼容（`connection-dialog*`、`connection-form*`、`connection-save`、`import-dialog*`、`import-choose-file`、`import-password-input`、`import-confirm` 等），能力受当前 `AppState`/`datazen-ui-api` 约束。（见 [testing/A1-connections-home.md](testing/A1-connections-home.md)、[testing/A2-connection-form-dialog.md](testing/A2-connection-form-dialog.md)）
- [x] A1 导航器基础组件与 ConnectionNavigatorTree 集成/样式重构（`c0da43bd0`，修复见本次提交）：新增 `crates/datazen-ui/src/ui/{mod,theme,nav}.rs` 基础原语（主题度量/图标/搜索框/工具按钮/树 section-branch-leaf 行），重建 `navigator_panel` 与 `navigator_connections_tree`（Databases 标题栏、带搜索图标的过滤输入、Recent/All 分组头与连接行、对象树分支/叶子图标与缩进）并集成到页面；连接树分组头补充语义图标且可点击收起/展开（Recent/All，过滤时自动展开），保持既有 key 与交互（`navigator-filter-input`、`nav-group-*`、`nav-conn*`、`object-tree`/`tree-*`、`open-query` 等）。（见 [testing/A1-connections-home.md](testing/A1-connections-home.md)）
- [x] A1 rail/弹窗对齐增量（`0aa7fa445`）：侧栏按 React 顺序重排并将 Settings/折叠按钮置于底部组；图标改为 lucide 语义；连接表单与导入改为 root modal layer（scrim + Escape，真浮层）。（见 [testing/A1-connections-home.md](testing/A1-connections-home.md)）
- [x] A2 连接表单对话框化（`1e383c421`, `7e06bf48e`，合流主轨；见 [testing/A2-connection-form-dialog.md](testing/A2-connection-form-dialog.md)）
- [x] A3 查询面板对齐（`3630c95f7`, `020a62b22`，合流主轨；见 [testing/A3-query-panel.md](testing/A3-query-panel.md)）
- [x] A4 表浏览对齐（`e041fc463`, `5ae657365`, `f8879f36c`，合流主轨；见 [testing/A4-table-browse.md](testing/A4-table-browse.md)）
- [x] A5 结果工作区对齐（`b7b25b518`, `ef38e1069`，合流主轨；见 [testing/A5-result-workspace.md](testing/A5-result-workspace.md)）
- [x] A6 设置导航分组（`27c5f2292`, `95b4a4c30`，合流主轨；见 [testing/A6-settings-nav.md](testing/A6-settings-nav.md)）

## N5 组装配方（调查结论，fc160ec 时点）

`RuntimeConnectionUseCases::new(profiles, policy, backend)` 内部自建全部管道（SessionRegistry over ObservedBackend、ExecutionGateway + BoundAuthorizer + 内存幂等、会话目录、令牌环、结果 sink）——宿主只需三个部件：

1. **profiles: `Arc<dyn ProfileRepository>`**（`connection::domain::profile`，7 个方法：list/get/create/compare_and_set/disable/advance_credential_revision）——由文件连接存储投影；`ProfileRecord` 带版本语义，CAS 冲突回 `PortError::CasConflict`。
2. **policy: `Arc<dyn PolicyService>`**（`execution::application::policy`：authorize/verify_delegation/authorize_target/isolation_key/subscribe_version_changes）——单租户宿主的允许策略 + 版本流。
3. **backend: `Arc<dyn SessionBackend>`**（`execution::runtime::registry::backend`：open/open_for_command/execute/cancel/finalize_handles/close/observability_snapshot）——走驱动的 ResourceProvider 契约（sqlite 已有 `SqliteResourceProvider`，经 factory 的 `resource_provider()` 取），资源语义参照桌面 `platform/session_backend{,.rs,/*}`（1480 行，含池化/隔离/预算——单会话宿主可裁）与 server `execution/driver_exec.rs`（500 行，含流式/守卫）。

提交路径（N6）额外需要：`ConnectionFacts`、`TableColumnSource`（schema 目录命令投影）、`ArtifactChunkReader`（runtime 固有 `read_artifact_chunk`，结果经 sink + `FileArtifactRepository` 落盘——桌面在 `platform/adapter.rs:480` 一带的装配是参照）。查询（N7）同理走 runtime 会话。

关键事实：桌面测试注释确认"Test drivers are registered via register_test_driver, not inventory catalog"——静态 registry（N3 的 `with_factories` 形态）在宿主测试与真机行为一致，这个决定已被 N3 验证。

## 约束
React 页面和组件保留。不删除、不改成空壳、不把现有入口摘掉。Rune 是并行的另一套界面。

## Completed Work
- [x] R0 Rune crate 能编译
- [x] R1 SQL Editor POC
- [x] R2 DataGrid POC
- [x] R3 UI 端口和内存表页
- [x] S1–S6、B1–B8、Q、O、P、D、W、H、C1–C2 全部完成（提交与测试报告见 [tasks.md](tasks.md)）
- [x] C3 收尾测量（数字见下节）
- [x] C4 未迁移区域提示（WAPPS/Onboarding 告知 React 页面还在，见 [testing/C4-unmigrated-notice.md](testing/C4-unmigrated-notice.md)）
- [x] A4 表浏览对齐：行号 gutter、按表列宽持久化与回填、FK 面包屑、状态横幅、待提交条（`e041fc463`, `5ae657365`；测试报告 `testing/A4-table-browse.md`）

## Verification
- 单行输入框居中与 `TextField` 组件（本次）：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` → **56 + 190 passed, 0 failed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。
- 最小窗口布局收口（本次）：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` → **56 + 187 passed, 0 failed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。
- 新建/导入连接窗口布局重构（本次提交）：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` → **183 + 56 passed, 0 failed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。
- A1 导航器基础组件与 ConnectionNavigatorTree 集成/样式重构（`c0da43bd0` + 本次修复）：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` → **183 + 56 passed, 0 failed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。
- 主轨集成（A2–A6 合流，含冲突解决）：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui --offline --lib -- --test-threads=1` → **176 passed, 0 failed**；`CARGO_TARGET_DIR="$PWD/target" cargo check -p datazen-rune-host --offline` → exit 0（既有 warnings）。
- A1 rail/弹窗对齐增量（`0aa7fa445`）：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui --offline --lib -- --test-threads=1` → **183 passed, 0 failed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。
- 分支/轨道门禁（合流前）：A2 163 passed；A4 164 passed（含回归修复）；A3 169 passed（含 query_workspace）；A5 171 passed（含 data_grid find 命中测试）；A6 165 passed。
- 历史基线（C3 收尾）：`cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：datazen-ui 113 passed、datazen-ui-api 54 passed。

## 收尾测量（C3）

2 万行和 200 列在 Rune 网格上的实测数字，量自产品面（表标签页的表格视图），量法是 C2 建立的无障碍树读法——树里一行节点就是一行挂载，一格节点就是一格建成：

- **2 万行的页挂载 21 行**：首帧静止（视口 1440×900、sheet 高 380pt、行高 28pt）时，挂载窗口为列表头部的 21 行，与列表长度无关。数字由 `twenty_thousand_rows_mount_a_window_and_two_hundred_columns_build_one_window_of_cells` 断言钉死（报告见 [testing/C3-mount-counts.md](testing/C3-mount-counts.md)）。
- **200 列的页每行建 12 格**：列窗口 COL_WINDOW=12，静止时冻结首列落在窗口内共 12 列，表头同样建 12 个；横向滚动后冻结列在窗口外另建一格，每行 13 格（既有单元测试的上界 ≤ COL_WINDOW+1）。
- 量法不改产品代码：规模页经既有 `load_page` 装入——2 万条空记录，格值走网格的 `r{r}c{c}` 回退，页只占约 480 KB 而非 400 万格。

## 增量：首页对齐 Tauri/React 参考（本次）

- **目标**：按 Tauri 参考截图（`site/assets/screenshots/01-main-window.png`）与 React `ConnectionWorkspaceHome`/home 组件对齐 Rune Connections 首页布局与观感（在当前契约/数据限制内）。
- **改动**：`crates/datazen-ui/src/views/connections_home.rs`、`crates/datazen-ui/src/icons.rs`
  - Hero：标题字号/权重与间距调整；副标题改为 `N connections · M groups · K engine types` 三段加粗计数（groups 因 `ConnectionProfile` 无分组元数据暂显示 0）；引擎徽章簇与对齐细节微调。
  - 连接区头部：保留 `connections-header`/`connections-section-title`/`home-connections-filter`/`import-connections`/`connection-new`，筛选框增加搜索图标容器与更贴近 React 的按钮文案/样式。
  - 连接卡片：卡片行高度/间距/hover、名称与引擎 chip、元信息行、状态 pill 与 Connect/Open 按钮样式对齐；Connect 按钮带播放图标；保持 `connection-{id}`、`conn-status-*`、`conn-open-*`、分页键等行为与测试兼容。
  - 最近查询：保留 `connections-recent-queries`/`connections-recent-title`/`connections-recent-card`/`connections-recent-empty`，头部增加右侧 `View all` 占位（`connections-recent-view-all`），空态文案/图标居中布局对齐。
  - 新增 MCP promo bar（`home-mcp-promo-bar`）：图标+标题/描述+命令展示+Copy 占位，契约未暴露可执行路径/复制能力时仅做布局对齐与诚实展示。
  - 快捷键 footer：保留 `connections-shortcut-footer`/`connections-shortcut-key`，扩展为多组快捷键提示（Run/Save/Format）以贴近 React footer 布局。
  - 图标：补充 `ICON_PLAY`、`ICON_COPY`、`ICON_TERMINAL`、`ICON_SPARKLES`。
- **限制**：未改 `packages/frontend`/`packages/web`、内核契约与 `datazen-ui-api`；`ConnectionProfile` 无 host/port/group、无真实 query history/MCP 配置来源，相关内容以现有数据与诚实占位呈现。
- **门禁**：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` → **56 + 183 passed, 0 failed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。本环境无法抓取运行中 GUI 截图（display capture 不可用），以本地参考图与 React 实现对齐。

## 增量：主窗口左侧功能栏对齐 React（本次）

- **目标**：对齐 React `WorkspaceModeSidebar` 左侧主导航栏（`Databases / Workflow / Dashboard / Workspace / Extensions`，底部 `Settings` + 折叠）。
- **改动**：`crates/datazen-ui/src/views/chrome.rs`、`crates/datazen-ui/src/types.rs`、`crates/datazen-ui/src/tests/{rail_modals.rs,host_jobs.rs,shell_grid.rs,io_tools.rs,sync_transfer.rs}`、`docs/development/rune-migration/testing/A1-connections-home.md`
  - Rail 顶部改为 React 顺序与标签：`Databases`、`Workflow`、`Dashboard`、`Workspace`、`Extensions`；底部为 `Settings` 与折叠按钮；移除非 React 主侧栏项 `Tools` 的 rail 入口（`Section::Tools` 功能仍保留在代码中，但不再显示在主 rail）。
  - `Section::label()` 对齐主导航展示文案（`Connections→Databases`、`Workflows→Workflow`、`Dashboards→Dashboard`、`Wapps→Extensions`）。
  - Rail 视觉细化：去圆角项样式、紧凑分组间距、卡片底色与右侧边框，贴近 React 侧栏观感。
  - 测试适配：更新 rail 顺序/图标/存在性断言；原依赖 `rail-tools` 点击进入 Tools 的用例改为 `state.select(Section::Tools)` 以保持行为覆盖。
- **门禁**：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` → **183 + 56 passed, 0 failed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。

## 增量：导航器搜索/分割线与对象树显示修复（本次）

- **问题**：连接搜索框文本未与图标垂直居中；导航器与主内容区分割线过粗；未连接会话时导航器下方出现多余水平分割线与 “No objects” 提示。
- **改动**：`crates/datazen-ui/src/ui/nav.rs`、`crates/datazen-ui/src/views/navigation.rs`、`crates/datazen-ui/src/splitter.rs`、`crates/datazen-ui/src/tests/{host_jobs.rs,workspace_tabs.rs}`
  - 搜索框：`search_field` 输入控件增加垂直居中对齐，保证文本与搜索图标对齐。
  - 分割线：导航器/主内容区 splitter 改为 1px 细分割线样式（更贴近 React）。
  - 对象树区域：仅在 `session_open()` 时渲染对象树分割线、对象树与会话操作区；无会话时不再显示多余分割线和 “No objects” 空态。同步调整相关测试期望。
- **门禁**：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` → **183 + 56 passed, 0 failed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。

## 增量：连接/导航搜索框垂直对齐微调（本次）
- 左侧导航 `Search…`（`navigator-filter-input`）与首页 `Filter connections…`（`home-connections-filter`）改为单行输入，并将输入框高度收敛到居中容器内（`h(20)` + `center_y`），以改善文字与搜索图标的垂直对齐。
- 门禁：`cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` → **183 + 56 passed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。

## 增量：最小窗口布局收口（本次）

- **取证方式**：headless `./target/debug/datazen-rune --snap/--tree/--size/--theme/--script/--at`（截图 + 元素树 + 按 key 的点击脚本），在 1440×816 与最小窗口 960×640、dark/light 两套主题下逐页（connections/workflows/dashboards/workspace/wapps/settings）与逐 section（settings 12 个 section）扫描元素 bounds，比对整窗宽高找越界节点。
- **修复**：
  - `crates/datazen-ui/src/app.rs`：标题栏 label `w_full` → `grow(1.0).min_w(0.0)`，主题切换按钮不再被推出窗口右缘。
  - `crates/datazen-ui/src/views/settings.rs`：`settings_page` 由 `h_full` 改 `grow(1.0).min_h(0.0).min_w(0.0)`（原先按整列 padding box 测量，把 `content-status-bar` 推出窗口底部）；settings 的 label/控件行（`settings-data`/`export-form`/`import-step-1`/`snippet-form`）在窄窗换行，placeholder 文案 `wrap`，favorite 名称框改为 `grow`，snippet SQL 框允许收缩。
  - `crates/datazen-ui/src/views/connections_home.rs`：960 宽下 header 动作行、快捷键 footer 允许换行；MCP promo 的文案列 `grow` 且副标题 `wrap().max_lines(2)`，去掉多余的 grow 占位。
  - `crates/datazen-ui/src/views/dashboards_workflows.rs`：widget 查询输入 `.shrink()`，960 宽下 Refresh 不再越界。
- **回归测试**（`crates/datazen-ui/src/tests/chrome_metrics.rs`）：`the_titlebar_keeps_its_theme_toggle_inside_the_window`（标题栏两档窗口）、`every_section_keeps_its_controls_inside_the_minimum_window_width`（六页 × 960×640 横向不越界）、`every_settings_section_keeps_its_rows_inside_the_minimum_window`（12 个 section 内容列不越界）、`the_settings_page_ends_above_the_status_bar`（页底不压状态栏）。前两条修复均已反向验证：回退对应源码后测试失败、恢复后通过。
- **结果**：1440×816 与 960×640（含 light 主题）下，六页与 settings 全部 section 无横向越界；settings 页在 1440×816 收在 144..736、状态栏 752..792，全部内容在窗内；960×640 下仅连接首页滚动内容（MCP 条与快捷键 footer）低于可视区，由 `connection-list` 滚动到达，属预期。

## 增量：连接对话框页脚动作区对齐（本次）

- **问题**：新建连接对话框的保存按钮在滚动表单末尾（960x640 下表单高 493pt 超过滚动视口 496pt，保存落在视口外），页脚只有测试连接/取消，且取消重复一份；React 的页脚固定为「测试连接 / 取消 / 保存」。
- **修复**：`views/connections.rs` 移除表单内动作行；`views/modals.rs` 页脚按 React 顺序改为测试连接/取消/保存（保存主按钮复用 `save_connection_form`，键 `connection-save` 保留，文案「保存」）。动作行移出后表单高 433pt，960 与 1440 下均无需滚动即可看全字段。
- **测试**：新增 `chrome_metrics::the_connection_and_import_dialogs_fit_the_minimum_window`（960x640 下两种对话框在窗内、页脚与主按钮可见），已反向验证。
- **门禁**：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` → **56 + 188 passed, 0 failed**；`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` → exit 0（既有 warnings）。

## 增量：单行输入框文字垂直居中（本次）

- **问题**：Rune 的 `text_input` 把行盒从内容盒顶边开始排，`.center_y()` 只作用于子元素（`AlignItems::CENTER`），因此凡是给输入框设了固定高度（`.h(24|28|36)`）的字段，文字与光标都贴着顶边。首页两个搜索框最明显：1440×816 实测 `navigator-filter-input` 文字墨迹 92.5..101.5、中心 97.0，而盒中心 104.0（高 7pt）；`home-connections-filter` 同病（238.25 vs 245.25）。此前的 `8e81314ed`（加 `single_line`）、`ae18d90ec`（高度改 20）、`3e9a3f897`（改回 28）只动了高度，都没能动文字位置。
- **修复**：新增共享助手 `crates/datazen-ui/src/ui/field.rs::centered_pad(style, height)` = `((height - style.line_px()) / 2.0).max(0.0)`（`TextStyle::line_px()` 见 `vendor/rune/crates/rune/src/tok.rs:131`，`type_::LABEL` 12pt → 15pt 行高），把余量对称地还给上下内边距：文字居中，同时整个字段仍是完整命中区（不像把字段压到行高那样只剩 15pt 可点）。
- **封装为组件**：助手本身只解决算数，仍要靠每处调用记得加 `.py(...)`。现已收进 `crates/datazen-ui/src/ui/field.rs` 的 `TextField` 组件，形状照 `rune-kit`：结构体持有 `key/edit/style/height/width/grow/min_width/shrink/pad_x/radius/fill/border/placeholder/ink/secure/label`，链式 setter 逐个设置，`on_edit`/`on_submit` 收进 `Vec<Box<dyn FnOnce(El) -> El>>`（与 kit 的 `Acts` 同构），`.build()` 才画元素。`build()` 内部固定 `text_input(edit, style).key(key).h(height).py(centered_pad(style, height)).single_line()` 再按字段追加外观；宽度缺省即 `w_full`，`.grow()` 展开为 `grow(1.0)+min_w(0.0)`。调用点不再接触 `text_input`、行高或内边距。
- **覆盖**：全部 22 处单行字段构建改为 `TextField::new(key, edit).height(h)….build()`（原先手写的 `.key/.h/.py(...)/.single_line()` 四步从各视图删除，`text_input` 导入随之清理）——`ui/nav.rs` 导航器搜索、`views/connections_home.rs` 首页筛选、`views/connections.rs` 连接表单/导入密码（36pt）、`views/settings.rs` 8 个设置输入、`views/structure.rs` 草稿名/类型、`views/dashboards_workflows.rs` widget 标题/查询、`views/workspace.rs` 结果查找、`views/tools_ops.rs` 值搜索、`data_grid/view.rs` 过滤与单元格编辑、`sql_editor/view.rs` 查找/替换。
- **实测**（1440×816 dark，headless `--snap` + 像素行分布，2 倍缩放）：导航器搜索文字中心 103.75（盒 104.0）、首页筛选 244.75（245.25）、widget 标题 182.25（182.25）、设置页大小 215.75（216.0）、连接对话框名称 221.0（221.0），均在 0.5pt 内；修复前两处搜索框偏 7pt。
- **回归测试**（`crates/datazen-ui/src/tests/chrome_metrics.rs`）：`text_fields_put_their_line_in_the_middle` 断言 `centered_pad` 在 24/28/36pt 与 13pt 字体下都满足 `2*pad + line_px == height`、不会给出负内边距、且 LABEL@28 恰为 6.5；`the_text_field_component_keeps_the_requested_geometry` 用探针视图渲染 24pt/36pt 两档 `TextField`，断言组件产出的字段仍占满所请求的高度与宽度（内边距在盒内、命中区不被压缩）。
- **为什么不用 rune-kit 的现成组件**：kit 的分层是 `crates/rune`（通用原语）+ `rune-kit`（token schema `Kit`: Palette/Elevation/Type/Radius/Motion/Glyphs + 组件），组件一律「结构体 + 链式 setter + `.build(&k)`」，尺寸/圆角/字体全由 `Kit` 的语义 token 决定（`Button` 见 `vendor/rune/crates/rune-kit/src/actions.rs:198,317`，`TextInput` 见 `vendor/rune/crates/rune-kit/src/text_input.rs:126,300`）。DataZen 的 `crates/datazen-ui` 只依赖 `rune` 核心、主题仍是自有的 `tok::Theme`，直接引入 kit 等于把全站换到 `Kit` token 体系，超出 A 系列范围；因此只借形状、不借依赖。另外 kit 的 `TextInput` 是「外层 row 定高 + 内层无高 `text_input` 靠 `center_y` 居中」，上下留白落在外层容器上（该处不可点击），本仓库既有测试会在字段任意位置点击，故内部改用等价的内边距方案，实测偏差 ≤0.5pt 且命中区完整。
