# TN O1 三层名字端口 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`525d8d9` `feat(rune): report database, schema and table names from the port (issue-rune O1)`
- `git show --stat 525d8d9` 一个文件：`crates/datazen-ui-api/src/lib.rs`（`Catalog`/`DatabaseNode`/`SchemaNode`、trait 加 `catalog()`、`StoredTable.schema`、演示数据加 analytics schema 的 catalog 宽表 30 列×60 行、1 条单测）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 20 passed（Q8 时 19，净增 1：`the_catalog_reports_three_levels_in_a_stable_order`），0.01s；`datazen-ui` 64 passed，2.90s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「端口返回库、schema、表三层名字。测：两层 schema 的顺序稳定」核对，断言来自 GR1 实跑：

1. **三层名字与顺序稳定**（`the_catalog_reports_three_levels_in_a_stable_order`）：未连接时目录为空；连接后一个库 "local"（连接 id），两层 schema 依次为 "public"（customers、orders）与 "analytics"（catalog）；两次调用 `assert_eq` 相等（顺序稳定）。通过

既有 64 条 UI 测试复跑通过：session.tables 平铺清单与 S2 树的旧行为不受影响（三层树的 UI 化是 O2）。

## 缺陷

无。开发中修一处实现内部的双重映射残留（编译期类型错误，未进提交前已修复）。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。端口有三层名字（库=连接、schema=表所属、表名清单），两层 schema 顺序稳定；演示数据具备双 schema 与宽表。

遗留（不构成拒绝）：

- 库层目前以连接 id 命名且仅一个；多库目录待真实驱动。
- 侧栏树仍渲染平铺表清单（S2 行为）；三层树 UI 与展开状态是 O2。
