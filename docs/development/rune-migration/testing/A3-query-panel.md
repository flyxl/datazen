# A3 查询面板对齐验证报告

## 范围

- 分支：`feature/a3-query-panel`（基于已合流 `feat/rune-native-ui-migration`，含 A2/A6）
- 任务：A3 查询面板对齐（`plan-native-5.md`）— 编辑器高度可拖持久化、工具栏动作、收藏/历史侧栏、结果多 tab（含 pin）
- 改动面：`crates/datazen-ui`（`views/workspace.rs`、`workspace_state.rs`、`state_def.rs`、`types.rs`、`constants.rs`、`sql_editor/view.rs`、`data_grid/model.rs`、`tests/part_10.rs`、`tests/mod.rs`）。未改 `packages/frontend`、`packages/web`、内核公开契约、`SharedSettings` 与 `crates/datazen-rune-host`。

## 结论

通过。查询工作区对齐为：可拖拽且持久化的编辑器高度、带执行/格式化/explain/片段插入/事务入口的工具栏、收藏/历史/片段侧栏，以及支持切换与 pin 的多结果 tab；既有运行/取消/结果网格路径保持可用。

## 设计与实现

### 编辑器高度可拖持久化
- 新增布局键 `query-editor-height`（默认 320，min 120 / max 900，`Setting::Layout`），复用 `splitter` 的水平分割与 `set_layout_size`/`layout_size` 持久化链路。
- 新增 `query-sidebar-width` 布局键用于查询侧栏宽度持久化（默认 240）。
- `sql_editor::panel` 改为可伸缩高度（`h_full` + 滚动区 `grow`），以适配可拖拽编辑器窗格并保持宽面滚动行为。

### 工具栏动作
- Run / Cancel：沿用既有 `run_query` / `cancel_query` 与 job/同步执行路径。
- Format / Explain / 事务（Begin/Commit/Rollback）：当前 ui-api/host 无对应能力，点击给出诚实提示（`query_notice`），不伪造执行或修改契约。
- 片段：侧栏 Snippets 列表可将片段 SQL 插入编辑器（`insert_snippet_sql` → `SqlEditor::insert`）。

### 收藏/历史侧栏
- 查询工作区左侧提供 Favorites / History / Snippets 切换面板。
- Favorites：接入既有 `host_favorites`/`open_favorite`。
- Snippets：接入既有 `host_snippets` 并支持插入。
- History：Rune UI/DesktopAdapter 当前未暴露查询历史列表，面板诚实提示“not exposed to the Rune UI in this host”。

### 结果多 tab（含 pin）
- 新增 `QueryResultTab`（id/label/generation/grid/pinned）与 `query_result_tabs`、`selected_query_result` 状态；`result_grid` 作为当前选中结果 tab 的兼容镜像，供既有渲染/键盘/焦点路径复用。
- 每次运行创建并选中新的结果 tab；交付落在对应 generation 的 tab。支持切换、Pin/Unpin、关闭；pinned tab 在后续运行中保留。
- 同步执行失败时恢复到先前选中的结果视图，保持“失败不破坏既有结果”的行为；取消仍通过 generation 使晚到结果失效。

## 测试

| 测试 | 位置 | 覆盖 |
|---|---|---|
| `a3_query_editor_height_split_persists_via_layout_setting` | `crates/datazen-ui/src/tests/part_10.rs` | `query-editor-height` 经 `Setting::Layout` 读写往返与默认值 |
| `a3_multi_result_tabs_switch_and_pin_across_runs` | `crates/datazen-ui/src/tests/part_10.rs` | 多次执行生成多结果 tab、切换保留内容、pin 后跨新运行保留 |
| `a3_query_workspace_renders_toolbar_sidebar_and_result_tabs` | `crates/datazen-ui/src/tests/part_10.rs` | 查询工具栏、侧栏与结果 tab 栏渲染 |

回归修复验证：`tests::part_01::sql_editor_scrolls_a_surface_wider_than_the_viewport`、`tests::part_02::a_failed_run_keeps_the_previous_result` 在 GR2 全量中通过。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR2 | `CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | **datazen-ui 169 passed / 0 failed；datazen-ui-api 56 passed / 0 failed** |
| GR4 | `CARGO_TARGET_DIR="$PWD/target" cargo check -p datazen-rune-host --offline` | 通过，exit 0（既有 warnings） |
| GR1/GR3 | 未改 ui-api 契约与 rune-host 逻辑 | 沿用，GR4 check 作为适用性确认 |

## 局限与后续

- Explain/Format/事务与查询历史列表依赖宿主/API 暴露能力；当前以诚实 UI 提示/占位呈现，待能力落地后接线。
- 多结果 tab 的内容随运行态保存在 UI 状态中；当前未做跨重启持久化（与既有结果生命周期一致）。
- 结果 tab 的“新运行替换未 pin”策略采用保守保留（新建并保留已有 tab，pinned 明确保留），避免破坏可回看性；如需更贴近 Tauri 的替换策略可后续细化。
