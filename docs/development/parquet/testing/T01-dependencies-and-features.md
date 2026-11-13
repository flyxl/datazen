# T01 依赖与特性拆分 — 独立测试报告

- 任务：Parquet 导出 T1（计划 `docs/archive/development/parquet-export-implementation-plan.md` §T1，验收见计划行 37；PRD `docs/archive/development/parquet-export-prd.md` F9 / §8 Q1）
- 分支：`feature/parquet-export`，worktree `~/workspace/datazen/.worktrees/datazen-parquet-export`
- 受测提交：`659662297` `feat(export): split export-xlsx/export-parquet sub-features and gate binary dispatch`（6 文件，+298/−15）、`875f4ae3e` `test(export): gate XLSX formatter tests on export-xlsx and prove the off-feature error`（1 文件，+51）
- 测试员：独立测试（未改任何源码/测试文件，未提交，未启动 T2）
- 结论：**PASS — T1 可以关闭**。无阻塞缺陷；5 项偏差全部裁决通过（见 §5）；2 项非缺陷观察（见 §6）。
- 报告日期：2026-11-12；HEAD `875f4ae3e`，验证前后 worktree 均 clean（`git status --short` 为空）。

## 1. 环境与命令

- 工具链：`rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1`、`cargo-llvm-cov 0.9.1`（stable，无 nightly）。
- 全部 Rust 命令均以 `CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'` 运行，工作目录为上述 worktree（其自有 `target/`，与主检出及 Issue #32 worktree 隔离）。
- 并行说明：验证全程 Issue #32 worktree 有 T12 全量 vitest 在跑（`ps` 始终见 7 个 vitest 进程）；本测试只跑 targeted Rust 命令并保持 `CARGO_BUILD_JOBS=1`，未与其争抢。个别编译耗时偏长（如 export-parquet-only 测试二进制首次构建约 7 分钟）是资源争用所致，非代码问题。
- 未做：`datazen_desktop` 编译/测试（本机缺 GTK/WebKit 开发库，留 CI）、E2E、vitest。
- 探针文件：未在 worktree 内新建任何探针文件；探针通过已有的测试二进制按名复跑 + `cargo tree` 完成，全部证据日志在 `/tmp/t1-*.log`（`t1-checks*.log`、`t1-test-*.log`、`t1-cov-*.log`、`t1-tree-*.txt`）。

## 2. 门控结果（全部独立复跑，exit code 实测）

| 检查 | 命令 | 结果 |
|---|---|---|
| check export-only | `cargo check -p datazen --features export` | exit 0，0 warnings |
| check export+xlsx | `cargo check -p datazen --features export,export-xlsx` | exit 0，0 warnings |
| check export+parquet | `cargo check -p datazen --features export,export-parquet` | exit 0，0 warnings |
| check 全特性 | `cargo check -p datazen --features export,export-xlsx,export-parquet` | exit 0，0 warnings |
| fmt | `cargo fmt --check -p datazen` | exit 0，干净 |
| test 全特性 export 过滤 | `cargo test -p datazen --lib --features export,export-xlsx,export-parquet export` | exit 0，**90 passed，0 failed**（1013 filtered out） |
| test export-only | `cargo test -p datazen --lib --features export export` | exit 0，**70 passed，0 failed**（1013 filtered out） |

- 0 warnings 的核实方式：首次 `cargo check` 只保留了 tail，之后 `touch` 三个 T1 源码文件（`mod.rs`、`binary.rs`、`fetcher.rs`，仅改 mtime、内容未动）强制重编 datazen crate，四配置全量输出重跑，`grep '^warning'` 计数为 0、`grep '^error'` 为 0。注意：依赖 crate 未重编，§5.4 的既存警告未在此重现（见该节诚实说明）。
- 数量自洽：全特性 lib 测试总数 1103，export-only 总数 1083。差值 = 21 个被门控编出的 formatter 测试（fetcher.rs 10 个 + binary.rs tests 模块 11 个）减去 1 个仅 export-only 存在的新测试，即 `1103 − 21 + 1 = 1083`；export 过滤同理 `90 − 21 + 1 = 70`。与 coder 声称的 90 / 70 完全一致，非抄录。

### 2.1 依赖图（Arrow 决策 (a)：钉 arrow-58 线，不升级）

- `cargo tree -p datazen --features export,export-parquet -i parquet` → 仅 `parquet v58.4.0`（1 处）。
- `cargo tree -p datazen --features export,export-parquet -i arrow` → 仅 `arrow v58.4.0`（1 处）。
- `Cargo.lock` 核查（脚本解析）：14 个 `arrow*` 条目版本全为 `58.4.0`，各 1 份；`parquet` 为 `58.4.0`。
- Lock diff 纯增量：`git show 659662297 -- Cargo.lock | grep '^-[^-]'` 计数 0，即 +210 行、零版本 bump。Coder 的 "58.4.1" 说法见 §6 O-1（注释笔误，lock 实为 58.4.0）。
- `cargo tree -p datazen --features export -i parquet` → `error: package ID specification 'parquet' did not match any packages`：export-only 图中无 parquet/arrow（另有 grep 命中 9 行均为 worktree 路径 `datazen-parquet-export` 含 "parquet" 子串的假阳性，已逐行排除）。
- 计划验收原文 `cargo tree -p datazen -e features -i parquet` 按字面跑同样报错——默认 features 为空，不加 `--features export,export-parquet` 时 parquet 本就不在图中。Coder 的备注正确；验收命令应补 features（见 §6 O-2），不是代码问题。

### 2.2 特性拆分与宿主核对（diff 全读）

- `crates/datazen/Cargo.toml`：`export` 去掉 `dep:rust_xlsxwriter`，只剩公共依赖；新增 `export-xlsx = ["dep:rust_xlsxwriter"]`、`export-parquet = ["dep:parquet", "dep:arrow"]`；`parquet`/`arrow` 为 workspace optional 依赖。符合计划。
- 根 `Cargo.toml`：新增 workspace `parquet = "58"`、`arrow = "58"`，注释说明钉 58 线的理由（版本号笔误见 O-1）。
- `crates/datazen-desktop/Cargo.toml:28`：datazen feature 列表补开 `"export-xlsx"`、`"export-parquet"`。符合计划。
- Server：`crates/datazen-server/Cargo.toml:34` 的 datazen feature 列表含 connection/execution/tunnel/jobs/settings/named_views/history/workflow/table-editing/table-browsing/application/ai/services/schema_diff/data_transfer/data_sync/dashboard/wapps，**无 `export`**（:187 另为 `test-harness`），无需改动。符合 coder 说法。
- `DataFormat` 变体本身未门控（`mod.rs:147-155` 仍为 Csv/Tsv/Json/Markdown/SqlInsert/SqlUpdate/Xlsx，**无 Parquet**——见 §5.5）。
- 引用面核查：`XlsxFormatter` / `sanitize_sheet_name` / `XLSX_MAX_*` 只出现在 `binary.rs`、`fetcher.rs`、`mod.rs` 三处，无其他调用方会因门控断裂；`format.rs` 的 Xlsx 分支只是格式名映射，不依赖 formatter。

## 3. 覆盖率（`cargo llvm-cov --lib -p datazen`，export 过滤测试，独立重测）

### 3.1 全特性（export,export-xlsx,export-parquet）——门控测量配置

| 文件 | Lines | Functions | Regions（分支最接近代理，见注） |
|---|---|---|---|
| `export/binary.rs` | **97.31%**（855 行，23 missed） | 91.43% | 96.06% |
| `export/fetcher.rs` | **99.33%**（2965 行，20 missed） | 100.00% | 99.90% |
| `export/mod.rs` | **87.19%**（1210 行，155 missed） | 94.12% | 89.85% |

三文件行覆盖率均 ≥80%，与 coder 声称的 97.31 / 99.33 / 87.19 **逐数一致**。测试本身 90 passed。

### 3.2 export-only

| 文件 | Lines |
|---|---|
| `export/fetcher.rs` | **99.26%**（2417 行，18 missed） |
| `export/mod.rs` | **71.85%**（1208 行，340 missed） |
| `export/binary.rs` | 报告中**不存在**（仅剩 trait 定义，无可覆盖代码） |

与 coder 声称一致。71.85% 的定性见 §5.3（不阻塞）。

### 3.3 分支覆盖率

**本地不可测**：stable 1.99.1 无 nightly，`-Z coverage-options=branch` 被拒；按计划 §5，分支覆盖由 CI 覆盖，本报告不声称分支覆盖率。最接近的本地代理是上表 Regions 列（全特性 96.06 / 99.90 / 89.85，与 coder 一致）。

### 3.4 未覆盖行抽查（`--show-missing-lines`）

- 全特性 `mod.rs` 缺失行含 `51, 56`（默认 `ExportSource::stream_query` 桩的 Err 分支，既存）、`729-731`（`binary_formatter` 的 `_` 兜底，见 §5.3）、其余为既存未覆盖分支；**T1 新增的错误臂 724-728 与 `let _`（716 行）不在缺失表中，即全特性下已被覆盖**。
- 全特性 `binary.rs` 缺失 `64-67`（`xlsx_error` 的 MaxStringLength 臂）、`243-246` / `276-279`（`u16::try_from` 列数上限守卫）：均为既存防御/上限分支，T1 在 binary.rs 只加了 `cfg` 属性、未加逻辑行。
- export-only `mod.rs` 缺失行：大段 `773-943` 为 `write_binary_file` 分发后的流式写循环（无 formatter 时不可达）、`729-731` 同上、其余为既存未覆盖行；**T1 新增的 724-728 与 716 行同样不在 export-only 缺失表中**，即新错误路径在该配置下也被新测试覆盖。

## 4. 对抗探针（独立执行）

| # | 探针 | 配置 | 结果 |
|---|---|---|---|
| P1 | XLSX 请求、无 export-xlsx：明确 Validation 错误含 `export-xlsx`、不建文件、不 panic | `export`（新测试按名复跑） | **通过**。`write_binary_file_xlsx_without_export_xlsx_feature_is_a_clear_error ... ok`；测试内断言 `ExportError::Validation` 含 `export-xlsx` 且 `!dest.exists()`。另在 `export,export-parquet` 下复跑同样通过（见 P5a），即“两个子特性皆无”与“仅有 parquet”两种无-xlsx 组合均被证实 |
| P2 | export-xlsx 开启时 XLSX 仍写出有效文件 | 全特性，`write_binary_file_sql_source_writes_xlsx` 按名复跑 | **通过**（calamine 读回路径，`ok`，0.16s） |
| P3 | export-only 图中无 parquet/arrow 代码 | `cargo tree --features export -i parquet` | **通过**：`did not match any packages`（§2.1 假阳性已排除） |
| P4 | 两个子特性皆无时二进制请求是明确错误而非 panic | `export`，全量 export 过滤 70 passed | **通过**：即 P1 的配置；全套件 0 failed、0 panic；除 Xlsx 外不存在其他可表达的二进制变体（`DataFormat` 无 Parquet，见 §5.5），`_` 兜底分支本身就是 Validation 错误 |
| P5a | 新错误测试在 export-parquet-only 下同样通过（coder 未报的配置） | `export,export-parquet` | **通过**：1 passed，0 failed（1082 filtered out） |
| P5b | 新测试在全特性下被编出 | 全特性，按名跑 | **通过**：0 passed，1103 filtered out（0 matched） |
| P6 | 计划验收命令字面形式 | `cargo tree -p datazen -e features -i parquet`（无 features） | 按字面**失败**（parquet 不在图），属预期：默认 features 为空；补 `--features export,export-parquet` 后见 §2.1 单份 arrow 58.4.0。计划措辞问题，非代码问题 |

补充：P1 的“直接探针”说明——本测试未在 worktree 内写探针文件，因为该行为的唯一可达调用面就是 `write_binary_file`，而 fetcher.rs 的测试脚手架（`StubSource`、`NoopEvents`）是 `#[cfg(test)]` 项，worktree 外的独立二进制无法复用；上述按名复跑（两种无-xlsx 配置）+ 全套件复跑即为直接执行验证，非抄录 coder 日志。

## 5. 5 项偏差裁决（逐项明确）

### 5.1 偏差 1：分发抽成 `binary_formatter()` helper（mod.rs:697-740）——通过，设计合理且行为保持

- 四配置 `cargo check` 均为 0 warnings，证实 helper 达到了去死代码的目的；若分发内联，无子特性构建下 `write_binary_file` 尾部确会成为死代码（8 个未使用警告之说与代码结构一致：`app`/`state`/`dest` 等在分发出错后不再被使用）。
- 行为保持：xlsx 开启时 Xlsx 臂与 T1 前逐行一致（`XlsxFormatter::new(&table.table_name, dest.to_path_buf())`）；`_` 兜底错误文本逐字未变；xlsx 关闭时的新错误臂是**新增配置下新增的行为**（T1 前 `export` 恒含 xlsx，该配置不可构建），非对既有行为的改变。
- 错误信封一致：`binary_formatter` 返回 `CommandError`，而 `mod.rs:37` 有 `use error::ExportError as CommandError`——即同一信封，新测试的 `ExportError::Validation` 匹配类型正确。
- `#[cfg(not(feature = "export-xlsx"))] let _ = (table, dest);` 在四配置下均正确：xlsx 开时编出、关闭时抑制未使用警告；parquet-only 配置下同样需要它（Xlsx 臂与 `_` 臂皆不使用 table/dest），已由该配置 0 warnings 证实。
- 对 T5 的注释（将来拓宽为 `not(any(...))`）方向正确；补充一点：即使不拓宽也无警告风险，因为 `table`/`dest` 均为 `Copy` 的引用，`let _` 在它们被使用时也无害——该注释是整洁性提示，非正确性前提。

### 5.2 偏差 2：10 个 XLSX 测试被门控——通过，列表精确，无守卫测试被错门控

- fetcher.rs 被门控的恰为 10 个测试 + 共享 helper `assert_xlsx_has_two_rows`（diff 中 11 个 `#[cfg(feature = "export-xlsx")]` + 1 个 `#[cfg(not(...))]`，逐项核对）：`write_binary_file_emits_progress_events`、`write_binary_file_sql_source_writes_xlsx`、`run_zip_export_writes_xlsx_entries`、`write_binary_file_begin_failure_is_reported`、`write_binary_file_rows_before_start_fall_back_to_requested_columns`、`write_binary_file_without_done_still_finishes_xlsx`、`write_binary_file_done_arriving_finishes_xlsx_once`、`write_binary_file_row_wider_than_excel_columns_is_rejected`、`write_binary_file_cancels_mid_stream`、`run_single_export_cancels_mid_stream_and_removes_partial_xlsx`。全部都需要真实 XLSX formatter，门控正确。
- 另有 binary.rs tests 模块整体被门控（`#[cfg(all(test, feature = "export-xlsx"))]`），内含 11 个测试（sheet 名清洗、roundtrip、calamine 读回、大整数、行列上限、finish 幂等等）。Coder 的 "10 个" 仅指 fetcher.rs 一侧；合计 21 个被编出，与 §2 的总数算术 `1103 − 21 + 1 = 1083` 闭合。
- 守卫测试未被错门控：`write_binary_file_rejects_text_format`、`prepare_export_forces_zip_for_multi_plan_xlsx`、`run_single_export_refuses_binary_multi_plan`、`prepare_export_rejects_malformed_sources_early` 均保持无门控，并在 export-only 复跑中逐一 `ok`（已从 `/tmp/t1-test-exportonly.log` 逐名核对）。

### 5.3 偏差 3：export-only 下 mod.rs 71.85%（低于 80% 文件级）——通过，不阻塞；全特性是正确的门控测量配置

- 缺失行已逐段核实（§3.4）：缺口主体是 `write_binary_file` 分发后的流式写循环（773-943 行）等**既存 formatter 路径代码**，在无二进制子特性的构建中按设计不可达；其余为既存未覆盖行。T1 新增的每一行（错误臂 724-728、`let _` 716）在 export-only 下**已被覆盖**（不在缺失表中）。
- `_` 兜底（729-731）是 T1 前就存在的文本（内联分发的 `_` 臂原样移入 helper），在两个配置下均未覆盖——它只有在未来出现新的二进制变体时才可达，与 T1 无关。
- 无 T1 前基线可比：export-only 配置本身是 T1 新引入的（此前 `export` 恒含 xlsx），不存在“T1 之前 export-only 是多少”的参照；用一个按设计编出被测代码的配置去卡文件级 80% 是不合理的度量。门控测量应取代码实际存在的配置，即全特性（87.19% ≥80%）。结论：export-only 的 71.85% 是特征门控的预期产物，不阻塞 T1。

### 5.4 偏差 4：未动文件中的既存警告原样保留——通过

- T1 diff 共 7 个文件（`Cargo.lock`、根 `Cargo.toml`、`crates/datazen-desktop/Cargo.toml`、`crates/datazen/Cargo.toml`、`binary.rs`、`mod.rs`、`fetcher.rs`）；T1 触及的 datazen crate 在四配置下强制重编后均为 0 warnings（§2），即 T1 未引入任何警告。
- Coder 所称的警告位置在 diff 之外（如 `qualify_sql` 相关在 `crates/datazen-desktop/src/...` 测试支持文件，grep 已核实位置），按定义不可能是 T1 引入的；不动它们是正确的（不顺手修无关警告）。
- 诚实说明：本测试未做全量 clean 重编，因此**未独立重现**那些既存警告的具体文本；裁决依据是“位置在 diff 外 + 本 crate 0 warnings”，而非逐条复现。留 CI 全量构建覆盖。

### 5.5 偏差 5 / Parquet-error 问题：计划验收称 export-only 下“Parquet 请求返回明确错误”，但 `DataFormat::Parquet` 要到 T2 才存在——明确裁决：XLSX 的 off-feature 错误测试足以证明 T1，Parquet 错误路径递延至 T2/T5，不阻塞

- 前提已核实：`DataFormat` 枚举（`mod.rs:147-155`）只有 Csv/Tsv/Json/Markdown/SqlInsert/SqlUpdate/Xlsx，`ALLOWED_EXPORT_EXTS`（`mod.rs:64`）无 `"parquet"`，`extension()` / `is_binary()` 均无 Parquet 分支。**在 T1 的代码上不存在可表达的 Parquet 请求**，因此“Parquet 请求返回明确错误”这句验收在 T1 范围内是不可测试的——这是计划措辞超前于任务边界的问题，不是 T1 实现的缺陷。
- 机制等价性已证实：XLSX 的 off-feature 路径与未来 Parquet 路径走同一 helper、同一 `CommandError::Validation` 信封、同一“明确点名缺失子特性 + 不建文件”语义（P1/P5a）；Parquet 臂只是该 match 里尚未写出的一臂（T5），其错误测试在 T2（变体存在后）/T5（分发臂落地后）补齐是唯一可行的顺序。要求 T1 写出 Parquet 错误测试，等于要求 T1 提前做 T2。
- 裁决：**通过，有条件**——条件是 T2/T5 必须补上 Parquet 的 off-feature 错误测试（`export` 开、`export-parquet` 关时 `DataFormat::Parquet` 请求返回含 `export-parquet` 的 Validation 错误且不建文件），并在 T5 验收中显式勾选本计划验收句。请协调人在 T2 任务书中写明此项，避免递延丢失。

## 6. 缺陷与非缺陷观察

### 缺陷列表

无。未发现阻塞缺陷，也未发现非阻塞缺陷。

### 非缺陷观察

- **O-1（注释版本号笔误，无行为影响）：** 根 `Cargo.toml` 的 Parquet 注释写“cargo resolves both to 58.4.1”，但 `Cargo.lock` 实际为 parquet 58.4.0 / arrow 58.4.0（§2.1 已核实）；coder 提交说明写的 58.4.0 才是正确的。建议在 T2 顺手把注释改成 58.4.0（一词之改），不值得为此单独提交。
- **O-2（计划验收命令缺 features）：** 计划行 37 的 `cargo tree -p datazen -e features -i parquet` 按字面跑不到 parquet（默认 features 为空，§2.1 / P6）。验收应写为 `cargo tree -p datazen --features export,export-parquet -e features -i parquet`，并注明期望“一份 arrow 58.4.0”。计划措辞问题。
- **O-3（export-parquet-only 下 parquet/arrow 暂无使用者）：** 该配置编译通过且 0 warnings，parquet/arrow 依赖已进图但尚无代码使用（要到 T2/T4）。Rust 对未使用的可选依赖不告警，属预期中间态，无需处理。
- **O-4（T5 注释补充）：** 见 §5.1 末段——`let _` 的拓宽是整洁性提示，即使不拓宽也不会产生警告。
- **O-5（分支覆盖率）：** 见 §3.3——本地不可测，CI 覆盖；Regions 代理全特性为 96.06 / 99.90 / 89.85。

## 7. 结论

**PASS — T1 可以关闭。**

- 依赖：parquet 58.4.0 / arrow 58.4.0 单份，lock 纯增量，决策 (a) 已落实。
- 特性拆分：`export` / `export-xlsx` / `export-parquet` 与计划一致；桌面补开两子特性；server 无 export，无需改动。
- 门控：四配置 check 均 exit 0 且 0 warnings；fmt 干净；两个 formatter 分发分支均已门控；`DataFormat` 变体未门控。
- 测试：全特性 90 passed / export-only 70 passed，均为 0 failed；新测试在两种无-xlsx 配置下通过、全特性下编出；XLSX 有效写出经 calamine 读回验证。
- 覆盖率：全特性三文件 97.31 / 99.33 / 87.19，均 ≥80%；export-only 的 mod.rs 71.85% 已裁决为门控预期产物，不阻塞。
- 递延项（非阻塞）：Parquet off-feature 错误测试随 T2/T5（§5.5）；O-1 注释笔误随 T2 顺手修；分支覆盖率由 CI 覆盖。
