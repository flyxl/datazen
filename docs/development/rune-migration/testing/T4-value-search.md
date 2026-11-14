# T4 值搜索 — 测试报告

- 任务：值搜索面板（值+匹配模式），对当前连接逐表逐列跑 `search_table_values`（作业化），结果清单（表·列·命中行数）点击开表并带过滤。
- 日期：2026-11-14
- 提交：见 tasks.md T4 行。

## 结论

通过。GR1 133+55、GR3 37 全绿。

## 实现面（如实）

- **词汇**：`AdapterJob::ValueSearch { generation, value, mode }` + `AdapterDelivery::ValueSearched { generation, hits }` + `ValueSearchHit { table, column, matches }`。mode 的五个取值对齐 React 的 `ValueSearchMode`（contains/notContains/exact/prefix/suffix）。
- **UI**：Tools 页新增 "Search values" 工具页签。面板：值输入、模式循环片、Search/Cancel、载入行、错误行、命中清单（`value-hit-N`，`表 · 列 · N rows`）。点击命中 = 定位意图：开该表的页签（页签生命周期会重置过滤，属 S3 语义），随后设 `ColumnFilter{column, contains: 搜索值}` 并重读——定位过滤用**搜索时的值**（`value_search_query` 快照），不受后续输入影响。取消 = 代次前移，扫描投递落地即弃（Q5/Q6 同款语义）。演示进程无活会话，诚实拒绝（"value search needs an open connection" / "needs the live host"），内存端口无扫描、不做假。
- **宿主**：worker 新增 `HostJob::ValueSearch`——锁会话拿 driver/handle/database（`ConnectionHandle` 可克隆，锁不跨 await），`get_tables` → 逐表 `get_table_schema` → 逐列 `search_table_values`（每列一次，与 React 扫描同粒度——命中数即列级答案），命中>0 进清单。宿主 datazen 特性加 `services`（value_search 所在，离线可解析）。runtime 启动失败的 fallback 臂同样覆盖。
- **范围（如实）**：暂停/继续（React 扫描的前端逐表驱动）未做——Rune 侧是单作业整扫 + 取消（代次弃置），宿主扫描在 worker 线程内连续执行，期间后续作业排队（本地 sqlite 有界 IO 类，与 N10 同判据；远程驱动接入时按计划移到分片作业）。

## 测试（GR1 新增 1 例；GR3 新增 1 例）

- UI `a_value_search_lists_hits_and_a_hit_locates_the_table_with_a_filter`：空值诚实拒绝 → 注入活会话（经 UI 自己的 `deliver_host_job`，JobAdapter 无连接可按）→ 作业在途 → 两轮投递落定 → 两条命中列出 → 点击命中二：页面切 Workspace、过滤列 name、值是搜索快照 → 二次搜索 + 取消：标记清、错误行出现、被弃投递不落清单。
- 宿主 `a_value_search_finds_the_columns_that_match`：夹具库（users 60 行 `user-00..59`、events 无匹配值）——contains "user-4" → users.name 10 行、events 零命中；suffix "2" → 6 行；exact "user-42" → 唯一列唯一行；无匹配值 → 诚实空清单。
- 踩坑两则（T1 教训重演）：工具页切换后浮现动画未完成时按钮不可点，测试补帧；`ConnectionOpened` 投递会切到 Workspace，会话注入须在导航去 Tools 之前。

## 门禁

- GR1 `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：133 + 55 passed。
- GR2 `cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline`：exit 0。
- GR3 `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1`：37 passed。
- GR4 `cargo check -p datazen-rune-host --offline`：exit 0。
- G1–G4（前端）：未改 `packages/frontend`、`packages/web`，不适用。

## 遗留（如实）

- 暂停/继续未做（见上）；扫描进度行只有 "Searching…" 无逐表百分比（作业通道单投递，进度流是后续任务）。
- 定位打开的表若在只读连接上，过滤照常生效（读路径本就支持）。
