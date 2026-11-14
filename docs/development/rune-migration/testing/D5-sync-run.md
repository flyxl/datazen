# TN D5 同步运行步 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交见 `git log`：`feat(rune): the sync run step as a job with snapshots (issue-rune D5)`
- 两个文件：`crates/datazen-ui-api/src/lib.rs`（`run_sync` 端口（端点校验 + 空选择拒绝；作业日志 "syncing local into replica"）+ 1 条单测）、`crates/datazen-ui/src/lib.rs`（预览步 Run sync/Refresh、`sync_job`/`sync_snapshot`、P7 面板复用 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 88 passed（净增 1：`the_sync_run_needs_tables_and_finishes_through_snapshots`），5.07s；`datazen-ui-api` 37 passed（净增 1：`a_sync_without_selected_tables_gets_no_job`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「同步：运行步复用 P7。测：没有 D2 的选择时不能运行」核对：

1. **无选择不能运行**（api + 运行时）：空表选择 `run_sync` Err、无作业；UI 清空选择点 Run → `sync_job` None + `sync-error` 可见。恢复选择后 Run → 作业建立、首快照 running；两次 Refresh → completed；`sync-progress` 面板呈现（P7 组件复用）。通过

## 缺陷

无。

## 结论与遗留

**放行。** 同步向导五步齐备（端点→表→选项→预览→运行），运行复用 P7 快照机制。

遗留：真实同步作业随驱动；停止（stop_job）未在同步界面暴露（端口已支持）。
