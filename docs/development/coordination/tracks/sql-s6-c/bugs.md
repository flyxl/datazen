# Track S6-C: Bugs

> Audit phase: read-only findings. No source modifications in this phase.

---

## BUG-1: SQL Server `quoteChar` is `"`, should be `[`

- **Severity:** Medium
- **File:** `packages/drivers/sqlserver/ui/meta.ts` line 11
- **Current:** `quoteChar: '"'`
- **Expected:** `quoteChar: '['` (per PRD §3.4.2)
- **Status:** OPEN — fix approved as Sub-Track S6-C-1
- **Impact:**
  - `escapeIdent()` in `src/lib/databaseTypes.ts` does not handle `[` bracket style — will return unquoted identifier when `quoteChar` is `[`
  - `BaseTableSqlGenerator.quote()` already handles `[` ✅
  - DDL/index SQL template literals already work with `[` ✅
  - Dialect adapter already has `bracket` profile for `sqlserver` ✅
- **Fix:** Change `quoteChar` to `[`, add bracket case to `escapeIdent()`, add tests

## BUG-2: ClickHouse metadata inconsistency — Product Decision Gate

- **Severity:** Low (no functional impact yet; deferred pending decision)
- **Files:**
  - `packages/drivers/clickhouse/ui/meta.ts`: `quoteChar: '"'`
  - `packages/drivers/clickhouse/src/structure.rs` line 308: uses `"`
  - `packages/drivers/clickhouse/src/sync_adapter.rs` line 231: uses `` ` ``
  - `src/components/sql-editor/semantic/dialectAdapter.ts` line 72: `quoteStyle: 'backtick'`
  - PRD §3.4.2: ClickHouse → backtick
- **Status:** BLOCKED — product decision needed
- **Impact:** No functional bug yet (ClickHouse supports both `"`` and `` ` ``). However, the inconsistency between meta.ts (`"`), dialect adapter (`` ` ``), and PRD (`` ` ``) must be resolved before the SQL editor's smart-quote feature is enabled for ClickHouse.
- **Options:**
  - A: Keep `"` in meta.ts, change dialect adapter to `double` → matches Rust structure.rs
  - B: Change meta.ts to `` ` ``, keep dialect adapter as `backtick` → matches PRD
  - C: Change both to `` ` `` → matches PRD, most consistent

## BUG-3: `escapeIdent()` missing bracket `[` handling

- **Severity:** Medium (prerequisite for BUG-1 fix)
- **File:** `src/lib/databaseTypes.ts` lines 66-71
- **Current:** Falls through to `return name` (no quoting) when `quoteChar` is `[`
- **Expected:** Should return `[ident]` with `]]` escaping
- **Status:** OPEN — fix approved as part of Sub-Track S6-C-1
- **Impact:** When SQL Server `quoteChar` is corrected to `[`, `escapeIdent()` will silently return unquoted identifiers for SQL Server databases
- **Fix:** Add `if (q === '[') return \`[\${name.replaceAll(']', ']]')}]\`;` to `escapeIdent()`

## NOTE-1: CodeMirror MSSQL mapping missing

- **Severity:** Low (S6-D task, not this track)
- **File:** `src/components/sql-editor/contracts.ts` lines 50-55
- **Current:** `CM_DIALECT_MAP` has no `sqlserver` entry; falls to `StandardSQL`
- **Expected:** Should map to `MSSQL` from `@codemirror/lang-sql`
- **Status:** Deferred to S6-D (central assembly track)
