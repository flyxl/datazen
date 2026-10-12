# Schema Diff Deploy User Guide

> Treat **source = desired schema**, generate a reviewable DDL plan, then deploy to the target under explicit safety gates.  
> Open via the **Schema Diff** menu (or related shortcuts on the main window).

---

## 1. Overview

| Step | What it does |
|------|----------------|
| **Compare** | Pick source/target connections and tables; inspect column/index diffs |
| **Plan** | Generate deploy SQL (additive-only by default) |
| **Review / Deploy** | Confirm transaction options and type `DEPLOY` when needed, then run on the target |

**Direction:** source = desired schema; target = apply site.

**Diff labels:** missing on target (ADD) · extra on target (DROP, needs allowDestructive) · changed columns.

---

## 2. UI layout

Schema Diff is a **five-step wizard** (same stepper chrome as Data Transfer):

```text
┌ TitleBar (title + help) ────────────────────────────────┐
├ Stepper: Endpoints → Objects → Compare → Plan → Deploy ┤
├ ① Endpoints: source/target (connection · database · schema) ┤
├ ② Objects: multi-select tables from source (select/clear all) ┤
├ ③ Compare: left table list │ column-level diff details ┤
├ ④ Plan: options + SQL list + export/import config       ┤
├ ⑤ Deploy: transaction options · DEPLOY confirm · result ┤
└ StatusBar ──────────────────────────────────────────────┘
```

- First open shows a **limitations** dialog (optional “don’t show again”).
- **Endpoints** uses a dedicated session to fetch the database list; PostgreSQL and similar list schemas (**Swap** not enabled yet).
- **Objects** loads the table list from the source catalog; check the tables to compare.
- **Plan** step offers `allowDestructive` / `includeIndexes` toggles plus SQL and risk badges.
- **Deploy** step: `DEPLOY` token, transaction options, and deploy status.

Code: `src/windows/schema-diff/` (`SchemaDiffWindow`, `SchemaDiffObjectsStep`, `SchemaDiffTableListPanel`, `SchemaDiffRightPanel`).

---

## 3. Quick start

1. Open **Schema Diff** (dismiss limitations dialog if shown)  
2. **Endpoints:** pick source/target connections and databases (must differ; pick a schema for PostgreSQL-style targets) → **Next**  
3. **Objects:** select tables → **Next** (compare runs automatically)  
4. **Compare:** left table list shows change badges; middle panel lists missing/extra/changed columns → **Next**  
5. **Plan:** click **Generate deploy script** first, then review SQL + risk badges (`additive` / `destructive` / `rewrite`)
6. **Deploy:** set options, type **`DEPLOY`** if required, click **Deploy to target**  
7. Read status (`committed` / `rolled_back` / `mixed` / `failed`)

You can **Copy SQL** or export/import config from the Plan step without deploying.

Note: the compare list and the Plan step are not the same state object. Entering Plan does not generate a plan yet — the page prompts **Generate deploy script** first; only after clicking it are both schemas re-read and DDL generated.

---

## 4. Safety defaults

- Default plan is **additive-only** (ADD COLUMN, widen nullability, CREATE INDEX, …)  
- Enable **Allow destructive** for DROP COLUMN/INDEX, narrowing ALTERs, SET NOT NULL, …  
- Any `destructive` or `rewrite` statement requires typing **`DEPLOY`** before run  
- Optional **Require complete rollback SQL** blocks deploy when any statement lacks `rollbackSql`  

For production: copy SQL out for human review and take a backup first.

---

## 5. Transactions & atomicity

| Dialect | DDL atomicity | Mid-failure status |
|---------|---------------|--------------------|
| PostgreSQL | Transactional | Usually `rolled_back` |
| SQLite | Transactional (limited ALTER) | Usually `rolled_back` |
| MySQL / MariaDB | Auto-commit per statement | `mixed` if some succeeded (never pretends a full rollback) |
| Other | Treated like auto-commit | `mixed` / `failed` |

The transaction checkbox is disabled when the target dialect does not support transactional DDL.

---

## 6. Multi-table & indexes

- Multiple tables are planned in one batch, concatenating column-level and index statements per table  
- Uncheck **Include indexes** to plan column changes only  
- DROP INDEX often has incomplete rollback → listed under rollback incompleteness  

Primary-key structure changes are **not** auto-planned; follow warnings and apply manually.

---

## 7. Cross-dialect

When dialects differ, types are mapped through the **sync IR** (`column_to_ir` → `ir_type_to_native`).

- Success → native ADD/MODIFY on the target dialect  
- Failure → plan **warning** and skip (no silent wrong SQL)  

SQLite remains ADD COLUMN / index oriented; complex DROP/MODIFY emits unsupported warnings.

---

## 8. Config JSON

Export / import clipboard JSON (config IDs only — no secrets):

```json
{
  "version": 2,
  "sourceConnectionId": "...",
  "targetConnectionId": "...",
  "tables": ["users", "orders"],
  "allowDestructive": false,
  "includeIndexes": true,
  "requireRollback": false
}
```

Import lands on **Objects** with table picks restored; **database / schema must be re-selected** (v2 does not persist them yet). Connections must already exist locally.

---

## 9. Relation to Data Sync / Data Transfer

| Product | What it does | User guide |
|---|---|---|
| **Schema Diff (this guide)** | Structural align + gated DDL deploy, no row sync | This guide |
| **Data Synchronization** | Identical schema + same PK → row diff → review → Execute | [data-sync-guide.md](./data-sync-guide.zh-CN.md) |
| **Data Transfer** | Heterogeneous / different schema / no PK → one-way move (V1 basics) | [data-transfer-guide.md](./data-transfer-guide.zh-CN.md) |

When schemas differ, do not use Data Sync to “sync only some columns”; align DDL with **Schema Diff** first, or switch to **Data Transfer**.

Cross-database CREATE TABLE DDL belongs to Transfer / adapter IR; the Deploy path is ALTERs on existing tables — do not mix it with Sync Change Sets.

---

## 10. Out of scope

- Views / functions / triggers / procedures  
- Online schema change (pt-osc / gh-ost)  
- Rename detection by similarity  
- Automatic backup before deploy  
- One-click MCP deploy (may wrap later as a high-risk tool)  

---

## 11. Troubleshooting

| Symptom | Likely cause |
|---------|----------------|
| Plan not generated yet | **Generate deploy script** was never clicked — absence of a plan means nothing yet |
| Plan empty, no warnings | With the current plan options there really is nothing executable |
| Plan empty but warnings present | Some diffs were skipped as destructive or unsupported cross-dialect |
| Compare shows ADD but plan still empty | Confirm **Generate deploy script** was clicked and neither schema changed since; if it still reproduces on the same snapshot with no warnings, treat as a bug and file repro info |
| Deploy rejected | Missing `DEPLOY`, or incomplete rollback while required |
| MySQL `mixed` | Earlier DDL already committed |
| Missing cross-dialect stmts | Type map failed — check warnings |
| Many SQLite warnings | Limited ALTER — expected |

Architecture notes: `docs/architecture/backend/schema-diff.md`.

Background jobs are independent of wizard state. Reopening the window shows job history and recovery evidence in the status panel. Historical plans do not overwrite a new endpoint scope or skip comparison and review.
