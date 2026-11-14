# TN O11 应用预览 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`32b75bf` `feat(rune): apply the draft preview through the port (issue-rune O11)`
- `git show --stat 32b75bf` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`apply_ddl`/`applied_ddl` 端口、StoredTable 加 `column_types`、table_columns 读真实类型、2 条单测）、`crates/datazen-ui/src/lib.rs`（`apply_column_drafts`、`draft-apply` 按钮 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改
- 语义：先整段解析（ADD COLUMN / ALTER COLUMN … TYPE），全部合法才落地（一票否决）；成功后列清单可见变化（新列 NULL 值、类型更新）且端口记录；只读表/未知语句/未知列拒绝且零改动零记录；UI 成功清草稿、失败草稿保留

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 28 passed（净增 2：`applying_a_draft_adds_the_column_and_the_port_records_it`、`a_failed_apply_changes_nothing`）；`datazen-ui` 75 passed（净增 1：`applying_the_drafts_lands_and_a_failure_keeps_them`），4.04s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「应用预览。端口记下已应用，失败时草稿还在。测：失败不把正式列改掉」核对，断言来自 GR1 实跑：

1. **应用成功落地且记录**（`applying_a_draft_adds_the_column_and_the_port_records_it`）：customers 应用 ADD COLUMN score int → 9 列、末列 score/int、每行 9 格且新格 "NULL"、`applied_ddl` 记录该脚本；再应用 ALTER COLUMN score TYPE bigint → 类型变 bigint。通过
2. **失败不把正式列改掉**（`a_failed_apply_changes_nothing`）：orders（只读）拒绝；DROP TABLE 语句拒绝；混合脚本一条坏行整段拒绝（不半应用）；全部失败后列数原样、`applied_ddl` 为空。通过
3. **UI：成功清草稿、失败保留**（`applying_the_drafts_lands_and_a_failure_keeps_them`）：customers 加 score 草稿 → Apply → 草稿与预览消失、正式列清单出现 `column-row-8`、端口记录 1 条；orders 上加草稿 → Apply 失败 → `draft-row-0` 保留、正式列仍 2 条。通过

开发中修正两处：column_types 追加前先补齐与 columns 等长（对齐错误，编译期断言暴露）；Apply 按钮淡入帧点击不响应（已知透明度规则，测试等帧）。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。O9 草稿 → O10 预览 → O11 应用的链路闭环：端口先解析后应用、记录已应用、失败零副作用且草稿留在界面。

遗留（不构成拒绝）：解析器只认两类 ALTER 语句（草稿能生成的）；真实 DDL 文法由驱动侧负责。O 系列全部完成。
