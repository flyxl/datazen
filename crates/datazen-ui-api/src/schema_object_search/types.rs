use std::collections::HashMap;

use crate::{
    DatabaseObject, DatabaseObjectKind, DatabaseType, TableInfo, TableSqlActionKind, TableType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectSearchObjectType {
    Table,
    View,
    Column,
    Function,
    Routine,
    Procedure,
    Trigger,
    Sequence,
    Type,
}

impl ObjectSearchObjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::Column => "column",
            Self::Function => "function",
            Self::Routine => "routine",
            Self::Procedure => "procedure",
            Self::Trigger => "trigger",
            Self::Sequence => "sequence",
            Self::Type => "type",
        }
    }
}

pub fn object_type_order(value: ObjectSearchObjectType) -> usize {
    match value {
        ObjectSearchObjectType::Table => 0,
        ObjectSearchObjectType::View => 1,
        ObjectSearchObjectType::Column => 2,
        ObjectSearchObjectType::Function => 3,
        ObjectSearchObjectType::Routine => 4,
        ObjectSearchObjectType::Procedure => 5,
        ObjectSearchObjectType::Trigger => 6,
        ObjectSearchObjectType::Sequence => 7,
        ObjectSearchObjectType::Type => 8,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectSearchMatchField {
    Name,
    Host,
    Database,
    Type,
    Schema,
    Object,
    Table,
    Column,
}

impl ObjectSearchMatchField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Host => "host",
            Self::Database => "database",
            Self::Type => "type",
            Self::Schema => "schema",
            Self::Object => "object",
            Self::Table => "table",
            Self::Column => "column",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectSearchSourceKind {
    Relation(TableType),
    Catalog(DatabaseObjectKind),
}

#[derive(Clone, Debug, Default)]
pub struct SchemaObjectIndexEntry {
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: DatabaseType,
    pub connection_name: Option<String>,
    pub host: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub tables: Vec<TableInfo>,
    pub views: Vec<TableInfo>,
    pub objects: Vec<DatabaseObject>,
    pub columns: HashMap<String, Vec<String>>,
    pub column_map: HashMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Default)]
pub struct SchemaObjectIndex {
    pub entries: Vec<SchemaObjectIndexEntry>,
}

#[derive(Clone, Debug)]
pub enum SchemaObjectSearchIndex {
    Entries(Vec<SchemaObjectIndexEntry>),
    Object(SchemaObjectIndex),
}

impl From<Vec<SchemaObjectIndexEntry>> for SchemaObjectSearchIndex {
    fn from(entries: Vec<SchemaObjectIndexEntry>) -> Self {
        Self::Entries(entries)
    }
}

impl From<SchemaObjectIndex> for SchemaObjectSearchIndex {
    fn from(index: SchemaObjectIndex) -> Self {
        Self::Object(index)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ObjectSearchFilters {
    pub connection_id: Option<String>,
    pub connection_ids: Vec<String>,
    pub database: Option<Option<String>>,
    pub databases: Vec<Option<String>>,
    pub schema: Option<Option<String>>,
    pub schemas: Vec<Option<String>>,
    pub object_type: Option<ObjectSearchObjectType>,
    pub object_types: Vec<ObjectSearchObjectType>,
    pub types: Vec<ObjectSearchObjectType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectSearchResult {
    pub id: String,
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: DatabaseType,
    pub connection_name: Option<String>,
    pub host: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub object_type: ObjectSearchObjectType,
    pub name: String,
    pub object_name: String,
    pub table_name: Option<String>,
    pub source_kind: Option<ObjectSearchSourceKind>,
    pub matched_fields: Vec<ObjectSearchMatchField>,
    pub match_reason: Option<ObjectSearchMatchField>,
    pub actions: Vec<TableSqlActionKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectSearchResultGroup {
    pub key: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub object_type: ObjectSearchObjectType,
    pub results: Vec<ObjectSearchResult>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_type_literals_and_order_are_stable() {
        let ordered = [
            ObjectSearchObjectType::Table,
            ObjectSearchObjectType::View,
            ObjectSearchObjectType::Column,
            ObjectSearchObjectType::Function,
            ObjectSearchObjectType::Routine,
            ObjectSearchObjectType::Procedure,
            ObjectSearchObjectType::Trigger,
            ObjectSearchObjectType::Sequence,
            ObjectSearchObjectType::Type,
        ];
        for (index, value) in ordered.iter().enumerate() {
            assert_eq!(object_type_order(*value), index);
        }
        assert_eq!(ObjectSearchObjectType::Table.as_str(), "table");
        assert_eq!(ObjectSearchObjectType::Type.as_str(), "type");
    }
}
