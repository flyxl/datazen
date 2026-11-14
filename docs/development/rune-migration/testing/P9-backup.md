# TN P9 备份页 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`3ecc608` `feat(rune): a backup page that starts, stops and logs (issue-rune P9)`
- `git show --stat 3ecc608` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`start_backup`（需会话）/`stop_job` 端口、MemoryJob 持久化日志与 stopped 标志、终态读取不再重复追加 + 1 条单测）、`crates/datazen-ui/src/lib.rs`（设置页 Backup 区：Start/Stop/Refresh，复用 `progress_panel`（"backup" 前缀）+ 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 34 passed（净增 1：`a_backup_stops_and_reports_stopped`）；`datazen-ui` 83 passed（净增 1：`a_backup_page_stops_and_shows_stopped`），4.61s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「备份页：开始、停止、日志。用 P7 的同一进度组件。测：停止后状态变成已停止」核对，断言来自 GR1 实跑：

1. **停止后状态已停止**（api + 运行时）：未连接不能备份；连接后 Start → running（日志含 "backing up the database"）；Stop → 快照 status "stopped"、日志末行 "stopped by the user"、对应 `backup-log-{n}` 行绘制；再读不变（settled）；重复停止被拒且界面 `backup-error` 可见；停止未知作业拒绝。通过
2. **同一进度组件**（代码核对）：Backup 区直接调 P7 的 `progress_panel(snapshot, "backup", theme)`

开发中修一处重构引入的回归：终态后再次读取会重复追加日志行（P7 既有测试抓住），改为终态行只在第 3 次读取的转换点追加一次。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。备份页开始/停止/日志齐备，复用导出进度组件；停止语义经端口。**P 系列全部完成。**

遗留（不构成拒绝）：备份产物落盘与真实驱动备份随 H 系列；定时备份不在范围。
