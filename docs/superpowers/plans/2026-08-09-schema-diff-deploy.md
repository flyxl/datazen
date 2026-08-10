# Schema Diff Deploy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn DataZen’s read-only schema comparison into a controlled “source = desired → generate DDL → review → deploy on target” pipeline, with honest transaction/rollback semantics.

**Architecture:** Keep `compare_table_schemas` as the diff engine; add a dialect-aware **DDL plan generator** (reuse sync IR + `SyncTargetAdapter` where possible); add a **deploy executor** that runs statement batches on the target connection and returns structured status (`committed` / `rolled_back` / `mixed` / `unsupported_atomic`). UI extends the existing `SchemaDiffWindow` with Plan → Review → Deploy steps. Never auto-run DROP without an explicit opt-in.

**Tech Stack:** Rust (`src-tauri/src/schema_diff/` new module, `sync/ir` + adapters), Tauri IPC, React `SchemaDiffWindow`, Vitest + `cargo test -p datazen --lib`.

## Global Constraints

- Direction is always **source = desired state, target = apply site** (same as DBX docs).
- Deploy requires an explicit confirmation string typed by the user for any statement containing `DROP` / `TRUNCATE` / type-narrowing ALTERs.
- Empty allowlist-style safety: default deploy mode is **additive-only** (ADD COLUMN / widen nullable); destructive ops gated by checkbox `allowDestructive`.
- Same-dialect first (P1); cross-dialect only via existing sync IR adapters (P3).
- Do **not** invent fake 2PC; classify atomicity by dialect capability + SQL risk (mirror DBX lessons).
- Frontend IPC args use `snake_case` keys.
- No JDBC path; path drivers only (postgres / mysql / sqlite first).

## Approaches considered

| Approach | Idea | Pros | Cons |
|----------|------|------|------|
| A. String templates from column diff | Format `ALTER` from `added/removed/changed` | Fast | Duplicates sync IR; weak cross-type; hard to extend indexes |
| **B. IR + adapter DDL plan (recommended)** | Diff → IR ops → `SyncTargetAdapter` renders native SQL → execute | Reuses `sync/`; testable; natural P3 cross-dialect | Needs ALTER builders beyond CREATE TABLE |
| C. Full DBX-parity object graph | Tables/views/procs/rename detection/impact report | Market parity | Multi-month; wrong next slice |

**Recommendation:** Approach B, delivered in three phases below. P1 is the first mergeable slice.

---

## Phase overview

| Phase | Scope | Exit criteria |
|-------|--------|---------------|
| **P1** | Same-dialect, single table, column-level plan + deploy | PG↔PG and MySQL↔MySQL: ADD/DROP/ALTER COLUMN; review UI; structured deploy result |
| **P2** | Multi-table selection, indexes/PK, tx classification, rollback completeness gate | Deploy step shows statement progress; incomplete rollback blocks Run |
| **P3** | Cross-dialect via IR rewrite; views (read-only compare first); saved diff configs | PG→MySQL column add works through IR; config JSON import/export |

---

### Task 1: Fix diff direction semantics + shared types

**Files:**
- Modify: `src-tauri/src/commands/sync.rs` (`diff_table_schemas` naming / docs)
- Modify: `src/types/index.ts` (`TableSchemaDiff`)
- Modify: `src/components/schema/SchemaDiffPanel.tsx`
- Create: `src-tauri/src/schema_diff/mod.rs`, `types.rs`
- Test: `src-tauri/src/schema_diff/types.rs` (unit) or sync.rs tests

**Interfaces:**
- Produces:
  ```rust
  pub struct ColumnPlanOp {
      pub kind: ColumnOpKind, // Add | Drop | AlterType | SetNullability
      pub column: String,
      pub detail: String,
  }
  pub struct SchemaDiffPlan {
      pub table: String,
      pub source_dialect: String,
      pub target_dialect: String,
      pub statements: Vec<PlanStatement>, // { sql, risk: Additive|Destructive|Rewrite, rollback_sql: Option<String> }
      pub warnings: Vec<String>,
  }
  pub enum DeployStatus { Committed, RolledBack, Mixed, Failed }
  pub struct SchemaDiffDeployResult {
      pub status: DeployStatus,
      pub executed_count: usize,
      pub statement_count: usize,
      pub errors: Vec<String>,
  }
  ```
- Clarifies UI labels: **missing_on_target** (need ADD), **extra_on_target** (need DROP), **changed**.

- [ ] **Step 1: Write failing tests for direction**

```rust
#[test]
fn source_column_missing_on_target_is_add() {
    // src has "email", tgt does not → plan op Add
}
#[test]
fn target_only_column_is_drop() {
    // tgt has "legacy", src does not → plan op Drop
}
```

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cargo test -p datazen --lib schema_diff:: -- --nocapture`

- [ ] **Step 3: Introduce `schema_diff` module; re-export from sync or move `diff_table_schemas`**

Keep IPC `compare_table_schemas` response backward compatible by mapping:
- `added` → columns missing on target (ops to ADD) **OR** add new fields `missingOnTarget` / `extraOnTarget` and deprecate confusing names in UI only.

**Decision (lock):** Prefer **new fields** `missingOnTarget` / `extraOnTarget` / `changed` in JSON; keep old keys as aliases for one release to avoid breaking Data Sync UI.

- [ ] **Step 4: Tests PASS; commit**

```bash
git add src-tauri/src/schema_diff src-tauri/src/commands/sync.rs src/types/index.ts src/components/schema
git commit -m "feat(schema-diff): clarify source-desired column diff semantics"
```

---

### Task 2: P1 DDL plan generator (same dialect)

**Files:**
- Create: `src-tauri/src/schema_diff/plan.rs`
- Create: `src-tauri/src/schema_diff/dialects/postgres.rs`
- Create: `src-tauri/src/schema_diff/dialects/mysql.rs`
- Create: `src-tauri/src/schema_diff/dialects/sqlite.rs`
- Modify: `src-tauri/src/schema_diff/mod.rs`
- Test: unit tests per dialect in `plan.rs` / dialect modules

**Interfaces:**
- Consumes: `TableSchema` src/tgt, dialect ids
- Produces:
  ```rust
  pub fn build_column_plan(
      table: &str,
      src: &TableSchema,
      tgt: &TableSchema,
      dialect: &str,
  ) -> Result<SchemaDiffPlan, String>;
  ```

- [ ] **Step 1: Failing tests — expected SQL strings**

```rust
#[test]
fn postgres_add_varchar_column() {
    let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
    assert!(plan.statements.iter().any(|s|
        s.sql.contains("ADD COLUMN") && s.sql.contains("email")
    ));
}

#[test]
fn mysql_drop_requires_destructive_flag_in_metadata() {
    let plan = build_column_plan("users", &src, &tgt, "mysql").unwrap();
    let drop = plan.statements.iter().find(|s| s.sql.contains("DROP COLUMN")).unwrap();
    assert_eq!(drop.risk, StatementRisk::Destructive);
}
```

- [ ] **Step 2: Implement renderers**

Postgres examples:
- ADD: `ALTER TABLE {q} ADD COLUMN {col} {type} [NOT NULL]`
- DROP: `ALTER TABLE {q} DROP COLUMN {col}`
- TYPE: `ALTER TABLE {q} ALTER COLUMN {col} TYPE {type}` (warn + risk Rewrite)
- NULL: `ALTER TABLE {q} ALTER COLUMN {col} SET/DROP NOT NULL`

MySQL:
- ADD/DROP COLUMN; MODIFY COLUMN for type/null

SQLite:
- ADD COLUMN only for P1; DROP/MODIFY → `warnings` + empty statements or explicit unsupported error

- [ ] **Step 3: Wire IPC**

```rust
#[tauri::command]
pub async fn prepare_schema_diff_plan(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
    allow_destructive: bool,
) -> Result<SchemaDiffPlan, CommandError>
```

When `allow_destructive == false`, filter out Drop / narrowing alters and push warnings.

- [ ] **Step 4: Register command in `lib.rs` + frontend `src/commands/sync.ts` (or new `schemaDiff.ts`)**

- [ ] **Step 5: `cargo test -p datazen --lib schema_diff::` PASS; commit**

```bash
git commit -m "feat(schema-diff): generate same-dialect column ALTER plans"
```

---

### Task 3: P1 Deploy executor

**Files:**
- Create: `src-tauri/src/schema_diff/deploy.rs`
- Modify: driver usage via `ConnectionManager` + `DatabaseDriver::execute` / `query_multi`
- Test: unit tests with a fake sequential executor (inject trait)

**Interfaces:**
- Consumes: `SchemaDiffPlan`, target `connection_id`, `use_transaction: bool`
- Produces: `SchemaDiffDeployResult`

```rust
pub async fn execute_schema_diff_deploy(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    plan: &SchemaDiffPlan,
    opts: DeployOptions, // use_transaction, stop_on_error
) -> SchemaDiffDeployResult
```

- [ ] **Step 1: Failing tests for status classification**

```rust
#[test]
fn all_ok_committed() { /* mock execute all Ok → Committed, executed == n */ }

#[test]
fn mid_fail_without_tx_is_mixed() { /* 2 ok, 1 err → Mixed, executed_count=2 */ }

#[test]
fn postgres_tx_rollback_on_fail() {
    // when dialect.supports_transactional_ddl && use_transaction
    // fail on stmt 2 → RolledBack, executed_count=0
}
```

- [ ] **Step 2: Capability table**

```rust
fn ddl_atomicity(dialect: &str) -> DdlAtomicity {
    match dialect {
        "postgresql" => DdlAtomicity::Transactional,
        "mysql" | "mariadb" => DdlAtomicity::AutoCommitPerStatement,
        "sqlite" => DdlAtomicity::Transactional, // limited ops
        _ => DdlAtomicity::Unknown,
    }
}
```

- [ ] **Step 3: Implementation rules**
  - If `Transactional` && `use_transaction`: `BEGIN` → run statements → `COMMIT` / `ROLLBACK` on error.
  - If `AutoCommitPerStatement`: never claim `RolledBack` for prior statements; return `Mixed` on partial failure.
  - Always return per-statement log in `errors` / optional `statement_results` for UI.

- [ ] **Step 4: IPC**

```rust
#[tauri::command]
pub async fn execute_schema_diff_deploy(
    state: State<'_, AppState>,
    target_connection_id: String,
    plan: SchemaDiffPlan,
    use_transaction: bool,
    confirm_destructive: Option<String>, // must equal "DEPLOY" if any Destructive
) -> Result<SchemaDiffDeployResult, CommandError>
```

Reject if destructive statements present and confirm ≠ `"DEPLOY"`.

- [ ] **Step 5: Tests PASS; commit**

```bash
git commit -m "feat(schema-diff): execute deploy plans with dialect atomicity"
```

---

### Task 4: P1 UI — Plan / Review / Deploy in SchemaDiffWindow

**Files:**
- Modify: `src/windows/schema-diff/SchemaDiffWindow.tsx`
- Create: `src/windows/schema-diff/SchemaDiffPlanPanel.tsx`
- Create: `src/windows/schema-diff/SchemaDiffDeployPanel.tsx`
- Modify: `src/commands/schemaDiff.ts` (new)
- Modify: locales `en.ts` / `zh-CN.ts` (+ sync other locales keys)
- Test: `src/lib/__tests__` or component test for confirm gate

**Interfaces:**
- Consumes: `prepare_schema_diff_plan`, `execute_schema_diff_deploy`
- Wizard steps: `compare` → `plan` → `review` → `result`

- [ ] **Step 1: After Compare, button「生成部署脚本」calls prepare**
- [ ] **Step 2: Show statement list with risk badges; toggle allowDestructive regenerates plan**
- [ ] **Step 3: Review: target host/db/table, statement count, use_transaction checkbox (disabled if dialect AutoCommit)**
- [ ] **Step 4: Deploy: if destructive, require typing `DEPLOY`; then invoke execute; show status + executed_count**
- [ ] **Step 5: Copy SQL / Export `.sql` always available without deploy**
- [ ] **Step 6: Manual smoke: two local PG databases, add column on source, deploy to target**

```bash
npx vitest run src/lib/__tests__/schemaDiffConfirm.test.ts
git commit -m "feat(schema-diff): plan/review/deploy UI for column DDL"
```

---

### Task 5: P2 — Multi-table + indexes/PK + rollback gate

**Files:**
- Modify: `prepare_schema_diff_plan` to accept `table_names: Vec<String>`
- Extend plan ops: `CreateIndex`, `DropIndex`, `AddPrimaryKey` (only when metadata available from `TableSchema.indexes` if present — extend `TableSchema` if needed)
- Modify deploy to attach `rollback_sql` per statement where reversible
- UI: table multi-select from `compare_databases` list
- Block Deploy button when any selected statement has `rollback_sql == None` && user enabled「Require rollback」

- [ ] **Step 1: Extend `get_table_schema` consumers — if indexes missing, skip index ops with warning**
- [ ] **Step 2: Multi-table plan concatenates statements with dependency order (base tables only in P2)**
- [ ] **Step 3: Frontend gate + i18n**
- [ ] **Step 4: Tests for rollback completeness struct**

```rust
pub struct RollbackCompleteness { pub complete: bool, pub missing: Vec<String> }
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(schema-diff): multi-table plans, indexes, rollback gate"
```

---

### Task 6: P3 — Cross-dialect via sync IR

**Files:**
- Modify: `schema_diff/plan.rs` to path through `table_to_ir` + `ir_type_to_native` when `src_dialect != tgt_dialect`
- Reuse `state.sync_adapters.ensure_pair`
- Unsupported type mappings → warning + skip statement (no silent wrong SQL)
- Optional: save/load diff config JSON (`sourceConfigId`, `targetConfigId`, `tables`, `allowDestructive`)

- [ ] **Step 1: Unit test PG→MySQL ADD COLUMN type rewrite**
- [ ] **Step 2: Implement cross-dialect plan branch**
- [ ] **Step 3: Config JSON in SchemaDiffWindow**
- [ ] **Step 4: Update `docs/competitive-comparison-dbx.md` Schema row to mention deploy P1–P3**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(schema-diff): cross-dialect deploy plans via sync IR"
```

---

### Task 7: Hardening & docs

**Files:**
- Create: `docs/schema-diff-deploy.md` (user-facing)
- Modify: `docs/architecture/backend/` (short module note)
- E2E (optional): `e2e/specs/schema-diff-deploy.ts` against docker PG if available

- [ ] Document atomicity matrix (PG / MySQL / SQLite)
- [ ] Document confirm token `DEPLOY` and additive-default
- [ ] Add architecture note linking sync IR ↔ schema_diff
- [ ] Commit

```bash
git commit -m "docs: schema diff deploy user guide and atomicity matrix"
```

---

## Out of scope (explicit)

- View/function/trigger/procedure sync (after P3 compare-only may land separately)
- Online schema change (pt-osc / gh-ost)
- Rename detection by similarity
- Deploy via MCP tools (may wrap later as `run_schema_diff_deploy` under high_risk_write)
- Automatic production backups before deploy

## Risk register

| Risk | Mitigation |
|------|------------|
| MySQL DDL auto-commit | Never report RolledBack for prior stmts; UI copy explains |
| Wrong DROP direction | Source-desired semantics tests + additive default |
| Type ALTER data loss | Mark Rewrite risk; require DEPLOY confirm |
| SQLite limited ALTER | Warnings; only ADD in P1 |

## Suggested schedule

- P1 (Tasks 1–4): ~1.5–2 weeks
- P2 (Task 5): ~1–1.5 weeks
- P3 (Tasks 6–7): ~1.5–2 weeks

---

## Approval gate

Do not start coding until product picks the first mergeable phase:

1. **P1 only** (recommended)
2. **P1+P2**
3. **Full P1–P3** as one epic with checkpoints after each phase
