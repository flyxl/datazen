# TN Q6 取消查询 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`fbfef17` `feat(rune): cancel invalidates the query generation (issue-rune Q6)`
- `git show --stat fbfef17` 一个文件：`crates/datazen-ui/src/lib.rs`（`cancel_query()` 走代次自增、1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 55 passed（Q5 时 54，净增 1：`a_cancelled_generation_never_reaches_the_grid`），2.37s；`datazen-ui-api` 19 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「取消把代次作废。测：取消后的结果不进网格」核对，断言来自 GR1 实跑：

1. **取消后的结果不进网格**（`a_cancelled_generation_never_reaches_the_grid`）：领代次后 `cancel_query()`；该代次的结果页到达——`result_grid` 仍为 None（被 `deliver_query_result` 丢弃）；随后领新代次执行 "select 1"——结果正常上屏，cell(0,0)=="1"。通过

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。取消 = 代次作废，作废代次的结果整页丢弃；取消后再跑不受影响。

遗留（不构成拒绝）：

- 取消按钮的 UI 仍是 Q7 的范围；`cancel_query` 与 `next_query_generation`/`deliver_query_result` 一样暂无生产调用点（dead_code 告警，GR2 仍 exit 0）。
- 真实驱动的传输层取消（向主机发 cancel 消息）在 H 系列适配器；本任务的作废机制与之正交。
