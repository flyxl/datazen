# TN W7 工作流运行结果 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`bde466a56` `feat(rune): workflow run results as a read-only list (issue-rune W7)`
- 两个文件：`crates/datazen-ui-api/src/lib.rs`（`WorkflowStepResult`、`run_workflow` 端口（步骤名/成败/一行摘要；内存端口让 apply 类步骤失败）+ 1 条单测）、`crates/datazen-ui/src/lib.rs`（编辑器 Run 按钮、只读结果列表（绿点/红点区分）+ 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 100 passed（净增 1：`workflow_run_results_separate_failures_from_successes`），5.68s；`datazen-ui-api` 46 passed（净增 1：`workflow_runs_separate_failing_and_successful_steps`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「运行结果只读列表：步骤名、成功或失败、一行摘要。测：失败步骤和成功步骤能分开」核对：audit 两步——compare structures（ok，"ok: 12 rows"，绿点 `run-ok-0`）与 apply plan（failed，"the replica refused the plan"，红点 `run-fail-1`，无 `run-ok-1`）；nightly-backup 无 apply 步骤全绿；未知工作流 Err。通过

开发中两处修正：Run 按钮插入的替换串与实际文本不符（"Save" 无 .to_owned()）静默未生效，探针定位后以精确编辑补上；探针已摘除。

## 缺陷

无。

## 结论与遗留

**放行。** **W 系列全部完成。** 遗留：真实工作流执行（驱动/宿主）随 H 系列。
