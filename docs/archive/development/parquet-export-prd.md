# Parquet 导出 PRD

- 状态：草稿，待评审
- 日期：2026-11-10
- 分支：`feature/parquet-export`（基点 `origin/main` `d868859ce`）
- 前置：XLSX 导出已合并（PR #232）；本需求是导出适配计划的 PR 2（见 `~/workspace/goals/datazen/files/xlsx-parquet-export-plan-2026-11-09.md` §5–§6）
- 实现方案：同目录 `parquet-export-implementation-plan.md`

## 1. 背景

DataZen 的批量导出（整表 / 多表，经 Rust `export_tables_stream` 流式落盘）目前支持 CSV / TSV / JSON / Markdown / SQL INSERT / SQL UPDATE / XLSX。XLSX（PR #232）走的是与文本路径平行的 `BinaryFormatter` 分支。Parquet 是该分支的第二个二进制格式。

为什么做 Parquet，而不是只用 XLSX：

- 列式存储，分析场景（过滤、聚合、只读部分列）更高效；大宽表文件通常比 XLSX 小。
- 与分析生态直接互通：DuckDB（`read_parquet`）、pyarrow / pandas、Spark、数据湖 / 数仓交接。XLSX 在这些场景里是“先转格式再分析”，Parquet 是直接可用。
- 与 XLSX 共用同一条 Rust 批量导出路径（建 SELECT、sql_guard、500 行一批流式读、进度事件、取消、错误处理），边际成本主要是 schema 推断和列式转置。

## 2. 目标用户与场景

- 数据分析师：把一张表导出去，直接用 DuckDB / Python（pyarrow / pandas）读，不再经过 CSV（类型丢失、分隔符问题）或 XLSX（1,048,576 行上限）。
- 数据工程师：表 → Parquet 文件 → 对象存储 / 数仓，作为轻量交接格式。
- 不针对：只看几行数据的用户（当前页 / 选中行导出已有前端路径）；需要把 Parquet 再读回 DataZen 的用户（本次不做 Parquet 读入）。

## 3. 目标 / 非目标

### 目标

- 桌面端批量导出（整表 / 过滤范围 / 多表）支持 Parquet，经现有 `BinaryFormatter` + `write_binary_file` 分支。
- Schema 推断：方言映射表 + 首批值推断 + Utf8 兜底，单列定不下来不让整表导出失败。
- 压缩默认 Snappy；内存有界（显式 row group 上限，不用默认的 1,048,576 行）。
- 导出的文件能被 pyarrow 和 DuckDB 读回，行数、列名、空值一致。
- 多表 Parquet 走 zip；取消（SQL 路径）可用；不可用场景用“禁用并解释原因”，不静默隐藏。

### 非目标（v1 不做）

- **provided-only Parquet**（当前页 / 选中行 / 图表 / 历史快照）：数据已在前端层，前端没有 Parquet writer；按导出计划默认值，Parquet 是 Rust-only。
- **Web Parquet**：Web 没有 Rust 批量导出路径（Web 批量导出是前端 blob 路径，只支持 CSV 等）。
- 压缩 / 编码的用户选项（v1 只给 Snappy 默认值）。
- Parquet 读入 / 导入；Parquet 文件预览。
- 时间逻辑类型：`Timestamp` 在 `Value` 里是 `String`，v1 按 Utf8 出，不解析时间字符串；`decimal` / `numeric` v1 按 Utf8 出（避免精度丢失），不做 Decimal128。
- 整表对话框路由（计划 PR 3，另起分支）；P2 重查（`export_tables_via_connection`）不在本次范围。

## 4. 功能需求

### F1. 格式接入

- `DataFormat` 增加 `Parquet` 变体；`extension()` 返回 `"parquet"`；`is_binary()` 返回 `true`。
- `ALLOWED_EXPORT_EXTS` 增加 `"parquet"`；`file.rs` 的 `ALLOWED_EXTENSIONS` 增加 `"parquet"`（`"xlsx"` 已有）。
- 未启用 Parquet 特性的构建（见 F9）收到 Parquet 请求时，返回明确的 validation 错误，不静默走别的格式。

### F2. ParquetFormatter（复用 BinaryFormatter trait）

- 实现 `BinaryFormatter`（`crates/datazen/src/export/binary.rs:40`，`begin / rows / finish`），与 `XlsxFormatter` 并列。
- `begin(columns)`：只记下列信息和方言，不建 writer（schema 要等首批值才能定，见 F3）。
- `rows(batch)`：首批到达时先定 schema、建 `ArrowWriter`，再把该批（≤500 行）转置成 `RecordBatch` 写入；后续批次直接写。
- `finish()`：`ArrowWriter::close()`，只调用一次；`Done` 没到时由 `write_binary_file` 的兜底调用（与 XLSX 同款兜底）。
- 空表（0 行）：`finish()` 时用方言映射 + Utf8 兜底定 schema，写出只有 schema 的合法文件，不报错、不产生空文件。

### F3. Schema 推断（本次最难的部分）

输入只有 `ColumnInfo { name, data_type: String, nullable: bool }`（`crates/datazen-driver-api/src/types.rs:306`），`data_type` 是各驱动原样透传的字符串，没有统一枚举。v1 按以下三步，顺序执行：

1. **方言映射表**：按请求上的 `database_type`（`ExportTablesRequest.database_type: Option<String>`，`mod.rs:193`）查表。`database_type` 为空时跳过这步。
2. **首批值推断**：映射不到的列，看首批非空 `Value` 的变体（`Integer→Int64`、`Float→Float64`、`Bool→Boolean`，其余 → Utf8）。首批某列全是 NULL 或值变体混杂 → 该列 Utf8。
3. **Utf8 兜底**：实在推断不出的列一律 Utf8，绝不能因为一列定不下来就让整表导出失败。

v1 方言映射最小集：

| 方言 | 映射 |
|---|---|
| PostgreSQL | `int2→Int16`、`int4→Int32`、`int8→Int64`、`float4→Float32`、`float8→Float64`、`bool→Boolean`、`varchar/text/char→Utf8`、`timestamp/timestamptz/date/time→Utf8`、`uuid→Utf8`、`json/jsonb→Utf8`、`bytea→Binary`、`numeric/decimal→Utf8` |
| MySQL | `int→Int32`、`bigint→Int64`、`float→Float32`、`double→Float64`、`decimal→Utf8`、`varchar/text→Utf8`、`datetime/timestamp/date→Utf8`、`blob→Binary`、`json→Utf8`；`tinyint→Int8`（`tinyint(1)` 是否按 Boolean 处理是开放问题，见 §7 Q2；v1 默认 Int8） |
| SQLite | `integer→Int64`、`real→Float64`、`text→Utf8`、`blob→Binary`、`numeric→Utf8`（SQLite 类型亲和度宽松，值推断比映射更重要） |
| SQL Server | `int→Int32`、`bigint→Int64`、`smallint→Int16`、`tinyint→UInt8`、`float→Float64`、`real→Float32`、`bit→Boolean`、`varchar/nvarchar/text→Utf8`、`datetime/datetime2/date→Utf8`、`varbinary→Binary`、`decimal→Utf8` |

类型映射补充规则：

- `Timestamp` 在 `Value` 里是 `String`，v1 一律 Utf8；`Json` → Utf8（`to_string()`）；`Bytes` → Binary。
- 空值能力取 `ColumnInfo.nullable`。
- Schema 在首批 `rows()` 调用时固定；后续批次出现与 schema 类型不兼容的值时，导出失败并报错（报错里写明列名与值类型），不静默改写、不静默置 NULL。

### F4. 内存有界

- 写入按 `WriterProperties` 显式设 row group 上限（v1：100k 行，`set_max_row_group_size(100_000)`；不用默认的 1,048,576 行）。具体数值实现时以实测为准，PRD 定的是“必须显式设一个远小于默认值的上限”。
- 上游仍按 500 行一批流式读，整表不进内存；`ArrowWriter` 的缓冲以一个 row group 为界。

### F5. 压缩与编码

- 压缩默认 Snappy（`parquet` crate 默认），v1 不给用户选项、不调编码。

### F6. 多表与打包

- 多表 Parquet 走 zip（与 XLSX 同款）；单表出单个 `.parquet` 文件。
- 二进制绝不进 `run_single_export` 的文本合并分支（多文件用 `-- ===== 文件名 =====` 横幅拼成一个文本文件会损坏二进制；已有 guard，Parquet 复用它）。

### F7. 取消与进度

- 复用 `write_binary_file` 的取消循环：`CancelToken` 按批检查（PR #231 已接好，Parquet 不用另写取消逻辑）。
- 进度事件与 XLSX 一致（`rows_written` / `ExportProgressEvent`），前端进度条不用改。

### F8. 前端接线（只改批量导出这一条线）

- `packages/frontend/src/commands/file.ts`：`ExportDataFormat` 增加 `'parquet'`。
- `packages/frontend/src/platform/exportPlan.ts`：`formatExtension` 增加 `'parquet' → 'parquet'`；`isBinaryFormat` 增加 parquet（现为 `format === 'xlsx'`，注释写明“today: XLSX only”，需同步更新）。
- `packages/frontend/src/components/export/BatchExportDialog.tsx`：`DATA_FORMAT_OPTIONS` 增加 `{ value: 'parquet', label: 'Parquet' }`。
- 格式“禁用并解释原因”（复用 `buildExportFormatOptions` 机制）：Parquet 只在桌面批量 / SQL 路径可用；provided 范围（当前页 / 选中行）与 Web 下禁用并附原因，不静默隐藏、不静默重置。XLSX“永不禁用”的规则不变，Parquet 不套用它。
- 不碰前端流式文本路径（`exportStream.ts`）、不碰 provided 导出（`formatRowsAsText` / `blobTablesExporter.ts`）。

### F9. 特性拆分（§3.5，PR 1  deferred 到本次）

- 把 `export` feature 下的二进制依赖拆成两个可选子 feature：`export-xlsx`（现有 `dep:rust_xlsxwriter` 移过去）、`export-parquet`（新增 `dep:parquet` + `dep:arrow`）。
- `export` 本身只留公共依赖（`serde_json / zip / tokio / tokio-util / tracing / uuid / datazen-driver-api / execution / jobs`）；桌面宿主（`datazen-desktop`）同时开 `export`、`export-xlsx`、`export-parquet`。
- 目的：基础包不被 Parquet 的依赖树拖大，守住 <15MB 安装包的定位；server 宿主可不开二进制子 feature。
- 未启用对应子 feature 的构建收到该格式请求时，返回明确错误（见 F1）。

### F10. 错误行为

- Schema 推断失败不导致整表失败（Utf8 兜底，见 F3）。
- 写文件失败、ArrowWriter 关闭失败、类型冲突（F3 末尾）走现有导出错误信封，报错写明列名 / 格式 / 原因，不吞错。
- 取消到达时按现有语义处理（`Aborted` + `partialDeleted`，与 XLSX 一致）。

## 5. 平台与范围矩阵

| 入口 | Parquet |
|---|---|
| 桌面批量导出（BatchExportDialog，整表 / 过滤 / 多表） | 支持 |
| 桌面 DataExportDialog / ExportDialog 的 SQL 范围 | 支持 |
| 桌面 provided 范围（当前页 / 选中行 / 图表 / 历史快照） | 不支持，禁用并附原因 |
| Web（批量 / 整表） | 不支持，禁用并附原因 |

## 6. 非功能需求

- **内存有界**：任意大表导出不把整表放进内存；row group 上限显式设置（F4）；500 行一批流式读。
- **包体积**：Parquet 依赖只挂在 `export-parquet` 子 feature 下（F9）；桌面全开后的安装包仍守 <15MB 定位，体积变化在 PR 里写明。
- **兼容性**：导出的文件必须能被 pyarrow（`ParquetFile`）和 DuckDB（`read_parquet`）读回；行数、列名、空值一致。这是 Parquet 唯一有意义的验收方式。
- **性能**：不设绝对数值目标；实现时测一轮（例如 100k 行、10 列的表）并把耗时 / 内存写进 PR 描述，与 XLSX 同规模对照。

## 7. 验收标准

- [ ] 桌面批量导出可选 Parquet，单表出 `.parquet` 文件，多表走 zip。
- [ ] 方言映射覆盖 PG / MySQL / SQLite / SQL Server 的 v1 最小集（§4 F3 表格），有单测。
- [ ] 映射不到的列走首批值推断；推断不出走 Utf8 兜底；单列问题不导致整表失败（有单测）。
- [ ] 空表导出产生只有 schema 的合法文件（有单测）。
- [ ] 取消（SQL 路径）按批生效（有单测）。
- [ ] 导出的文件经 pyarrow 与 DuckDB 读回，行数、列名、空值一致（验收测试 / 手动验收记录）。
- [ ] provided 范围与 Web 下 Parquet 禁用并附原因（有单测）。
- [ ] `export-xlsx` / `export-parquet` 子 feature 拆分完成；桌面开两个子 feature；未启用时请求返回明确错误（有单测）。
- [ ] Rust 测试 + vitest 全套件 + tsc + fmt 全绿；改动文件覆盖率行 + 分支 ≥80%。
- [ ] 安装包体积变化写进 PR 描述。

## 8. 风险与开放问题

- **Q1（最大风险）：parquet crate 的 arrow 版本对齐。** `Cargo.lock` 里已有 `arrow 58.4.0`（duckdb 驱动带进来的传递依赖），仓库没有任何 crate 直接依赖 `parquet`。钉版本时必须让 `parquet` 的 arrow 依赖与 58.4 对齐，否则出现两份 arrow。若最新 parquet 要求 arrow 59+，二选一：(a) 钉一个兼容 arrow 58 的 parquet 版本；(b) 升级 arrow（牵连 duckdb 驱动）。实现时以 `cargo tree` 验证为准，决定写进实现记录。
- **Q2：`tinyint(1)` 是否按 Boolean 处理。** MySQL 里 `tinyint(1)` 常被当布尔用；v1 默认 `tinyint→Int8`，记为已知限制，不阻塞。
- **Q3：后续批次类型冲突。** v1 规则是报错（F3 末尾）。SQLite 这类类型亲和度宽松的库更容易触发；若实测中频繁遇到，v2 再考虑放宽（例如冲突列整体降级 Utf8 需重写 schema，代价大）。
- **Q4：row group 上限数值。** v1 定 100k 行，具体以实测内存为准调整。

## 9. 依赖与前置

- 无代码前置：XLSX 分支（`BinaryFormatter` + `write_binary_file` + 取消 + guard）已在 main 上；本分支基点即最新 main。
- 唯一新增依赖树：`parquet` + `arrow`（直连），版本对齐见 Q1。
