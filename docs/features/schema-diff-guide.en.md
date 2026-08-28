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

---

## 2. UI layout

**Five-step wizard** (same stepper chrome as Data Transfer):

1. **Endpoints** — source/target connection, database, schema (when supported)  
2. **Objects** — multi-select tables loaded from the source catalog  
3. **Compare** — table list + column-level diff panel  
4. **Plan** — options, SQL list, export/import config  
5. **Deploy** — transaction options, `DEPLOY` token, deploy result  

First open shows a **limitations** dialog (optional “don’t show again”).  
**Swap** on Endpoints is not enabled yet (see [backlog](../todo/migration-tools-backlog.md)).  
Code: `src/windows/schema-diff/`.

---

## 3. Quick start

1. Open **Schema Diff** (dismiss limitations dialog if shown)  
2. **Endpoints:** pick source/target connections and databases → **Next**  
3. **Objects:** select tables → **Next** (compare runs automatically)  
4. **Compare:** review diffs → **Next**  
5. **Plan:** click **Generate deploy script** and review SQL + risk badges  
6. **Deploy:** set options, type **`DEPLOY`** if required, click **Deploy to target**  
7. Read status (`committed` / `rolled_back` / `mixed` / `failed`)

You can **Copy SQL** or export/import config from the Plan step without deploying.

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

- Multiple tables are planned in one batch  
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

Import lands on **Objects** with table picks restored; **database / schema must be re-selected** (v2 does not persist them yet — see [backlog](../todo/migration-tools-backlog.md)). Connections must already exist locally.

---

## 9. Relation to Data Sync / Data Transfer

- **Data Sync**: compare + row copy; schema diff view shares source=desired semantics  
- **Schema Diff**: structural align + gated DDL deploy (no row sync)  

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
| Empty plan | Already identical, or destructive diffs skipped by default |
| Deploy rejected | Missing `DEPLOY`, or incomplete rollback while required |
| MySQL `mixed` | Earlier DDL already committed |
| Missing cross-dialect stmts | Type map failed — check warnings |
| Many SQLite warnings | Limited ALTER — expected |

Architecture notes: `docs/architecture/backend/schema-diff.md`.
