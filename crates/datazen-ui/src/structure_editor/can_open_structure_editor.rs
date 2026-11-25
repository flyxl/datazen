//! Port of `packages/frontend/src/lib/structureEditor/canOpenStructureEditor.ts`.
//!
//! The nine-line predicate is kept branch-for-branch: absent meta, key-value or
//! document connection views, an explicitly disabled structure editor, and a
//! driver without SQL all refuse. The `structureEditor?.enabled === false`
//! test is strict — an absent config or an absent `enabled` field is *not* a
//! refusal, mirroring optional chaining plus `=== false` in TypeScript.

use super::{ConnectionView, DatabaseTypeMeta};

/// Port of `canOpenStructureEditor(meta)`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub fn can_open_structure_editor(meta: Option<&DatabaseTypeMeta>) -> bool {
    let Some(meta) = meta else {
        return false;
    };
    if meta.is_key_value || meta.connection_view == ConnectionView::Document {
        return false;
    }
    if meta
        .structure_editor
        .as_ref()
        .and_then(|config| config.enabled)
        == Some(false)
    {
        return false;
    }
    if !meta.supports_sql {
        return false;
    }
    true
}
