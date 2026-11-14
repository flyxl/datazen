# TN W3 部件编辑表单 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`745b60c3b` `feat(rune): the widget edit form (issue-rune W3)`
- 一个文件：`crates/datazen-ui/src/lib.rs`（`widget_title`/`widget_query` 输入、`save_widget_form`、表单渲染与错误行 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 96 passed（净增 1：`a_widget_without_a_title_cannot_be_saved`），5.43s；`datazen-ui-api` 43 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「部件编辑表单：标题和查询文本。保存进端口。测：空标题不能保存」核对：空标题保存 → 拒绝（错误可见、空态仍在、无部件）；填标题与查询 → 保存 → `widget-1` 入列且图表渲染、错误消失、字段清空。通过（保存端口已在 W2 落地，本任务交付表单与校验路径）

## 缺陷

无。

## 结论与遗留

**放行。** 遗留：查询文本未驱动图表数据（内存端口给固定序列）；部件无删除/编辑既有部件。
