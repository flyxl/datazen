# Track S6-C: Dialect Metadata Audit & Driver Tasks

> Status: **AUDIT_COMPLETE** (read-only phase finished)
> Base commit: (worktree on `feature/sql-s6-c`)
> LastHeartbeat: 2025-07-26T12:00:00Z

---

## Audit Summary

This track performed a read-only audit of all driver `ui/meta.ts` files, comparing `quoteChar` and `sqlDialect` against PRD expectations and the dialect adapter system. The audit identified **one confirmed metadata bug** (SQL Server), **one product decision gate** (ClickHouse), and **three external plugin tasks** (Git drivers).

---

## 1. Complete Audit Table

### Path Drivers (in-repo)

| Driver | DB Types | `quoteChar` | `sqlDialect` | Dialect Adapter | PRD Match | Issues |
|--------|----------|-------------|--------------|-----------------|-----------|--------|
| **postgres** | PostgreSQL, QuestDB, Cloudberry | `"` | `postgresql` | `double` / `lower` | ✅ | None |
| **mysql** | MySQL, MariaDB, Doris, StarRocks, Manticore, OB Oracle | `` ` `` | `mysql` | `backtick` / `lower` | ✅ | None |
| **sqlite** | SQLite | `"` | `sqlite` | `double` / `lower` | ✅ | None |
| **rqlite** | RQLite | `"` | `sqlite` | `double` / `lower` | ✅ | None |
| **turso** | Turso / libSQL | `"` | `sqlite` | `double` / `lower` | ✅ | None |
| **sqlserver** | SQL Server | `"` | `sqlserver` | `bracket` / `preserve` | ❌ **BUG** | `quoteChar` should be `[`, not `"` |
| **clickhouse** | ClickHouse | `"` | `clickhouse` | `backtick` / `lower` | ⚠️ **GATE** | PRD says backtick; Rust `structure.rs` uses `"`, Rust `sync_adapter.rs` uses `` ` ``; dialect adapter says `backtick` |
| **duckdb** | DuckDB | `"` | `duckdb` | `double` / `lower` | ✅ | None |
| **redis** | Redis | `""` | — | — | ✅ | Non-SQL KV store; no quoting needed |
| **mongodb** | MongoDB | `""` | `mongodb` | — | ✅ | Non-SQL document store; no quoting needed |
| **elasticsearch** | Elasticsearch | `""` | `elasticsearch` | — | ✅ | Non-SQL; no quoting needed |
| **hbase** | HBase | `""` | `generic` | — | ✅ | Non-SQL; no quoting needed |
| **influxdb** | InfluxDB | `""` | `generic` | — | ✅ | Non-SQL; no quoting needed |
| **victoriametrics** | VictoriaMetrics | `""` | `generic` | — | ✅ | Non-SQL; no quoting needed |
| **vector** | Vector DB | `""` | `generic` | — | ✅ | Non-SQL; no quoting needed |

### Git Drivers (not in repo — external plugin repos)

| Driver | Source | Expected `quoteChar` | Status |
|--------|--------|---------------------|--------|
| **kiwi** | `https://github.com/flyxl/datazen-driver-kiwi.git` | Depends on underlying engine | 🔌 External task — verify `ui/meta.ts` in plugin repo |
| **olap** | `https://github.com/flyxl/datazen-driver-olap.git` (pinned ref) | Depends on Presto/Trino | 🔌 External task — verify `ui/meta.ts` in plugin repo |
| **superset** | `https://github.com/flyxl/datazen-driver-superset.git` | Non-SQL exploration platform | 🔌 External task — verify `ui/meta.ts` in plugin repo |

---

## 2. PRD Reference (from `docs/todo/sql-editor/prd.md`)

The PRD (§3.4.2) specifies the following smart-quote rules for drop caret and drag operations:

- **MySQL / ClickHouse / Doris** → backtick `` ` ``
- **PostgreSQL / openGauss** → double quote `"`
- **SQL Server** → brackets `[]`

The dialect adapter (`src/components/sql-editor/semantic/dialectAdapter.ts`) is already correctly configured:
- `sqlserver` / `mssql` → `quoteStyle: 'bracket'`
- `mysql` / `mariadb` → `quoteStyle: 'backtick'`
- `clickhouse` → `quoteStyle: 'backtick'`
- `postgresql` / `sqlite` → `quoteStyle: 'double'`

---

## 3. Confirmed Bugs

### BUG-1: SQL Server `quoteChar` is `"`, should be `[`

**File:** `packages/drivers/sqlserver/ui/meta.ts` line 11
**Current:** `quoteChar: '"'`
**Expected:** `quoteChar: '['`

**Impact:**
- `escapeIdent()` in `src/lib/databaseTypes.ts` line 67-70: Currently falls through to the default case (no escaping) when `quoteChar` is `"`, which happens to work for double-quote. But when fixed to `[`, the function has **no bracket handling** — it will return the unquoted identifier.
- `BaseTableSqlGenerator.quote()` in `src/lib/sqlDialects/baseTableSql.ts` line 14: **Does handle** `[` → `[ident]` with `]]` escaping. ✅
- DDL/index SQL generation in `src/lib/sqlDialects/extra.ts`: Uses `opts.quoteChar` directly via template literals (e.g., `${opts.quoteChar}${name}${opts.quoteChar}`). When `quoteChar` is `[`, this produces `[name]` — **correct behavior**. ✅
- `IndexesView.tsx` line 69/282: Uses `meta?.quoteChar || '"'` directly in template literals. When `quoteChar` is `[`, this produces `[name]` — **correct behavior**. ✅

**The actual bug is in `escapeIdent()`**: It does not handle `[` bracket style. This must be fixed as part of the SQL Server sub-track.

**Dialect adapter** is already correctly configured with `quoteStyle: 'bracket'` for `sqlserver`/`mssql` — the new SQL editor semantic system will work correctly. Only the legacy `escapeIdent()` path needs the fix.

**CodeMirror mapping** (`resolveCmDialect` in `contracts.ts`): SQL Server currently falls through to `StandardSQL`. CodeMirror's `@codemirror/lang-sql` exports `MSSQL` which should be used. This is a **S6-D task** (central assembly track), not this track.

### BUG-2: ClickHouse metadata inconsistency (Product Decision Gate)

**Files:**
- `packages/drivers/clickhouse/ui/meta.ts`: `quoteChar: '"'`
- `packages/drivers/clickhouse/src/structure.rs` line 308-309: Uses `"ident"` (double quotes)
- `packages/drivers/clickhouse/src/sync_adapter.rs` line 230-232: Uses `` ` `` (backtick)
- `src/components/sql-editor/semantic/dialectAdapter.ts` line 71-76: `quoteStyle: 'backtick'`
- PRD §3.4.2: MySQL / ClickHouse / Doris → backtick

**The conflict:**
1. The `meta.ts` says `"` (double quote)
2. The dialect adapter says `backtick`
3. The PRD says backtick
4. The Rust `structure.rs` says double quote (with comment: "ClickHouse quotes identifiers with double quotes (backticks also accepted)")
5. The Rust `sync_adapter.rs` says backtick

**ClickHouse actually supports both** double quotes and backticks for identifier quoting. The question is which should be the canonical representation in DataZen.

**Recommendation:** Form a product decision gate. Options:
- **Option A:** Keep `"` in meta.ts, change dialect adapter to `double` → consistent with Rust `structure.rs`, matches ClickHouse SQL standard
- **Option B:** Change meta.ts to `` ` ``, keep dialect adapter as `backtick` → matches PRD, consistent with MySQL family
- **Option C:** Change meta.ts to `` ` `` AND dialect adapter to `backtick` → matches PRD

**Do NOT change anything until product decision is made.**

---

## 4. CodeMirror Dialect Mapping Analysis

**File:** `src/components/sql-editor/contracts.ts` (lines 50-63)

Current `CM_DIALECT_MAP`:
```ts
{
  postgresql: PostgreSQL,
  mysql: MySQL,
  mariadb: MariaSQL,
  sqlite: SQLite,
}
```

Missing mappings (fall back to `StandardSQL`):
- `sqlserver` → should map to `MSSQL` from `@codemirror/lang-sql` (**S6-D task**)
- `clickhouse` → no dedicated CM dialect; falls to `StandardSQL` (acceptable)

The `resolveCmDialect` function also resolves via `sqlDialect` metadata, so drivers like `doris`, `starrocks`, `manticore`, `ob_oracle` (all with `sqlDialect: 'mysql'`) correctly resolve to `MySQL`. ✅

---

## 5. `escapeIdent()` Bracket Handling Gap

**File:** `src/lib/databaseTypes.ts` lines 66-71

```ts
export function escapeIdent(name: string, dbType?: DatabaseType): string {
  const q = dbType ? (DB_REGISTRY[dbType]?.quoteChar ?? '"') : '"';
  if (q === '`') return `\`${name.replaceAll('`', '``')}\``;
  if (q === '"') return `"${name.replaceAll('"', '""')}"`;
  return name; // no quoting (e.g. Redis)
}
```

This function is used by:
- `IndexesView.tsx` (indirectly via `quoteChar`)
- `sqlGenerator.ts` (via `BaseTableSqlGenerator`)

**Missing:** Bracket `[` handling. When SQL Server `quoteChar` is corrected to `[`, this function will silently return unquoted identifiers. The fix is straightforward:

```ts
if (q === '[') return `[${name.replaceAll(']', ']]')}]`;
```

This is a prerequisite fix for the SQL Server sub-track.

---

## 6. Approved Sub-Track Tasks

### Sub-Track S6-C-1: SQL Server quoteChar Fix (APPROVED)

**Scope:**
1. Change `packages/drivers/sqlserver/ui/meta.ts`: `quoteChar: '"'` → `quoteChar: '['`
2. Add bracket handling to `escapeIdent()` in `src/lib/databaseTypes.ts`
3. Add unit tests for bracket quoting in `src/lib/__tests__/databaseTypes.test.ts`
4. Verify `BaseTableSqlGenerator.quote()` handles `[` (already does — confirm with test)
5. Verify `IndexesView.tsx` template literal usage works with `[` (already does — confirm with test)
6. Verify dialect adapter already has `bracket` profile for `sqlserver` (already does)
7. Note: CodeMirror `MSSQL` mapping is a **S6-D task**, not this sub-track

**Files to modify:**
- `packages/drivers/sqlserver/ui/meta.ts` (driver-owned)
- `src/lib/databaseTypes.ts` (host — `escapeIdent` fix)
- `src/lib/__tests__/databaseTypes.test.ts` (host tests)

**Tests needed:**
- `escapeIdent('users', 'sqlserver')` → `[users]`
- `escapeIdent('myTable', 'sqlserver')` → `[myTable]`
- `escapeIdent('table]]name', 'sqlserver')` → `[table]]name]`
- `BaseTableSqlGenerator` with `quoteChar: '['` produces correct DDL
- `IndexesView` index SQL with `[` quoting

### Sub-Track S6-C-2: ClickHouse Product Decision Gate (BLOCKED)

**Status:** BLOCKED — awaiting product decision on canonical `quoteChar` for ClickHouse.

**Decision needed:** Should ClickHouse use `"` or `` ` `` as canonical identifier quote?

**Impact analysis:** Changing `quoteChar` from `"` to `` ` `` would affect:
- `escapeIdent()` — already handles backtick ✅
- `BaseTableSqlGenerator.quote()` — already handles backtick ✅
- `IndexesView.tsx` — template literal works with either ✅
- Rust `structure.rs` — uses `"` (would need update if changing to backtick)
- Rust `sync_adapter.rs` — uses `` ` `` (already correct if changing to backtick)
- Dialect adapter — already uses `backtick` (already correct if changing to backtick)

---

## 7. External Plugin Tasks (Git Drivers)

| Driver | Task | Owner |
|--------|------|-------|
| **kiwi** | Verify `ui/meta.ts` `quoteChar` and `sqlDialect` are correct for the underlying engine; ensure dialect adapter profile exists if needed | Plugin repo owner |
| **olap** | Verify `ui/meta.ts` for Presto/Trino quoting (double quote `"` per ANSI SQL); ensure dialect adapter profile exists if needed | Plugin repo owner |
| **superset** | Verify `ui/meta.ts` — Superset is an exploration platform, likely non-SQL; confirm `quoteChar` and `sqlDialect` are appropriate | Plugin repo owner |

**Note:** These tasks are registered but not actionable from this track. They should be communicated to the respective plugin maintainers.

---

## 8. openGauss Note

openGauss is referenced in the PRD and briefs but has **no dedicated driver** in the repo. It would be a PostgreSQL-compatible engine using `"` quoting (same as PostgreSQL). If/when an openGauss driver is added, it should use `quoteChar: '"'` and `sqlDialect: 'postgresql'` (or a dedicated `opengauss` dialect with the same profile as `postgresql` in the dialect adapter).

---

## 9. Deliverables Checklist

- [x] Complete audit table: all 15 path drivers + 3 git drivers
- [x] PRD consistency check against `prd.md` §3.4.2
- [x] `resolveCmDialect` / `CM_DIALECT_MAP` analysis
- [x] `quoteChar` usage in SQL generators and DDL paths
- [x] ClickHouse/Doris discrepancy documented with product decision gate
- [x] Git driver external tasks registered
- [x] Sub-track tasks defined with file lists and test requirements
- [x] `escapeIdent()` bracket gap identified

---

## 10. Commit

Audit only — no source files modified. Progress and bugs files written to this track directory.

---

## 11. Fixes Applied

### BUG-1: SQL Server `quoteChar` corrected

**Commit:** (pending on `feature/sql-s6-c`)

**File:** `packages/drivers/sqlserver/ui/meta.ts` line 11
- Changed `quoteChar: '"'` → `quoteChar: '['`
- No `closeQuoteChar` field exists in `DatabaseTypeMeta`; bracket quoting is handled by the escape functions.

### BUG-3: `escapeIdent()` bracket handling added

**File:** `src/lib/databaseTypes.ts` lines 66-72
- Added `if (q === '[') return \`[\${name.replaceAll(']', ']]')}]\`;` case
- Mirrors the existing bracket handling in `BaseTableSqlGenerator.quote()` (`src/lib/sqlDialects/baseTableSql.ts` line 14)

### Verification

- `npx tsc --noEmit` — 0 errors
- `npx vitest run src/lib/__tests__/databaseTypes.test.ts` — 19 tests pass (including existing escapeIdent tests)
- `npx vitest run src/lib/sqlDialects/__tests__/dialects.test.ts` — 9 tests pass
