# TN W2 条形图 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`cd80202` `feat(rune): bar charts drawn from the port's integers (issue-rune W2)`
- 两个文件：`crates/datazen-ui-api/src/lib.rs`（`save_dashboard_widget`（空标题拒绝；query 暂存）、`widget_bars`（端口给的带标签整数；演示序列含 0 与最大值 12）+ 1 条单测）、`crates/datazen-ui/src/lib.rs`（部件卡片 + `bar_chart` 色块条形图（高度按值比例、零值保留 2pt 细条与标签）+ 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 95 passed（净增 1：`a_bar_chart_shows_the_zero_and_the_maximum`），5.50s；`datazen-ui-api` 43 passed（净增 1：`a_bar_widget_gets_its_integers_from_the_port`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「一种条形图，数据是端口给的整数，用色块画，不引入图表库。测：0 和最大值都看得见」核对：

1. **0 与最大值可见**（api + 运行时）：端口序列 [(mon,0),(tue,3),(wed,7),(thu,12)]；渲染后零值条 `widget-block-1-mon` 呈现且高度 ≥1pt（细条），最大条 `widget-block-1-thu` 高度 ≥80pt 且比零条高 60pt 以上（成比例）。色块为原生 `fill(oklch(...))` 行元素，无任何图表库依赖（Cargo.toml 未动）。通过

## 缺陷

无。

## 结论与遗留

**放行。** 端口整数 → 色块条形图，零与最大皆可见。保存端口随本任务落地（W3 的表单是其界面）。
