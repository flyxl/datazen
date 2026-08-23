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

**Direction (same as DBX)**

| Side | Meaning |
|------|---------|
| Source | Desired schema |
| Target | Apply site |

Labels:

- **Missing on target (ADD)** — on source only → usually `ADD COLUMN`
- **Extra on target (DROP)** — on target only → requires **Allow destructive**
- **Changed** — type / nullability / PK flag differ

---

## 2. Quick start

1. Open the **Schema Diff** window  
2. Choose **source** and **target** (must differ)  
3. Enter one or more table names (one per line or comma-separated)  
4. Click **Compare**  
5. Click **Generate deploy script**  
6. Review each statement and its risk badge (`additive` / `destructive` / `rewrite`)  
7. Open **Review / Deploy**; set transaction / rollback-completeness options  
8. If the plan is destructive or rewrite-heavy, type **`DEPLOY`**, then **Deploy to target**  
9. Read status: `committed` / `rolled_back` / `mixed` / `failed`

You can **Copy SQL** or **Copy summary** at any time without deploying.

---

## 3. Safety defaults

- Default plan is **additive-only** (ADD COLUMN, widen nullability, CREATE INDEX, …)  
- Enable **Allow destructive** for DROP COLUMN/INDEX, narrowing ALTERs, SET NOT NULL, …  
- Any `destructive` or `rewrite` statement requires typing **`DEPLOY`** before run  
- Optional **Require complete rollback SQL** blocks deploy when any statement lacks `rollbackSql`  

For production: copy SQL out for human review and take a backup first.

---

## 4. Transactions & atomicity

| Dialect | DDL atomicity | Mid-failure status |
|---------|---------------|--------------------|
| PostgreSQL | Transactional | Usually `rolled_back` |
| SQLite | Transactional (limited ALTER) | Usually `rolled_back` |
| MySQL / MariaDB | Auto-commit per statement | `mixed` if some succeeded (never pretends a full rollback) |
| Other | Treated like auto-commit | `mixed` / `failed` |

The transaction checkbox is disabled when the target dialect does not support transactional DDL.

---

## 5. Multi-table & indexes

- Multiple tables are planned in one batch  
- Uncheck **Include indexes** to plan column changes only  
- DROP INDEX often has incomplete rollback → listed under rollback incompleteness  

Primary-key structure changes are **not** auto-planned; follow warnings and apply manually.

---

## 6. Cross-dialect

When dialects differ, types are mapped through the **sync IR** (`column_to_ir` → `ir_type_to_native`).

- Success → native ADD/MODIFY on the target dialect  
- Failure → plan **warning** and skip (no silent wrong SQL)  

SQLite remains ADD COLUMN / index oriented; complex DROP/MODIFY emits unsupported warnings.

---

## 7. Config JSON

Export / import clipboard JSON (config IDs only — no secrets):

```json
{
  "version": 1,
  "sourceConfigId": "...",
  "targetConfigId": "...",
  "tables": ["users", "orders"],
  "allowDestructive": false,
  "includeIndexes": true,
  "requireRollback": false
}
```

After import, run Compare / Generate again; connections must already exist locally.

---

## 8. Relation to Data Sync

- **Data Sync**: compare + row copy; schema diff view shares source=desired semantics  
- **Schema Diff**: structural align + gated DDL deploy (no row sync)  

---

## 9. Out of scope

- Views / functions / triggers / procedures  
- Online schema change (pt-osc / gh-ost)  
- Rename detection by similarity  
- Automatic backup before deploy  
- One-click MCP deploy (may wrap later as a high-risk tool)  

---

## 10. Troubleshooting

| Symptom | Likely cause |
|---------|----------------|
| Empty plan | Already identical, or destructive diffs skipped by default |
| Deploy rejected | Missing `DEPLOY`, or incomplete rollback while required |
| MySQL `mixed` | Earlier DDL already committed |
| Missing cross-dialect stmts | Type map failed — check warnings |
| Many SQLite warnings | Limited ALTER — expected |

Architecture notes: `docs/architecture/backend/schema-diff.md`.
