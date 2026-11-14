# TN Q7 执行上屏 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`b204b3e` `feat(rune): run the statement at the cursor into the result grid (issue-rune Q7)`
- `git show --stat b204b3e` 两个文件：`crates/datazen-ui/src/lib.rs`（`run_query()`、查询工具栏 `query-toolbar`（Run/Cancel/快捷键提示）、`app_key` 接 Command+Enter、4 条运行时测试）、`crates/datazen-ui/src/sql_editor.rs`（`set_main_caret()` 公共 API）。没有 `packages/frontend`、`packages/web`。React 未改
- 链路：Command+Enter 或 Run 按钮 → `run_query`（领代次、Q3 取光标语句、Q4 执行）→ `deliver_query_result`（Q5 代次校验）→ B8 共享结果网格（S4 下半区）

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 59 passed（Q6 时 55，净增 4：`the_run_button_fills_the_lower_half_with_the_result_grid`、`command_enter_runs_the_statement_at_the_cursor`、`a_failed_run_keeps_the_previous_result`、`the_cancel_button_drops_a_result_that_arrives_after_it`），2.79s；`datazen-ui-api` 19 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0（Q5 报告遗留的 dead_code 告警随本提交接线消除） |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「查询结果放进 S4 的下半区，走 B8。测：执行后能看到返回的格子」核对，断言来自 GR1 实跑：

1. **执行后能看到格子**（`the_run_button_fills_the_lower_half_with_the_result_grid`）：文档 "select 1"，点 `sql-run` → `result-grid` 呈现在编辑器下方（S4 下半区）、`result-cell-0-0` 可见、占位符消失、cell(0,0)=="1"。通过
2. **Command+Enter 跑光标语句**（`command_enter_runs_the_statement_at_the_cursor`）：文档 "SELECT * FROM CUSTOMERS; select 1"——光标在第一条时执行得 customers 页（25 行、customer-0）；光标移入第二条再执行只得 "select 1"（1 行 1 格）——Q3 切语句与执行端口贯通。通过
3. **失败不改界面**（`a_failed_run_keeps_the_previous_result`）：先成功跑出 "1"，换成 "drop table customers" 再跑——旧结果原样保留（Q4 的零副作用在界面侧成立）。通过
4. **取消丢弃迟到结果**（`the_cancel_button_drops_a_result_that_arrives_after_it`）：成功执行后点 `sql-cancel`，被取消代次的 customers 页迟到——被丢弃，旧结果 "1" 保持。通过

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。Run/Cancel 按钮与 Command+Enter 落地；执行链路 Q3（取语句）→ Q4（执行）→ Q5/Q6（代次）→ B8（共享结果网格）全链贯通；失败执行与取消均不扰动界面。

遗留（不构成拒绝）：

- 执行失败静默（无错误提示区）；错误展示留给 C 系列或接入真实驱动时处理。
- 无执行中状态（spinner/耗时）；内存执行同步返回，异步化后（H 系列）补充。
