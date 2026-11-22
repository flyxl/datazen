#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaTreeIconDef {
    pub icon: &'static str,
    pub icon_id: &'static str,
    pub color: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaTreeCategoryDef {
    pub id: &'static str,
    pub label_key: &'static str,
    pub icon: &'static str,
    pub icon_id: &'static str,
    pub color: &'static str,
}

pub const BASE_CATEGORIES: [SchemaTreeCategoryDef; 2] = [
    SchemaTreeCategoryDef {
        id: "tables",
        label_key: "schemaTree.tables",
        icon: "Table2",
        icon_id: "schema.table",
        color: "text-blue-400",
    },
    SchemaTreeCategoryDef {
        id: "views",
        label_key: "schemaTree.views",
        icon: "Eye",
        icon_id: "schema.view",
        color: "text-purple-400",
    },
];

pub const OBJECT_KIND_CATEGORIES: [(&str, SchemaTreeCategoryDef); 5] = [
    (
        "function",
        SchemaTreeCategoryDef {
            id: "function",
            label_key: "schemaTree.functions",
            icon: "Braces",
            icon_id: "schema.function",
            color: "text-orange-400",
        },
    ),
    (
        "procedure",
        SchemaTreeCategoryDef {
            id: "procedure",
            label_key: "schemaTree.procedures",
            icon: "Braces",
            icon_id: "schema.procedure",
            color: "text-emerald-400",
        },
    ),
    (
        "trigger",
        SchemaTreeCategoryDef {
            id: "trigger",
            label_key: "schemaTree.triggers",
            icon: "Zap",
            icon_id: "schema.trigger",
            color: "text-amber-400",
        },
    ),
    (
        "sequence",
        SchemaTreeCategoryDef {
            id: "sequence",
            label_key: "schemaTree.sequences",
            icon: "Hash",
            icon_id: "schema.sequence",
            color: "text-cyan-400",
        },
    ),
    (
        "type",
        SchemaTreeCategoryDef {
            id: "type",
            label_key: "schemaTree.types",
            icon: "Shapes",
            icon_id: "schema.type",
            color: "text-pink-400",
        },
    ),
];

pub const LEAF_KIND_ICONS: [(&str, SchemaTreeIconDef); 9] = [
    (
        "table",
        SchemaTreeIconDef {
            icon: "Table2",
            icon_id: "schema.table",
            color: "text-blue-400",
        },
    ),
    (
        "view",
        SchemaTreeIconDef {
            icon: "Eye",
            icon_id: "schema.view",
            color: "text-purple-400",
        },
    ),
    (
        "materializedView",
        SchemaTreeIconDef {
            icon: "Eye",
            icon_id: "schema.view",
            color: "text-purple-400",
        },
    ),
    (
        "systemTable",
        SchemaTreeIconDef {
            icon: "Table2",
            icon_id: "schema.systemTable",
            color: "text-gray-400",
        },
    ),
    (
        "function",
        SchemaTreeIconDef {
            icon: "Braces",
            icon_id: "schema.function",
            color: "text-orange-400",
        },
    ),
    (
        "procedure",
        SchemaTreeIconDef {
            icon: "Braces",
            icon_id: "schema.procedure",
            color: "text-emerald-400",
        },
    ),
    (
        "trigger",
        SchemaTreeIconDef {
            icon: "Zap",
            icon_id: "schema.trigger",
            color: "text-amber-400",
        },
    ),
    (
        "sequence",
        SchemaTreeIconDef {
            icon: "Hash",
            icon_id: "schema.sequence",
            color: "text-cyan-400",
        },
    ),
    (
        "type",
        SchemaTreeIconDef {
            icon: "Shapes",
            icon_id: "schema.type",
            color: "text-pink-400",
        },
    ),
];

pub fn object_kind_category(kind: &str) -> Option<SchemaTreeCategoryDef> {
    OBJECT_KIND_CATEGORIES
        .iter()
        .find(|(candidate, _)| *candidate == kind)
        .map(|(_, category)| *category)
}

pub fn leaf_kind_icon(kind: &str) -> Option<SchemaTreeIconDef> {
    LEAF_KIND_ICONS
        .iter()
        .find(|(candidate, _)| *candidate == kind)
        .map(|(_, icon)| *icon)
}

pub fn get_categories_for_driver(
    supported_object_kinds: Option<&[String]>,
) -> Vec<SchemaTreeCategoryDef> {
    let kinds = match supported_object_kinds {
        None => return BASE_CATEGORIES.to_vec(),
        Some(kinds) if kinds.is_empty() => return BASE_CATEGORIES.to_vec(),
        Some(kinds) => kinds,
    };
    let mut result = BASE_CATEGORIES.to_vec();
    for kind in kinds {
        if let Some(category) = object_kind_category(kind) {
            result.push(category);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_categories_are_pinned() {
        assert_eq!(get_categories_for_driver(None), BASE_CATEGORIES);
        assert_eq!(BASE_CATEGORIES[0].id, "tables");
        assert_eq!(BASE_CATEGORIES[1].label_key, "schemaTree.views");
        assert_eq!(BASE_CATEGORIES[0].color, "text-blue-400");
    }

    #[test]
    fn object_kind_categories_and_leaf_icons_are_pinned() {
        let function = object_kind_category("function").unwrap();
        assert_eq!(function.label_key, "schemaTree.functions");
        assert_eq!(function.icon_id, "schema.function");
        assert_eq!(function.color, "text-orange-400");
        assert_eq!(object_kind_category("unknown"), None);
        assert_eq!(
            leaf_kind_icon("materializedView").map(|icon| icon.icon_id),
            Some("schema.view")
        );
        assert_eq!(
            leaf_kind_icon("systemTable").map(|icon| icon.color),
            Some("text-gray-400")
        );
        assert_eq!(leaf_kind_icon("mystery"), None);
    }

    #[test]
    fn driver_categories_append_only_supported_known_kinds() {
        let kinds = [
            "function".to_owned(),
            "unknown".to_owned(),
            "trigger".to_owned(),
        ];
        let categories = get_categories_for_driver(Some(&kinds));
        assert_eq!(categories.len(), 4);
        assert_eq!(categories[2].id, "function");
        assert_eq!(categories[3].id, "trigger");
        assert_eq!(get_categories_for_driver(Some(&[])), BASE_CATEGORIES);
    }
}
