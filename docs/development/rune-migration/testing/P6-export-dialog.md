# TN P6 导出对话框 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`230de1e` `feat(rune): an export dialog that collects format and range (issue-rune P6)`
- `git show --stat 230de1e` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`start_export` 端口（范围必填、格式白名单）、内存作业记录 + 1 条单测）、`crates/datazen-ui/src/lib.rs`（设置页 Export 区：格式芯片循环、范围输入、Export 按钮、错误行与作业行 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 32 passed（净增 1：`an_export_needs_a_range_to_get_a_job`）；`datazen-ui` 80 passed（净增 1：`an_export_without_a_range_gets_no_job`），4.54s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「导出对话框只收集格式和范围，然后向端口要一个 job id。测：没选范围时没有 job」核对，断言来自 GR1 实跑：

1. **没范围没作业**（api + 运行时）：空/空白范围与未知格式都 Err、无作业；填范围后 Export → 端口发 job id 1、`export-job` 行呈现、错误行消失；格式芯片循环 csv→json。通过

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。对话框只收集格式与范围并向端口要 job id；缺范围/坏格式零作业。

遗留（不构成拒绝）：进度与日志呈现是 P7（同页进度组件）；文件落盘随 H 系列宿主。
