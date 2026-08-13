# F5 QA 报告：ChangeSet → 参数化 INSERT/UPDATE/DELETE + 只读 Preview SQL

| 项 | 值 |
|---|---|
| 切片 | F5 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat` |
| 被测代码 | `src-tauri/src/data_sync/sql.rs` |
| 规格 | 执行用参数绑定；Preview 用字面量；标识符 quote；DELETE WHERE 用 PK；NULL PK 用 IS NULL；未变更行不得生成 SQL。PRD V1.2 §14 / §28.6；方案 `docs/data-synchronization-implementation-plan.zh-CN.md` §2.4 / §30.4 / §38.4 |
| 测试角色 | 独立 QA；未修改任何产品代码；未 commit |
| 日期 | 2026-08-13 |
| **总评** | **PASS** |

---

## 1. 测试环境

| 工具 | 版本 |
|---|---|
| OS | macOS darwin 24.6.0 (arm64) |
| rustc | 1.90.0 (1159e78c4 2025-09-14) |
| cargo | 1.90.0 (840b83a10 2025-07-30) |
| toolchain | stable-aarch64-apple-darwin |
| cargo-llvm-cov | 0.8.7 |
| crate | `datazen` 0.0.9（`src-tauri`） |
| 桌面应用 | **未要求**（本切片是纯 SQL 生成器，无 IPC） |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat
```

覆盖率原始摘要：`docs/progress/f5-coverage.txt`。

---

## 2. 本切片测试设计（对照规格）

规格要点（本切片）：

- 执行 SQL 使用占位符 + `parameters`（PostgreSQL `$n` / MySQL `?`），禁止把值拼进执行语句
- Preview SQL 使用 `format_literal` 字面量；与执行语句结构一致
- 表名 / 列名经 `quote_ident`（`"` 加倍、`` ` `` 加倍）
- DELETE `WHERE` 只按 PK 列，不用非 PK 列
- PK 值为 `Value::Null` → `IS NULL`，且不产生绑定参数
- `ChangeOperation::Unchanged` 不得生成语句（实现为 validation error）
- 无 PK 列不得生成 SQL

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-SQL-01 | Rust | INSERT/UPDATE/DELETE 参数化 + Preview 字面量 | `insert_update_delete_parameterized_and_preview` |
| UT-SQL-02 | Rust | MySQL `?` + 反引号 | `mysql_placeholders_and_backticks` |
| UT-SQL-03 | Rust | Unchanged 拒绝；空 PK 拒绝 | `rejects_unchanged_and_bad_arity` |
| UT-SQL-04 | Rust | NULL PK → `IS NULL`，parameters 空 | `null_pk_uses_is_null` |
| UT-SQL-05 | Rust | 各 `Value` 字面量 + quote 内嵌引号加倍 | `format_literal_covers_value_kinds` |
| E2E-F5-* | E2E | Preview IPC + 真实执行走绑定参数 | **无 Preview/Execute IPC → BLOCKED**（用例见 §5） |

---

## 3. 单元测试

### 3.1 命令与结果

```bash
cargo test -p datazen --lib data_sync::sql -- --nocapture
```

| 项 | 数量 |
|---|---:|
| 通过 | **5** |
| 失败 | **0** |
| 忽略 | 0 |
| 过滤掉的其它 lib 测试 | 873 |
| 耗时 | 0.00s（编译约 10s） |

失败详情：无。

编译告警（与 `data_sync::sql` 无关，不计入本切片）：

- `src-tauri/src/dashboard/execute.rs` unused import `RefreshMode`
- `src-tauri/src/dashboard/create.rs` unused variable `registry`

### 3.2 已落地单测清单（对照规格）

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-SQL-01 | `insert_update_delete_parameterized_and_preview` | PG：`INSERT … VALUES ($1, $2)`；`UPDATE SET "name" = $1 WHERE "id" = $2`（2 个参数）；`DELETE FROM "clients" WHERE "id" = $1`；Preview `WHERE "id" = 3`；INSERT Preview 含 `'a'`；标识符双引号 | PASS |
| UT-SQL-02 | `mysql_placeholders_and_backticks` | `INSERT INTO \`t\` (\`id\`) VALUES (?)` | PASS |
| UT-SQL-03 | `rejects_unchanged_and_bad_arity` | Unchanged → Err；`pk_columns=[]` → Err（「cannot generate SQL without primary key columns」） | PASS |
| UT-SQL-04 | `null_pk_uses_is_null` | `DELETE FROM "t" WHERE "id" IS NULL`；`parameters` 为空 | PASS |
| UT-SQL-05 | `format_literal_covers_value_kinds` | `NULL` / `TRUE`/`FALSE` / Float / `o''reilly` / Bytes / Timestamp / Json；`quote_ident_sql("na\"me")` → `"na""me"` | PASS |

### 3.3 手工对照（不改代码）

对照 UT-SQL-01 夹具（PG quote=`"`，`$n`）：

| 操作 | 执行 SQL | Preview | parameters |
|---|---|---|---|
| INSERT `(1,'a')` | `INSERT INTO "clients" ("id", "name") VALUES ($1, $2)` | VALUES 含 `1, 'a'` | `[Integer(1), String("a")]` |
| UPDATE name `old→b` | `UPDATE "clients" SET "name" = $1 WHERE "id" = $2` | SET 字面量 + WHERE `id = 2` | 2 个（SET + PK） |
| DELETE id=3 | `DELETE FROM "clients" WHERE "id" = $1` | `WHERE "id" = 3` | `[Integer(3)]` |

与实现一致。DELETE 未把 `name` 写入 WHERE。Unchanged 走 `statement_for_change` 的 validation 分支，不会 `push` 任何 `SqlStatement`。

---

## 4. 覆盖率（仅 `src-tauri/src/data_sync/sql.rs`）

### 4.1 测量方法

1. `cargo llvm-cov --version` → 0.8.7。
2. `--ignore-filename-regex` 负向前瞻**不能**把统计限制到单文件（llvm-cov 仍汇总整个 crate）。**不以整 crate 覆盖率为验收标准。**
3. 未过滤的 `cargo llvm-cov -p datazen --lib` 会因无关测试失败中断（`sync::adapter_registry::tests::ensure_type_wire_aliases_succeed`）。不属 F5。
4. 实际采用：

```bash
cargo llvm-cov -p datazen --lib --json --output-path /tmp/datazen-f5-cov/f5-coverage.json -- data_sync::sql
```

然后按路径 `/data_sync/sql.rs` 过滤。摘要写入 `docs/progress/f5-coverage.txt`（完整 crate JSON 未入库）。

### 4.2 行覆盖率

| 文件 | 行 covered | 行 total | 行覆盖率 | 函数 | 区域 |
|---|---:|---:|---:|---:|---|
| `sql.rs` | 324 | 349 | **92.84%** | 86.96%（20/23） | 95.68%（487/509） |

**门槛：行覆盖率 ≥ 80% → 满足（92.84%）。**

### 4.3 未覆盖行（不构成 FAIL）

全部为校验失败臂，现有 5 个测试未构造这些非法 ChangeSet：

| 位置 | 说明 |
|---|---|
| `125–126` | INSERT 缺 `source_row` |
| `128` | INSERT 行宽 ≠ `column_names` |
| `175–176` | UPDATE 缺 `source_row` |
| `178` | UPDATE `changed_columns` 为空 |
| `188–189` | UPDATE 变更列不在列清单 |
| `254` | `pk_columns.len() != key.len()`（测试名含 arity，实际只打了空 PK 列表） |

测试缺口（建议后续补，不改本切片结论）：

1. 复合 PK：`WHERE "a" = $1 AND "b" = $2`；中间列 `NULL` → `IS NULL` 且后续占位符序号不递增。
2. INSERT `None` 单元格 → 执行参数 `Value::Null`，Preview `NULL`。
3. UPDATE 把列设为 NULL；UPDATE + NULL PK。
4. PK arity 不匹配（2 列 PK vs 1 元 key）。
5. INSERT/UPDATE 缺行、列宽/未知列名的显式断言（文案）。
6. MySQL Preview 与 `` ` `` 内嵌反引号加倍。
7. UT-SQL-01 应对 INSERT Preview 做精确字符串断言，并断言 `parameters` 值（不仅长度 / contains）。

---

## 5. E2E 用例表（未来接 Preview / Execute IPC）

现状：**BLOCKED**。`generate_table_sql` / `SqlStatement` **未接到任何 IPC 或 UI**：

- `src/commands/` 无 `preview_sql` / `generate_table_sql` / `sync_preview`
- `src-tauri/src/commands/sync/` 仍是旧 `compare_table_data` / `sync_tables`（覆盖拷贝已拒绝，见 F3）
- `DataSyncWindow.tsx` 无 SQL Preview 面板，不引用本生成器
- `SyncSession::generate_change_set` 只组 ChangeSet，**不调用** `generate_table_sql`（相位名 `GeneratingSql` 名不副实）
- 现有 `e2e/specs/data-sync-real.ts` 不能当 F5 验收

无 `sync_preview_sql` / `sync_execute_changeset`。Execute 专用 IPC 属 F6。

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F5-01 INSERT 参数化 + Preview 字面量 | 同族两库、结构+PK 一致；Source 多一行；生成 Preview | 执行 SQL 为 `INSERT … VALUES ($n/? )`，无字面量姓名；Preview 含格式化字面量；`parameters` 与列对齐 | **BLOCKED**（无 Preview IPC） |
| E2E-F5-02 UPDATE 只 SET 变更列 + WHERE PK | 同 PK，仅 `name` 不同 | 执行 `UPDATE … SET "name" = $1 WHERE "id" = $2`；不得 SET 未变列；Preview SET/WHERE 用字面量 | **BLOCKED** |
| E2E-F5-03 DELETE WHERE 仅 PK | Target-only 行且 options.delete 开并勾选 | `DELETE FROM … WHERE pk…`；WHERE 不含非 PK 列 | **BLOCKED** |
| E2E-F5-04 NULL PK → IS NULL | 可空 PK 为 NULL 的 DELETE/UPDATE | `col IS NULL`；该列不出现在 `parameters` | **BLOCKED** |
| E2E-F5-05 未变更行不出现 SQL | 部分行完全一致 | Preview/执行列表无 UNCHANGED 语句；不得因 Unchanged 整表失败 | **BLOCKED** |
| E2E-F5-06 未勾选行不生成 SQL | Review 取消某 INSERT | 该行不出现在 Preview，也不进入 Execute | **BLOCKED** |
| E2E-F5-07 标识符 quote | 表/列名为保留字或含 `"` / `` ` `` | 走 Target `quote_ident`；内嵌 quote 加倍 | **BLOCKED** |
| E2E-F5-08 Preview 字符串转义 | 单元格 `o'reilly` | Preview `'o''reilly'`；执行仍绑定原值 | **BLOCKED** |
| E2E-F5-09 复合 PK AND | PK `(tenant_id, id)` | WHERE 两列 AND；顺序与 PK 定义一致 | **BLOCKED** |
| E2E-F5-10 复合 PK 混 NULL | `(1, NULL)` | `"tenant_id" = $1 AND "id" IS NULL` | **BLOCKED** |
| E2E-F5-11 Preview 只读 | UI 展示 Preview | 不可编辑后当执行脚本；Execute 走 ChangeSet 绑定，不解析 Preview 文本（PRD §14） | **BLOCKED** |
| E2E-F5-12 执行 ≠ Preview 字符串 | spy 驱动 `execute`/`query_with_params` | 实际调用带 `parameters`；不得把 Preview 全文当 SQL 执行 | **BLOCKED**（属 F6） |
| E2E-F5-13 方言占位符 | 同 ChangeSet 在 MySQL vs PostgreSQL Target | MySQL 全 `?`；PG `$1…$n`；quote 分别为 `` ` `` / `"` | **BLOCKED** |
| E2E-F5-14 空 ChangeSet | 全 UNCHANGED 或全不选 | 不得生成可执行 SQL；validate_executable 失败 | **BLOCKED** |
| E2E-F5-15 无 PK 表 | 门闸应已 INCOMPATIBLE；若仍调生成器 | 拒绝生成（「without primary key columns」） | **BLOCKED** |
| E2E-F5-16 BLOB Preview 占位 | 大 Bytes 列 UPDATE | Preview 截断/hash/hex 前缀；执行参数仍为完整值（PRD §14） | **BLOCKED**（生成器目前 utf8_lossy 全量拼字面量，见 §6） |
| E2E-F5-17 旧 `sync_tables` 不得当 Preview | 若仍暴露覆盖拷贝 IPC | 产品 Preview 不得来自 DROP+INSERT 路径 | **BLOCKED**（旧 IPC 仍在且已拒绝执行；未接线到新生成器） |

F6/F9 落地建议：E2E-F5-01–05/09/11/12 进 Host 契约 journey（MySQL + PG 夹具）；E2E-F5-12 用 mock/`query_with_params` 调用计数即可，不必 UI。

---

## 6. 规格缺口审查（F5 范围内）

审查范围：`quote_ident_sql` / `mysql_placeholder` / `postgres_placeholder` / `format_literal` / `generate_table_sql` 及其 INSERT/UPDATE/DELETE/`where_pk`。Execute IPC、事务、UI Preview 属 F6/F8，标 BLOCKED / 切片外，不记本切片 P0 缺陷。

| 规格 | 实现 | 结论 |
|---|---|---|
| 执行用参数绑定 | `sql` 仅占位符；值在 `parameters` | 一致 |
| Preview 用字面量 | `preview_sql` + `format_literal` | 一致（与执行同结构） |
| 标识符 quote | 回调 `quote_ident`；`quote_ident_sql` 加倍 | 一致（F6 须注入 `driver.quote_ident`） |
| DELETE WHERE 用 PK | `where_pk(pk_columns, change.key)` | 一致 |
| NULL PK → IS NULL | `Value::Null` 分支；不 push param | 一致 |
| 未变更行不得生成 SQL | `Unchanged` → validation Err，不 `push` | 一致（比静默 skip 更严；ChangeSet 本就不含 Unchanged） |
| 无 PK 不得生成 | 空 `pk_columns` → Err | 一致 |
| 禁止把用户值拼进执行 WHERE/SET/VALUES | 执行路径只用 placeholder | 一致 |

**未记为 P0 冲突**的残留（不构成 FAIL；不改代码）：

1. **未复用 Driver `build_update_sql` / `build_delete_sql`**：方案 §2.4 / §41 写「Execute 复用」这两套。它们内部用 `format_sql_literal` **内联字面量**，本身不是参数化执行 SQL。本切片 Host 生成器反而更符合 §38.4 / PRD §28.6。F6 接线时应以本文件的 `sql`+`parameters` 走 `query_with_params`，Driver 的 `build_*_sql` 最多当 Preview 对照，不能当执行语句。
2. **`format_literal` 不可注入**：`quote_ident` / `placeholder` 是回调，Preview 字面量写死 Host `format_literal`。与默认 `DatabaseDriver::format_sql_literal` 相同，但 MySQL 覆盖把 Bool 格式化成 `1`/`0`。F6 若要 Preview 与驱动完全一致，需传入 `driver.format_sql_literal`。
3. **BLOB / 超长文本未截断**：PRD §14 要求 Preview 占位、完整值仍绑定。当前 Bytes 走 `from_utf8_lossy` 全量进 Preview。属 Preview 质量；不破坏参数化执行。
4. **`SyncSession::generate_change_set` 不调用本生成器**：相位 `GeneratingSql` 只组 ChangeSet。属 F6/F7 编排。
5. **UPDATE 不校验行宽**：INSERT 检查 `row.len() == column_names.len()`；UPDATE 对缺失下标 `get(pos)` 当 NULL。合法 Compare 输出不会触发；非法 ChangeSet 可能静默 SET NULL。
6. **schema 限定表名**：`quote_ident("public.users")` 会变成单个标识符。当前 `target_table` 是裸表名、schema 在 Endpoint，F6 须 `quote_ident(schema)` + `.` + `quote_ident(table)`，不要把 `schema.table` 一次 quote。
7. **`generate_table_sql` 不看 `selected`**：未勾选行若被塞进 `TableChangeSet.changes` 仍会出 SQL。正常路径 `ChangeSet::from_comparison` 已过滤；调用方契约要保持。

---

## 7. Bug 列表

无。

---

## 8. 总评

| 门槛 | 结果 |
|---|---|
| `cargo test -p datazen --lib data_sync::sql` | **5 passed / 0 failed** |
| `sql.rs` 行覆盖 ≥80% | **92.84%**（324/349） |
| 与 P0 规格冲突的缺陷 | 无 |
| Preview / Execute IPC E2E | 全部 **BLOCKED**（本切片无 IPC） |

**总评：PASS**

ChangeSet → 参数化 INSERT/UPDATE/DELETE + 只读 Preview 字面量（标识符 quote、DELETE 仅 PK WHERE、NULL PK 用 IS NULL、Unchanged 拒绝生成）与本切片规格 / PRD §14 / §28.6 在 `generate_table_sql` 边界内一致。Driver `quote_ident` 注入、Preview IPC、以及执行走绑定参数（禁止执行 Preview 文本）留给 F6/F8/F9。
