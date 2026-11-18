# T07 读回验收与边界测试 — 独立测试报告

- 任务：Parquet 导出 T7（计划 `docs/archive/development/parquet-export-implementation-plan.md` §T7，验收见计划行 70–74；PRD §6 兼容性 / §7 验收 5–7 项 / F6）
- 分支：`feature/parquet-export`，worktree `/home/z/my-project/datazen/.worktrees/datazen-parquet-export`
- 受测提交：`7adaedb0b` `test(export): read-back acceptance and Parquet boundary tests (T7)`（2 文件，+257）
- 测试员：独立测试（分工与验证方式同 T02–T06；数字全部来自本机独立运行）。
- 结论：**PASS — T7 可以关闭**。无阻塞缺陷；2 项偏差裁决通过（见 §5）；**pyarrow 与 DuckDB 读回验收双 PASS（23/23 检查项）**，PRD §6 的"唯一有意义的验收方式"落地。
- 报告日期：2026-11-12；验证前后 worktree clean；HEAD = 远端 HEAD = `7adaedb0b`。

## 1. 环境与命令

Rust 侧同 T03–T05（llvm-cov 仪表化运行承载测试计数）；外部读回验收用 Python：`pyarrow 26.0.1` + `duckdb 2.0.0`（阿里 PyPI 镜像安装），对 Rust 导出器产出的代表性文件逐项断言。验收脚本与输出留档于 PR 描述（本报告 §2 全文引用）。

## 2. pyarrow / DuckDB 读回验收（手动验收，记录进 PR 描述）

代表性文件由 `#[ignore]` 的 `parquet_acceptance_dump_writes_representative_file` 用真实 `ParquetFormatter` 写出：7 列覆盖全部推导类型（int4→Int32、float8→Double、text→Utf8、bool→Bool、bytea→Binary、timestamptz→Utf8、jsonb→Utf8），3 行含全 NULL 行、Unicode（`alpha, βeta`）、引号与竖线（`quote " and | pipe`）、字节（`00 ff fe`）、负数与负浮点。

| 检查项 | pyarrow 26.0.1 | DuckDB 2.0.0 |
|---|---|---|
| 行数 | **OK**（3） | **OK**（count=3） |
| 列名 | **OK**（7 列逐一相符） | —（列名即类型断言的键） |
| 列类型 | **OK**（int32/double/string/bool/binary/string/string） | **OK**（INTEGER/DOUBLE/VARCHAR/BOOLEAN/BLOB/VARCHAR/VARCHAR） |
| 空值 | **OK**（每列 null 计数：1/1/1/1/2/1/2） | **OK**（FILTER 计数相符） |
| 值一致性 | **OK**（id [1,−42,None]、score [1.5,−0.25,None]、Unicode 逐字、bytes `00 ff fe`、时间戳文本、JSON 文本） | **OK**（list(id)、sum(score)=1.25、Unicode 往返、JSON 文本） |
| 行组 / 压缩 | **OK**（1 row group、SNAPPY） | — |
| **合计** | **15/15 OK** | **8/8 OK** |

`ACCEPTANCE PASS`——Rust 导出的文件被 pyarrow（`ParquetFile`）与 DuckDB（`read_parquet`）读回，行数、列名、空值与写入逐项一致（PRD §7 验收第 6 项达成）。

## 3. Rust 侧测试与门禁（全部独立复跑）

| 检查 | 配置 | 结果 |
|---|---|---|
| 测试（全特性） | `-- export` | **124 passed，1 ignored（验收助手），0 failed**（1013 filtered；lib 总数 1138） |
| 测试（export-parquet） | 同上 | **104 passed，1 ignored，0 failed**（总数 1118） |
| 测试（export-only） | 同上 | **73 passed，0 failed**（总数 1086） |
| check ×4 | 全部 exit 0，**0 warnings** | |
| fmt | exit 0，干净 | |

- 数量自洽：全特性 1138 = T6 前的 1134 + 3（zip / 冲突 / 无门控的合并拒绝）；export-parquet 1118 = 1114 + 3 + 1（dump 测试）；export-only 1086 = 1085 + 1（无门控拒绝测试——守卫基于 `is_binary()`，本就该在所有构建成立）。
- 覆盖率（全特性）：`binary.rs` **86.72%**（2055 行；缺口主体为 `#[ignore]` 验收助手的 ~55 行——按设计不进常规套件，其余同 T04 §3.2 定性）；`fetcher.rs` **99.38%**；`mod.rs` 87.36%。均 ≥80%。

## 4. 对抗探针（独立执行）

| # | 探针 | 结果 |
|---|---|---|
| P1 | 文本合并守卫 | **通过**：绕过 prepare_export 直连 run_single_export 的两表 Parquet 计划被拒（错误含 "zip"，不建文件），且该测试**无门控**——守卫按 `is_binary()` 键控，四种构建都必须成立 |
| P2 | 多表 zip | **通过**：`run_zip_export` 产出 a.parquet / b.parquet 两个条目；逐条目抽出后经 parquet reader 读回，schema（Int64/Utf8，sqlite 方言值推断）与 2 行数据完整 |
| P3 | 类型冲突（PRD F3 尾部规则，经真实分发） | **通过**：postgresql 方言下 int4 首批定为 Int32，第二批 String → 整个 run 以 Validation 失败，错误链 `binary export failed: … Parquet column "id" expects int32 … String`——列名、期望类型、值种类三要素齐备 |
| P4 | 部分文件契约 | **通过**（沿既有契约）：冲突后 write_binary_file 留下部分文件由 run 层调用方清理——与 XLSX 取消测试记录的 writer/caller 分工一致，未引入新语义 |
| P5 | 读回验收 | 见 §2，**双 PASS 23/23** |

## 5. 偏差裁决（2 项）

### 5.1 偏差 1：`#[ignore]` 验收助手测试——通过

计划说 pyarrow / DuckDB 读回"手动 + CI 覆盖；不在 Rust 单测里调 Python"。需要一个由**本仓库导出器**产出（而非外部拼造）的代表文件才能做这个验收；`#[ignore]` 是 Rust 生态表达"手动/专项用例"的惯用机制：常规套件与 CI 默认跳过（不计入 pass、不拖慢套件），`cargo test parquet_acceptance -- --ignored --nocapture` 显式运行并打印文件路径。测试自身先经 parquet reader 断言再交外部读者，手动运行即是一次真验收。裁决：**通过**；其 ~55 行在常规覆盖率中缺失属机制代价（binary.rs 仍 86.72% ≥80%）。

### 5.2 偏差 2：取消到达 artifact-publish 阶段不被受理——已知缺口，本任务不修（按计划）

与 XLSX 同款的已知缺口（取消 token 在 artifact-publish 阶段的竞态窗口），共享循环的既有行为，计划明确"不在本任务修"。T5 的预取消 / 中途取消测试已钉住按批取消的常规语义。裁决：**按计划递延**，写入 PR 描述已知限制。

## 6. 缺陷与非缺陷观察

- **O-1：** 验收文件（2206 字节）留在 `/tmp/datazen-parquet-acceptance/`——沙箱会话产物，不入库；复跑 `--ignored` 可再生。
- **O-2：** pyarrow 对 `binary` 列返回 Python `bytes`、DuckDB 返回 `BLOB`，JSON 列两边都是文本（`{"k":1}`）——与 v1"Timestamp/Json 按 Utf8 出"的 PRD 决策一致。
- **O-3：** 分支覆盖率本地不可测（同前），Regions 代理：binary.rs 86.99%、fetcher.rs 99.91%；CI 覆盖。

## 7. 结论

**PASS — T7 可以关闭。**

- 读回验收：pyarrow 15/15 + DuckDB 8/8，行数 / 列名 / 类型 / 空值 / 值（含 Unicode、字节、JSON 文本）逐项一致；SNAPPY 行组经 pyarrow 元数据证实。
- 边界：文本合并拒绝（无门控，全构建成立）、多表 zip 双条目读回、类型冲突经真实分发报错含列名三要素，全部钉住。
- 三配置计数 124 / 104 / 73（+1 忽略）与 T1–T6 基线逐项闭合；四组合 check 0 warnings；fmt 干净。
- 递延项：取消竞态缺口（已知，随 XLSX 同款）；vitest 全套件、前端覆盖率、tsc 全量、plain `cargo test` 全量计数随 T8。
