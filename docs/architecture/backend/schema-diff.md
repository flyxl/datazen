# Schema Diff Deploy

Module: `src-tauri/src/schema_diff/` · UI: `src/windows/schema-diff/`

Turns read-only schema comparison into **source = desired → DDL plan → review → deploy on target**.

## Backend layout

| File | Role |
|------|------|
| `compare.rs` | Column/index diff (`missing_on_target` / `extra_on_target`) |
| `plan.rs` | Dialect-aware ALTER / index plan + rollback completeness |
| `dialects/*` | PG / MySQL / SQLite SQL fragments |
| `deploy.rs` | Execute with transactional vs auto-commit status; PG uses `begin_transaction` API |
| `types.rs` | Plan / result DTOs |

IPC: `commands/schema_diff.rs` → `compare_table_schemas`, `prepare_schema_diff_plan`, `execute_schema_diff_deploy`.

## Frontend layout

| Component | Role |
|-----------|------|
| `SchemaDiffWindow.tsx` | 5 步向导 shell；compare / plan / deploy 编排 |
| `useSchemaDiffEndpoints.ts` + `MigrationEndpointsBar` | Connection, database, schema; dedicated sessions |
| `SchemaDiffObjectsStep.tsx` | Objects 步表多选 |
| `SchemaDiffTableListPanel.tsx` | Compare 步左侧表列表 + diff badges |
| `SchemaDiffRightPanel.tsx` | Plan / Deploy 步 SQL 与部署面板 |
| `SchemaDiffLimitationsDialog.tsx` | First-run capability limits |

## Sync IR link

Same-dialect plans use native type strings from `TableSchema`.

Cross-dialect compare/plan uses `SyncAdapterRegistry`:

`SyncSourceAdapter::column_to_ir` → `SyncTargetAdapter::ir_type_to_native`

Compare IPC may use `diff_table_schemas_ir` (same IR normalization as Sync). Unsupported mappings become plan **warnings** (statement skipped).

**Note:** IR compare must check target column presence via `tgt_map` (not `src_map`).

Deploy on PostgreSQL wraps statements in a driver-held transaction (`deploy.rs` → `begin_transaction` / `commit` / `rollback`), not pool `execute("BEGIN")`.

## User docs

- [schema-diff-guide.zh-CN.md](../../features/schema-diff-guide.zh-CN.md)
- [schema-diff-deploy.md](../../features/schema-diff-deploy.md)
- [migration-tools-review-v0.1.0.md](../../development/migration-tools-review-v0.1.0.md)
- [migration-tools-backlog.md](../../todo/migration-tools-backlog.md)

## Tests

- Host: `npx vitest run src/windows/schema-diff`
- E2E: `pnpm e2e:schema-diff` — see [e2e-coverage.md](../../development/e2e-coverage.md)
