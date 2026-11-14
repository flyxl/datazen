# TN W5 工作流表单 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`a122f55a8` `feat(rune): the workflow form with up and down steps (issue-rune W5)`
- 两个文件：`crates/datazen-ui-api/src/lib.rs`（`save_workflow` 端口（按名 upsert；空名拒绝）+ 1 条单测）、`crates/datazen-ui/src/lib.rs`（点击工作流行开编辑器：名字 + 步骤列表每步 up/down、Save/Close、`move_workflow_step` 边界守卫 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 98 passed（净增 1：`the_first_workflow_step_cannot_move_up`），5.50s；`datazen-ui-api` 45 passed（净增 1：`saving_a_workflow_replaces_its_steps`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「工作流表单：名字和步骤列表，步骤只能上下移。测：首步不能再上移」核对：audit 编辑器中 step-up-0 无操作（首步不动）；step-down-0 交换、step-up-1 换回；末步 step-down-1 无操作；Save 后端口中步骤顺序持久；空名拒绝、新名追加。通过

## 缺陷

无。

## 结论与遗留

**放行。** 步骤只能上下移（无增删改）；首末步边界守卫；保存经端口。
