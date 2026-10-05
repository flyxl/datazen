//! Schema diff planning and deploy (source = desired state → target).

pub mod compare;
pub mod dependencies;
pub mod dependency_graph;
pub mod deploy;
pub mod ir;
pub mod object_identity;
pub mod objects;
pub mod transaction;
mod operation_dependencies;
mod operation_dependency_references;
pub mod operations;
pub mod plan;
pub mod profile;
pub mod reviewed;
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
pub use types::*;

// Force-link path drivers so their inventory registrations take effect when
// running this crate's own tests.
#[cfg(test)]
extern crate datazen_driver_mysql;
#[cfg(test)]
extern crate datazen_driver_postgres;
#[cfg(test)]
extern crate datazen_driver_sqlite;
