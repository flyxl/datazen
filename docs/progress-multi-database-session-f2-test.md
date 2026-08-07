# F2 Test Report — Frontend session multi-DB (MySQL / MariaDB)

| Field | Value |
|-------|-------|
| **Feature** | F2 — Frontend session multi-DB (mysql/mariadb): registry, schemaStore, SchemaTree, QueryPanel, WorkflowPanel + Vitest + E2E spec |
| **Date** | 2026-08-07 |
| **Testers** | test-agent (fresh, testing-only) |
| **Branch** | `feat/multi-database-session-ui` |
| **Scope** | Vitest (store + SchemaTree routing) + static wiring + dedicated MySQL multi-db E2E spec |

## Requirements verified (from progress doc)

| # | Requirement | Static / unit | UI / E2E |
|---|-------------|---------------|----------|
| 1 | `mysql` / `mariadb` `hasMultiDatabase: true` | ✅ | — |
| 2 | `schemaStore.isMultiDatabase = hasMultiDatabase && databases.length > 1` | ✅ | — |
| 3 | `SchemaTree` → `MultiDatabaseSchemaTree` when capability set | ✅ (component) | ✅ |
| 4 | `QueryPanel` / `WorkflowPanel` selectors use session `isMultiDatabase` (length > 1) | ✅ | ✅ (QueryPanel) |
| 5 | IPC `use_database` + called on DB switch / `loadTables` | ✅ | ✅ (expand DB) |
| 6 | Vitest: `databaseTypes` + `schemaStore` + SchemaTree | ✅ | — |

## Test commands run

### 1. Vitest (F2 targets) — PASS

```text
$ npx vitest run \
    src/stores/__tests__/schemaStore.test.ts \
    src/lib/__tests__/databaseTypes.test.ts \
    src/windows/connection/schema-tree/__tests__/SchemaTree.test.tsx

 Test Files  3 passed (3)
      Tests  20 passed (20)
   Duration  1.25s
```

**Result:** 20/20 passed.

### 2. E2E MySQL multi-db — PASS

```text
$ pnpm e2e:skip-build -- --spec e2e/specs/mysql-multi-database.ts

[e2e-runner] Starting app: target/debug/datazen
[e2e-runner] WebDriver plugin is ready on port 4445.

[webkit macos] MySQL 多库会话 UI (F2-E2E)
   ✓ 无默认库连接后侧边栏应列出多个数据库节点 (F2-E2E-001)
   ✓ 展开数据库节点后应加载表 (F2-E2E-002)
   ✓ 多库时 QueryPanel 应显示数据库选择器 (F2-E2E-003)

Spec Files:  1 passed, 1 total (100% completed) in 00:00:15
```

| Detail | Value |
|--------|-------|
| Binary | `target/debug/datazen` (webdriver feature, pre-built) |
| MySQL | `127.0.0.1:3306` reachable; credentials from `e2e/.env` |
| `get_databases` | 6 databases returned |
| `use_database` + `get_tables` | `datazen_test` → 6 tables |
| Duration | ~10.5s spec / ~21s total |

## Static spot-check (wired paths)

| Check | Location | Verdict |
|-------|----------|---------|
| `hasMultiDatabase: true` for mysql/mariadb | `src/lib/databaseTypes.ts` | **OK** |
| `postgresql` has no multi-db flag | `databaseTypes.ts` + Vitest | **OK** |
| `computeIsMultiDatabase` / preferred DB | `schemaStore.ts` | **OK** |
| `loadTables` → `useDatabase` then `getTables` | `schemaStore.ts` | **OK** |
| IPC `use_database` | Rust + `databaseCommands` | **OK** |
| `SchemaTree` routes by `hasMultiDatabase` | `SchemaTree.tsx` + component tests | **OK** |
| `QueryPanel` selector gated on `isMultiDatabase` | `QueryPanel.tsx` | **OK** |
| `WorkflowPanel` selector: capability && length > 1 | `WorkflowPanel.tsx` | **OK** (not E2E-covered) |

## Full case table

| ID | Scenario | Result |
|----|----------|--------|
| F2-UT-001~007 | databaseTypes + schemaStore | **Pass** |
| F2-UT-008 | SchemaTree mysql length > 1 | **Pass** |
| F2-UT-009 | SchemaTree mysql length === 1 | **Pass** |
| F2-UT-010 | SchemaTree postgresql → Standard | **Pass** |
| F2-UT-011 | Expand mysql DB loads tables via useDatabase | **Pass** |
| F2-STATIC-001~005 | Registry / routing / selectors / IPC | **Pass** |
| F2-E2E-001 | Connect no default DB → ≥2 DB nodes | **Pass** |
| F2-E2E-002 | Expand DB → tables / empty state | **Pass** |
| F2-E2E-003 | QueryPanel DB selector when length > 1 | **Pass** |
| F2-UI-004~006 | Switch DB SQL / preferred DB / pg+sqlite | Still manual / future |

## Bugs found

**None.**

## Overall verdict

**PASS**

- 20/20 Vitest green (store + registry + SchemaTree routing).
- 3/3 E2E green (F2-E2E-001~003) against local MySQL via webdriver debug binary.
- WorkflowPanel multi-db selector and MariaDB-specific UI not separately E2E-covered (same code paths as mysql).
