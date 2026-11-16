## Current Task
- **任务**: A1 收口与全局视觉/交互对齐验证（plan-native-5）— A2–A6 已合流主轨，待统一门禁、契约内收尾与逐页视觉一致性校验
- **负责人**: Coder Agent
- **状态**: L1–L7 已完成；A1 进行中（首页与导航器 Recent/All/过滤已落地，pinned/用户分组受契约限制）。A2 连接表单对话框化、A3 查询面板对齐、A4 表浏览对齐、A5 结果工作区对齐、A6 设置导航分组已合流主轨。第五阶段计划见 [plan-native-5.md](plan-native-5.md)。

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
