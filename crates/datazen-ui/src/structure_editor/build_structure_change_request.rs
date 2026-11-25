//! Port of `packages/frontend/src/lib/structureEditor/buildStructureChangeRequest.ts`.
//!
//! The React helper filters blank (post-trim) draft names out of the two
//! `current*` arrays, trims the table name, passes `schema` through with `??`
//! semantics, and keeps the `original*` snapshots only in `alter` mode. Drafts
//! themselves are never mutated: a column named `"  id  "` survives with its
//! original text, exactly as the JS `filter` keeps the original object.

use super::{
    StructureChangeMode, StructureChangeRequest, StructureColumnDraft, StructureIndexDraft,
};

/// Mirror of `BuildStructureChangeRequestParams`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub struct BuildStructureChangeRequestParams {
    pub mode: StructureChangeMode,
    pub table: String,
    pub schema: Option<String>,
    pub original_columns: Vec<StructureColumnDraft>,
    pub current_columns: Vec<StructureColumnDraft>,
    pub original_indexes: Vec<StructureIndexDraft>,
    pub current_indexes: Vec<StructureIndexDraft>,
}

/// Port of `buildStructureChangeRequest(params)`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub fn build_structure_change_request(
    params: BuildStructureChangeRequestParams,
) -> StructureChangeRequest {
    // `params.currentColumns.filter((c) => c.name.trim())` — a string is
    // truthy iff it is non-empty, so only whitespace-only names are dropped.
    let named_columns = params
        .current_columns
        .into_iter()
        .filter(|column| !column.name.trim().is_empty())
        .collect();
    let named_indexes = params
        .current_indexes
        .into_iter()
        .filter(|index| !index.name.trim().is_empty())
        .collect();

    let keep_originals = params.mode == StructureChangeMode::Alter;

    StructureChangeRequest {
        mode: params.mode,
        table: params.table.trim().to_owned(),
        schema: params.schema,
        original_columns: if keep_originals {
            params.original_columns
        } else {
            Vec::new()
        },
        current_columns: named_columns,
        original_indexes: if keep_originals {
            params.original_indexes
        } else {
            Vec::new()
        },
        current_indexes: named_indexes,
    }
}
