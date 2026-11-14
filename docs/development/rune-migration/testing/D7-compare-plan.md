# TN D7 对比计划文本 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`de44da5` `feat(rune): the compare plan text with an empty state (issue-rune D7)`
- 两个文件：`crates/datazen-ui-api/src/lib.rs`（`compare_plan` 端口：每个缺失对象一行 CREATE TABLE 语句并标注目标；无差异为空串 + 1 条单测）、`crates/datazen-ui/src/lib.rs`（compare 工具计划区：非空逐行绘制、空计划专属空态 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 90 passed（净增 1：`an_empty_compare_plan_has_its_own_empty_state`），5.41s；`datazen-ui-api` 39 passed（净增 1：`a_compare_plan_lists_missing_objects_and_can_be_empty`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「结构对比：计划文本。测：空计划有空态」核对：

1. **计划文本与空态**（api + 运行时）：local→replica 计划含 "CREATE TABLE catalog" 与 "-- missing on replica"，逐行绘制；target 切到 local（同端点）→ 空计划 → `compare-plan-empty` 空态呈现、无计划行；未知端点 Err。通过

## 缺陷

无（测试时机修正同前例）。

## 结论与遗留

**放行。** 计划文本端口与页面齐备，空计划有专门空态。遗留：执行状态是 D8。
