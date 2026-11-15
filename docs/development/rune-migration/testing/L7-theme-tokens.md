# L7 主题 token 修正验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 任务：L7 主题 token 修正：`crates/datazen-ui/src/data_grid.rs`、`crates/datazen-ui/src/sql_editor.rs`、ER 相关硬编码暗色/oklch 改为随 theme token（亮色正确）。
- 被测提交：见 tasks.md L7 行（feat）；本报告随 docs 提交落库。
- 改动面：`crates/datazen-ui/src/data_grid.rs`、`crates/datazen-ui/src/sql_editor.rs`、`crates/datazen-ui/src/lib.rs`（渲染链路 dark 贯穿与 ER 盒配色、测试）。不改 `packages/frontend`、`packages/web`、宿主与内核契约。

## 结论

通过。数据网格（表头/冻结列/选中/斑马行）、SQL 编辑器（补全面板、插入组合下划线、光标、语法高亮色板）与 ER 盒背景的硬编码暗色值改为按 `dark` 主题分支（亮色模式使用适配对比度的浅色值），并在视图链路（`page_body` → workspace/tools/workflows → grid/editor/ER）中贯穿 `dark` 标志；亮色主题下 grid / SQL editor / ER 页面可正常渲染。

## 实现面

- **data_grid.rs**：`panel`/`result_panel`/`sheet`/行与单元渲染增加 `dark` 参数；表头、冻结列、active/selected/斑马纹填充按明暗主题取值。
- **sql_editor.rs**：`panel`/`plain_panel`/`highlight_sql`/补全/光标/组合下划线与语法色函数增加 `dark` 参数，亮色模式使用深色语法/光标对比色。
- **lib.rs**：`workspace_body`/`table_workspace`/`query_workspace`/`er_view`/`er_box`/`tools_page`/`sync_tool`/`workflows_page`/`page_body` 贯穿 `dark`；ER 盒填充按主题取值；同步预览与 workflow YAML 面板传递 `dark`。

## 测试

| 测试 | 位置 | 覆盖 |
|---|---|---|
| `light_theme_renders_grid_editor_and_er_with_theme_tokens` | `crates/datazen-ui/src/lib.rs` | 亮色主题下 toolbar/chrome、数据网格、SQL 编辑器、ER 页面与画布可渲染 |

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR2 | `CARGO_TARGET_DIR=$PWD/target cargo test -p datazen-ui --offline --lib -- --test-threads=1` | **159 passed / 0 failed** |
| GR4 | `CARGO_TARGET_DIR=$PWD/target cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| GR1 | 沿用（未改 ui-api） | 56 passed |
| GR3 | 沿用（未改宿主） | 59+3 环境失败 |

## 备注

- 仅改 Rune UI 与文档；React/内核契约未动。真机 GUI 不可得，亮色正确性以 headless 渲染存在性与主题分支配色推进。
