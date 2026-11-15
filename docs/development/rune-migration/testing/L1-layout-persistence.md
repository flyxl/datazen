# L1 布局尺寸持久化 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 任务：L1 布局尺寸端到端持久化（`Setting::Layout { key, value: u32 }`）——ui-api 枚举与共享校验、MemoryWorkspace 按区域键去重、宿主 live.rs 读写映射到 `ui.layout` 子对象、UI 侧 `layout_size`/`set_layout_size` helper。
- 被测提交：见 tasks.md L1 行（feat）；本报告随 docs 提交落库。
- 改动面：`crates/datazen-ui-api/src/lib.rs`、`crates/datazen-rune-host/src/live.rs`、`crates/datazen-ui/src/lib.rs`（不改 `packages/frontend`、`packages/web`、内核 `SharedSettings` 契约）。

## 结论

通过。布局尺寸经 `Setting::Layout` 走既有 settings 链路持久化：整数 pt 值（`Eq` 兼容）、`ui.layout` 子对象落盘、一键一槽（同区域替换、不同区域共存）；共享 `Setting::validate()` 让 memory 端口与宿主给出同一拒绝句。GR1/GR2 全绿；GR3 为 59+3 基线环境条件性失败，与本改动无关。

## 实现面

- **ui-api**：`Setting` 新增 `Layout { key: String, value: u32 }`；`Setting::validate()` 共享校验（key：`[a-z][a-z0-9-]{0,47}`；value：`1..=10000`；标量沿用既有 1..=500 / 1..=10000）；MemoryWorkspace `write_setting` 改为先 `validate()` 后按槽去重；`replaces_slot` 对 layout 按键去重（同一 region 替换、跨 region 共存），标量仍按 kind。
- **宿主 live.rs**：读映射从 `ui.layout` 子对象收集 layout 项；`write_ui_setting` 统一走 `setting.validate()`，`UiWrite::{Scalar,Layout}` 分派（layout 写入 `ui.layout[region]`）；测试扩展：布局往返/同键替换/异键共存/重开可见/坏键与坏值拒绝且磁盘不变。
- **ui**：`AppState::{layout_size,set_layout_size}` helper（读走 memory-speed `ui_settings()`，写走 `write_ui_setting` 的宿主优先/回退路径）；测试假适配器补 `Layout` 槽位；新增 `layout_sizes_round_trip_through_the_settings_ports`（回退路径、同键替换、拒绝且不落存）。

## 测试

| 测试 | 位置 | 覆盖 |
|---|---|---|
| `layout_settings_validate_and_dedup_per_region_key` | `crates/datazen-ui-api/src/lib.rs` | 合法写入、同 region 替换、异 region 共存、坏键/空键/越界拒绝且旧值不变、标量与 layout 共享校验 |
| `ui_settings_persist_through_the_file_and_survive_a_reopen`（扩展） | `crates/datazen-rune-host/src/live.rs` | layout 落盘/重开、同键替换、异键共存、坏键 `"layout key"` 与坏值 `"1..=10000"` 拒绝且文件不变 |
| `layout_sizes_round_trip_through_the_settings_ports` | `crates/datazen-ui/src/lib.rs` | UI helper 经 memory 回退读写、同键替换、坏键拒绝且不落存 |

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `CARGO_TARGET_DIR=$PWD/target cargo test -p datazen-ui-api --offline --lib` | **56 passed / 0 failed** |
| GR2 | `CARGO_TARGET_DIR=$PWD/target cargo test -p datazen-ui --offline --lib -- --test-threads=1` | **147 passed / 0 failed** |
| GR3 | `CARGO_TARGET_DIR=$PWD/target cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | **59 passed / 3 failed**（基线环境条件性：`remote_drivers_answer_honest_refusals_from_a_closed_port`——本机 6379 有 redis-server 监听；`a_keyring_that_cannot_be_reached_fails_closed_without_a_legacy_key`——macOS 钥匙串可达；`an_encrypted_password_without_a_key_is_an_honest_unavailability`——钥匙串可达但密钥不匹配时明文透传的既有产品缺口） |
| GR4 | 沿用基线（未改宿主入口/feature，GR2/GR3 编译已覆盖） | — |
| fmt | 未重排：分支基线 17 个文件（794 处）与 rustfmt 1.99.1/1.100.0 均不一致，CI 不跑 fmt；按周边风格手写避免噪音 | — |

## 备注

- 值类型取整数 pt（`Setting: Eq`）；拖拽产生的分数由后续 L2 在持久化前取整。
- 运行 GR3 前后清理了测试写入的钥匙串条目（service `datazen-native`，account `app-encryption-key`）。
