//! Port of `packages/frontend/src/lib/structureEditor/draftDefaults.ts`.
//!
//! The three helpers build fresh column/index drafts with the exact React
//! field values: empty name, the caller-supplied default type/method, nullable
//! columns, no default/comment, all flags false; an index starts with an empty
//! column list and its `indexType` set to the default method. The create-table
//! seed is two columns — the first is `id`, not nullable and a primary key —
//! built by overriding the fields of one `emptyColumnDraft`, just as the React
//! object spread does.

use super::draft_ids::{new_column_id, new_index_id};
use super::{StructureColumnDraft, StructureEditorUiConfig, StructureIndexDraft};

/// Port of `emptyColumnDraft(defaultType)`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub(crate) fn empty_column_draft(default_type: &str) -> StructureColumnDraft {
    StructureColumnDraft {
        id: new_column_id(),
        name: String::new(),
        data_type: default_type.to_owned(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
        is_unique: false,
    }
}

/// Port of `emptyIndexDraft(defaultMethod)`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub(crate) fn empty_index_draft(default_method: &str) -> StructureIndexDraft {
    StructureIndexDraft {
        id: new_index_id(),
        name: String::new(),
        columns: Vec::new(),
        is_unique: false,
        is_primary: false,
        index_type: Some(default_method.to_owned()),
        include_columns: None,
        filter: None,
        comment: None,
    }
}

/// Port of `defaultCreateColumns(uiConfig)`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub(crate) fn default_create_columns(
    ui_config: &StructureEditorUiConfig,
) -> Vec<StructureColumnDraft> {
    let mut first = empty_column_draft(&ui_config.default_column_type);
    first.name = "id".to_owned();
    first.nullable = false;
    first.is_primary_key = true;

    vec![first, empty_column_draft(&ui_config.default_column_type)]
}
