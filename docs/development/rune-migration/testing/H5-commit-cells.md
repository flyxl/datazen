# TN H5 提交脏格接到现有 use case 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`235574f7 feat(rune): commit dirty cells through the existing use case (issue-rune H5)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（`UseCaseAdapter::commit_cells` 委托现有提交用例：批次逐条 `stage_cell` 到打开页再 `commit_staged`）、`crates/datazen-ui/src/data_grid.rs`（`staged_edits()`：把网格暂存导出为 (行, 列名, 值) 批次，按行排序）、`crates/datazen-ui/src/lib.rs`（提交按钮的 H5 流程）

## 接线方式

- **现有提交用例**：内存工作区的暂存-提交链（`stage_cell` 按打开页窗口映射行号，`commit_staged` 一次应用全部暂存、失败不清空）。适配器批次里的行号按 UI 最后读的那一页编号——接线世界里读路径（H4）恰好维护着这同一页窗口，契约在方法注释中写明。
- **UI 提交流程**：提交按钮先问适配器（带网格暂存批次）。`DESKTOP_NOT_IMPLEMENTED`（固定"未实现"错误）= 宿主尚未接此用例，回落到端口自己的暂存提交（原 B6 行为，未接线测试全走这条路）；**任何其他失败都是真失败：不重载、脏标记原样保留**（判据）。成功则重载页面，`load_page` 清空网格暂存。
- **行号口径**：网格页内行号（与端口 `stage_cell` 的口径一致），适配器经自己世界的打开页窗口换算存储行。

## 测试

| 测试 | 位置 | 断言 |
| --- | --- | --- |
| `the_wired_adapter_commits_a_batch_through_the_existing_use_case` | datazen-ui-api | 两格批次提交落库 2 条，读回 alice/bob |
| `a_failed_batch_keeps_the_edits_that_did_stage` | datazen-ui-api | 批次中途失败（未知列），已暂存的格子保留并覆盖后续读 |
| `a_failed_commit_keeps_the_dirty_marks` | datazen-ui（runtime） | 注入"读可写、提交拒绝"的适配器：编辑一格后提交按钮失败，脏标记与编辑值原样保留（判据） |
| `a_wired_commit_lands_in_the_adapters_use_case_world` | datazen-ui（runtime） | 注入 UseCaseAdapter：编辑-提交后标记清空，提交值经适配器世界读回 |

既有 B6 测试（`commit_and_rollback_buttons_drive_the_staged_edits`，未接线路径）不受影响，全过。

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（datazen-ui 107 passed，datazen-ui-api 54 passed） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 提交脏格经适配器接到现有用例，失败时脏标记还在。Host 系列接缝（H2 类型、H3 列连接、H4 读页、H5 提交）全部就位。遗留：真机桌面宿主注入真实存储/驱动用例；读路径的"任何适配器错误都回落端口"语义与提交路径的哨兵区分语义将来统一为类型化错误时一并收口。
