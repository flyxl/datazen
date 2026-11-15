# A5 结果工作区对齐

- **任务**: 结果工作区对齐（表格/图表 toggle + 状态条 + FindBar），plan-native-5 A5
- **分支**: `feature/a5-result-workspace`
- **提交**: `feat(rune): ... (issue-rune A5)`（本轮）

## UI / state 设计

- `AppState` 新增 `result_view: ResultView`（`Table|Chart`，默认 `Table`）与 `result_find: TextEdit`；`load_result_page` 重置为表格视图并清空查找输入/命中状态，新结果不继承旧查找。
- 查询结果区（`query_workspace` → `result_workspace`）由四块组成：视图切换条、FindBar、结果状态条、结果体。
  - **表格/图表 toggle**：`result-view-toggle`（`result-view-table` / `result-view-chart`）切换 `state.result_view`。表格视图复用既有 `data_grid::result_panel`（`GridSlot::Result`）渲染只读结果网格；图表视图渲染 `result-chart`。
  - **状态条**：`result-status-bar` / `result-status-text` 展示执行态（查询进行中为 `Running…`，否则 `Ready`，取自 `query_loading`）、结果行列数、当前选区/焦点 `selection r..c..`（来自 `DataGrid::rect/focus`）。
  - **FindBar**：`result-find-bar`，搜索输入 `result-find-input`（`on_edit` 绑定 `result_find`，Enter/`Find` 应用）、`Prev`/`Next` 导航、`result-find-count` 显示当前命中序号与总数或 `No matches`。应用/导航调用 `apply_result_find` / `result_find_{next,prev}`，作用于当前 `result_grid`。
- `DataGrid`（model）增加查找状态：`find_needle`、`find_hits: Vec<(u32,u32)>`（行优先全表扫描）、`find_index`。API：`set_find_needle`（空/空白针清空；非空时大小写不敏感子串匹配，命中序按行优先，当前命中选取焦点处或之后首个并 wrap 到首个，同时移动焦点并 `reveal_focus`）、`find_next`/`find_prev`（循环）、`find_hit_count`/`find_current_index`(1-based)/`current_find_hit`/`is_find_hit`/`is_current_find_hit`、`clear_find`；`load_page` 清空查找。
- 渲染（`data_grid/view.rs` cell）：当前命中单元格使用最强高亮（琥珀色调），其他命中次级高亮，优先于选区/焦点/斑马纹，便于定位。

## 图表映射

- 从结果网格推导条形数据 `result_chart_bars`：无行列或无数值列返回不可用。
  - **值列**：首个在任意行可 `trim().parse::<f64>()` 的列。
  - **标签列**：存在其他列时取首个非值列，否则用行号 `r{r}`；空标签回退行号。
  - **值**：`abs().round() as u32`（缺失/不可解析视为 0），用于与仪表盘一致的块高度比例；标签显示截断（≤8 字符）并附原始数值。
- `result_chart_view` 复用仪表盘 `bar_chart` 的同款色块/比例条样式（`oklch(0.72,0.13,250)`），不可用时显示 “Chart unavailable: no numeric data”。

## 测试与门禁

- 新增 `data_grid` 单测（`crates/datazen-ui/src/data_grid/tests.rs`）覆盖 find 命中行为：
  - `find_sets_hits_in_row_major_order_and_selects_first_hit`
  - `find_next_and_prev_navigate_hits_and_wrap`
  - `find_is_case_insensitive_and_trims_empty_needle`
  - `find_with_no_matches_reports_zero_and_navigation_is_false`
  - `clear_find_resets_hits_and_new_page_clears_find`
- **GR2**：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui --offline --lib -- --test-threads=1` → `171 passed; 0 failed`（含上述 5 条新增）。
- **GR1/GR4**：本任务仅改 `crates/datazen-ui`（视图/状态/网格）与 Rune 迁移文档，未改 `datazen-rune-host`/宿主装配；GR4 不适用。G1–G4 不适用（未改 `packages/frontend`/`packages/web`）。

## 已知限制 / 备注

- 查找为显式应用（Find/Enter）而非逐字实时过滤；匹配为子串、大小写不敏感、跨全表单元格扫描，适配当前只读结果页。
- 状态条展示“就绪/运行中”、行列与选区；执行耗时/错误字段当前结果工作区无独立契约来源，按“可用信息”展示。
