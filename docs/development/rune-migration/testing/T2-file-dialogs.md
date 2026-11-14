# T2 文件对话框 — 测试报告

- 任务：开 rune `native-file-dialog` 特性；导出/备份经 `save_file` 选保存路径；导入向导经 `open_file` 选输入文件；对话框不可用或取消时如实提示。
- 日期：2026-11-14
- 提交：见 tasks.md T2 行。

## 结论

通过。GR1 131+55、GR3 33 全绿，wasm 目标照常编译（H6 不变量保持）。

## 实现面（如实）

- **特性开启**：`datazen-ui/Cargo.toml` 在 `cfg(not(target_arch = "wasm32"))` 下给 rune 加 `native-file-dialog`（mac/win 走 rfd，Linux 走 XDG portal over zbus）。锁文件已含 zbus 6.2.1 与 rfd，离线解析成功。wasm 目标不编这套（H6 结论保持）。
- **词汇演进**：`DesktopAdapter::start_export` 增 `path: Option<&str>`、`start_backup` 增 `path: Option<&str>`——导出/备份请求现在自带目的地；`None` 维持宿主默认目录。宿主 `spawn_export`/`spawn_backup` 增 `picked: Option<PathBuf>`，选定路径覆盖默认命名（父目录仍会创建）。
- **UI 时序**：Export/Start backup 按钮改 `on_press_tx`，先 `tx.save_file`（导出带表名/格式建议名与过滤器，备份建议 `datazen-backup.sql`），应答后开作业。三条应答路径：
  - `Ok(Some(path))` → 带路径开作业；
  - `Ok(None)`（取消）→ 什么都不写，如实提示"cancelled; nothing was written"；
  - `Err`（无 portal/无窗口/特性缺席/对话框已开）→ 如实提示"path picker is unavailable"并**回落宿主默认目录**开作业——N11 的无对话框导出能力不回归。
- **导入向导**：Step 1 加 Browse… 按钮 → `tx.open_file`（无过滤器）→ 选定路径填入 source 输入框；Err 如实提示"type the path instead"。
- **发现并修正一处死接线**：无。T1 已修过 on_paste 的死接线；本任务未发现新的（`tx.save_file/open_file` 的 headless 接缝由 rune 的 `Runtime::file_dialog`/`resolve_file_dialog` 提供，非死代码）。

## 测试（GR1 新增 4 例 + 改 3 例；GR3 内改 4 处断言 + 新 2 段）

新增：
1. `an_export_uses_the_picked_save_path`：假适配器记录请求 → 选定 `/picked/exports/customers.csv` → 适配器收到 `(table, csv, range, Some(path))`，作业号回填。
2. `a_cancelled_save_dialog_starts_no_export`：取消 → 适配器零调用、无作业、提示含 "cancelled"。
3. `a_save_dialog_that_cannot_show_falls_back_to_the_default_directory`：对话框开着时再按 Export → 第二次请求被拒（Err）→ 回落默认目录（适配器收到 `None`）且提示含 "default directory"；先开的对话框事后应答，路径照用。
4. `browsing_picks_the_import_source`：Browse → Open 对话框 → 选 `/imports/dump.csv` → source 输入框填入。

改写（时序从"按下即开作业"变为"按下开对话框、应答后开作业"）：`an_export_without_a_range_gets_no_job`、`a_failed_export_shows_its_error_line_from_the_snapshot`、`a_backup_page_stops_and_shows_stopped`——均加 `answer_save_dialog` 辅助。

宿主侧（GR3）：`export_jobs_write_real_csv_and_json` 补"选定路径覆盖默认目录"段（`picked-exports/users.csv` 落盘 61 行）；`backup_jobs_write_the_dump` 补选定路径段。

## 门禁

- GR1 `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：131 + 55 passed。
- GR2 `cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline`：exit 0（`[lints]` 清单 warning 为全仓预先存在）。
- GR3 `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1`：33 passed。
- GR4 `cargo check -p datazen-rune-host --offline`：exit 0。
- **wasm（H6 不变量）**：`cargo build -p datazen-ui --target wasm32-unknown-unknown --offline` → Finished，cdylib `datazen_ui.wasm` 产出。排障：磁盘满导致 bin 链接 SIGBUS，清 `registry/src`（cache 内有 .crate 可离线重解压）后通过。
- G1–G4（前端）：未改 `packages/frontend`、`packages/web`，不适用。

## 遗留（如实）

- 对话框开着时再按 Export 的双重作业竞态：第二次（回落）与第一次（事后应答）各开一个作业，`export_job` 字段以后者为准——如实记录，未做互斥（真实桌面一次只显示一个对话框，portal 拒绝第二次请求本身就是互斥）。
- 真机 portal/rfd 行为（Linux 无会话总线、macOS sheet）无法在沙箱验证，接缝经 `resolve_file_dialog` 全覆盖；真机验证留给用户环境。
