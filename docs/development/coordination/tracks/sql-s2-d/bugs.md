# Bugs: sql-s2-d

> Tester 登记。编码提交 `5cce0a7b2`。状态流转：`待修复 → 修复中 → 待复测 → 已修复` 或回到 `待修复`（最多 5 轮）。

## sql-s2-d-BUG-001（P1 · 安全）

- **状态**：已修复（Round 3 全新 Tester 独立复测通过）
- **文件**：`src-tauri/src/sql_guard/safety.rs`（`has_top_level_where`，line 196-224）
- **复测结论（Round 3，独立 Tester）**：`check_sql("UPDATE t SET x = 1 /* WHERE g */",false,true)` → `Err` ✓；`check_sql("DELETE FROM t -- WHERE g",false,true)` → `Err` ✓；`check_sql("UPDATE t SET x = 1 # WHERE g",false,true)` → `Err` ✓；`check_sql("UPDATE t SET x = $q$WHERE$q$",false,true)` → `Err` ✓。全部命中，BUG-001 + BUG-002 残域均已拦截，无绕过。
- **复测结论（Round 2，Tester 独立实测）**：`check_sql("UPDATE t SET x = 1 /* WHERE g */",false,true)` → `Err` ✓；`check_sql("DELETE FROM t -- WHERE g",false,true)` → `Err` ✓（block/line 注释路径已修复）。但该漏洞类经 `#`-comment 与 dollar-quote 仍可绕过，见 BUG-002。完整的复测数字与新增测试见 `progress.md`。
- **描述**：Host Guard 的 Safe Mode no-WHERE 检查对 `UPDATE`/`DELETE` 语句，把**块注释/行注释内，以及 PG dollar-quote body 内的 `WHERE` 当作真实顶层 WHERE**，从而放行本应被 Safe Mode 拦截的无 WHERE 写操作。这与规范 §6.8「WHERE 必须属于目标 UPDATE/DELETE 顶层；CTE、subquery、字符串或注释中的 WHERE 不计」，以及前端风险 classifier（用 S2-A scanner 词法区判定，正确排除注释/dollar-quote 内 WHERE）**语义漂移**。前端对同一 SQL 判为 `hasTopLevelWhere=false` + `no-where` finding，后端却 `Ok(())` 放行。
- **严重度**：安全 — Safe Mode 是最终防线（`_common.md §5` 不变量：Rust `readOnly` + Safe Mode 是最终防线），该漏洞使无 WHERE 的 `UPDATE`/`DELETE` 通过注释内伪造 `WHERE` 绕过 Safe Mode；SQL 引擎执行时注释被剥离，实际会全表/全量更新。
- **重现步骤**：
  1. 单元复现 —— `check_sql("UPDATE t SET x = 1 /* WHERE g */", /*read_only=*/false, /*safe_mode=*/true)` 返回 `Ok(())`（应 `Err`，提示 Safe Mode 需要 WHERE）。
  2. `check_sql("DELETE FROM t /* WHERE g */", false, true)` 返回 `Ok(())`（同应拦截）。
  3. 对照：`check_sql("UPDATE t SET x = 'WHERE'", false, true)` 返回 `Err(...)`（字符串内 WHERE 被正确排除）——证明仅是**注释/dollar-quote** 分支漏处理。
  4. 前端对照：`classifyRisk("UPDATE t SET x = 1 /* WHERE g */")` → `statements[0].hasTopLevelWhere === false`、`findings = [{type:"no-where"}]`。前端行为正确，后端行为错误。
- **根因**：`safety.rs::has_top_level_where` 是手写字符状态机，调用 `scanner::skip_quoted` 仅覆盖 `'`/`"`/`` ` ``（`scanner.rs` line 3-20），**不处理 `--`/`#`/`/* */` 注释，也不处理 `$tag$…$tag$` dollar-quote**。因此注释/dollar-quote 内的 `WHERE` 单词落入 `ident`，在 `depth==0` 时被误判为真实 WHERE。
- **影响范围**：所有 `safe_mode=true` 连接上，无 WHERE 的 UPDATE/DELETE 若在注释/dollar-quote 内出现 `WHERE`，guard 放行；`read_only` 不受影响（该类语句必然命中 `is_write_verb` 走 readOnly 分支）。
- **方向**：前端 classifier 是符合规范的；应修复后端 `has_top_level_where` 使其跳过注释与 dollar-quote（对齐前端 scanner 词法区/`strip_sql_comments` 语义），或改为复用 S2-A scanner 词法区判定；并把 `UPDATE ... /* WHERE */`、`DELETE ... -- WHERE`、dollar-quote WHERE 三类用例加入共享 fixture `sqlRiskCases.json`（期望 `guardAllowed:false`），确保 frontend/Host 零漂移。
- **建议不变量**：修复后，`check_sql` 对含注释/字符串/dollar-quote WHERE 的无 WHERE UPDATE/DELETE 一律 `Err`；新增测试用 `test_tester_` 前缀登记。

## sql-s2-d-BUG-002（P1 · 安全 · BUG-001 残域）

- **状态**：已修复（Round 3 全新 Tester 独立复测通过）
- **文件**：`src-tauri/src/sql_guard/safety.rs`（`check_sql` line 143/164；`strip_sql_comments` line 33-78）
- **复测结论（Round 3，独立 Tester）**：`check_sql("UPDATE t SET x = $q$WHERE$q$",false,true)` → `Err` ✓；`check_sql("DELETE FROM t # WHERE g",false,true)` → `Err` ✓；`check_sql("UPDATE t SET x = $body$WHERE$body$",false,true)` → `Err` ✓；`check_sql("DELETE FROM t $q$WHERE$q$",false,true)` → `Err` ✓。正例控制：`check_sql("DELETE FROM t WHERE id = $q$WHERE$q$",false,true)` → `Ok` ✓；`check_sql("UPDATE t SET x = 'a#b' WHERE id = 1",false,true)` → `Ok` ✓；`check_sql("UPDATE t SET x = 'a$q$b' WHERE id = 1",false,true)` → `Ok` ✓（字符串内 `#`/`$q$` 不被误判为注释/dollar-quote，真实顶层 WHERE 仍放行）。
- **描述**：BUG-001 修复（f2e2a2efc）将 `has_top_level_where(&stmt)` 改为 `has_top_level_where(&stripped)`，其中 `stripped = strip_sql_comments(&stmt)`。`strip_sql_comments` 仅剥离 `--` 行注释与 `/* */` 块注释，**不处理**：① MySQL/H2 的 `#` 行注释（code 35 等价），② PG `$tag$…$tag$` dollar-quote 字符串体。而在 `has_top_level_where` 内 `skip_quoted` 只覆盖 `'`/`"`/`` ` ``，同样不处理上述两类。因此注释/dollar-quote 内的 `WHERE` 在 depth==0 时仍被误判为真实顶层 WHERE，Safe Mode 放行本应拦截的无 WHERE 写操作。
- **严重度**：安全 — 与 BUG-001 同类（P1）；Rust `readOnly` + Safe Mode 是最终防线（`_common.md §5`）。下述任一无 WHERE UPDATE/DELETE 均可借此绕过 Safe Mode，SQL 引擎执行时注释/dollar-quote 被剥离，实际全表/全量更新。
- **重现步骤（Tester 独立实测，本次全部命中 `Ok(())`——应 `Err`）**：
  1. `check_sql("UPDATE t SET x = $q$WHERE$q$", /*read_only=*/false, /*safe_mode=*/true)` → `Ok(())`（应 `Err`，dollar-quote 内 WHERE 被误判为真实顶层 WHERE）。
  2. `check_sql("DELETE FROM t # WHERE g", false, true)` → `Ok(())`（应 `Err`，`#` 行注释内 WHERE 未剥离）。
  3. 对照（已修复路径，应 `Err`，实际 `Err`）：`check_sql("UPDATE t SET x = 1 /* WHERE g */", false, true)` → `Err("Safe Mode requires a WHERE clause...")`；`check_sql("DELETE FROM t -- WHERE g", false, true)` → `Err`。
- **前端对照（语义漂移）**：`classifyRisk("UPDATE t SET x = $q$WHERE$q$")` → `statements[0].hasTopLevelWhere === false`、`findings = [{type:"no-where"}]`、`highRisk=true`；`classifyRisk("DELETE FROM t # WHERE g")` 同理 `hasTopLevelWhere=false`、`no-where`。前端行为正确（排除 dollar-quote/`#` 内 WHERE），后端错误放行 → frontend/Host 漂移。前端 scanner 在第 134 行（`#`→`LineComment`）与第 228 行（dollar-quote→`DollarQuoted`）正确处理，故漂移仅存在于后端。
- **根因**：修复只切换 `check_sql` 调用的入参（`&stmt`→`&stripped`），未扩展 `strip_sql_comments` 处理 `#` 与 dollar-quote，也未让 `has_top_level_where` 跳过 `#`/dollar-quote 区域（可复用 `scanner::dollar_quote_end` 或 `scanner::non_replaceable_ranges`）。
- **影响范围**：所有 `safe_mode=true` 连接上，无 WHERE 的 UPDATE/DELETE 若在 `#` 注释或 dollar-quote 体出现 `WHERE`，guard 放行；`read_only` 不受影响（该类语句必命中 `is_write_verb`）。
- **方向**：扩展 `strip_sql_comments`（或 `has_top_level_where`）以剥离/跳过 `#` 行注释与 `$tag$…$tag$` dollar-quote，对齐前端 scanner 词法区；并把 `UPDATE ... $q$WHERE$q$`、`DELETE ... # WHERE` 两条加入共享 fixture `sqlRiskCases.json`（期望 `guardAllowed:false`），确保 frontend/Host 零漂移。
- **建议不变量**：修复后，`check_sql` 对含注释（`--`/`#`/`/* */`）/字符串/dollar-quote WHERE 的无 WHERE UPDATE/DELETE 一律 `Err`；新增测试用 `test_tester_` 前缀登记。
