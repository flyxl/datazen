# TN N2 宿主 crate 与文件连接存储 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`0737388 feat(rune): the native host crate over a file-backed connection store (issue-rune N2)`
- 改动面：新 crate `crates/datazen-rune-host`（lib + bin `datazen-native`）、`crates/datazen-ui/src/lib.rs`（`run_with_adapter`）、`crates/datazen-ui-api/src/lib.rs`（导出 `DESKTOP_ORGANIZATION`）、根 `Cargo.toml`（workspace members）

## 实现要点

- **宿主 crate 的边界**（计划不变量 4）：`datazen-rune-host` 是活宿主胶水的家——bin `datazen-native` 依赖 `datazen-ui`（共享视图）、`datazen-ui-api`（接缝类型）、`datazen`（connection/execution 特性切片，与 ui-api 的 UseCaseAdapter 相同）。`datazen-ui` 与 `datazen-ui-api` 不新增真实驱动依赖。
- **`run_with_adapter`**（datazen-ui）：`AppSpec::init` 是裸函数指针装不下宿主的适配器；rune 的 `App::init` 接受捕获闭包，`run()` 演示进程的 `init_state` 语义（首帧前询问适配器）原样搬到活宿主。
- **文件仓储**（store.rs）：一个 JSON 文件（`connections` + `groups`），原子写（兄弟临时文件 + rename）。缺文件 = 全新安装，开成空仓不报错；**存在但读不出/解析失败的文件拒绝打开**——打开会让下一次保存把用户数据换成空仓，宁可进程退出并给出句子。单锁同时守住文件与内存列表，读写不脱节。
- **范围语义**：仓储只服务 `DESKTOP_ORGANIZATION`（从 ui-api 导出，与 UseCaseAdapter 查询的范围配对），越界访问报错。
- **接法**：`UseCaseAdapter::from_repository(Arc<FileConnectionRepository>)`——H3 留的接缝原样使用，连接用例一行未改。默认路径 `${XDG_DATA_HOME:-~/.local/share}/datazen-native/connections.json`，`DATAZEN_NATIVE_CONNECTIONS` 可覆盖。
- **N2 完成标准达成**：`datazen-native` 的连接列表来自文件而非演示种子——仓库里有文件时列表是文件内容（seam 测试断言），空文件/缺文件列表为空（不是 local/replica 演示连接）。

## 测试（全在 datazen-rune-host）

| 测试 | 位置 | 断言 |
|---|---|---|
| `a_missing_file_opens_empty_and_the_use_case_lists_nothing` | store.rs | 缺文件不创建、经用例列出空列表 |
| `a_saved_connection_round_trips_through_the_use_case_and_the_file` | store.rs | 经用例保存 → 文件含该 id；重开仓储再经用例列出，凭据已脱敏 |
| `delete_and_reorder_land_in_the_file` | store.rs | 重排与删除落到文件；`touch_last_connected_at` 打上时间戳 |
| `a_corrupt_file_refuses_to_open` | store.rs | 坏文件报错且句子含 parse，不静默清空 |
| `the_adapter_lists_the_file_store_through_the_desktop_seam` | store.rs | `UseCaseAdapter::from_repository(文件仓储)` 的 `list_connections` 列出文件内容——接缝端到端 |

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（118 + 55 passed，无回归） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（5 passed，新 crate） |
| GR4 | `cargo check -p datazen-rune-host --offline --all-targets` | 通过，exit 0（含 `datazen-native` bin） |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 活宿主有了家：进程能以文件为连接事实来源启动，接缝（`from_repository`）经测试走通。遗留：点开文件里的连接仍是演示世界的行为（N3 接真 sqlite 会话）；`~/.local/share` 路径的文档随 N13 的使用说明一起写。
