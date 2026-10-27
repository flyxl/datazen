//! What a unified schema diff plan request must contain.
//!
//! The desktop host and a web host build the same plan from the same
//! request, so the rules that decide whether a request can become a plan
//! belong to the plan, not to the host: an empty selection has no plan, and
//! a table list whose source and target lengths differ cannot be paired.

use datazen_driver_api::DatabaseObject;

/// Why a plan request cannot become a plan.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanRequestError {
    #[error("Select at least one table or schema object before preparing a unified plan")]
    EmptySelection,
    #[error("table_names and target_table_names must have the same length")]
    MismatchedTableCounts,
}

/// Validate the selection a unified plan is built from.
///
/// The source and target table lists are paired one-to-one, so their lengths
/// must agree; a selection that names no table and no schema object has
/// nothing to plan.
pub fn validate_plan_selection(
    table_names: &[String],
    target_table_names: &[String],
    target_only_table_names: &[String],
    source_objects: &[DatabaseObject],
    target_objects: &[DatabaseObject],
) -> Result<(), PlanRequestError> {
    if table_names.is_empty()
        && target_only_table_names.is_empty()
        && source_objects.is_empty()
        && target_objects.is_empty()
    {
        return Err(PlanRequestError::EmptySelection);
    }
    if table_names.len() != target_table_names.len() {
        return Err(PlanRequestError::MismatchedTableCounts);
    }
    Ok(())
}
