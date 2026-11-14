# TN W1 仪表盘列表 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`bb00522` `feat(rune): the dashboards page and its widget list (issue-rune W1)`
- 两个文件：`crates/datazen-ui-api/src/lib.rs`（`DashboardWidget`、`dashboard_widgets` 端口（内存初始为空）+ 1 条单测）、`crates/datazen-ui/src/lib.rs`（侧栏加 Dashboards/Workflows 节、仪表盘页（部件行 `widget-{id}`：id·标题·类型；空列表专属空态）+ 1 条运行时测试；S6 旧断言更新——workflows 作为 W 系列的真实页面回归，其余三个旧占位键仍不存在）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 94 passed（净增 1：`an_empty_dashboard_has_its_own_empty_state`；S6 断言更新），5.32s；`datazen-ui-api` 42 passed（净增 1：`a_dashboard_starts_with_no_widgets`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「仪表盘列表：部件 id、标题、类型。测：空列表有空态」核对：端口初始无部件；页面空态 `dashboard-empty` 呈现、无 `widget-1`。部件行格式 "#id · title · kind"（有部件时，W2/W3 接通后覆盖）。

## 缺陷

无。

## 结论与遗留

**放行。** 遗留：图表（W2）、编辑表单（W3）。
