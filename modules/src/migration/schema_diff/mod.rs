//! Schema diff planning and deploy (source = desired state → target).
//!
//! D5（2026-10-26）自独立 crate `schema-diff` 整体归位；旧 crate 已在
//! D8 退役，本模块保留唯一实现。

pub mod api;
pub mod compare;
pub mod dependencies;
pub mod dependency_graph;
pub mod deploy;
pub mod ir;
pub mod job;
pub mod object_identity;
pub mod object_selection;
pub mod objects;
mod operation_dependencies;
mod operation_dependency_references;
pub mod operations;
pub mod plan;
pub mod plan_request;
pub mod profile;
pub mod profile_repository;
pub mod reviewed;
pub mod transaction;
pub mod types;
pub mod unified;
mod unified_objects;
pub mod unified_scope;
mod unified_sequence;
mod unified_type_validation;
mod unified_validation;

pub use compare::diff_table_schemas;
pub use deploy::{execute_schema_diff_deploy, execute_schema_diff_deploy_at, DeployOptions};
pub use object_identity::SchemaObjectIdentity;
pub use plan::{build_column_plan, build_schema_diff_plan, PlanOptions};
pub use profile::SchemaDiffProfile;
pub use profile_repository::{SchemaDiffProfileRepository, SchemaDiffProfileRepositoryError};
pub use types::*;
