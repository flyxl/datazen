# Schema 基础设施整合实施计划

> PRD: `docs/todo/schema-infra-consolidation-prd.md`  
> 集成分支: `feat/schema-diff-hardening`

## 波次编排

### Wave 1（无依赖，并行）

| Track | 任务 | 改动文件 | 冲突面 |
|-------|------|---------|--------|
| **infra-a** | `fetch_full_column_types` 去重 | `transfer/mod.rs`, 新建 `transfer/full_types.rs`, `data_transfer/structure.rs`, `commands/sync/compare.rs` | 无 |
| **infra-b** | `effective_primary_key` 去重 | `packages/driver-api/src/types.rs`, `data_sync/gate.rs`, `schema_diff/ir.rs` | 无 |

### Wave 2（依赖 Wave 1 合并后）

| Track | 任务 | 改动文件 | 冲突面 |
|-------|------|---------|--------|
| **infra-c** | 驱动级 TypeNormalizer | `driver-api/schema_migration.rs`, `driver-api/traits.rs`, `drivers/*/migration.rs`(或新文件), `schema_diff/compare.rs`, `schema_diff/ir.rs`, `schema_diff/plan.rs`, `commands/schema_diff.rs`, `data_sync/types_eq.rs` | ir.rs 与 infra-b 冲突 |
| **infra-d** | TransactionScope | 新建 `services/transaction.rs`, `schema_diff/deploy.rs` | 无 |
| **infra-e** | Job Cancel 推广 | 新建 `services/job_registry.rs`, `commands/` 新 IPC, 前端 | 无 |

## 落点侦察

### Track A: fetch_full_column_types 去重

| 文件 | 行号 | 内容 |
|------|------|------|
| `data_transfer/structure.rs` | 34-58 | `pub async fn fetch_full_column_types(adapter, driver, handle, table) -> Result<HashMap, TransferError>` |
| `commands/sync/compare.rs` | 10-34 | `pub(crate) async fn fetch_full_column_types(adapter, driver, handle, table) -> Result<HashMap, CommandError>` |
| `transfer/mod.rs` | 1-20 | 模块导出（需新增 `pub mod full_types;`） |
| `data_transfer/structure.rs:221` | 调用者 `create_target_tables` |
| `data_transfer/structure.rs:281` | 调用者 `drop_and_recreate_table` |
| `commands/schema_diff.rs:266,274` | 调用者 `compare_table_schemas_impl` |

### Track B: effective_primary_key 去重

| 文件 | 行号 | 内容 |
|------|------|------|
| `schema_diff/ir.rs` | 7-17 | `fn effective_primary_keys(schema: &TableSchema) -> Vec<String>` |
| `data_sync/gate.rs` | 77-87 | `pub fn effective_primary_key(schema: &TableSchema) -> Vec<String>` |
| `driver-api/types.rs` | 290-298 | `TableSchema` struct（无 impl 块） |
| `schema_diff/ir.rs:90-91` | 调用处 `diff_to_operations` |
| `data_sync/gate.rs:91-92` | 调用处 `check_table_gate` |

### Track C: TypeNormalizer

| 文件 | 行号 | 内容 |
|------|------|------|
| `driver-api/schema_migration.rs` | 1-129 | MigrationRenderer/Capabilities/Statement（TypeNormalizer trait 落点） |
| `driver-api/traits.rs` | 23-31 | `migration_renderer()`, `migration_capabilities()`（添加 `type_normalizer()` 方法） |
| `data_sync/types_eq.rs` | 9-52 | `canonical_type`；60-75 `parse_type`；98-122 `alias_base` |
| `schema_diff/compare.rs` | 45 | `if col.data_type != tgt_col.data_type`（需改为规范化比较） |
| `schema_diff/ir.rs` | 19 | `diff_to_operations`（需透传 normalizer） |
| `schema_diff/plan.rs` | 163 | `plan_single_table`（需传递 normalizer） |

### Track D: TransactionScope

| 文件 | 行号 | 内容 |
|------|------|------|
| `schema_diff/deploy.rs` | 全文 | `DriverStatementExecutor`（需改用 TransactionScope） |
| 新建 `services/transaction.rs` | — | TransactionScope + DdlAtomicity |

### Track E: Job Cancel

| 文件 | 行号 | 内容 |
|------|------|------|
| `commands/sync/jobs.rs` | 全文 | `SyncJobRegistry`（参考实现） |
| 新建 `services/job_registry.rs` | — | 通用 JobRegistry |
