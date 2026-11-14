# TN D8 对比执行状态 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`860c4e0` `feat(rune): the compare plan applies as a job (issue-rune D8)`
- 两个文件：`crates/datazen-ui-api/src/lib.rs`（`run_compare_plan` 端口（空计划拒绝；作业带可选失败句——replica 拒建 analytics 表）、MemoryJob 加 `fail_line`、job_progress 终态统一 + 1 条单测）、`crates/datazen-ui/src/lib.rs`（compare 工具 Apply plan/Refresh、快照面板复用、错误行 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 91 passed（净增 1：`a_failed_plan_apply_keeps_the_plan_visible`），5.14s；`datazen-ui-api` 40 passed（净增 1：`a_failed_plan_apply_keeps_the_plan`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「结构对比：执行状态复用 P7。测：失败不把计划清掉」核对：

1. **失败不清计划**（api + 运行时）：local→replica 计划 Apply → 作业 running → 两次 Refresh → failed，错误句 "the replica cannot create analytics tables" 在快照日志末行；`compare-plan` 仍呈现（差异仍在即计划仍在）；空计划（同端点）Apply → 无作业、`compare-error` 可见；合成可应用计划 → completed。通过
2. **复用 P7**（代码核对 + `compare-progress` 呈现）：进度经共享 `progress_panel`

## 缺陷

无。

## 结论与遗留

**放行。** 对比三步齐备（对象/计划/执行）；失败保计划、错误句可见；作业失败语义统一为按行配置。
