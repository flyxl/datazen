# A4 表浏览对齐

## 范围
- 数据网格行号 gutter（`#` 表头 + 全局 1 基行号）。
- 列宽按表持久化：`Setting::Layout` 存储，按 (scope, table, column) 生成稳定 key，加载表页时回填。
- FK 面包屑：从 FK 跳转（出站 `go_to_referenced_row` / 入站 `locate_inbound_ref`）时压栈来源上下文并可回退；展示当前表名与可点击的返回路径。
- 状态横幅：表页上方汇总加载/保存中、只读原因、当前过滤条件与 FK 错误（沿用 `fk-error` key 兼容既有用例）。
- 待提交条：有未提交变更时显示醒目的 pending bar（变更数 + Commit/Rollback），并保持与 `commit_staged_edits` / `rollback_staged_edits` 对齐的提交/回滚行为。

## 关键设计
- **列宽持久化 key**：`colw-{fnv1a64(scope\0table\0column):016x}`，符合 Layout key 约束（小写字母/数字/连字符），且对任意表名/列名稳定。`scope` 为 `desktop_connection`，无连接时回退 `local`；在 `set_table_column_width` 写入并在表页加载/投递后 `apply_table_column_widths` 回填。
- **行号**：gutter 固定宽 40pt，表头 `head-rownum`，每行 `rownum-{row}`，显示 `page * page_size + row + 1`（无页大小时 `row + 1`）。
- **列宽模型**：`DataGrid` 维护 `col_widths`，提供 `col_w/span_width/x_of_col/sheet_data_width`，视图与水平滚动/定位按实际列宽计算；默认 120pt 保持既有窗口行为。
- **FK 面包屑**：`AppState.fk_breadcrumbs: Vec<FkBreadcrumb { label, table, page, sort, filter }>`，仅在 FK 导航时入栈，`navigate_fk_breadcrumb` 截断并恢复上下文；连接切换/断开时清空。
- **状态与待提交**：`status_banner` 聚合状态，`pending-changes-bar` 在 `dirty>0` 且可写时展示并承载 `commit-staged` / `rollback-staged` 交互键（兼容既有测试）。

## 测试
- `tests::chrome_metrics::a4_row_number_gutter_renders_header_and_row_indices`：加载表页后断言 `head-rownum`/`rownum-0`/`rownum-1` 与文本 `#`、`1`、`2`。
- `tests::chrome_metrics::a4_column_width_persistence_round_trips_per_table`：`set_table_column_width` 写入 layout、`grid.col_w` 即时生效、`reload_table_page` 后按持久化 key 回填。

## 门禁
- GR2：`CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui --offline --lib -- --test-threads=1` → **162 passed, 2 failed**（`query_results::commit_and_rollback_buttons_drive_the_staged_edits`、`workspace_tabs::a_wired_commit_lands_in_the_adapters_use_case_world` 仍失败；A4 新增用例与其余既有用例通过，待后续修复回归）。
- GR4：`CARGO_TARGET_DIR="$PWD/target" cargo check -p datazen-rune-host --offline` → 退出码 0（仅既有 warnings）。

## 备注
- 本次未改 `packages/frontend` / `packages/web` 与内核公开契约；布局沿用既有 `Setting::Layout` 通道。
