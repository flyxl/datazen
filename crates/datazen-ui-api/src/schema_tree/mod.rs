mod categories;
mod drag;

pub use categories::{
    get_categories_for_driver, leaf_kind_icon, object_kind_category, BASE_CATEGORIES,
    LEAF_KIND_ICONS, OBJECT_KIND_CATEGORIES,
};
pub use drag::{
    build_legacy_table_payload, build_schema_object_payload, set_drag_payload, DragMimeEntry,
    DragMimeValue, DragPayloadOptions, DragPayloadPlan, LegacyTableDragPayload, LegacyTableEntry,
    SchemaObjectDragKind, SchemaObjectDragPayloadV1, SchemaObjectNamespace, LEGACY_TABLE_MIME,
    SCHEMA_OBJECT_MIME, SCHEMA_PAYLOAD_VERSION, TEXT_PLAIN_MIME,
};

pub const SCHEMA_TREE_SURFACE: [&str; 10] = [
    "BASE_CATEGORIES",
    "OBJECT_KIND_CATEGORIES",
    "LEAF_KIND_ICON",
    "getCategoriesForDriver",
    "SCHEMA_OBJECT_MIME",
    "LEGACY_TABLE_MIME",
    "buildSchemaObjectPayload",
    "buildLegacyTablePayload",
    "setDragPayload",
    "SCHEMA_TREE_GAP",
];

pub const SCHEMA_TREE_GAP: [&str; 6] = [
    "lucide-react icon components and their concrete SVG rendering",
    "DB_REGISTRY metadata for supportedObjectKinds per driver type",
    "the UiIconId union owned by the icon-id module",
    "DataTransfer.setData / effectAllowed and JSON.stringify in onDragStart",
    "window.getSelection and removeAllRanges for selection cleanup",
    "editor drop handling, caret placement and identifier quoting",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surfaces_and_gaps_are_pinned_and_root_reachable() {
        assert_eq!(SCHEMA_TREE_SURFACE.len(), 10);
        assert_eq!(SCHEMA_TREE_GAP.len(), 6);
        assert_eq!(crate::SCHEMA_TREE_SURFACE.len(), 10);
        assert_eq!(crate::SCHEMA_TREE_GAP.len(), 6);
        assert_eq!(SCHEMA_OBJECT_MIME, "application/datazen-schema-object");
        assert_eq!(LEGACY_TABLE_MIME, "application/datazen-table");
    }
}
