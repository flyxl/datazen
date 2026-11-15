# TE E1 结构读作业化 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`fc1c65804 feat(rune): the structure reads travel as one job (issue-rune E1)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（`TableStructure` 四合一载体；`AdapterJob::ReadStructure`/`AdapterDelivery::StructureRead`）、`crates/datazen-ui/src/lib.rs`（AppState 增 `structure_generation`/`structures` 缓存与 `StructureState`；开表/切视图/开 ER/应用列草稿四处发作业；列/索引/外键/DDL 四视图、ER 盒子、出站 FK 导航改读缓存；投递分发带代次；4 个新测试 + 测试假适配器补臂 + `StructureStrictHost`）、`crates/datazen-rune-host/src/live.rs`（`HostJob::ReadStructure`/`HostAnswer::Structure`；worker 分支；`read_structure` 异步函数；`poll_structure` 助手；2 个新测试）

## 实现要点

- **四读一作业**：`AdapterJob::ReadStructure { generation, table }` 一次取 DDL+列+索引+外键——worker 经活会话两个驱动调用（`get_table_schema` 给三段并暖共享 schema 缓存，`get_object_ddl` 给 DDL）合成一个投递，远程驱动的网络 IO 对帧循环只花一次投递的耐心。
- **渲染零适配器**（不变量 2）：四个结构视图、ER 画布、出站 FK 按键全部只读 `structures` 缓存；`StructureStrictHost` 的四个同步读方法 panic，测试渲染五个视图 + ER 页全程不触发。
- **代次按表计**：每表一个条目各带自己的 generation——换表/切视图/重开使旧答案作废；同一投递批次里旧代次答案先到也不写缓存（多表并发的 ER 场景互不误伤）。
- **诚实状态**：读在途（"reading the table's structure…"）、失败句子、未加载提示（"switch the view or reopen the table"）都如实渲染；FK 按键在缓存未齐时给句子而非静默无操作，结构落地后的成功导航清掉上一句。
- **DDL 段降级**：DDL 命令失败退空串（DDL 视图自己的空态），不算结构读失败——驱动没有 DDL 命令不等于列/索引/外键读失败。
- **拒绝即回退**：`begin_job` 拒绝（演示进程）时同步读内存端口当场填同一缓存——渲染路径两种宿主形态都无适配器调用。
- **实测教训**：match 守卫在整个 match 语句期间持有 `schemas` 锁，None 分支内再取锁同线程死锁——先 `let cached = ...` 落 guard 再 match（宿主测试 3 秒超时抓到，测试先行兑现）。
- **新会话即新真相**：`ConnectionOpened` 与断开清空结构缓存。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `a_table_open_sends_one_structure_job_and_the_delivery_fills_the_cache` | ui lib.rs | 开表即发一条作业、条目在途；投递落地后四段齐（列名/唯一索引/非空 DDL） |
| `a_late_structure_delivery_for_a_superseded_generation_is_dropped` | ui lib.rs | 切视图使条目换代次；同批投递旧代次先到不写缓存，当前代次答案落地 |
| `the_outbound_fk_key_reads_the_cache_and_is_honest_while_it_lands` | ui lib.rs | 结构在途时按键给"still loading"句子；落地后按 FK 配对导航开目标表并带列过滤器 |
| `the_structure_views_render_from_the_cache_and_never_touch_the_adapter` | ui lib.rs | 四视图+数据视图+ER 页逐帧渲染，同步结构读 panic 的严格宿主全程不触发；ER 盒子结构落地无 pending 提示 |
| `structure_jobs_answer_the_four_segments_from_sqlite` | live.rs | 真会话四段：orders 三列名序、唯一索引在列、外键 user_id→users、DDL 非空 |
| `a_structure_job_for_an_unknown_table_answers_a_sentence` | live.rs | 未知表诚实句 "the catalog has no table 'nope'" |

既有测试零回归（开表路径的内存回退被既有演示流程隐式覆盖）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（55 + 142 passed，新增 4） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib` | 通过，exit 0（55 passed，新增 2） |
| GR4 | `cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 帧循环上不再有结构读：四个结构视图、ER 画布与出站 FK 导航只读缓存。遗留：部件柱状图（`bar_chart` 每帧同步查询）与入站引用清单仍是帧循环同步读、宿主的同步结构读路径（`sync_session`/`cached_schema`）仍在——分别由 E2、E3 按计划处理。
