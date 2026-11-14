# TN C1 网格翻页导航键 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`163db778 feat(rune): PageUp, PageDown, Home and End in the grid (issue-rune C1)`
- 改动面：`crates/datazen-ui/src/data_grid.rs`（on_key 增加四键）、`crates/datazen-ui/src/lib.rs`（1 个运行时测试）

## 实现

- **PageDown / PageUp**：焦点按一整个已加载页的行数移动（`nudge(±rows, 0, extend)`），经既有钳位不会越界——到底后再按停在同一行。
- **Home / End**：本行的第一列 / 最后一列（`nudge(0, ±cols, extend)`，同样钳位）。
- 四键都支持 Shift 扩选（沿用 nudge 的 anchor 语义），复用 `reveal_focus()` 的列窗口跟随。

## 测试

| 测试 | 位置 | 断言 |
| --- | --- | --- |
| `page_home_and_end_move_the_focus_and_it_stays_in_the_model` | datazen-ui（runtime，经 `app_key` 真实键路） | PageDown 0→24 行、到底钳位不动；PageUp 回 0；End 到本行末列、Home 回首列；每次移动后焦点 (r,c) 仍在 `rows × cols` 模型内（判据"焦点移动后仍在模型里"） |

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（datazen-ui 110 passed，datazen-ui-api 54 passed） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 网格四键导航就位，焦点始终在模型内。遗留：无（Ctrl+Home/End 跳 A1/末格属产品化增强，不在本任务判据内）。
