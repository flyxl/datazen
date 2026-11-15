# L5 ContentToolbar 48 + PanelTabBar 48 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 任务：L5 ContentToolbar 48pt（按 section 出动作行）+ PanelTabBar 48pt（tablist 键盘 Arrow/Home/End、active 下划线、关闭钮）。
- 被测提交：见 tasks.md L5 行（feat）；本报告随 docs 提交落库。
- 改动面：`crates/datazen-ui/src/lib.rs`（`content_toolbar`、`tab_strip` 重构、页面骨架接线、测试）。不改 `packages/frontend`、`packages/web`、宿主与内核契约。

## 结论

通过。工作区页面在 `page_body` 上方增加 48pt `content-toolbar`（按当前 section 提供上下文与已接线动作：Workspace 暴露 `toolbar-new-query`），并将 Workspace 的标签栏重构为 48pt `tab-strip`（`role="tablist"` + `focus_group()`，支持 roving 焦点的 Arrow/Home/End 语义；每个 tab 为 focusable `role="tab"`，含 active 背景、底部下划线 `tab-underline-{id}` 与 `tab-close-{id}` 关闭控件）。既有交互键 `tab-strip` / `tab-{id}` / `tab-close-{id}` 保留，兼容既有 tab 选择/关闭测试。

## 实现面

- **ContentToolbar**：`content_toolbar(state, theme)`，48pt 全宽行，左侧 section 上下文标题，右侧按 section 挂载已实现动作（当前 Workspace：`toolbar-new-query` 调用 `open_query_tab("Query")`；其余 section 预留扩展位）。
- **PanelTabBar**：`tab_strip` 改为 48pt、`role("tablist")`、`focus_group()`；tab 项 48pt、`role("tab")`、`focusable(true)`，选中态填充 + 2pt 下划线元素；关闭钮独立 focusable hit target 并保留 `tab-close-{id}` 键。
- **页面骨架**：`datazen-page` 变为 `content_toolbar → (tab_strip | tab-strip-absent) → page_body → footer`。

## 测试

| 测试 | 位置 | 覆盖 |
|---|---|---|
| `content_toolbar_and_panel_tab_bar_render_at_forty_eight_points` | `crates/datazen-ui/src/lib.rs` | toolbar 存在且 48pt；Workspace 下 tablist 48pt、tab role/focusable、underline/close 元素存在、toolbar-new-query 动作存在 |
| `tab_strip_selects_and_closes_table_tabs`（既有） | 同上 | 重构后选择/关闭 tab 行为与键名兼容 |

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR2 | `CARGO_TARGET_DIR=$PWD/target cargo test -p datazen-ui --offline --lib -- --test-threads=1` | **157 passed / 0 failed** |
| GR4 | `CARGO_TARGET_DIR=$PWD/target cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| GR1 | 沿用（未改 ui-api） | 56 passed |
| GR3 | 沿用（未改宿主） | 59+3 环境失败 |

## 备注

- 仅改 Rune UI 与文档；React/内核契约未动。按分支约定，按周边风格手写，避免与历史 rustfmt 基线噪音冲突。
