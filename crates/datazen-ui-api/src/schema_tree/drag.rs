pub const SCHEMA_OBJECT_MIME: &str = "application/datazen-schema-object";
pub const LEGACY_TABLE_MIME: &str = "application/datazen-table";
pub const TEXT_PLAIN_MIME: &str = "text/plain";
pub const SCHEMA_PAYLOAD_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaObjectDragKind {
    Table,
    View,
}

impl SchemaObjectDragKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaObjectNamespace {
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaObjectDragPayloadV1 {
    pub version: u8,
    pub kind: SchemaObjectDragKind,
    pub namespace: SchemaObjectNamespace,
    pub connection_id: String,
    pub db_session_id: Option<String>,
    pub database_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyTableEntry {
    pub table_name: String,
    pub schema: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyTableDragPayload {
    pub tables: Vec<LegacyTableEntry>,
    pub connection_id: String,
    pub database_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragPayloadOptions {
    pub kind: SchemaObjectDragKind,
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
    pub connection_id: String,
    pub db_session_id: Option<String>,
    pub database_type: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DragMimeValue {
    Versioned(SchemaObjectDragPayloadV1),
    Legacy(LegacyTableDragPayload),
    Plain(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DragMimeEntry {
    pub mime: String,
    pub value: DragMimeValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DragPayloadPlan {
    pub entries: Vec<DragMimeEntry>,
    pub effect_allowed: &'static str,
    pub clear_selection: bool,
}

pub fn build_schema_object_payload(opts: &DragPayloadOptions) -> SchemaObjectDragPayloadV1 {
    SchemaObjectDragPayloadV1 {
        version: SCHEMA_PAYLOAD_VERSION,
        kind: opts.kind,
        namespace: SchemaObjectNamespace {
            database: opts.database.clone(),
            schema: opts.schema.clone(),
            table: opts.table.clone(),
        },
        connection_id: opts.connection_id.clone(),
        db_session_id: opts.db_session_id.clone(),
        database_type: opts.database_type.clone(),
    }
}

pub fn build_legacy_table_payload(opts: &DragPayloadOptions) -> LegacyTableDragPayload {
    LegacyTableDragPayload {
        tables: vec![LegacyTableEntry {
            table_name: opts.table.clone(),
            schema: opts.schema.clone(),
        }],
        connection_id: opts.connection_id.clone(),
        database_type: opts.database_type.clone(),
    }
}

pub fn set_drag_payload(
    opts: &DragPayloadOptions,
    selection_non_collapsed: bool,
) -> DragPayloadPlan {
    DragPayloadPlan {
        entries: vec![
            DragMimeEntry {
                mime: SCHEMA_OBJECT_MIME.to_owned(),
                value: DragMimeValue::Versioned(build_schema_object_payload(opts)),
            },
            DragMimeEntry {
                mime: LEGACY_TABLE_MIME.to_owned(),
                value: DragMimeValue::Legacy(build_legacy_table_payload(opts)),
            },
            DragMimeEntry {
                mime: TEXT_PLAIN_MIME.to_owned(),
                value: DragMimeValue::Plain(opts.table.clone()),
            },
        ],
        effect_allowed: "copy",
        clear_selection: selection_non_collapsed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> DragPayloadOptions {
        DragPayloadOptions {
            kind: SchemaObjectDragKind::View,
            database: "shop".to_owned(),
            schema: Some("public".to_owned()),
            table: "users".to_owned(),
            connection_id: "c1".to_owned(),
            db_session_id: Some("s1".to_owned()),
            database_type: "postgresql".to_owned(),
        }
    }

    #[test]
    fn versioned_payload_carries_the_full_namespace() {
        let payload = build_schema_object_payload(&opts());
        assert_eq!(payload.version, 1);
        assert_eq!(payload.kind, SchemaObjectDragKind::View);
        assert_eq!(payload.namespace.database, "shop");
        assert_eq!(payload.namespace.schema.as_deref(), Some("public"));
        assert_eq!(payload.namespace.table, "users");
        assert_eq!(payload.db_session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn legacy_payload_wraps_one_table_without_a_session() {
        let payload = build_legacy_table_payload(&opts());
        assert_eq!(payload.tables.len(), 1);
        assert_eq!(payload.tables[0].table_name, "users");
        assert_eq!(payload.tables[0].schema.as_deref(), Some("public"));
        assert_eq!(payload.connection_id, "c1");
    }

    #[test]
    fn drag_plan_emits_both_mimes_and_a_plain_fallback() {
        let plan = set_drag_payload(&opts(), false);
        assert_eq!(plan.entries.len(), 3);
        assert_eq!(plan.entries[0].mime, SCHEMA_OBJECT_MIME);
        assert_eq!(plan.entries[1].mime, LEGACY_TABLE_MIME);
        assert_eq!(plan.entries[2].mime, "text/plain");
        assert_eq!(
            plan.entries[2].value,
            DragMimeValue::Plain("users".to_owned())
        );
        assert_eq!(plan.effect_allowed, "copy");
        assert!(!plan.clear_selection);

        let with_selection = set_drag_payload(&opts(), true);
        assert!(with_selection.clear_selection);
    }

    #[test]
    fn view_and_table_kinds_keep_their_wire_literals() {
        assert_eq!(SchemaObjectDragKind::Table.as_str(), "table");
        assert_eq!(SchemaObjectDragKind::View.as_str(), "view");
    }
}
