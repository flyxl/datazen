# TN N11 导出与备份 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`05c7f95 feat(rune): exports and backups run as real jobs behind the UI's job vocabulary (issue-rune N11)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（trait 增 `start_export`/`start_backup`/`host_job_progress`/`stop_host_job`，默认固定错误/None）、`crates/datazen-ui/src/lib.rs`（导出表单带上所选表名、宿主优先四路径 + `job_progress` 合流、内存回落不变）、新 `crates/datazen-rune-host/src/jobs.rs`（HostJobs 线程作业 + CSV/JSON 写出 + 测试）、`crates/datazen-rune-host/src/live.rs`（LiveAdapter 持 HostJobs/数据目录、四实现、`plain_config` 助手、测试）

## 实现要点

- **UI 词汇**：P6 导出对话框与 P9 备份页的既有 job+progress 模型原样保留——宿主优先、`DESKTOP_NOT_IMPLEMENTED` 回落内存端口（演示进程零变化）；`job_progress` 合流（宿主作业查宿主表，否则查端口）。
- **真作业**（每作业一线程一 runtime 一连接——整表读与 dump 不是帧循环该等的）：共享状态表存 `ProgressSnapshot`（百分比/日志/终态），`stop` 置停标（作业在步间检查；已完作业答其所成）。
- **导出**：`SELECT *` 整表 → CSV（表头+每行、含逗号/引号/换行的字段加引号转义）或 JSON，落在 `data-dir/exports/export-<table>-<ts>.<ext>`；**范围与格式诚实拒绝**：非空且非 "all" 的 range、未接线的 sql 格式都报失败句而非瞎猜。
- **备份**：驱动自己的 `dump_database` → `data-dir/backups/backup-<db>-<ts>.sql`；数据库名净化成安全文件名（sqlite 的 database 是文件路径——首跑把备份写进了嵌套路径，测试当场抓住）。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `export_jobs_write_real_csv_and_json` | live.rs | CSV 表头恰为 `id,name`、61 行（60+表头）、首末行值正确；不支持的范围失败；未接线的 sql 格式失败 |
| `backup_jobs_write_the_dump` | live.rs | 备份文件存在、非空、含 CREATE TABLE 与表名（dump 带真 schema） |

既有测试零回归（宿主 26、UI 122+55）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（122 + 55 passed，P6/P9 既有测试覆盖内存路径） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（26 passed；新增 2 个测试） |
| GR4 | `cargo check -p datazen-rune-host --offline --all-targets` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 计划验收标准"CSV 行列数对；备份文件存在且非空"达成。遗留：导出目标路径目前固定在数据目录（原生文件对话框是宿主能力任务，rune 的 FileChooser 特性现为关）；sql 格式导出与压缩备份未接线（如实拒绝）；N12 同步/对比/传输。
