# TN Q5 请求代次 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`9db816d` `feat(rune): query generations drop stale results (issue-rune Q5)`
- `git show --stat 9db816d` 一个文件：`crates/datazen-ui/src/lib.rs`（`AppState.query_generation`、`next_query_generation()`、`deliver_query_result(generation, page)`、1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改
- 机制：每次发起请求领一个新代次；结果页带着代次回来，`deliver_query_result` 只放行当前代次——旧代次的结果整页丢弃（不进网格、不动状态）

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 54 passed（Q4 时 53，净增 1：`a_result_from_an_old_generation_is_dropped`），2.34s；`datazen-ui-api` 19 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「请求代次。后返回的旧结果丢掉。测：代次 1 在代次 2 之后到达时被忽略」核对，断言来自 GR1 实跑：

1. **旧代次被忽略**（`a_result_from_an_old_generation_is_dropped`）：领取代次 1、代次 2；代次 1 的结果页（真实 `execute("SELECT * FROM CUSTOMERS")` 产物）后到——`deliver_query_result(1, …)` 后 `result_grid` 仍为 None，界面无结果；代次 2 的结果（"select 1"）随后到达——网格装载，`result-grid` 呈现，cell(0,0)=="1"。通过

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。代次机制落地：领号、比对、旧代次整页丢弃；当前代次经 B8 的 `load_result_page` 上屏。

遗留（不构成拒绝）：

- `next_query_generation`/`deliver_query_result` 暂无生产调用点（运行按钮与端到端流程是 Q7），编译存在 dead_code 告警——Q7 接线后消除；GR2 仍 exit 0。
- 真实驱动的执行是异步的，代次在适配器层（H 系列）还需穿透到主机往返；内存实现同步返回，机制先行。
