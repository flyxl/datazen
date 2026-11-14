# TN O3 DDL 页 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`83cd143` `feat(rune): a DDL page fed by the port's string (issue-rune O3)`
- `git show --stat 83cd143` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`table_ddl` 端口 + 1 条单测）、`crates/datazen-ui/src/lib.rs`（`TableView::{Data,Ddl}`、视图切换器 `table-view-switcher`（`view-data`/`view-ddl`）、`ddl_view` 渲染、1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 21 passed（O2 时 20，净增 1：`table_ddl_returns_the_create_statement_or_empty`），0.01s；`datazen-ui` 67 passed（净增 1：`the_ddl_view_shows_the_port_string_and_an_empty_state`），3.20s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「DDL 页只显示端口返回的字符串。空字符串有单独的空态」核对，断言来自 GR1 实跑：

1. **端口字符串**（`table_ddl_returns_the_create_statement_or_empty`）：customers 的 DDL 以 "CREATE TABLE customers (" 开头、含 "  id text"/"  name text"、以 ");" 收尾；analytics 的 catalog 表返回空串；未知表 Err。通过
2. **DDL 页与空态**（`the_ddl_view_shows_the_port_string_and_an_empty_state`）：表标签默认 Data（网格在）；点 `view-ddl` → `table-ddl` 呈现、网格离开；点 `view-data` 回网格；切到 catalog 表（DDL 空）→ `table-ddl-empty` 空态呈现、`table-ddl` 不在。通过

页面逐行渲染端口字符串，不做任何加工（没有语法高亮、没有编辑）——"只显示端口返回的字符串"。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。表标签有 Data/DDL 双视图；DDL 页逐行呈现端口字符串；空字符串有专门空态；未知表错误不进页面。

遗留（不构成拒绝）：

- DDL 无高亮无复制按钮；C 系列打磨。
- 视图选择是全局的（所有表标签共享），不随标签记忆。
