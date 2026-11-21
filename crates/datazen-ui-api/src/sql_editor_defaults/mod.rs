mod fingerprint;
mod references;

pub use fingerprint::table_clause_fingerprint;
pub use references::{
    infer_default_schema, infer_default_table, tables_referenced_in_sql, ReferencedRelation,
    RelationSourceKind, ScopeRelations, StatementRange,
};
