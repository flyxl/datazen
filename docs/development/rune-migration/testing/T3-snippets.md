# T3 片段持久化 — 测试报告

- 任务：宿主 `snippets.json` 文件仓储（原子写）接 P5 片段端口，宿主优先、内存端口回退。
- 日期：2026-11-14
- 提交：见 tasks.md T3 行。

## 结论

通过。GR1 132+55、GR3 36 全绿。

## 实现面（如实）

- **词汇**：`DesktopAdapter` 增四个默认方法 `snippets`/`save_snippet`/`update_snippet`/`delete_snippet`（默认固定错误，与 N8/N9 同款回退语义）。UI 的片段表单与清单改为宿主优先：读 `host_snippets()`（宿主在则宿主，否则内存端口），写走宿主、`DESKTOP_NOT_IMPLEMENTED` 落内存端口（演示路径零变化）。
- **宿主仓储** `crates/datazen-rune-host/src/snippets.rs`：`FileSnippets`——JSON 数组、原子写（`.json.tmp` + rename）、缺文件/空文件为空仓。语义与内存端口逐字对齐：建拒绝重名、改/删拒绝未知名。序列化走宿主私有行类型（ui-api 刻意无 serde，依赖纯净性保持）。
- **live.rs**：`LiveAdapter` 持 `Arc<FileSnippets>`（`data_dir/snippets.json`），四个方法直通仓储；读是内存速（文件构造时载入）。

## 测试（GR1 新增 1 例；GR3 新增 3 例）

- UI `the_snippets_panel_reads_and_writes_the_host_store_first`：宿主适配器带一条片段 → 面板列出它且内存端口为空；表单保存落到宿主（`snippet-row-1` 出现、端口仍空）。
- 宿主 `snippets_round_trip_through_the_file`：建/拒绝重名/改/拒绝未知名/删/二次删拒绝，经重开的仓储读回；文件承载内容。
- 宿主 `an_empty_or_missing_file_starts_empty`：缺文件与空白文件都是空仓。
- 宿主 `snippets_persist_through_the_adapter_and_the_file`：经适配器（DesktopAdapter 面）建/改/删后，直接重开文件仓储读回同一条；`snippets.json` 落盘。踩坑：活会话持有 sqlite 文件锁，"重开整个适配器"会锁死——重开验证走文件仓储而非适配器（适配器直通仓储的等价性由前两者钉死）。

## 门禁

- GR1 `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：132 + 55 passed。
- GR2 `cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline`：exit 0。
- GR3 `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1`：36 passed。
- GR4 `cargo check -p datazen-rune-host --offline`：exit 0。
- G1–G4（前端）：未改 `packages/frontend`、`packages/web`，不适用。

## 遗留（如实）

- 片段与连接无关联（桌面版片段带 connectionId 语义的收藏是 T6 的域，普通片段维持 P5 的全局形状）。
