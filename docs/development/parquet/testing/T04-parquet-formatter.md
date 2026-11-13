# T04 ParquetFormatter — 独立测试报告

- 任务：Parquet 导出 T4（计划 `docs/archive/development/parquet-export-implementation-plan.md` §T4，验收见计划行 51–55；PRD §4 F2 / F4 / F5 / F3 尾部规则）
- 分支：`feature/parquet-export`，worktree `/home/z/my-project/datazen/.worktrees/datazen-parquet-export`
- 受测提交：`92385653b` `feat(export): add the Parquet binary formatter (T4)`（1 文件，+376/−2）、`4bc1a220e` `test(export): cover the Parquet binary formatter end to end (T4)`（1 文件，+449）
- 测试员：独立测试（coder 与测试员分工同 T02/T03；验证数字全部来自本机独立运行）。
- 结论：**PASS — T4 可以关闭**。无阻塞缺陷；4 项偏差全部裁决通过（见 §5）。
- 报告日期：2026-11-12；验证前后 worktree clean；HEAD = 远端 HEAD = `4bc1a220e`。

## 1. 环境与命令

与 T03 报告 §1 相同（2 核 / 4G / 9.9G 磁盘、rustc 1.99.1 stable、cargo-llvm-cov 0.9.1、rsproxy / npmmirror、GitHub HTTP/1.1；测试计数取自 llvm-cov 仪表化运行，plain `cargo test` 全量并入 T8，见 T03 §6 O-2 的方法论注记）。

## 2. 门控结果（全部独立复跑，exit code 实测）

| 检查 | 配置 | 结果 |
|---|---|---|
| check ×4 | `export` / `export,export-xlsx` / `export,export-parquet` / 全特性 | 全部 exit 0，**0 warnings** |
| fmt | `cargo fmt --check -p datazen` | exit 0，干净 |
| 测试 + 覆盖率（全特性） | `cargo llvm-cov --lib -p datazen --features export,export-xlsx,export-parquet -- export` | exit 0，**115 passed，0 failed**（1013 filtered；lib 总数 1128） |
| 测试 + 覆盖率（export-parquet） | `--features export,export-parquet -- export` | exit 0，**95 passed，0 failed**（1013 filtered；lib 总数 1108） |
| 测试 + 覆盖率（export-only） | `--features export -- export` | exit 0，**72 passed，0 failed**（与 T03 逐数一致，T4 代码全部门控不在该配置编译） |

- 数量自洽：全特性 1128 = T3 的 1118 + 10（`parquet_tests` 模块）；export-parquet 1108 = 1098 + 10；export-only 72 不变。按名过滤器 `parquet` 单独复跑 25 passed（14 parquet_schema + 10 parquet_tests + 1 T2 stream_formatter 测试），与各模块计数逐项吻合。

## 3. 覆盖率（export 过滤测试，独立重测）

### 3.1 全特性——门控测量配置

| 文件 | Lines | Functions | Regions |
|---|---|---|---|
| `export/binary.rs` | **89.66%**（1964 行，203 missed） | 80.00% | 90.20% |
| `export/parquet_schema.rs` | 100.00%（438，0 missed） | 100.00% | 100.00% |
| `export/fetcher.rs` | 99.33%（3002，20 missed） | 100.00% | 99.90% |
| `export/mod.rs` | 87.28%（1211，154 missed） | 94.12% | 90.24% |
| `export/format.rs` | 95.15%（557，27 missed） | 96.97% | 94.77% |

binary.rs 从 855 行长到 1964 行（T4 实现 + 449 行测试），行覆盖率 89.66% ≥ 80%（T1 时代为 97.31%，但那时的文件只有 XLSX 一半；新增 Parquet 半区的大量防御分支见 §3.2 定性）。

### 3.2 缺失行抽查（`--show-missing-lines`，全特性）

binary.rs 的 203 行缺失逐段核实：XLSX 半区为既存防御路径（`check_cell_length` 溢出臂、`xlsx_error` 的 MaxStringLength/行列上限臂、`u16::try_from` 守卫等，T01 §3.4 已定性）；**Parquet 半区新增代码的缺失为同级别防御路径**——各窄化类型的越界守卫（int16/int32/int8/uint8 的 `out_of_range` 臂）、Boolean/Binary 列的类型冲突臂（Int64 列的冲突臂已被测试覆盖）、`File::create` 失败映射、以及 `unsupported inferred column type` 不可达兜底臂。主路径（转置、schema 固定、行组切分、写读往返）全部被 10 个测试覆盖。

### 3.3 export-parquet 配置

`export/binary.rs` **83.77%**（1109 行，180 missed；trait + Parquet 半区，XLSX 代码不在该配置编译）≥ 80%。export-only 配置 binary.rs 不在报告中（仅剩 trait 定义，与 T02/T03 同款形态）。

### 3.4 分支覆盖率

本地不可测（同前），Regions 代理：binary.rs 全特性 90.20%、export-parquet 85.98%。CI 覆盖。

## 4. 对抗探针（独立执行）

| # | 探针 | 结果 |
|---|---|---|
| P1 | 行组上限真的生效 | **通过**：100,001 行写入（200×500 批 + 1 行）后 footer 元数据 `num_row_groups()==2`、第一组恰好 100,000 行、第二组 1 行——`set_max_row_group_row_count(Some(100_000))` 按声明切分，不是纸面承诺 |
| P2 | 压缩编码 | **通过**：列块 `compression()==SNAPPY`（PRD F5 默认，显式设置） |
| P3 | 空表（0 行） | **通过**：文件存在、PAR1 魔数首尾俱全、读回 0 行、schema 为方言映射 + Utf8 兜底（int8→Int64、money→Utf8）——合法 schema-only 文件，不是 0 字节 |
| P4 | finish 恰好一次 | **通过**：二次 `finish()` 返回 Ok 且文件完好——与 write_binary_file 的 Done/兜底双调用契约咬合（集成路径随 T5） |
| P5 | 值种全往返 | **通过**：Integer→Int64、Float→Float64、Bool→Boolean、Bytes→Binary（字节逐位）、String/Timestamp/Json→Utf8 文本逐字、None→null 槽；多批行序保持 |
| P6 | 类型冲突报错 | **通过**：首批定 Int64 后第二批给 String → `Validation`，消息含列名 "id"、期望类型 "int64"、值种类 "String"（PRD F3 尾部规则的三个要素） |
| P7 | 协议校验 | **通过**：rows 先于 header、二次 header、无 header 的 finish（且不建文件）均为 Validation |
| P8 | 计数自洽 | 见 §2：115 / 95 / 72（总数 1128 / 1108 / 1085）与 T1–T3 基线逐项闭合 |

## 5. 偏差裁决（4 项）

### 5.1 偏差 1：Utf8 列对任何值种类都不冲突（渲染为文本）——通过

PRD F3 尾部规则"后续批次值与 schema 类型不兼容时报错"对 Utf8 列的解读：Utf8 是万能兜底（方言映射的 varchar、混杂推断的兜底都落在它），任何标量都能无损渲染为文本——数字经 Display、Bytes 为裸 hex（与 XLSX 单元格同一约定，无 CSV 往返标记）、Json 经 to_string。若 Utf8 也按冲突报错，"单列定不下来绝不让整表失败"的兜底承诺会被自己推翻。XLSX formatter 的 write_cell 正是同样的逐值渲染语义。冲突报错仅对有物理宽度的类型（整型/浮点/Bool/Binary）生效。裁决：**通过**，测试 `parquet_utf8_columns_render_every_value_kind` 钉住该语义。

### 5.2 偏差 2：无 header 的 `finish()` 报 Validation 且不建文件（XLSX 是保存空工作簿）——通过

Parquet 没有列就无法表达文件（arrow 至少要一列），"空工作簿"没有对等物。该路径仅在流从未产生 StatementStart 时可达（write_binary_file 在流错误时提前返回、根本到不了 finish 兜底），明确的 Validation 消息比吞掉错误或写出伪文件诚实。裁决：**通过**。

### 5.3 偏差 3：空 rows() 批不定 schema——通过

计划只说"首批定 schema"；一个空批不携带任何推断信号，若在空批上建 writer 会把所有列提前固化为 Utf8 兜底，违背方言映射的意图（空表的 schema 应该走 finish() 的"映射 + 兜底"路径，测试 P3 证明了该路径）。裁决：**通过**，代码注释已写明。

### 5.4 偏差 4：`write_binary_file` 路径的 finish 兜底/部分文件清理集成测试递延至 T5/T7——通过，按计划结构

计划 T4 测试列有"finish() 兜底（Done 未到时 write_binary_file 的后调）"，但 T4 尚无 Parquet 分发臂（T5 才把 `ParquetFormatter` 接进 `binary_formatter`）——该集成测试在 T4 无法构造。格式器层的幂等与合法性（P4）已覆盖；分发臂、`database_type` 透传、取消语义与部分文件清理随 T5 验证清单落地。与 T1 §5.5 的递延先例同构。裁决：**通过，附条件**——T5 验收必须包含走通 `write_binary_file` 的 Parquet 端到端测试。

## 6. 缺陷与非缺陷观察

### 缺陷列表

无。

### 非缺陷观察

- **O-1（100,001 行测试的时长与体积）：** 行组切分测试写入约 100k 行，文件约 500KB，全程数秒内完成（上游本来就是 500 行/批的流式语义），属可接受的集成测试成本。
- **O-2（API 版本注记）：** parquet 58 的 `set_max_row_group_size` 已改名 `set_max_row_group_row_count(Option<usize>)`、`ArrowWriter` 各方法返回 `ParquetError`、`BinaryArray` 的 `From` 走 `Vec<Option<&[u8]>>`——实现均已按 58 的真实 API 落地，四配置 0 warnings 佐证。
- **O-3（防御分支未覆盖）：** §3.2 所列防御臂（越界守卫、File::create 失败、不可达类型兜底）无测试——按 T01 对 binary.rs 既存防御分支的同款口径接受，不顺手造伪失败测试。
- **O-4（分支覆盖率）：** 本地不可测，Regions 代理见 §3.4，CI 覆盖。

## 7. 结论

**PASS — T4 可以关闭。**

- `ParquetFormatter` 与 XLSX 同 trait、同错误信封、同 exactly-once 语义；懒建 writer（首批定 schema）、100,000 行组上限（实测切分）、Snappy（实测编码）、空表 schema-only 文件（PAR1 魔数验证）全部落地。
- 10 个新测试全绿；三配置计数 115 / 95 / 72（总数 1128 / 1108 / 1085）逐项自洽；binary.rs 全特性 89.66%、export-parquet 83.77%，均 ≥80%；四组合 check 0 warnings；fmt 干净。
- 递延项：`write_binary_file` 分发臂 + 端到端集成随 T5（§5.4 附条件）；pyarrow / DuckDB 读回随 T7；分支覆盖率 CI。
