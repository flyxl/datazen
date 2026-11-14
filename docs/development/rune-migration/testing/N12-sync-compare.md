# TN N12 同步/对比/传输 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`54ca210 feat(rune): one-table sync and schema compare between two live connections (issue-rune N12)`
- 改动面：`crates/datazen-rune-host/src/live.rs`（`sync_table`/`compare_table` 两个宿主函数、NoAdapters/NoPreview 两个端口、测试）、宿主 crate 清单（schema_diff 特性）。**UI 侧零改动**——D 系列页面仍走内存端口（接线是前端任务，另立）。

## 实现要点

- **同步**（`sync_table`，组合既有用例、宿主零自有 SQL）：源读走读用例（新源连接上的 SessionRunner、500 行/页翻页）；目标写走共享插入构造器（`build_insert_statement`，N6 修过的同一占位符翻译）+ 执行用例（`execute_at_target`）。**目标表须已存在**——桌面同步要求用户确认的同一契约。返回落库行数。
- **对比**（`compare_table`）：schema-diff 用例的 `compare_table_schemas` 双端点直调，**raw 对比形态**——跨方言适配器不装配（NoAdapters/NoPreview），sqlite↔sqlite 无需；`missing_on_target` 即同步的 ADD 清单，是原始对比自己的答案。
- **测试设计教训**（首跑抓到的真行为）：往缺列的旧形表里 insert 被诚实拒绝（SqlError）——同步与"缺失列发现"是两条独立的验收线，夹具按各自形状分开造。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `sync_copies_a_table_and_compare_finds_the_missing_column` | live.rs | 两个真库：60 行源数据经同步落进同形目标表（第三连接数回 60）；旧形 orders 表的 note 列出现在 `missing_on_target`（计划验收标准两条全中） |

既有测试零回归（宿主 27、UI 122+55）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（122 + 55 passed，本任务无 UI 改动） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（27 passed；新增 1 个测试） |
| GR4 | `cargo check -p datazen-rune-host --offline --all-targets` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 宿主具备两连接间的单表数据搬运与结构对比能力，经真用例与真双库验证。遗留：D 系列 UI 页面接这两个宿主函数（前端任务）；多表/键集游标/变更集（data_sync 域的完整机械）与 data_transfer 的列映射按需另立任务——本系列的验收线（单表同步 + 缺失列）已达成。
