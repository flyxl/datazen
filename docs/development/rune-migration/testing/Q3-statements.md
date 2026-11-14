# TN Q3 语句切分 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`ec844ee` `feat(rune): split the document into statements at semicolons (issue-rune Q3)`
- `git show --stat ec844ee` 一个文件：`crates/datazen-ui/src/sql_editor.rs`（`statements()` 字节区间切分、`statement_at_cursor()` 取光标所在语句、4 条单测）。没有 `packages/frontend`、`packages/web`。React 未改
- 本任务为纯模型层：切分与取语句供 Q4 的执行端口消费，界面不改

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 53 passed（Q2 时 49，净增 4：`statements_split_at_semicolons_and_skip_empty_ones`、`the_cursor_picks_its_own_statement`、`a_document_without_semicolons_is_one_statement`、`an_empty_document_has_no_statement`），2.36s；`datazen-ui-api` 16 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「按分号切语句，执行位置取光标所在那条。测：光标在第二条时只取出第二条」核对，断言来自 GR1 实跑：

1. **按分号切**（`statements_split_at_semicolons_and_skip_empty_ones`）："a;; b ;  c" → 三段 "a"、" b "、"  c"，空段跳过。通过
2. **光标在第二条只取第二条**（`the_cursor_picks_its_own_statement`）："first; second; third" 光标置 10（second 内）→ "second"；光标 0 → "first"；光标 7（first 的分号后）→ "second"（下一条）；光标在文末 → "third"。通过
3. **无分号整篇一条**（`a_document_without_semicolons_is_one_statement`）。通过
4. **空文档与纯空白无语句**（`an_empty_document_has_no_statement`）："  ;  " 也无。通过

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。按分号切非空语句段、光标定位语句（分号后归下一条、越过末条归最后一条）、空/空白文档无语句。

遗留（不构成拒绝）：

- 注释与字符串里的分号同样切分（如 "-- x;" 会截断语句）；带词法 awareness 的切分留给接入真实解析器时处理。
- 无选区优先语义（有选区时整选区作为一条语句）——SSMS 行为，留给 Q 系列执行端口的输入策略。
