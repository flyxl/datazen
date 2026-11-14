# TN D3 同步选项开关 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交见 git log（本任务单提交）：`feat(rune): the sync options step with three toggles (issue-rune D3)`
- 一个文件：`crates/datazen-ui/src/lib.rs`（`SYNC_OPTION_LABELS`、`toggle_sync_option`、第 3 步三个 on/off 芯片渲染 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 86 passed（净增 1：`the_sync_options_default_and_round_trip`），4.81s；`datazen-ui-api` 35 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「同步：三个选项的开关。测：默认值和改动能往返」核对：

1. **默认值与往返**（`the_sync_options_default_and_round_trip`）：默认 [false,false,true]（不建表、不删表、拷数据）；点开选项 0 → [true,false,true]；关选项 2 → [true,false,false]；再各点一次 → 回到默认。通过

## 缺陷

无。

## 结论与遗留

**放行。** 三个开关默认明确、可往返。遗留：选项未入端口（内存向导状态；真实同步的选项语义随驱动）。
