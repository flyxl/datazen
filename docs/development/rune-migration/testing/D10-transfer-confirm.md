# TN D10 传输确认作业 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`1f0eecc` `feat(rune): the transfer confirms before the port hands out a job (issue-rune D10)`
- 两个文件：`crates/datazen-ui-api/src/lib.rs`（`run_transfer` 端口（空配对拒绝；作业日志 "transferring N column pairs"）+ 1 条单测）、`crates/datazen-ui/src/lib.rs`（transfer 工具确认行（Confirm/Cancel）、作业行 + Refresh、快照面板 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 93 passed（净增 1：`cancelling_the_transfer_confirmation_leaves_no_job`），5.15s；`datazen-ui-api` 41 passed（净增 1：`a_transfer_without_pairs_gets_no_job`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「传输：确认后要 job id，进度复用 P7。测：取消确认没有 job」核对：

1. **取消确认没有 job**（api + 运行时）：无配对时确认 → 拒绝（无作业、错误可见）；有配对时打开确认行点 Cancel → `transfer_job` None、无作业行；点 Confirm → 端口发 job id、首快照 running、两次 Refresh → completed、`transfer-progress` 面板呈现（P7 复用）。通过

开发中三处测试时机修正（确认行/作业行/刷新按钮淡入帧点击不响应——B 系列以来的已知规则）。

## 缺陷

无。

## 结论与遗留

**放行。** D 系列全部完成：同步五步、对比三步、传输两步，全部作业走 P7 快照机制。
