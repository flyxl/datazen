# TN D9 传输列对应 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`ca5ab44` `feat(rune): the transfer column mapping (issue-rune D9)`
- 一个文件：`crates/datazen-ui/src/lib.rs`（transfer 工具：左右表芯片、左右列芯片、Pair 按钮、配对列表、两侧未对应列显式清单、错误行 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 92 passed（净增 1：`a_column_already_in_a_pair_cannot_pair_again`），5.10s；`datazen-ui-api` 40 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「传输：列对应。左右列名，未对应的列显式列出来。测：两列重名不能配成一对」核对：

1. **重名不能配成一对**（`a_column_already_in_a_pair_cannot_pair_again`）：配对 id→id 成功；再点 Pair（同名重复使用）→ 拒绝——无第二对、`transfer-error` 可见（"a column pairs once"）；换新列名 name→name → 配对成功。**解读说明**：任务原文"两列重名不能配成一对"按"已出现在配对中的列名（重名）不能再配"实现——即任一侧列在映射中至多出现一次。通过
2. **未对应列显式列出**：`transfer-unmapped-left/right` 两块清单始终呈现，配对后相应列从清单消失。通过

## 缺陷

无。

## 结论与遗留

**放行。** 列对应配对、重名拒绝、未对应显式清单。遗留：确认后 job 是 D10。
