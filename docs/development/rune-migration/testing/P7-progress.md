# TN P7 进度快照 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`9b248ef` `feat(rune): export progress drawn from one snapshot (issue-rune P7)`
- `git show --stat 9b248ef` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`ProgressSnapshot`、`job_progress` 端口（内存端每读推进一步：0%→50%→终态；"broken" 格式或 "no-such-table" 范围失败并带错误句）+ 1 条单测）、`crates/datazen-ui/src/lib.rs`（`export_snapshot` 状态、`refresh_export_progress` 单次拉取、共享 `progress_panel`（状态行/百分比/逐行日志）、Refresh 按钮 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 33 passed（净增 1：`a_progress_snapshot_advances_and_failures_show_their_error_line`）；`datazen-ui` 81 passed（净增 1：`a_failed_export_shows_its_error_line_from_the_snapshot`），4.73s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「用一份进度快照画进度和日志。快照由端口给，界面不拉流。测：失败状态能看见错误句」核对，断言来自 GR1 实跑：

1. **快照推进与失败错误句**（api 单测）：csv 任务依次 running 0%（2 行日志）→ running 50% → completed 100%（"done" 收尾），终态后再读不变；broken 格式与 "no-such-table" 范围第三读 failed 且最后一行日志为各自错误句；未知作业 None。通过
2. **界面单份快照绘制**（运行时）：导出（范围 no-such-table）→ 首份快照 running 0%、`export-progress`/`export-log-0` 呈现；点 Refresh → 50%；再点 → failed，快照日志末句 "no such table no-such-table"，对应 `export-log-{n}` 行画在页上。通过
3. **不拉流**（代码核对）：快照只在 Export 启动与 Refresh 点击时经端口取一次，无轮询无订阅

开发中两处修正：失败触发由"格式芯片无法产生的 broken"改为更自然的"范围指向不存在的表"；Refresh 按钮淡入帧点击不响应（已知规则）。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。进度与日志由整份快照绘制；失败状态的错误句可见；共享进度组件备 P9 复用。

遗留（不构成拒绝）：进度条无百分比条形图（文本百分比）；自动刷新不做（明确不拉流）。
