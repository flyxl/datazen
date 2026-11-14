# TN O7/O8 ER 画布 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`8641bd5` `feat(rune): an ER canvas of schema table boxes that drag (issue-rune O7, O8)`
- `git show --stat 8641bd5` 一个文件：`crates/datazen-ui/src/lib.rs`（`er_schema`/`er_positions`/`er_drag` 状态、`open_er`/`close_er`、`er_view`/`er_box`、侧栏 `er-button`、tab 事件与断开关闭 ER、2 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改
- O7：ER 只画当前 schema（选中表标签所属 schema，无则第一个）的表盒子；超过 12 列默认折叠为 "N columns"。O8：盒子经 `El::on_drag` 拖动（Down 记起点、Move 按累计位移应用、Up 清手势），位置存 `er_positions`（UI 状态，不进端口）

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 72 passed（O6 时 70，净增 2：`the_er_page_draws_the_current_schema_and_collapses_wide_tables`、`er_boxes_drag_and_the_position_survives_rerenders`），3.72s；`datazen-ui-api` 24 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「ER 只画当前 schema 的表盒子，超过 12 列的表默认折叠。测：宽表显示列数而不是每一列」「ER 盒子可拖，位置存在 UI 状态，不进端口。测：拖完再渲染，位置还在」核对，断言来自 GR1 实跑：

1. **当前 schema 与宽表折叠**（`the_er_page_draws_the_current_schema_and_collapses_wide_tables`）：customers 标签（public）开 ER → `er-canvas` 内 `er-box-customers`（8 列全列名 `er-col-customers-0..7`）与 `er-box-orders`，无 analytics 的 catalog 盒；换 catalog 标签（analytics）再开 ER → `er-box-catalog` 呈 `er-collapsed-catalog`（"30 columns"），`er-col-catalog-12` 不存在。通过
2. **拖动与位置保持**（`er_boxes_drag_and_the_position_survives_rerenders`）：拖 customers 盒 (+80, +50) → `er_positions["customers"] == (80, 50)`，`presented` 矩形实际移动 ≥(70, 40)；再渲染一帧位置保持；手势结束 `er_drag` 为 None。位置只在 AppState（代码核对：端口无任何位置方法）。通过

调试记录（探针 `/home/z/my-project/scripts/probes/o8_probe.rs`，已删）：ER 页进入帧透明度为 0，透明度 ≤0.5 的元素不响应指针（S6 报告已知规则）；测试在打开 ER 后等一帧完成淡入再拖，状态机（Down/Move/Up 三段）与累计位移语义经探针验证后落为正式断言。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。O7：当前 schema 的表盒子、宽表折叠为列数；O8：盒子可拖、位置在 UI 状态、重渲染保持、不进端口。

遗留（不构成拒绝）：

- 盒子间无连线（外键边）；外键清单（O6）已有数据，画线留给后续打磨。
- 折叠态不可展开查看列；点击盒子打开表标签是自然延伸，待需要时加。
- 位置不做画布边界约束（仅下限 0）；画布不缩放。
