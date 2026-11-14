# TN Q4 查询执行端口 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`c0048c6` `feat(rune): a QueryApi that runs fixed SQL against memory (issue-rune Q4)`
- `git show --stat c0048c6` 一个文件：`crates/datazen-ui-api/src/lib.rs`（`QueryApi` trait、`MemoryWorkspace::execute`、`result_page` 共享构造、3 条单测）。没有 `packages/frontend`、`packages/web`。React 未改
- 固定 SQL：`SELECT 1`（一列一行）与 `SELECT * FROM CUSTOMERS`（customers 的 25 行页）；空白折叠、大小写不敏感、容忍尾分号。未知 SQL 报错且**不改任何状态**（会话、打开的表、暂存、结果锁都不动）。`execute` 不触碰 `result_open`——是否上屏由调用方决定（Q7 走 B8 的 `open_result`）

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 19 passed（Q3 时 16，净增 3：`execute_answers_fixed_sql_with_a_page`、`unknown_sql_is_an_error_and_changes_nothing`、`execute_without_a_session_is_an_error`），0.00s；`datazen-ui` 53 passed，2.41s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「`QueryApi::execute`，内存对固定 SQL 返回一页。测：未知 SQL 返回错误，不改界面状态」核对，断言来自 GR1 实跑：

1. **固定 SQL 返回一页**（`execute_answers_fixed_sql_with_a_page`）：`select 1`（小写、无分号）→ 一列 "1"、一行 "1"、`read_only=="query result"`；`SELECT  *  FROM customers;`（双空格、尾分号）→ customers 列、25 行、首行 "customer-0"。通过
2. **未知 SQL 报错且不改状态**（`unknown_sql_is_an_error_and_changes_nothing`）：`drop table customers` 与空串都 Err；打开的表与暂存值原样（before/after 逐行相等）；随后继续暂存成功——失败的 execute 没有打开结果、没有动会话。通过
3. **无会话执行报错**（`execute_without_a_session_is_an_error`）。通过

开发中修正一处测试预期（customers 查询的 `total_rows` 是结果页本身 25 行，不是全表 40——结果页合同与 B1 表页不同），属预期笔误。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。QueryApi 端口落地；内存对两条固定 SQL 返回只读结果页；未知/空 SQL 报错且零副作用；与 B8 的 open_result 共享结果页构造但不越权触碰标志。

遗留（不构成拒绝）：

- 固定 SQL 集很小（两条）；真实驱动接入后由适配器实现 QueryApi，内存实现保留为演示与测试底座。
- 执行与界面（运行按钮、结果上屏、代次）尚未接线——分别是 Q5/Q6/Q7 的范围。
