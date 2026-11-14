# TN N8 设置持久化 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`f0babc4 feat(rune): the settings page persists through the settings use case (issue-rune N8)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（trait 增 `read_ui_settings`/`write_ui_setting` 默认实现）、`crates/datazen-ui/src/lib.rs`（`ui_settings`/`write_ui_setting` 助手，table_page_size/主题切换/设置页/视图全部消费点改读助手；测试）、新 `crates/datazen-rune-host/src/settings.rs`（FileSettingsRepository + 测试）、`crates/datazen-rune-host/src/live.rs`（LiveAdapter 持设置服务与缓存文档、两个实现、测试）、宿主 crate 清单（settings 特性）

## 实现要点

- **trait 扩展**：`read_ui_settings(&self)`（内存速读——宿主缓存文档）与 `write_ui_setting(&mut self)`，默认答 `DESKTOP_NOT_IMPLEMENTED`，UI 按固定错误回落内存端口（H2 模式）——演示进程行为零变化。
- **UI 消费点**：网格分页大小、主题切换、视图配色、设置页三字段，全部经 `ui_settings()`（宿主优先）与 `write_ui_setting()`（宿主优先、真失败显示）。P1 的"一项失败不影响其他项"语义保留（每项独立写、独立报错）。
- **宿主实现**：校验用产品规则（页大小 1..=500、最大行 1..=10000，与内存端口同句），文档缓存在内存、写经 `SettingsService::save_app_settings`（文件原子写）。**读缓存一致性**：写成功后回存服务返回的快照。
- **文件形态**：数据目录 `settings.json`（app-settings 文档，UI 三项在 `"ui"` 键下）；缺文件 = 全新安装（无 ui 设置，UI 回落内存端口默认值——两组默认值一致：25/100/dark）；坏文件拒绝读不静默清空。**shared 半边（类型化 SharedSettings）诚实拒绝**——本宿主尚无消费者，宁拒不造假。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `a_missing_file_reads_empty_and_the_first_write_lands` | datazen-rune-host/settings.rs | 缺文件读空文档；首写落盘；新仓储重开可见（持久化非会话缓存） |
| `a_corrupt_file_refuses_to_read` | datazen-rune-host/settings.rs | 坏文件报错句 |
| `ui_settings_persist_through_the_file_and_survive_a_reopen` | datazen-rune-host/live.rs | 新适配器无 ui 设置→写两项→读回两项→重开适配器仍见；越界值被拒且磁盘未变 |
| `a_host_that_persists_settings_answers_the_reads_and_writes` | datazen-ui（runtime） | 宿主答设置时网格按宿主页大小分页；写到达宿主并生效；内存端口不被镜像（仍是 25，回落非复制） |

既有测试零回归（UI 122+55、宿主 19）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（122 + 55 passed；新增 1 个测试） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（19 passed；新增 3 个测试） |
| GR4 | `cargo check -p datazen-rune-host --offline --all-targets` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 设置页/主题/页大小在活宿主上跨进程持久。遗留：P5 的 SQL 片段与 P4 的连接表单仍走内存端口（各自独立任务：片段可存进 app-settings 文档的扩展键，连接表单应走 ProfileRepository::create——后者已在 N5 备好接缝）；N9 查询历史。
