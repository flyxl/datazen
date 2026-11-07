//! Structured schema-object dependency catalog contracts and dialect SQL.
//!
//! Results distinguish complete catalogs from dependencies that could be
//! observed. Callers must block an object when complete is false; an empty
//! partial result is never proof of no edges.

mod mysql;
mod postgres;
mod postgres_function;

use crate::schema_objects::{dialect_family, DatabaseObject};

pub use mysql::{
    mysql_dependency_grants_are_complete, mysql_table_dependencies_sql,
    mysql_table_dependency_grants_are_complete, MYSQL_DEPENDENCY_GRANTS_SQL, MYSQL_UDF_CATALOG_SQL,
};
pub use postgres::{
    postgres_sequence_dependencies_sql, postgres_table_dependencies_sql,
    postgres_trigger_dependencies_sql, postgres_type_dependencies_sql,
};
pub use postgres_function::postgres_function_dependencies_sql;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaObjectDependencies {
    pub complete: bool,
    pub dependencies: Vec<DatabaseObject>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_dependency_usages: Vec<TypeDependencyUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_dependency_usages: Option<Vec<SequenceDependencyUsage>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TypeDependencyUsage {
    pub dependency: DatabaseObject,
    pub usage: TypeDependencyUsageKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_name: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TypeDependencyUsageKind {
    ColumnType,
    Expression,
    Constraint,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SequenceDependencyUsage {
    pub sequence: DatabaseObject,
    pub owner_table: DatabaseObject,
    pub column_name: String,
    pub usage: SequenceDependencyUsageKind,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SequenceDependencyUsageKind {
    ColumnDefault,
    OwnedBy,
}

impl SchemaObjectDependencies {
    pub fn incomplete() -> Self {
        Self {
            complete: false,
            dependencies: Vec::new(),
            type_dependency_usages: Vec::new(),
            sequence_dependency_usages: None,
        }
    }
}

pub fn view_dependencies_sql(db_type: &str, name: &str, schema: Option<&str>) -> Option<String> {
    let family = dialect_family(db_type);
    let name = sql_string(name);

    match family {
        "postgresql" => {
            let schema_filter = schema
                .map(|schema| format!(" AND view_ns.nspname = {}", sql_string(schema)))
                .unwrap_or_default();
            Some(postgres::postgres_view_dependencies_sql(
                &name,
                &schema_filter,
            ))
        }
        "mysql" => {
            let schema_filter = match schema {
                Some(schema) => format!(" AND TABLE_SCHEMA = {}", sql_string(schema)),
                None => " AND TABLE_SCHEMA = DATABASE()".into(),
            };
            Some(mysql::mysql_view_dependencies_sql(&name, &schema_filter))
        }
        _ => None,
    }
}

pub(super) fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_objects::DatabaseObject;

    fn object(kind: &str, schema: &str, name: &str) -> DatabaseObject {
        DatabaseObject {
            kind: kind.into(),
            schema: Some(schema.into()),
            name: name.into(),
            signature: None,
            target_schema: None,
            target_name: None,
        }
    }

    #[test]
    fn sequence_dependency_usage_serde_preserves_exact_identities_and_empty_proof() {
        let owned = SchemaObjectDependencies {
            complete: true,
            dependencies: vec![object("table", "public", "serial_table")],
            type_dependency_usages: Vec::new(),
            sequence_dependency_usages: Some(vec![SequenceDependencyUsage {
                sequence: object("sequence", "public", "serial_table_id_seq"),
                owner_table: object("table", "public", "serial_table"),
                column_name: "id".into(),
                usage: SequenceDependencyUsageKind::OwnedBy,
            }]),
        };
        let encoded = serde_json::to_value(owned).unwrap();
        assert_eq!(
            encoded["sequenceDependencyUsages"],
            serde_json::json!([{
                "sequence": {"kind":"sequence","schema":"public","name":"serial_table_id_seq"},
                "ownerTable": {"kind":"table","schema":"public","name":"serial_table"},
                "columnName": "id",
                "usage": "owned_by"
            }])
        );

        let unowned = SchemaObjectDependencies {
            complete: true,
            dependencies: Vec::new(),
            type_dependency_usages: Vec::new(),
            sequence_dependency_usages: Some(Vec::new()),
        };
        assert_eq!(
            serde_json::to_value(unowned).unwrap()["sequenceDependencyUsages"],
            serde_json::json!([]),
            "an empty array proves the catalog found no sequence usages"
        );

        let unavailable = SchemaObjectDependencies::incomplete();
        assert!(
            serde_json::to_value(unavailable)
                .unwrap()
                .get("sequenceDependencyUsages")
                .is_none(),
            "incomplete catalogs must omit the optional proof field"
        );
    }

    #[test]
    fn postgres_dependency_sql_uses_full_catalog_and_exact_routine_identity() {
        let sql = view_dependencies_sql("postgresql", "view'name", Some("sales"))
            .expect("PostgreSQL views have a dependency catalog");
        assert!(sql.contains("pg_catalog.pg_depend"));
        assert!(sql.contains("information_schema.view_table_usage"));
        assert!(sql.contains("information_schema.view_routine_usage"));
        assert!(sql.contains("pg_catalog.nameconcatoid(procedure.proname, procedure.oid)"));
        assert!(sql.contains("pg_catalog.pg_get_function_identity_arguments(procedure.oid)"));
        assert_eq!(
            sql.matches("referenced.oid <> selected.oid").count(),
            2,
            "both pg_depend and view_table_usage edges must exclude the selected view by OID"
        );
        assert!(sql.contains("view_rel.relname = 'view''name'"));
        assert!(sql.contains("view_ns.nspname = 'sales'"));
        assert!(sql.contains("unsupported_count"));
    }

    #[test]
    fn mysql_view_sql_uses_standard_dependency_catalogs_and_scope() {
        let sql = view_dependencies_sql("mysql", "active_view", Some("app"))
            .expect("MySQL views have an information-schema dependency catalog");
        assert!(sql.contains("information_schema.VIEW_TABLE_USAGE"));
        assert!(sql.contains("information_schema.VIEW_ROUTINE_USAGE"));
        assert!(sql.contains("routine.SPECIFIC_NAME = view_usage.SPECIFIC_NAME"));
        assert!(sql.contains("TABLE_SCHEMA = 'app'"));
        assert!(sql.contains("TABLE_NAME = 'active_view'"));
    }

    #[test]
    fn mysql_table_sql_tracks_exact_composite_and_cross_schema_foreign_keys() {
        let sql = mysql_table_dependencies_sql("child'table", Some("child_db"));
        assert!(sql.contains("information_schema.TABLE_CONSTRAINTS"));
        assert!(sql.contains("information_schema.KEY_COLUMN_USAGE"));
        assert!(sql.contains("information_schema.REFERENTIAL_CONSTRAINTS"));
        assert!(sql.contains("constraint_row.CONSTRAINT_TYPE = 'FOREIGN KEY'"));
        assert!(sql.contains("key_column.REFERENCED_TABLE_SCHEMA"));
        assert!(sql.contains("key_column.REFERENCED_TABLE_NAME"));
        assert!(sql.contains("metadata.distinct_reference_positions = metadata.column_count"));
        assert!(sql.contains(
            "BINARY metadata.unique_constraint_schema = BINARY metadata.dependency_schema"
        ));
        assert!(sql.contains(
            "BINARY referenced_table.TABLE_SCHEMA = BINARY key_column.REFERENCED_TABLE_SCHEMA"
        ));
        assert!(sql.contains(
            "BINARY dependency.dependency_schema = BINARY (SELECT selected_schema FROM selected_tables)"
        ));
        assert!(sql.contains("BINARY TABLE_SCHEMA = BINARY 'child_db'"));
        assert!(sql.contains("BINARY TABLE_NAME = BINARY 'child''table'"));
        assert!(sql.contains("orphan_count"));

        let current_database = mysql_table_dependencies_sql("child", None);
        assert!(current_database.contains("BINARY TABLE_SCHEMA = BINARY DATABASE()"));
    }

    #[test]
    fn postgres_trigger_sql_returns_structured_relation_and_function_edges() {
        let sql = postgres_trigger_dependencies_sql(
            "audit'trigger",
            Some("app"),
            Some("app"),
            Some("orders"),
        );
        assert!(sql.contains("pg_catalog.pg_trigger"));
        assert!(sql.contains("trigger.tgrelid AS target_oid"));
        assert!(sql.contains("trigger.tgfoid AS function_oid"));
        assert!(sql.contains("pg_catalog.pg_get_function_identity_arguments(procedure.oid)"));
        assert!(sql.contains("trigger.tgname = 'audit''trigger'"));
        assert!(sql.contains("target_ns.nspname = 'app'"));
        assert!(sql.contains("target_rel.relname = 'orders'"));
        assert!(sql.contains("unsupported_count"));
        assert!(sql.contains("selected.constraint_oid <> 0"));
    }

    #[test]
    fn postgres_function_catalog_proves_only_the_exact_trigger_passthrough_body() {
        let sql = postgres_function_dependencies_sql("audit'trigger", Some("app"), Some(""));
        assert!(sql.contains("pg_catalog.pg_proc"));
        assert!(sql.contains("pg_catalog.pg_depend"));
        assert!(sql.contains("'BEGINRETURNNEW;END', 'BEGINRETURNNEW;END;'"));
        assert!(sql.contains("lang.lanname = 'plpgsql'"));
        assert!(sql.contains("pg_get_function_identity_arguments(proc.oid) = ''"));
        assert!(sql.contains("proc.proname = 'audit''trigger'"));
        assert!(sql.contains("unsupported_count"));
        assert!(sql.contains("WHERE NOT selected.body_dependencies_proven_empty"));
    }

    #[test]
    fn postgres_type_catalog_tracks_dependencies_for_supported_user_types() {
        let sql = postgres_type_dependencies_sql("account_status", Some("public"));
        assert!(sql.contains("user_type.typtype IN ('e', 'd', 'r')"));
        assert!(sql.contains("domain_check.contypid = selected.oid"));
        assert!(sql.contains("pg_catalog.pg_depend"));
        assert!(sql.contains("pg_get_function_identity_arguments(procedure.oid)"));
        assert!(sql.contains("type_ns.nspname = 'public'"));
        assert!(sql.contains("unsupported_count"));
    }

    #[test]
    fn postgres_sequence_and_table_catalogs_return_structured_owned_and_type_edges() {
        let sequence_sql = postgres_sequence_dependencies_sql("orders_id_seq", Some("sales"));
        assert!(sequence_sql.contains("sequence.relkind = 'S'"));
        assert!(sequence_sql.contains("dependency.deptype IN ('a', 'i')"));
        assert!(sequence_sql.contains("owner_rel.relname AS name"));
        assert!(sequence_sql.contains("dependency.refobjsubid"));
        assert!(sequence_sql.contains("owner_column.attname::text AS owner_column_name"));
        assert!(sequence_sql.contains("'owned_by'::text AS sequence_usage"));
        assert!(sequence_sql.contains("owner_column.attnum IS NULL"));
        assert!(sequence_sql.contains("dependency.deptype NOT IN ('a', 'i')"));
        assert!(sequence_sql.contains("owner_rel.relkind NOT IN ('r', 'p', 'f')"));
        assert!(sequence_sql.contains("sequence_ns.nspname = 'sales'"));

        let unqualified_sequence_sql = postgres_sequence_dependencies_sql("orders_id_seq", None);
        assert!(unqualified_sequence_sql.contains("sequence.relname = 'orders_id_seq'"));
        assert!(
            !unqualified_sequence_sql.contains("sequence_ns.nspname = 'public'"),
            "an omitted schema must detect same-name sequences in multiple schemas"
        );

        let table_sql = postgres_table_dependencies_sql("orders", Some("sales"));
        assert!(table_sql.contains("referenced_type.typtype IN ('e', 'd', 'r', 'c')"));
        assert!(table_sql.contains("constraint_row.conrelid = selected.oid"));
        assert!(table_sql.contains("pg_catalog.pg_attrdef"));
        assert!(table_sql.contains("pg_catalog.pg_depend"));
        assert!(table_sql.contains("AS type_usage"));
        assert!(table_sql.contains("dependency_column.attname"));
        assert!(table_sql.contains("type_constraint.contype = 'c'"));
        assert!(table_sql.contains("'column_default'"));
        assert!(table_sql.contains("default_row.adnum"));
        assert!(table_sql.contains("default_column.attname::text"));
        assert!(table_sql.contains("AS sequence_schema"));
    }

    #[test]
    fn unknown_catalog_has_no_dependency_sql() {
        assert!(view_dependencies_sql("sqlite", "view", None).is_none());
    }

    #[test]
    fn mysql_visibility_requires_direct_select_execute_and_no_revokes() {
        let grants = vec!["GRANT SELECT, EXECUTE ON *.* TO 'reader'@'localhost'".into()];
        assert!(mysql_dependency_grants_are_complete(&grants));
        assert!(mysql_dependency_grants_are_complete(&[
            "GRANT ALL PRIVILEGES ON *.* TO 'reader'@'localhost' WITH GRANT OPTION".into()
        ]));
        assert!(!mysql_dependency_grants_are_complete(&[
            "GRANT SELECT ON *.* TO 'reader'@'localhost'".into(),
            "GRANT EXECUTE ON app.* TO 'reader'@'localhost'".into(),
        ]));
        assert!(!mysql_dependency_grants_are_complete(&[
            "GRANT SELECT, EXECUTE ON *.* TO 'reader'@'localhost'".into(),
            "REVOKE EXECUTE ON `private`.* FROM 'reader'@'localhost'".into(),
        ]));
    }

    #[test]
    fn mysql_table_visibility_requires_global_select_and_no_partial_revokes() {
        assert!(mysql_table_dependency_grants_are_complete(&[
            "GRANT SELECT ON *.* TO 'reader'@'localhost'".into()
        ]));
        assert!(mysql_table_dependency_grants_are_complete(&[
            "GRANT ALL PRIVILEGES ON *.* TO 'reader'@'localhost'".into()
        ]));
        assert!(!mysql_table_dependency_grants_are_complete(&[
            "GRANT SELECT ON app.* TO 'reader'@'localhost'".into()
        ]));
        assert!(!mysql_table_dependency_grants_are_complete(&[
            "GRANT SELECT ON *.* TO 'reader'@'localhost'".into(),
            "REVOKE SELECT ON private_db.* FROM 'reader'@'localhost'".into()
        ]));
        assert!(!mysql_table_dependency_grants_are_complete(&[]));
    }
}
