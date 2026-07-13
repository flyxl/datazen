# Architecture Refactor — Test Baseline

**Date:** 2026-07-13  
**Branch:** `refactor/architecture-extensibility`  
**Plan:** [2026-07-13-architecture-refactor.md](./2026-07-13-architecture-refactor.md)

## Rust Tests (`cargo test --manifest-path src-tauri/Cargo.toml`)

```
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured
```

Tests:
- `services::query_executor::tests::no_explicit_sort_composite_pk_uses_all_pk_columns`
- `services::query_executor::tests::explicit_sort_overrides_default_pk_order`
- `services::query_executor::tests::no_explicit_sort_uses_primary_key_order`
- `services::query_executor::tests::no_pk_no_explicit_sort_still_has_order_by_first_column`

## Frontend Unit Tests (`npx vitest run`)

```
Test Files  11 passed (11)
     Tests  88 passed (88)
```

Note: `npm test` script is not defined; use `npx vitest run`.

## E2E Specs by Database Type

| Database | Spec File | Coverage |
|----------|-----------|----------|
| PostgreSQL | (implicit via core/db specs) | connection-window, sql-query, table-data, table-edit, table-structure, export-import, data-types |
| MySQL | `e2e/specs/mysql.ts` | MySQL-specific connection & queries |
| SQLite | `e2e/specs/sqlite.ts` | File-mode connection |
| Redis | `e2e/specs/redis.ts` | Key-value view, scan keys |
| Kiwi | `e2e/specs/kiwi.ts` | Multi-db tree, OAuth login, 1000-row pagination |

### Full E2E Spec List (22 files)

```
e2e/specs/sqlite.ts
e2e/specs/homepage-features.ts
e2e/specs/sql-query.ts
e2e/specs/kiwi.ts
e2e/specs/detail-panel.ts
e2e/specs/table-structure.ts
e2e/specs/table-data.ts
e2e/specs/connection-window.ts
e2e/specs/table-edit.ts
e2e/specs/export-import.ts
e2e/specs/main-window.ts
e2e/specs/backup-database.ts
e2e/specs/data-sync-real.ts
e2e/specs/redis.ts
e2e/specs/drag-drop-groups.ts
e2e/specs/i18n-menu.ts
e2e/specs/settings.ts
e2e/specs/connection-search-group.ts
e2e/specs/new-connection.ts
e2e/specs/edit-delete-connection.ts
e2e/specs/mysql.ts
e2e/specs/data-types.ts
```

### E2E Run Commands

```bash
npm run e2e:core    # UI/core features
npm run e2e:db      # SQL database features
npm run e2e:kiwi    # Kiwi-specific
# Redis/SQLite: included in full e2e or individual --spec
```

## Regression Gate (per Phase)

After each Phase, run:
1. `cargo test --manifest-path src-tauri/Cargo.toml`
2. `npx vitest run`
3. Relevant e2e specs for touched areas
