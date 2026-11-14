# TN N10 结构页真目录 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`48e8713 feat(rune): the structure pages read the live catalog (issue-rune N10)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（trait 增 4 个结构读取、默认固定错误）、`crates/datazen-ui/src/lib.rs`（AppState 4 个 structure 助手；列/索引/外键/DDL/ER 五个视图面改读助手；三个视图签名从 `&MemoryWorkspace` 改 `&AppState`）、`crates/datazen-rune-host/src/live.rs`（schema 缓存、专属同步 runtime + 惰性同步连接、cached_schema/sync_session、四个实现、夹具加 orders 表与唯一索引、测试）

## 实现要点

- **词汇映射**：驱动 `TableSchema` 一次给全列+PK+索引+外键——列/索引/外键三页从同一缓存映射（`ColumnSchema→ColumnSpec`、`IndexInfo→IndexSpec`、`ForeignKeyInfo→ForeignKeySpec`）；DDL 走驱动自己的 `get_object_ddl` 命令（`{ddl}` 应答）。
- **缓存与预热**：读作业在页读取旁顺手暖缓存（本地 sqlite 双读同速级）；缓存 miss 时同步取。**专属同步 runtime + 独立连接**（实测教训）：sqlx 需要 Tokio 上下文，且活会话的单连接池属于 worker 的 runtime（跨 runtime 借出会 pool timed out）——同步读走自己的惰性连接（按连接 id 复用、换连失效）。
- **空 schema = 无此表**：驱动对未知表答空列——按诚实边界报 "the catalog has no table 'x'" 而非空清单。
- **ER 画布**（O7/O8）的结构消费同路升级（盒子的列来自真目录）。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `structure_reads_answer_the_real_catalog` | live.rs | 列表 3 列名/类型/可空与建表一致；唯一索引在列（名/列/唯一性）；外键整体保留（user_id→users.id）；DDL 是驱动自己的渲染（含 CREATE TABLE 与表名）；未知表失败句 |

既有测试零回归（夹具从 2 表扩到 3 表——catalog 断言随之更新；宿主 24、UI 122+55）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（122 + 55 passed，本任务无新 UI 测试——五个视图面是纯读替换，既有 O 系列测试覆盖渲染） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（24 passed；新增 1 个测试） |
| GR4 | `cargo check -p datazen-rune-host --offline --all-targets` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 结构四页 + ER 在活会话下显示真目录。遗留：远程驱动（postgres 等）的结构读取需走作业通道（网络 IO 不属同步类的边界，接入时改造）；O9–O11 的列草稿/应用仍走内存端口（草稿应用要 DDL 执行，接 `execute_at_target` 即可，属后续扩展）。
