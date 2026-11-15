# L4 两级侧栏与 Navigator 面板 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 任务：L4 两级侧栏与内容区 navigator——40pt 图标 rail（7 个内联 SVG lucide 语义）+ 展开 112pt 文字 + 折叠持久化；对象树挪入内容区 navigator 面板（280pt，min 200/max 500，可拖拽持久化，复用 L2 splitter）。
- 被测提交：见 tasks.md L4 行（feat）；本报告随 docs 提交落库。
- 改动面：`crates/datazen-ui/src/lib.rs`（rail/navigator/布局重构、图标常量、状态 helper、测试）。不改 `packages/frontend`、`packages/web`、宿主与内核契约。

## 结论

通过。旧固定 224 侧栏（标题/导航/树/动作混排）替换为 **rail + navigator + splitter + page** 的 body 布局：rail 展开 112pt（图标+文案）/折叠 40pt（仅图标），状态以 `sidebar-rail-width`（40/112）经 `Setting::Layout` 持久化；对象树与工作区动作（open query / ER / refresh / disconnect）移入 navigator 面板，宽度 `navigator-width` 默认 280、夹持 200–500，垂直分割条复用 `splitter_handle` 持久化。既有导航键名由 `nav-*` 统一为 `rail-*`（测试同步迁移）。GR2 全绿（156 passed）。

## 实现面

- **图标**：7 个 section + 折叠/展开 chevron 的 `rune::Icon` 静态（24/16 viewBox，lucide 语义 stroke 路径，`currentColor`）。
- **Rail**：`rail_view` 渲染 7 个 `rail-*` tab 项与 `rail-toggle`；`sidebar_expanded/sidebar_rail_width/toggle_sidebar_expanded` 读写 `SIDEBAR_RAIL_WIDTH_KEY`。
- **Navigator**：`navigator_panel`（`datazen-navigator`）承载 `object-tree` 与动作按钮，宽度 `navigator_width()` 夹持后生效；`splitter::splitter_handle(..., SplitAxis::Vertical, 200, 500, 280)` 接线 `NAVIGATOR_WIDTH_KEY`。
- **布局健壮性**：page/body 增加 `min_w(0)`，避免 flex 子项撑破可用宽度（修复 `sql_editor_scrolls_a_surface_wider_than_the_viewport` 在新布局下的视口约束回归）。

## 测试

| 测试 | 位置 | 覆盖 |
|---|---|---|
| `the_rail_renders_sections_toggles_collapse_and_persists` | `crates/datazen-ui/src/lib.rs` | 7 个 rail 项+toggle 存在、旧 sidebar 不存在、默认展开 112、切换折叠/展开的状态与 `sidebar-rail-width` 持久化、从持久化值重建默认态可恢复折叠 |
| `the_navigator_panel_holds_the_tree_and_persists_width` | 同上 | navigator/object-tree/open-query/分割条存在、默认 280、持久化读回与越界夹持 200–500 |

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR2 | `CARGO_TARGET_DIR=$PWD/target cargo test -p datazen-ui --offline --lib -- --test-threads=1` | **156 passed / 0 failed** |
| GR4 | `CARGO_TARGET_DIR=$PWD/target cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| GR1 | 沿用（未改 ui-api） | 56 passed |
| GR3 | 沿用（未改宿主） | 59+3 环境失败 |

## 备注

- rail 折叠态持久化为实际宽度值（40/112）而非 0/1，以满足 `Setting::Layout` 的 `1..=10000` 校验语义。
- 分支 rustfmt 基线不一致，按周边风格手写；CI 不跑 fmt。