# TN P1 设置端口 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`7c95aa3` `feat(rune): a settings port with per-entry validation (issue-rune P1)`
- `git show --stat 7c95aa3` 一个文件：`crates/datazen-ui-api/src/lib.rs`（`Setting::{Theme,TablePageSize,QueryMaxRows}`、`SettingsApi` trait、内存实现（按种类单条替换）、1 条单测）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 29 passed（净增 1：`an_invalid_setting_is_rejected_and_the_old_values_stay`）；`datazen-ui` 75 passed，4.05s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「设置端口：读、写、一项失败不影响其他项。测：非法值被拒绝，旧值还在」核对，断言来自 GR1 实跑：

1. **非法值被拒绝，旧值还在**（`an_invalid_setting_is_rejected_and_the_old_values_stay`）：TablePageSize 0/501、QueryMaxRows 0 均被拒；三次失败写后 `read_settings()` 与之前逐项相等（含其他项）；合法写（50）按种类原位替换、三种各一条、其余两项原样；Theme 写入翻转生效。通过

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。设置端口读写齐备、逐项校验、失败零影响。

遗留（不构成拒绝）：仅三项设置（P2/P3 消费，P4 起按需扩展）；持久化到磁盘随 H 系列宿主接入。
