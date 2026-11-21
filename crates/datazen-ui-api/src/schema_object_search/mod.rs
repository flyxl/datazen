mod filter_group;
mod search;
mod table_match;
mod types;

pub use filter_group::{
    filter_object_search_results_by_type, filter_schema_objects_by_type,
    group_object_search_results, group_schema_objects,
};
pub use search::search_schema_objects;
pub use table_match::{matching_columns, table_matches_object_search};
pub use types::{
    object_type_order, ObjectSearchFilters, ObjectSearchMatchField, ObjectSearchObjectType,
    ObjectSearchResult, ObjectSearchResultGroup, ObjectSearchSourceKind, SchemaObjectIndex,
    SchemaObjectIndexEntry, SchemaObjectSearchIndex,
};
