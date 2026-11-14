# TN D6 结构对比选对象 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`176b0cf` `feat(rune): the compare step marks same-name objects (issue-rune D6)`
- 两个文件：`crates/datazen-ui-api/src/lib.rs`（`CompareObject`、`compare_objects` 端口（同名即在目标存在；同端点比较合法且全部同名）+ 1 条单测）、`crates/datazen-ui/src/lib.rs`（compare 工具：端点芯片 + 对象列表（绿点同名/红点仅源）+ 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 89 passed（净增 1：`the_compare_step_marks_same_name_objects`），4.94s；`datazen-ui-api` 38 passed（净增 1：`the_same_object_name_is_marked_on_both_ends`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「结构对比：选对象。测：同名对象标出来」核对，断言来自 GR1 实跑：

1. **同名标出**（api + 运行时）：local→replica：customers/orders 同名（绿点 `compare-match-*`），catalog 仅源（红点 `compare-missing-catalog`）；local→local 全部同名；未知端点 Err。通过

## 缺陷

无。

## 结论与遗留

**放行。** 对象清单与同名标记落地。遗留：计划文本是 D7。
