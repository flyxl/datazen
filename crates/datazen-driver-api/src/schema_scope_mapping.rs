//! Optional, driver-owned mapping of a schema object's exact scope references.

use crate::schema_objects::DatabaseObject;

/// Creation semantics captured for a MySQL view. Scope mapping may proceed
/// only when the target renderer's default CREATE VIEW semantics are proven
/// equivalent to this metadata.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MySqlViewMetadata {
    pub algorithm: String,
    pub definer: String,
    pub security_type: String,
    pub check_option: String,
    pub character_set_client: String,
    pub collation_connection: String,
    pub has_explicit_column_list: bool,
}

/// A catalog-proven dependency identity and its target-scope counterpart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaObjectScopeDependency {
    pub source: DatabaseObject,
    pub target: DatabaseObject,
}

/// Result of mapping one supported schema object into a target scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaObjectScopeMapping {
    pub definition: String,
    pub dependencies: Vec<DatabaseObject>,
}
