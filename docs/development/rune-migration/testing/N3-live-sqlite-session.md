# TN N3 真实 sqlite 会话 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`cb72a28 feat(rune): a real sqlite session and the live catalog in the tree (issue-rune N3)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（`AdapterJob::OpenConnection`、`AdapterDelivery::ConnectionOpened` 带连接 id 回传）、`crates/datazen-ui/src/lib.rs`（`desktop_catalog`/`desktop_connection`/`connect_loading`、`catalog()`/`session_open()`/`refresh_catalog()`、三个 catalog 消费点改读助手、`open_connection` 作业路径、投递臂、连接页 Connecting 标记、三个会话按钮的门控）、新 `crates/datazen-rune-host/src/live.rs`（JobRunner 工作线程 + `LiveAdapter`）、`crates/drivers/sqlite/src/lib.rs`（导出 `factory()`）、宿主 crate 清单（sqlite 驱动 + tokio + futures-executor）

## 实现要点

- **作业化打开**（不变量 3）：连接点击先问作业。受理后连接页保持列表 + `Connecting…` 标记；投递到达前不离开连接页。目录 Ok → live 目录成为树的真相（`AppState::catalog()` 优先），工作区打开第一张真表（读路径照旧，N4 接真数据前读会被拒、网格保持原样）；失败句落进 H2 的适配器提示行。
- **会话语义**：ER/刷新/断开三个按钮改按 `session_open()`（任一世界的会话）门控——live 会话下断开按钮不再消失；断开清 live 目录；刷新经同一作业路径重开 live 连接。
- **宿主工作线程**：一根线程 + current-thread tokio runtime 吃作业通道，`poll_jobs` 排空答案通道；`block_on` 只出现在线程内。
- **驱动解析（静态形式）**：`DriverRegistry::with_factories([sqlite::factory()])`。选静态不选 inventory 懒扫描的原因（测试里实测）：懒形式的 inventory 注册在测试二进制链接时会被丢弃（诊断测试显示工厂清单为空），静态形式在测试与真机行为一致，且宿主的驱动清单是显式的一行接缝。sqlite crate 因此新增 `pub fn factory()` 导出（增量改动，注册宏不动）。
- **目录塑形**：走驱动的 catalog 契约（get_databases/get_tables/get_schemas），schema 层按 registry 自己的推导规则（驱动报名字，否则一个以库名命名的节点——sqlite 无 schema 层）。
- **sqlite 事实**：`create_if_missing=false`，文件不存在是连接错误不是新库——正是"坏路径"语义；夹具先建空文件（与仓库既有真 sqlite 夹具写法一致）。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `an_accepted_open_job_fills_the_tree_with_the_live_catalog` | datazen-ui（runtime） | 受理后连接页等待 + 标记；投递后工作区打开、树是 live 目录的表（users/events，非演示 customers）；断开按钮在 live 会话下可见，断开后目录随会话清空 |
| `a_failed_open_keeps_the_connections_page_and_shows_the_sentence` | datazen-ui（runtime） | 失败投递清标记、停留在连接页、句子进提示行（经 `elements` 读文本断言） |
| `the_live_adapter_opens_a_real_sqlite_session_and_lists_its_tables` | datazen-rune-host | 真驱动+真文件：连接列表来自文件；打开作业返回 main 库单 schema 节点，两表按 sqlite_master 名序；会话留存给 N4/N5 |
| `a_bad_database_file_answers_an_error_and_the_adapter_survives` | datazen-rune-host | 垃圾文件报错不崩；随后好连接照常打开 |
| `an_unknown_connection_answers_a_sentence` | datazen-rune-host | 未知 id 的失败句含 "no connection" |

既有 118 + 55 + 5 个测试零回归。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（120 + 55 passed；新增 2 个测试） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（8 passed；新增 3 个测试） |
| GR4 | `cargo check -p datazen-rune-host --offline --all-targets` | 通过，exit 0（含 bin） |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 树是真目录、会话是真 sqlite 文件、坏路径如实报错且不崩。遗留：N4 把读表页接到 `table_browsing` 用例（会话已在 `LiveAdapter.session()` 里等）；真机窗口的目视验证仍留给用户环境（沙箱无显示器）。
