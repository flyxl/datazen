# Schema Diff Deploy

Module: `src-tauri/src/schema_diff/`

Turns read-only schema comparison into **source = desired → DDL plan → review → deploy on target**.

## Layout

| File | Role |
|------|------|
| `compare.rs` | Column/index diff (`missing_on_target` / `extra_on_target`) |
| `plan.rs` | Dialect-aware ALTER / index plan + rollback completeness |
| `dialects/*` | PG / MySQL / SQLite SQL fragments |
| `deploy.rs` | Execute with transactional vs auto-commit status |
| `types.rs` | Plan / result DTOs |

IPC: `commands/schema_diff.rs` → `prepare_schema_diff_plan`, `execute_schema_diff_deploy`.

## Sync IR link

Same-dialect plans use native type strings from `TableSchema`.

Cross-dialect plans call `SyncAdapterRegistry::ensure_pair`, then:

`SyncSourceAdapter::column_to_ir` → `SyncTargetAdapter::ir_type_to_native`

Unsupported mappings become plan **warnings** (statement skipped).

Compare-only DDL previews in Data Sync still use `transfer/ddl.rs` `build_create_table_ddl`.

## User docs

See [docs/schema-diff-deploy.md](../../schema-diff-deploy.md).
