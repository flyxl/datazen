//! Thin command-layer adapter for the shared Data Sync plan registry.
//!
//! The reviewed comparison, its selection model and the plan registry
//! (issue / claim / page / apply) live in
//! [`datazen_data_sync::plan_store`], next to the comparison engine and the
//! file-backed `ComparisonStore` it owns. The command layer only forwards
//! names so host call sites keep their paths.
//!
//! A comparison response is useful for review, but it is not an execution
//! authority: the complete reviewed comparison stays in the registry and
//! later commands receive only an opaque plan id plus a validated selection.

pub use datazen_data_sync::plan_store::*;
