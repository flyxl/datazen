# T05 write_binary_file 分发 — 独立测试报告

- 任务：Parquet 导出 T5（计划 `docs/archive/development/parquet-export-implementation-plan.md` §T5，验收见计划行 57–60；PRD §4 F1 / F7）
- 分支：`feature/parquet-export`，worktree `/home/z/my-project/datazen/.worktrees/datazen-parquet-export`
- 受测提交：`df1d51212` `feat(export): dispatch Parquet through write_binary_file (T5)`（1 文件，+25/−13）、`7beed02d6` `test(export): cover the Parquet write_binary_file dispatch end to end (T5)`（1 文件，+285）
- 测试员：独立测试（分工与验证方式同 T02–T04；数字全部来自本机独立运行）。
- 结论：**PASS — T5 可以关闭**。无阻塞缺陷；2 项偏差裁决通过（见 §5）；**T04 报告 §5.4 的附条件（write_binary_file 路径端到端测试）已兑现**。
- 报告日期：2026-11-12；验证前后 worktree clean；HEAD = 远端 HEAD = `7beed02d6`。

## 1. 环境与命令

与 T03/T04 报告相同（llvm-cov 仪表化运行承载测试计数，plain `cargo test` 并入 T8；T03 §6 O-2 注记适用）。

## 2. 门控结果（全部独立复跑）

| 检查 | 配置 | 结果 |
|---|---|---|
| check ×4 | `export` / `+xlsx` / `+parquet` / 全特性 | 全部 exit 0，**0 warnings**（let- 矩阵在四组合下均无未用参数警告） |
| fmt | `cargo fmt --check -p datazen` | exit 0，干净 |
| 测试（全特性） | `-- export` 过滤 | **121 passed，0 failed**（1013 filtered；lib 总数 1134） |
| 测试（export-parquet） | 同上 | **101 passed，0 failed**（总数 1114） |
| 测试（export-only） | 同上 | **72 passed，0 failed**（与 T04 逐数一致；T5 的分发臂与测试在该配置均不编译） |

数量自洽：全特性 1134 = T4 的 1128 + 6；export-parquet 1114 = 1108 + 6；export-only 72 不变。

## 3. 覆盖率（export 过滤测试，独立重测）

| 文件 | 全特性 | export-parquet | export-only |
|---|---|---|---|
| `export/mod.rs` | **87.36%**（1218 行） | **85.36%**（1216 行） | 72.05%（1213 行；既存流式循环不可达模式，T02 §5.4 口径，非阻塞） |
| `export/binary.rs` | **90.68%**（1964 行） | 83.77%（既测） | 不存在（仅 trait） |
| `export/fetcher.rs` | **99.34%**（3348 行，含 +285 测试行） | — | — |
| `export/format.rs` / `parquet_schema.rs` | 95.15% / 100% | 同前 | 同前 |

- **mod.rs 的 export-parquet 配置从 T4 时代的 71.96% 升至 85.36%**：Parquet 分发臂在该配置首次编译且被 6 个集成测试覆盖——T5 的新增行在两个含 parquet 配置下全覆盖，门禁口径（全特性 87.36%）达标。
- export-only 的 72.05% 与 T02–T04 的 71.98% 属同款既存模式（分发后流式循环在无二进制子特性时不可达），T02 §5.4 裁决适用，不阻塞。

## 4. 对抗探针（独立执行）

| # | 探针 | 结果 |
|---|---|---|
| P1 | 分发链路 + 方言透传 | **通过**：postgresql 请求 → 文件 schema 中 int4 映射为 Int32（而非无方言时的 Int64 值推断）——T3 推断 → T4 格式器 → T5 分发的整条链在一个读回断言里闭合 |
| P2 | `database_type` 为 None | **通过**：值推断 + Utf8 兜底写出合法文件（Int64/Utf8 schema） |
| P3 | 取消语义（复用既有循环） | **通过**：预取消不流式不建文件（stream_calls==0）；CancellingSource 中途取消返回 Cancelled——与 XLSX 共用同一条按批检查路径，无 Parquet 专属取消逻辑 |
| P4 | Done 未到的兜底（T04 §5.4 递延项） | **通过**：`emit_done=false` 的流结束后由 write_binary_file 的流后兜底 finish 恰好一次关闭 writer，文件读回 2 行完整 |
| P5 | 无 StatementStart 的 header 兜底 | **通过**：col_0/col_1（data_type "text" → Utf8）schema 写出 |
| P6 | 计数自洽 | 121 = 115+6；101 = 95+6；总数 1134/1114 闭合 |

## 5. 偏差裁决（2 项）

### 5.1 偏差 1：`binary_formatter` 以显式 `database_type: Option<&str>` 参数代替计划字面的 `request` 访问——通过

计划写 `ParquetFormatter::new(request.database_type.clone(), ...)`；实现把方言作为显式参数传入 helper（调用点 `request.database_type.as_deref()`），helper 保持纯函数、不携带整个请求上下文，且四种特性组合下未用参数由 let- 矩阵显式消音（四组合 0 warnings 佐证）。行为等价。裁决：**通过**。

### 5.2 偏差 2：测试超出计划三要点（预取消 + header 兜底两额外用例）——通过

计划 T5 验收列"走通 / None 走通 / 取消按批复用"；实现另覆盖了预取消早退守卫与无 StatementStart 的 header 兜底——两者都是 write_binary_file 既有循环对 Parquet 同样生效的证明，与"复用既有循环，不另写"的验收精神一致。裁决：**通过**。

## 6. 缺陷与非缺陷观察

- **O-1：** T4 报告 §5.4 附条件兑现：`write_binary_file_without_done_still_finishes_parquet` 走真实分发路径证明兜底契约。
- **O-2：** 既存警告（T02 §6 O-4）原样保留，位置在 T5 diff 之外。
- **O-3：** 分支覆盖率本地不可测，Regions 代理：mod.rs 全特性 90.31%、export-parquet 88.30%；CI 覆盖。

## 7. 结论

**PASS — T5 可以关闭。**

- Parquet 请求经 write_binary_file 完整走通：分发臂（带方言透传）、既有取消循环、既有 Done 兜底、既有 header 兜底，全部按计划复用、零新控制流。
- 6 个集成测试全绿；三配置计数 121 / 101 / 72（总数 1134 / 1114 / 1085）自洽；mod.rs 全特性 87.36%、export-parquet 85.36%；四组合 check 0 warnings；fmt 干净。
- 递延项：多表 Parquet zip、类型冲突经 write_binary_file 的报错面随 T7；pyarrow / DuckDB 读回随 T7；前端接线随 T6。
