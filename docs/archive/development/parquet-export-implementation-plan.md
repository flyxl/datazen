# Parquet 导出实现方案

- 状态：草稿，待评审
- 日期：2026-11-10
- 分支：`feature/parquet-export`（基点 `origin/main` `d868859ce`）
- 工作目录：`~/workspace/datazen/.worktrees/datazen-parquet-export`
- PRD：同目录 `parquet-export-prd.md`
- 前置方案：`~/workspace/goals/datazen/files/xlsx-parquet-export-plan-2026-11-09.md`（§5 Parquet、§6 实施顺序 PR 2、§3.5 特性拆分）

## 1. 基线（动手前核对过的现状）

| 位置 | 现状 |
|---|---|
| `crates/datazen/src/export/mod.rs:147` | `DataFormat { Csv, Tsv, Json, Markdown, SqlInsert, SqlUpdate, Xlsx }`，无 Parquet |
| `mod.rs:171` `is_binary()` / `mod.rs:703` `write_binary_file` | 二进制分支已存在（XLSX 用）；`write_data_file` 拒收二进制（`mod.rs:530` guard）；`run_single_export` 的文本合并分支有二进制 guard |
| `crates/datazen/src/export/binary.rs:40` | `BinaryFormatter` trait：`begin(&[ColumnInfo]) / rows(&[Vec<Option<Value>>]) / finish(&mut self)`；`XlsxFormatter::new`（`binary.rs:112`）构造时只接表名和目标路径 |
| `crates/datazen/src/export/mod.rs:723` | `write_binary_file` 按格式分发：`Xlsx => XlsxFormatter::new(...)`，其余二进制变体进 `"{format:?} is marked binary but has no binary formatter"` 错误 |
| `crates/datazen/src/export/mod.rs:193` | `ExportTablesRequest.database_type: Option<String>`——方言映射的输入就从这里取；为 `None` 时跳过映射、走值推断 |
| `crates/datazen-driver-api/src/types.rs:306` | `ColumnInfo { name: String, data_type: String, nullable: bool }`，无统一类型枚举 |
| `crates/datazen/Cargo.toml:111` | `export` feature：`serde_json / zip / rust_xlsxwriter / tokio / tokio-util / tracing / uuid / datazen-driver-api / execution / jobs`；`rust_xlsxwriter` 是 workspace optional（`:397`）；`calamine 0.31` 是 dev 依赖（`:451`，XLSX 读回验收用） |
| `Cargo.lock` | `arrow 58.4.0`（duckdb 驱动带进来的传递依赖）；`parquet` crate 不存在（grep 计数 0）；没有任何 crate 直接依赖 `arrow` |
| `crates/datazen-desktop/Cargo.toml:28` | 桌面宿主开 `export`（与其他几十个 feature 并列）；server 是否开 `export`——实现 T1 时核对（grep 只命中桌面这一处） |
| `packages/frontend/src/commands/file.ts:61` | `ExportDataFormat` 含 `'xlsx'`（`:68`） |
| `packages/frontend/src/platform/exportPlan.ts:31,51` | `formatExtension`（`'xlsx' → 'xlsx'`）、`isBinaryFormat`（现为 `format === 'xlsx'`，注释写明“today: XLSX only”） |
| `packages/frontend/src/components/export/BatchExportDialog.tsx:56` | `DATA_FORMAT_OPTIONS` 含 `{ value: 'xlsx', label: 'Excel (XLSX)' }`；`batchExport.ts` 已在清理中删除（D 项），前端接线只剩上述三处 + 格式规则 |
| 工作树 | basic 驱动集（默认）；`generated-locales.ts` 已退役、无生产引用；Pro 检出缺失（无已知测试后果） |

关键设计约束（PRD §4 F3 的实现含义）：`begin()` 只拿到 `ColumnInfo`，没有值，所以**值推断不能在 `begin()` 里做**。`ParquetFormatter` 必须延迟建 writer：`begin()` 只存列信息和方言；首个 `rows()` 到达时，用该批非空值推断 schema、建 `ArrowWriter`、再写该批；后续批次直接写。首批 ≤500 行，缓冲有界。空表（0 行）时 `finish()` 再用“方言映射 + Utf8 兜底”定 schema，写出只有 schema 的合法文件。

## 2. 任务拆分（每个任务拆 code 和 test 两个提交；覆盖率 ≥80% 才进下一个）

### T1. 依赖与特性拆分（code + test）

- `crates/datazen/Cargo.toml`：新增 `parquet` 与 `arrow` 直接依赖（optional），钉版本使 `parquet` 的 arrow 与 lock 里的 **58.4.0** 对齐（`cargo tree` 验证，不得出现两份 arrow）。若最新 parquet 要求 arrow 59+，二选一：(a) 钉兼容 arrow 58 的 parquet 版本；(b) 升级 arrow（牵连 duckdb 驱动）——决定写进提交说明，不静默选。
- 拆 `export-xlsx` / `export-parquet` 子 feature（PRD F9）：`export` 只留公共依赖；`export-xlsx = ["dep:rust_xlsxwriter"]`；`export-parquet = ["dep:parquet", "dep:arrow"]`。`datazen-desktop` 的 feature 列表补开两个子 feature；核对 server 是否开 `export`，按需处理。
- 门控：两个 formatter 及其分发分支挂 `#[cfg(feature = "export-xlsx")]` / `#[cfg(feature = "export-parquet")]`；`DataFormat` 变体本身不门控（线形稳定），对应子 feature 未启用时 `write_binary_file` 返回明确的 validation 错误。
- 验收：`cargo tree -p datazen -e features -i parquet` 只有一份 arrow 58.4.0；`cargo check -p datazen --features export`（不开子 feature）通过且 Parquet 请求返回明确错误；`cargo check -p datazen --features export,export-parquet` 通过。

### T2. `DataFormat::Parquet` 与扩展名（code + test）

- `mod.rs`：`DataFormat` 加 `Parquet`；`extension()` 返回 `"parquet"`；`is_binary()` 返回 `true`；`ALLOWED_EXPORT_EXTS` 加 `"parquet"`；`crates/datazen-desktop/src/commands/file.rs` 的 `ALLOWED_EXTENSIONS` 加 `"parquet"`。
- 前端类型：`file.ts` 的 `ExportDataFormat` 加 `'parquet'`（不改其他）。
- 测试：`extension()` / `is_binary()` 单测；非法扩展名仍被拒；前端 planner 镜像测试（`exportPlan.test.ts`）同步加用例。

### T3. Schema 推断模块（code + test）

- 新文件 `crates/datazen/src/export/parquet_schema.rs`（纯函数，与 writer 解耦、独立可测）：`infer_schema(columns: &[ColumnInfo], dialect: Option<&str>, first_batch: &[Vec<Option<Value>>]) -> arrow_schema::Schema`。
- 实现 PRD F3 的三步：方言映射表（PG / MySQL / SQLite / SQL Server 最小集）→ 首批值推断（`Integer→Int64`、`Float→Float64`、`Bool→Boolean`，其余 → Utf8；某列全 NULL 或值变体混杂 → Utf8）→ Utf8 兜底。`Timestamp→Utf8`、`Json→Utf8`、`Bytes→Binary`；空值能力取 `ColumnInfo.nullable`。
- 测试：每方言每映射一个用例；映射不到 → 值推断；推断不出 → Utf8；全 NULL 列 → Utf8；混杂值列 → Utf8；空列集 → 空 schema（不 panic）。

### T4. `ParquetFormatter`（code + test）

- `binary.rs` 内新增 `ParquetFormatter`（与 `XlsxFormatter` 并列，同一 trait、同一评审面）：`new(dialect: Option<String>, dest: PathBuf)`；`begin()` 存列 + 方言；首个 `rows()` 定 schema、建 `ArrowWriter`（`WriterProperties` 显式 `set_max_row_group_size(100_000)` + Snappy 默认）、把该批转置成 `RecordBatch` 写入；`finish()` 调 `close()`，只调用一次。
- 转置：每批 500 行按列建成 Arrow 数组（这是 Parquet 接入里唯一需要新写核心逻辑的地方）；列类型由 schema 固定，值按 schema 类型转换。
- 测试：单列 / 多列 / 空值 / 空表（0 行）/ 首批全 NULL / 大批量（多批）/ `finish()` 兜底（`Done` 未到时 `write_binary_file` 的后调）；读回用 `parquet` crate 的 reader 断言行数、列名、空值。

### T5. `write_binary_file` 分发（code + test）

- `mod.rs:723` 的 match 加 `Parquet` 分支：`ParquetFormatter::new(request.database_type.clone(), dest.to_path_buf())`；保留其余二进制变体进“no binary formatter”错误的兜底。
- 测试：Parquet 请求走通；`database_type` 为 `None` 时仍走通（值推断 + Utf8 兜底）；取消 token 在二进制路径按批检查（复用既有循环，不另写）。

### T6. 前端接线（code + test）

- `exportPlan.ts`：`formatExtension` 加 `'parquet' → 'parquet'`；`isBinaryFormat` 加 parquet，更新“today: XLSX only”注释。
- `BatchExportDialog.tsx`：`DATA_FORMAT_OPTIONS` 加 `{ value: 'parquet', label: 'Parquet' }`。
- 格式“禁用并解释原因”（`buildExportFormatOptions`）：Parquet 只在桌面批量 / SQL 路径可用；provided 范围与 Web 下禁用并附原因。XLSX“永不禁用”规则不动。
- 测试：planner 镜像测试（extension / isBinary / 多表 zip 判定）；dialog 格式选项测试（Parquet 出现、可选、禁用规则）。
- 不碰：`exportStream.ts`、provided 导出（`formatRowsAsText` / `blobTablesExporter.ts`）、`DataExportDialog` / `ExportDialog` 的整表路由（PR 3）。

### T7. 读回验收与边界测试（test）

- Rust 侧读回验收（自动化）：用 `parquet` crate 的 reader 读回 T4 产出的文件，断言行数、列名、空值与写入一致；覆盖多批、空表、含 NULL 列。
- pyarrow / DuckDB 读回验收（手动 + CI 覆盖）：`pyarrow.parquet.ParquetFile` 与 DuckDB `read_parquet` 各读一遍代表性文件，记录结果进 PR 描述；不在 Rust 单测里调 Python。
- 边界：多表 Parquet 走 zip（不进文本合并分支）；取消到达 artifact-publish 阶段不被受理（与 XLSX 同款已知缺口，不在本任务修）；类型冲突（后续批次值与 schema 不兼容）报错含列名。

### T8. 全量验证与提 PR（test + docs）

- `cargo test -p datazen --lib --features export,export-xlsx,export-parquet` 全量 + export 过滤；`cargo fmt --check`；Rust 覆盖率（`binary.rs` / `parquet_schema.rs` / `mod.rs` 行 + 分支 ≥80%，分支本地不可测的按既有方式处理，CI 覆盖）。
- vitest 全套件、`tsc` 权威脚本；前端覆盖率（`exportPlan.ts` / `BatchExportDialog.tsx` 相关文件行 + 分支 ≥80%）。
- PR 描述写清：功能、schema 推断规则、验证数字、pyarrow / DuckDB 读回结果、安装包体积变化、已知限制（Q2–Q4）。

## 3. 验证清单（提 PR 前逐项打勾）

- [ ] Rust 全量 + export 过滤 0 失败；fmt 干净
- [ ] vitest 全套件 0 失败；tsc exit 0
- [ ] 改动文件覆盖率行 + 分支 ≥80%（Rust 分支本地不可测时写明、CI 覆盖）
- [ ] `cargo tree` 只有一份 arrow 58.4.0
- [ ] pyarrow 与 DuckDB 各读回一个代表性文件，行数 / 列名 / 空值一致
- [ ] 空表、取消、多表 zip、provided/Web 禁用规则均有测试覆盖
- [ ] 安装包体积变化已测量并写进 PR 描述

## 4. 不要做的事

- 别把 `StreamFormatter` 改成返回 `Vec<u8>`（牵连全部文本路径）。
- 别做 provided-only Parquet、Web Parquet、Parquet 读入、压缩 / 编码用户选项、时间逻辑类型、Decimal128。
- 别在本次做整表对话框路由（PR 3）或 P2 重查；别动 `export_tables_stream` 之外的导出路径。
- 别让二进制进 `run_single_export` 的文本合并分支。
- 别手写 Parquet 文件格式；别用 `parquet2` / 其他非标准实现（以 `parquet` crate 为准）。

## 5. 已知限制与环境注意事项

- E2E 与 `datazen_desktop` 测试留 CI（本机缺 GTK/WebKit 开发库）。
- Rust 分支覆盖率本地不可测（stable 1.99.1 拒 `-Z coverage-options=branch`，无 nightly），PR 描述写明、CI 覆盖。
- vitest 全套件在内存压力下会跑得很慢（上次 46 分钟，中途只剩 389MB 可用）；跑之前尽量释放内存。
- 不要 `pnpm install`（破坏 worktree 的 node_modules 软链）；本 worktree 是 basic 驱动集，宿主单测结论只限 basic 语义。
- 本仓库 fetch refspec 只跟踪 main，`--force-with-lease` 需用显式 lease（先 `ls-remote` 核实远端 HEAD 再推）。
- 每完成一个小任务立刻提交并 push；code 和 test 分开提交；卡住停下来报告，不要绕过。
