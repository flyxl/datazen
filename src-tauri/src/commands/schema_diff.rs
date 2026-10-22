//! Schema Diff Deploy IPC commands.
//!
//! The command surface is split by responsibility:
//!
//! * [`object_scope`] — schema-scope gating and schema object selection.
//! * [`profile_commands`] — Schema Diff profile IPC commands.
//! * [`dependency_catalog`] — target dependency catalog reads and validation.
//! * [`plan_commands`] — table-level plan preparation.
//! * [`object_plan_commands`] — view / routine / sequence / type plans.
//! * [`deploy_commands`] — deploy execution and cancellation.
//! * [`compare_commands`] — single-table schema comparison.
//! * [`reviewed_store`] — reviewed-plan storage port implementation (desktop, single tenant).
//!
//! [`job`] and [`unified_plan`] keep their own subtrees.

pub mod job;
pub mod unified_plan;

mod compare_commands;
mod dependency_catalog;
mod deploy_commands;
mod object_plan_commands;
mod object_scope;
mod plan_commands;
mod profile_commands;
pub(crate) mod reviewed_store;

#[cfg(test)]
mod tests;

use super::error::{CmdExt, CommandError};
use super::sync::compare::diff_table_schemas_ir;
use super::AppState;
use crate::schema_diff::deploy::{
    execute_schema_diff_deploy_at as run_schema_diff_deploy, plan_has_destructive, DeployOptions,
    DESTRUCTIVE_CONFIRM_TOKEN,
};
use crate::schema_diff::diff_table_schemas;
use crate::schema_diff::objects::{
    build_routine_trigger_migration_plan_with_components,
    build_sequence_migration_plan_with_components, build_type_migration_plan_with_components,
    build_view_migration_plan_with_components, SchemaObjectSnapshot,
};
use crate::schema_diff::plan::{is_source_unbounded_text, PlanOptions};
use crate::schema_diff::types::TableColumnDiff;
use crate::schema_diff::types::{
    normalize_dialect, resolve_table_for_dialect, uses_schema_scope, ColumnTypeOverride,
    PlanRequirement, SchemaDiffDeployResult, SchemaDiffPlan,
};
use crate::schema_diff::SchemaDiffProfile;
use crate::transfer::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::transfer::ddl::build_create_table_ddl;
use crate::transfer::full_types::fetch_full_column_types;
use datazen_driver_api::SqlTarget;
use tauri::State;

// Re-export the command surface so that `crate::commands::<command>` keeps
// resolving and `generate_handler!` can still reach the `__cmd__*` macros the
// `#[tauri::command]` attribute generates in each submodule.
pub use compare_commands::*;
pub use deploy_commands::*;
pub use object_plan_commands::*;
pub use plan_commands::*;
pub use profile_commands::*;

// Helpers shared with the `job` and `unified_plan` subtrees.
use dependency_catalog::{
    fetch_target_table_dependency_catalog, fetch_target_table_schema, has_table_drop_statement,
    is_table_missing_error, resolve_profile_table, resolve_reviewed_table_snapshot,
    schema_catalog_database, schema_catalog_scope, validate_target_dependency_catalog,
};
use object_scope::{
    apply_sqlserver_object_scope_gate, ensure_distinct_schema_scope, fetch_schema_object,
    fetch_schema_view, list_schema_objects, list_schema_views, select_schema_object_pair,
};
pub(crate) use reviewed_store::reviewed_scope;
