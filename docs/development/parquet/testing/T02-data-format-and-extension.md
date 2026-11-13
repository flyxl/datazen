# T02 DataFormat::Parquet 与扩展名 — 独立测试报告

- 任务：Parquet 导出 T2（计划 `docs/archive/development/parquet-export-implementation-plan.md` §T2，验收见计划行 41–43；PRD `docs/archive/development/parquet-export-prd.md` §4 F1）
- 分支：`feature/parquet-export`，worktree `/home/z/my-project/datazen/.worktrees/datazen-parquet-export`（环境已迁移：T01 报告的 `~/workspace` 现场不可用，本报告在全新沙箱复现验证，见 §1）
- 受测提交：`23cd4a111` `feat(export): add DataFormat::Parquet with extension and binary flag (T2)`（5 文件，+28/−13）、`3b51f4243` `test(export): cover DataFormat::Parquet metadata and prove the off-feature error (T2)`（2 文件，+92/−23）
- 测试员：独立测试。T2 代码由前一会话提交、本测试员未参与编写，全部数字来自本机独立运行，非抄录 coder 记录。
- 结论：**PASS — T2 可以关闭**。无阻塞缺陷；4 项偏差全部裁决通过（见 §5）；T1 报告 §5.5 递延项（Parquet off-feature 错误测试）确认闭环（见 §4 P1）。
- 报告日期：2026-11-12；HEAD `3b51f4243`（= 远端 HEAD），验证前后 worktree 均 clean（`git status --short` 为空）。

## 1. 环境与命令

- 硬件/沙箱：2 核 / 4G 内存 / 9.9G 磁盘（原环境为 1.5 核 / 6G；本机磁盘紧张，coverage 前清理了非仪表化 `target/debug`，不影响测量口径）。
- 工具链：`rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1`、`cargo-llvm-cov 0.9.1`、`rustfmt 1.10.0-stable`（stable，无 nightly——分支覆盖率与 T01 同口径，由 CI 覆盖）。
- 网络：crates 走 rsproxy sparse 镜像（`~/.cargo/config.toml`）；npm 走 registry.npmmirror.com；GitHub 强制 HTTP/1.1。
- 全部 Rust 命令均以 `CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'` 运行（覆盖率命令与 T01 §1 同口径），工作目录为上述 worktree（其自有 `target/`；llvm-cov 使用 `target/llvm-cov-target`，为 cargo-llvm-cov 默认目录）。
- 前端：pnpm 9.15.9（corepack），依赖按 `pnpm-lock.yaml` 完成安装。环境注记：`xlsx-0.20.3.tgz`（`packages/frontend/package.json:33` 直接依赖、`exportFormat.ts:34` 静态 import）本沙箱直连 cdn.sheetjs.com 极慢，经低速直接拉取官方 tarball 获得，openssl sha512 与 `pnpm-lock.yaml:4077` 记录的 integrity **逐字节一致**后使用；无仓库文件改动（§6 O-2）。
- 未做：`datazen_desktop` 编译/测试（本机缺 GTK/WebKit 开发库，留 CI）、E2E、vitest 全套件（T8）、pyarrow/DuckDB 读回（T7）、分支覆盖率（CI）。

## 2. 门控结果（全部独立复跑，exit code 实测）

| 检查 | 命令 | 结果 |
|---|---|---|
| check export-only | `cargo check -p datazen --features export` | exit 0，0 warnings |
| check export+xlsx | `cargo check -p datazen --features export,export-xlsx` | exit 0，0 warnings |
| check export+parquet | `cargo check -p datazen --features export,export-parquet` | exit 0，0 warnings |
| check 全特性 | `cargo check -p datazen --features export,export-xlsx,export-parquet` | exit 0，0 warnings |
| fmt | `cargo fmt --check -p datazen` | exit 0，干净 |
| test 全特性 export 过滤 | `cargo test -p datazen --lib --features export,export-xlsx,export-parquet export` | exit 0，**91 passed，0 failed**（1013 filtered） |
| test export-only | `cargo test -p datazen --lib --features export export` | exit 0，**72 passed，0 failed**（1013 filtered） |
| test export+xlsx（Parquet 关、XLSX 开——T01 P5a 的镜像配置） | `cargo test -p datazen --lib --features export,export-xlsx export` | exit 0，**92 passed，0 failed**（1013 filtered） |
| vitest 定向 | `npx vitest run packages/frontend/src/platform/__tests__/exportPlan.test.ts` | **32 passed / 32**（vitest 4.1.10） |

- 数量自洽：全特性 lib 总数 1104 = T1 的 1103 + 1（`stream_formatter_parquet_header_and_tail_are_empty_by_construction`，无门控）；export-only 1085 = T1 的 1083 + 2（前者 + `write_binary_file_parquet_without_export_parquet_feature_is_a_clear_error`，仅在无 `export-parquet` 配置编出）；export+xlsx 1105 = 1103 + 2 同理。export 过滤计数 91 / 72 / 92 与总数逐项吻合，非抄录。
- `0 warnings` 的核实：四个 check 组合均为全量输出捕获后 `grep '^warning'` 计数 0；另强制重编 lib test 抓到的 4 条警告全部位于 T1/T2 diff 之外（§6 O-4），datazen crate 本体在四配置下 0 warnings。

## 3. 覆盖率（`cargo llvm-cov --lib -p datazen`，export 过滤测试，独立重测）

### 3.1 全特性（export,export-xlsx,export-parquet）——门控测量配置

| 文件 | Lines | Functions | Regions（分支最接近代理，见注） |
|---|---|---|---|
| `export/binary.rs` | **97.31%**（855 行，23 missed） | 91.43% | 96.06% |
| `export/fetcher.rs` | **99.33%**（3002 行，20 missed） | 100.00% | 99.90% |
| `export/mod.rs` | **87.28%**（1211 行，154 missed） | 94.12% | 90.24% |
| `export/format.rs` | **95.15%**（557 行，27 missed） | 96.97% | 94.77% |

- `binary.rs` 97.31% 与 T01 逐数一致（T2 未动该文件）。`fetcher.rs` 较 T01 +37 行（新测试代码），百分比不变。`mod.rs` 87.28% 对 T1 的 87.19%：T2 新增臂已覆盖。`format.rs` 首次进入门禁口径，≥80%。四文件全部 ≥80%。

### 3.2 export-only

| 文件 | Lines |
|---|---|
| `export/fetcher.rs` | **99.28%**（2494 行，18 missed） |
| `export/format.rs` | **95.15%**（557 行，27 missed） |
| `export/mod.rs` | **71.98%**（1210 行，339 missed） |
| `export/binary.rs` | 报告中**不存在**（仅剩 trait 定义，无可覆盖代码）——与 T01 §3.2 同款形态 |

`mod.rs` 71.98% 的定性见 §5.4（不阻塞；复用 T01 §5.3 的裁决口径）。

### 3.3 分支覆盖率

本地不可测：stable 1.99.1 无 nightly，`-Z coverage-options=branch` 被拒；按计划 §5，分支覆盖由 CI 覆盖，本报告不声称分支覆盖率。最接近的本地代理是 Regions 列（全特性 96.06 / 99.90 / 90.24 / 94.77）。

### 3.4 未覆盖行抽查（`--show-missing-lines`，独立复跑）

- export-only `mod.rs` 缺失行含 `740-742`（`_` 兜底，既存文本，T2 前的裁决见 T01 §5.3；新二进制变体出现前不可达）；**T2 新增的 Parquet off-feature 臂（`mod.rs:734-739`）不在缺失表——被新门控测试覆盖**。大段 `785` 以下的 `write_binary_file` 流式写循环为既存 formatter 路径（无二进制子特性时按设计不可达），与 T01 §3.4 逐段同款。
- 全特性 `format.rs` 缺失 `38, 319-321, 396, 409-416, 426-427, 436, 441`——全部为既存 SQL 路径与文档行；**T2 的三处 Parquet 臂（`new()` 192–196、header 266、tail 449–453）均不在缺失表**（`stream_formatter_parquet…` 测试按名执行过 header/tail，构造臂随测试构造覆盖）。
- 全特性 `mod.rs` 缺失行同样含 `740-742`，其余为既存未覆盖行；`fetcher.rs` 20 行缺失为既存防御/上限分支（同 T01 §3.4 口径）。

## 4. 对抗探针（独立执行）

| # | 探针 | 配置 | 结果 |
|---|---|---|---|
| P1 | Parquet 请求、无 export-parquet：明确 Validation 错误含 `export-parquet`、不建文件、不 panic | export-only 与 export+xlsx 全套件 | **通过**。`write_binary_file_parquet_without_export_parquet_feature_is_a_clear_error` 在两种无-parquet 配置下均 `ok`（72/92 计数内含它）；测试内断言 `ExportError::Validation` 含 `export-parquet` 且 `!dest.exists()`。**T1 报告 §5.5 递延项就此闭环** |
| P2 | 同一测试在全特性下编出为无 | 全特性按名过滤 | **通过**：91 中不含该测试，与 §2 总数自洽（1104 vs 1085） |
| P3 | 扩展名 allow-list | `validate_export_path_still_refuses_illegal_extension` | **通过**：`export.parquet` 接受；`export.exe` 与无扩展名仍拒——原“拒绝 .parquet”的测试按 T2 语义改写为钉住“非法扩展名仍拒”的不变量，正确 |
| P4 | Serde 往返 + 元数据 | DataFormat 元数据测试 | **通过**：`extension()==="parquet"`、`is_binary()`、`"parquet"` ↔ `DataFormat::Parquet` 往返 |
| P5 | T1 O-1 注释修正兑现 | rg 根 `Cargo.toml` | **通过**：注释为 58.4.0（T1 报告 §6 O-1 的“随 T2 顺手修”承诺已兑现） |
| P6 | 前端 wire 类型 | vitest 32 passed | **通过**：`REQUEST({ dataFormat: 'parquet' })` 编译并运行通过；planner 镜像断言按提交说明递延至 T6（§5.3） |
| P7 | 二进制不进文本路径 | `write_data_file_rejects_binary_format`（既存）+ stream_formatter_parquet | **通过**：Parquet 与 XLSX 同款——文本路径先拒，构造不 panic，header/tail 恒空 |
| P8 | worktree/远端状态 | `git status` / `ls-remote` | 验证前后 clean；本地 HEAD = 远端 HEAD = `3b51f4243`，测试期间无第三方推送 |

## 5. 偏差裁决（4 项，逐项明确）

### 5.1 偏差 1：off-feature 错误臂在 T2 就落进 `binary_formatter`（计划把 Parquet 分发臂排在 T5）——通过

计划 T2 的验收只列变体 / 扩展名 / 二进制标志 / allow-list / 前端类型 / 测试；`binary_formatter` 的 Parquet **错误臂**（`mod.rs:734-739`）属于分发（T5 范围）。但 T1 报告 §5.5 明确要求“Parquet 的 off-feature 错误测试须在 T2/T5 补上，并在 T5 验收中显式勾选”。实现选择错误臂随 T2 落地、formatter 臂留给 T5：门控机制在 formatter 存在前即被证明（镜像 XLSX 的 T1 模式），错误测试的验收落点比 T5 更早，与 T1 递延条件完全兼容。错误文案与 XLSX off-feature 臂逐字同构（仅特性名不同），信封同为 `CommandError::Validation`。裁决：**通过**。

### 5.2 偏差 2：新增 `stream_formatter_parquet_header_and_tail_are_empty_by_construction`（计划 T2 测试清单未列）——通过

`format.rs` 为保 exhaustive match 加了三处 Parquet 臂（192–196 / 266 / 449–453），必须有测试执行它们，否则 format.rs 会出现无覆盖死区。测试与 XLSX 同款约定（二进制不走文本 formatter；header/tail 恒空），语义正确，且是 §3.4 覆盖证明的直接来源。裁决：**通过**。

### 5.3 偏差 3：`exportPlan.test.ts` 只加 type-only 测试，`formatExtension` / `isBinaryFormat` 镜像断言递延 T6——通过，有条件

计划 T2 写“前端 planner 镜像测试（exportPlan.test.ts）同步加用例”；但 planner 的接线（`formatExtension` 加 `'parquet' → 'parquet'`、`isBinaryFormat` 加 parquet）按计划是 T6 的活——现在断言会失败（planner 尚无 parquet 分支）。type-only 测试钉住 wire 类型（`ExportDataFormat` 接受 `'parquet'`，REQUEST 构造通过），是 T2 时点唯一可断言的正确内容。裁决：**通过，附条件**——T6 必须补 `formatExtension('parquet')` / `isBinaryFormat('parquet')` 镜像断言（实施计划 §T6 已列，移交 T6 任务书）。同批提交中的 prettier 重排（`it.each` 单行化、`defaultExportName` 参数折叠）为纯格式化，32 个用例全绿证明语义未变。

### 5.4 偏差 4：export-only 下 `mod.rs` 71.98%（低于 80% 文件级）——通过，不阻塞（复用 T01 §5.3 裁决口径）

缺失行已逐段核实（§3.4）：缺口主体是 `write_binary_file` 分发后的流式写循环等**既存 formatter 路径代码**，在无二进制子特性的构建中按设计不可达；外加既存 `_` 兜底（740-742）。**T2 新增的每一行（734-739）在 export-only 下已被覆盖**（不在缺失表）。门控测量应取代码实际存在的配置，即全特性（87.28% ≥ 80%）；export-only 的 71.98% 是特征门控的预期产物，与 T1 的 71.85% 同款，不阻塞 T2。

## 6. 缺陷与非缺陷观察

### 缺陷列表

无。未发现阻塞缺陷，也未发现非阻塞缺陷。

### 非缺陷观察

- **O-1（T1 O-1 已兑现）：** 根 `Cargo.toml` 注释 `58.4.1 → 58.4.0` 已随 T2 feat 提交修正（P5 核实），T01 报告的建议闭环。
- **O-2（环境注记，供后续会话）：** 本沙箱直连 cdn.sheetjs.com 极慢（HTTP/2 帧层故障 + HTTPS 低速），而 `xlsx-0.20.3.tgz` 是 `packages/frontend` 的直接依赖且被静态 import，vitest 无法绕开。处理方式：低速直接拉取官方 tarball，`openssl dgst -sha512` 与 `pnpm-lock.yaml:4077` 的 integrity **完全一致**（字节级等价）后，经 pnpm 临时 override 完成安装，随后 `package.json` / `pnpm-lock.yaml` 立即还原、git 树 clean——无仓库文件改动残留。后续会话若重装 pnpm 依赖，需要同样处理或可直连的网络。
- **O-3（file_job.rs 0%）：** `export/file_job.rs` 117 行 0%——T1/T2 均未触碰、export 过滤测试本就不经过（T01 口径同样不含该文件），非 T2 缺陷，不在门禁范围。
- **O-4（既存警告原样保留）：** 强制重编 datazen lib test 时可见 1 条既存 dead_code（`execution/runtime/tunnel/source_scan_test_support.rs:154` `Literal.at`）与 3 个驱动 crate 的 `qualify_sql` 未用参数警告——位置均在 T1/T2 diff 之外，按 T01 §5.4 口径接受，不顺手修，留 CI。
- **O-5（分支覆盖率）：** 本地不可测，Regions 代理见 §3.3，CI 覆盖。

## 7. 结论

**PASS — T2 可以关闭。**

- 变体接入：`DataFormat::Parquet` / `extension()=="parquet"` / `is_binary()` / `ALLOWED_EXPORT_EXTS` 与桌面 allow-list 加 `"parquet"` / 前端 `ExportDataFormat` 加 `'parquet'`，与计划 T2 逐项一致；文本 `StreamFormatter` 三处 exhaustive match 补齐。
- 门控：off-feature 错误（T1 §5.5 递延项）在 export-only 与 export+xlsx 两配置证实——明确 `Validation` 错误含 `export-parquet`、不建文件；全特性下该测试编出为无，计数自洽（91 / 72 / 92，lib 总数 1104 / 1085 / 1105）。
- 门禁：四配置 check 全部 exit 0 且 0 warnings；fmt 干净；vitest 定向 32 passed。
- 覆盖率：全特性 binary 97.31 / fetcher 99.33 / mod 87.28 / format 95.15，全部 ≥80%；export-only 的 mod 71.98% 为门控预期产物，T2 新增行在两配置下全覆盖。
- 递延项（非阻塞，移交任务书）：`formatExtension` / `isBinaryFormat` 镜像断言随 T6（§5.3）；pyarrow / DuckDB 读回随 T7；分支覆盖率由 CI 覆盖。
