//! Schema diff planning and deploy (source = desired state → target).

pub mod compare;
pub mod deploy;
pub mod dependencies;
pub mod ir;
pub mod operations;
pub mod dialects;
pub mod plan;
pub mod renderer;
pub mod types;

pub use compare::diff_table_schemas;
pub use deploy::{execute_schema_diff_deploy, DeployOptions};
pub use plan::{build_column_plan, build_schema_diff_plan, PlanOptions};
pub use types::*;
