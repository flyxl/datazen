# TN P8 导入向导 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`77d86e6` `feat(rune): an import wizard of two steps (issue-rune P8)`
- `git show --stat 77d86e6` 一个文件：`crates/datazen-ui/src/lib.rs`（`import_step`/`import_source`/`import_target`/`import_error` 状态、`import_next`/`import_finish`/`import_back`、`import_section` 两步向导 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 82 passed（净增 1：`the_import_wizard_needs_a_target_to_move_on`），4.62s；`datazen-ui-api` 33 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「导入向导只做前两步：来源、目标表。测：缺目标表不能到下一步」核对，断言来自 GR1 实跑：

1. **两步与门槛**（`the_import_wizard_needs_a_target_to_move_on`）：第 1 步无来源点 Next → 停留并显示错误；填来源 → 第 2 步；第 2 步无目标表点 Finish → 停留（`import-ready` 不出现）且错误可见；填目标表 → `import-ready` 呈现（来源与目标都在行内）。通过

## 缺陷

无。测试时机修正同前例（第二步按钮淡入帧）。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。向导只做来源与目标表两步，缺一不可前进；纯 UI 状态。

遗留（不构成拒绝）：实际导入作业不在本任务范围（真实驱动接入后走 P6/P7 的作业与快照机制）。
