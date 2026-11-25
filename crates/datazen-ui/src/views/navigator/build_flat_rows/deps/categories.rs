//! `windows/connection/schema-tree/schemaTreeCategories.ts`, ported for T570
//! (the parts a row builder needs: the category ladder per driver). Removal
//! stage: shard 2 owns the schema-tree family; when that port lands, this
//! table and [`get_categories_for_driver`] move next to it.
//!
//! React's category def carries a lucide component and a tailwind colour; the
//! builder only consumes `id` and `labelKey` (the row carries the id, the row
//! renderer resolves the icon), so those two are what survives here — in the
//! same order React declared them, because the ladder's order is the tree's
//! category order.

use super::super::inputs::DriverMeta;

/// React `SchemaTreeCategoryDef` (id + labelKey; the icon lives view-side).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaTreeCategory {
    pub id: &'static str,
    pub label_key: &'static str,
}

/// React `BASE_CATEGORIES` — tables then views, always first.
pub const BASE_CATEGORIES: [SchemaTreeCategory; 2] = [
    SchemaTreeCategory {
        id: "tables",
        label_key: "schemaTree.tables",
    },
    SchemaTreeCategory {
        id: "views",
        label_key: "schemaTree.views",
    },
];

/// React `OBJECT_KIND_CATEGORIES` — one per object kind, in declaration order.
fn object_kind_category(kind: &str) -> Option<SchemaTreeCategory> {
    Some(match kind {
        "function" => SchemaTreeCategory {
            id: "function",
            label_key: "schemaTree.functions",
        },
        "procedure" => SchemaTreeCategory {
            id: "procedure",
            label_key: "schemaTree.procedures",
        },
        "trigger" => SchemaTreeCategory {
            id: "trigger",
            label_key: "schemaTree.triggers",
        },
        "sequence" => SchemaTreeCategory {
            id: "sequence",
            label_key: "schemaTree.sequences",
        },
        "type" => SchemaTreeCategory {
            id: "type",
            label_key: "schemaTree.types",
        },
        _ => return None,
    })
}

/// React `getCategoriesForDriver`: tables/views plus every object kind the
/// driver supports; drivers with no object kinds get the base pair only.
pub fn get_categories_for_driver(meta: Option<&DriverMeta>) -> Vec<SchemaTreeCategory> {
    let mut cats: Vec<SchemaTreeCategory> = BASE_CATEGORIES.to_vec();
    let Some(meta) = meta else { return cats };
    for kind in &meta.supported_object_kinds {
        if let Some(cat) = object_kind_category(kind) {
            cats.push(cat);
        }
    }
    cats
}
