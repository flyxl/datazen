# TN W4 工作流列表 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`6fc883c35` `feat(rune): the workflow list, sorted by name (issue-rune W4)`
- 两个文件：`crates/datazen-ui-api/src/lib.rs`（`Workflow`、`workflows` 端口（存储无序、读取按名排序；三条演示流程）+ 1 条单测）、`crates/datazen-ui/src/lib.rs`（工作流页：每行名字 + 步数 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 97 passed（净增 1：`the_workflow_list_is_sorted_by_name`），5.56s；`datazen-ui-api` 44 passed（净增 1：`workflows_read_sorted_by_name`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「工作流列表。测：按名字排序」核对：端口读取返回 [audit, nightly-backup, weekly-report]（存储序不同）；页面三行呈现且 y 坐标严格递增（audit 上、weekly-report 下）。通过

## 缺陷

无。

## 结论与遗留

**放行。** 遗留：表单与步骤编辑是 W5。
