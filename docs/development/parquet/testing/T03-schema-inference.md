# T03 Parquet Schema 推断模块 — 独立测试报告

- 任务：Parquet 导出 T3（计划 `docs/archive/development/parquet-export-implementation-plan.md` §T3，验收见计划行 45–49；PRD `docs/archive/development/parquet-export-prd.md` §4 F3）
- 分支：`feature/parquet-export`，worktree `/home/z/my-project/datazen/.worktrees/datazen-parquet-export`
- 受测提交：`84ce31baf` `feat(export): add dialect- and value-based Parquet schema inference (T3)`（2 文件，+186）、`5916e63bb` `test(export): cover the Parquet schema inference mapping tables and fallbacks (T3)`（1 文件，+321）
- 测试员：独立测试。coder 与测试员分工同 T02（实现为本会话 T3 提交，验证数字全部来自本机独立运行）。
- 结论：**PASS — T3 可以关闭**。无阻塞缺陷；3 项偏差全部裁决通过（见 §5）；环境方法论注记 1 项（见 §6 O-2）。
- 报告日期：2026-11-12；验证前后 worktree 均 clean；HEAD = 远端 HEAD = `5916e63bb`。

## 1. 环境与命令

环境与 T02 报告 §1 相同（2 核 / 4G / 9.9G 磁盘沙箱、rustc 1.99.1 stable、cargo-llvm-cov 0.9.1、rsproxy / npmmirror 镜像、GitHub HTTP/1.1），仅两点差异需要说明：

- **测试计数取自 llvm-cov 仪表化运行**（见 O-2）：本沙箱磁盘无法同时容纳 check / plain-test / 仪表化三套构建产物，测试与覆盖率合并为 `cargo llvm-cov --lib -p datazen --features … -- export` 一条命令执行——测试二进制与 `cargo test` 完全相同，计数与过滤语义不变；`cargo test` 全量 plain 运行并入 T8 验证清单。
- 覆盖率口径与 T01/T02 一致：export 过滤测试、`--show-missing-lines` 抽查、分支覆盖率留 CI。

## 2. 门控结果（全部独立复跑，exit code 实测）

| 检查 | 配置 | 结果 |
|---|---|---|
| check ×4 | `export` / `export,export-xlsx` / `export,export-parquet` / 全特性 | 全部 exit 0，**0 warnings** |
| fmt | `cargo fmt --check -p datazen` | exit 0，干净 |
| 测试 + 覆盖率（全特性） | `cargo llvm-cov --lib -p datazen --features export,export-xlsx,export-parquet -- export` | exit 0，**105 passed，0 failed**（1013 filtered；lib 总数 1118） |
| 测试 + 覆盖率（export-only） | `--features export -- export` | exit 0，**72 passed，0 failed**（1013 filtered；lib 总数 1085） |
| 测试 + 覆盖率（export-parquet） | `--features export,export-parquet -- export` | exit 0，**85 passed，0 failed**（1013 filtered；lib 总数 1098） |

- 数量自洽：全特性 1118 = T2 的 1104 + 14（`parquet_schema::tests`）；export-only 1085 与 T2 完全一致（模块 `#[cfg(feature = "export-parquet")]` 不编译，计数不应也确实未变）；export-parquet 1098 = T1 基线 1083 + T2 的 1 个无门控测试 + 14。export 过滤计数 105 / 72 / 85 逐项吻合。
- T3 未触碰 xlsx-combo 配置的任何编译内容（mod 声明 cfg 排除、其余文件未动），该配置维持 T02 验证的 92 结论，不再复跑。

## 3. 覆盖率（export 过滤测试，独立重测）

### 3.1 全特性——门控测量配置

| 文件 | Lines | Functions | Regions |
|---|---|---|---|
| `export/parquet_schema.rs` | **100.00%**（438 行，0 missed） | 100.00% | 100.00% |
| `export/binary.rs` | 97.31%（855，23 missed） | 91.43% | 96.06% |
| `export/fetcher.rs` | 99.33%（3002，20 missed） | 100.00% | 99.90% |
| `export/mod.rs` | 87.28%（1211，154 missed） | 94.12% | 90.24% |
| `export/format.rs` | 95.15%（557，27 missed） | 96.97% | 94.77% |

T3 未触碰的四个文件与 T02 逐数一致；新模块 100%，`--show-missing-lines` 输出为空。

### 3.2 export-parquet 配置（新模块的最小编译配置）

`export/parquet_schema.rs` 同为 **100/100/100**；`mod.rs` 71.96%、`fetcher.rs` 99.27%、`format.rs` 95.15% ——与 T02 同款形态（缺口为既存 formatter 路径代码在该配置下不可达，T2 §5.4 口径）。export-only 配置与 T02 数字逐数一致（72 / fetcher 99.28% / mod 71.98%），不再赘列。

### 3.3 分支覆盖率

本地不可测（同 T01 §3.3 / T02 §3.3），Regions 代理：parquet_schema.rs 全特性 100.00%；其余文件与 T02 一致。CI 覆盖。

## 4. 对抗探针（独立执行）

| # | 探针 | 结果 |
|---|---|---|
| P1 | PRD F3 方言映射表逐项核对 | **通过**：PG 19 项（int2/4/8、float4/8、bool、bytea、varchar/text/char、timestamp 族、uuid、json/jsonb、numeric/decimal）、MySQL 16 项（含 tinyint(1)→Int8、varchar(255)/decimal(10,2) 归一化）、SQLite 8 项（含声明大写 INTEGER/REAL/TEXT）、SQL Server 15 项（含 tinyint→UInt8、bit→Boolean）全部与 PRD 表格一致，无一项漏映射或多映射 |
| P2 | 未知方言（oracle/questdb 等）与 `None` 方言 | **通过**：跳过查表直落值推断（Integer/Float/Bool 一致则 Int64/Float64/Boolean） |
| P3 | 单列定不下来绝不让整表失败 | **通过**：全 NULL、空表、混杂变体（Integer+Float、Integer+Bool）一律 Utf8；映射不到 + 推断不出 → Utf8；空列集 → 空 schema 不 panic |
| P4 | 映射优先于值推断 | **通过**：PG varchar 列即使首批值全是 Integer 也保持 Utf8（schema 不依赖数据是否到达） |
| P5 | String/Bytes/Timestamp/Json 值语义 | **通过**：值推断中均为 Utf8（Timestamp 是字符串、Bytes 不走 Binary 兜底——PRD F3 原文"其余 → Utf8"） |
| P6 | 列名与空值能力 | **通过**：`Field` 的 name 与 nullable 逐列来自 `ColumnInfo`，未自作主张 |
| P7 | 计数自洽 | 见 §2；三配置总数 1118/1085/1098 与 T1/T2 基线逐项闭合 |
| P8 | 树状态 | 验证前后 `git status` clean，无未提交改动混入 |

## 5. 偏差裁决（3 项）

### 5.1 偏差 1：返回类型经 `arrow::datatypes::Schema` 引用，未按计划字面新增 `arrow-schema` 直连依赖——通过

计划签名写 `-> arrow_schema::Schema`；实现用 `arrow::datatypes::Schema`（arrow 对 arrow-schema 的同类型再导出，同一 crate 同一类型，T4 的 `ArrowWriter` 也以该路径消费）。少一个 manifest 改动面，且 export-parquet 特性本就引入 arrow。类型等价、零行为差异。裁决：**通过**。

### 5.2 偏差 2：测试超出计划最低清单（额外 6 个语义用例）——通过

计划要求：每方言每映射 / 映射不到→值推断 / 推断不出→Utf8 / 全 NULL→Utf8 / 混杂→Utf8 / 空列集不 panic，全部落地；另加映射优先级、方言串归一化（大小写/空白）、列名与 nullability 传递、短行静默、空表下映射仍生效、各方言落空路径逐方言覆盖。新增用例均来自 PRD F3 的明文规则，非测试员臆造，模块覆盖率因此达 100%。裁决：**通过**。

### 5.3 偏差 3：未映射方言（questdb / cloudberry / mariadb / doris / starrocks 等）没有 v1 映射表——通过，按设计递延

PRD F3 的 v1 方言最小集就是 PG / MySQL / SQLite / SQL Server 四张表；其余 DatabaseType 走"跳过查表 → 值推断 → Utf8 兜底"，安全性由三步流水线保证（PG 协议系的 questdb/cloudberry 有自己的类型名习惯，v1 不猜）。实现模块文档已明写此递延。裁决：**通过**，v2 若要覆盖再按数据补表。

## 6. 缺陷与非缺陷观察

### 缺陷列表

无。

### 非缺陷观察

- **O-1（schema 模块缺行分析）：** `parquet_schema.rs` 在全特性与 export-parquet 两配置下 `--show-missing-lines` 输出为空——四个方言表的 `_ => None` 落空臂都有对应方言的"money 不在表里"断言覆盖，无死区。
- **O-2（方法论注记）：** 测试计数取自 llvm-cov 仪表化运行的 `test result` 行（测试二进制与 `cargo test` 相同）。原因：本沙箱 9.9G 磁盘无法同时保留 check、plain-test、仪表化三套构建产物（T02 期间为腾覆盖率空间已清理 plain-test 依赖缓存一次）。plain `cargo test` 全量运行列入 T8 验证清单补齐。此为环境约束，非代码问题。
- **O-3（既存警告原样保留）：** 与 T02 §6 O-4 相同的既存警告（`source_scan_test_support.rs` dead_code、三驱动 `qualify_sql` 未用参数），位置在 T3 diff 之外，不动。
- **O-4（file_job.rs 0%）：** 与 T02 §6 O-3 相同，既存且不在门禁范围。
- **O-5（分支覆盖率）：** 本地不可测，Regions 代理见 §3.3，CI 覆盖。

## 7. 结论

**PASS — T3 可以关闭。**

- 三步流水线（方言映射 → 首批值推断 → Utf8 兜底）与 PRD §4 F3 逐条对应，四方言最小集映射表无一遗漏；单列不可判定永不失败整表导出。
- 14 个新测试全绿；三配置计数 105 / 72 / 85（总数 1118 / 1085 / 1098）与 T1/T2 基线完全自洽。
- 新模块覆盖率 100% lines / functions / regions（两种含 parquet 配置）；四组合 check 0 warnings；fmt 干净。
- 递延项：ParquetFormatter 消费 `infer_schema` 于 T4；plain `cargo test` 全量计数于 T8；分支覆盖率 CI。
