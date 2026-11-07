use super::sql_string;

/// MySQL's information-schema dependency rows are privilege-filtered. This
/// proof query is necessary before those rows can be marked complete.
pub const MYSQL_DEPENDENCY_GRANTS_SQL: &str = "SHOW GRANTS FOR CURRENT_USER()";

/// MySQL omits loadable UDFs from VIEW_ROUTINE_USAGE. If any are installed,
/// the view dependency list cannot prove that no such function is referenced.
pub const MYSQL_UDF_CATALOG_SQL: &str = "SELECT COUNT(*) AS udf_count FROM mysql.func";

/// Conservatively prove global visibility for view dependencies.
///
/// A direct global SELECT grant covers view and table/view usage rows, and a
/// direct global EXECUTE grant covers stored functions. ALL PRIVILEGES covers
/// both. Any partial revoke invalidates the proof.
pub fn mysql_dependency_grants_are_complete(grants: &[String]) -> bool {
    grants_prove_global_privileges(grants, &["SELECT", "EXECUTE"])
}

/// MySQL table/FK metadata is privilege filtered. A global SELECT grant with
/// no partial revokes is the minimum proof that the information-schema rows
/// used by the table dependency query are visible across schemas.
pub fn mysql_table_dependency_grants_are_complete(grants: &[String]) -> bool {
    grants_prove_global_privileges(grants, &["SELECT"])
}

fn grants_prove_global_privileges(grants: &[String], required: &[&str]) -> bool {
    let mut granted = required
        .iter()
        .map(|privilege| (*privilege, false))
        .collect::<std::collections::HashMap<_, _>>();
    let mut revoked = false;

    for grant in grants {
        let normalized = grant.trim().to_ascii_uppercase();
        if normalized.starts_with("REVOKE ") {
            revoked = true;
            continue;
        }
        if !normalized.starts_with("GRANT ") {
            continue;
        }
        let Some(grant_end) = normalized.find(" ON *.* TO ") else {
            continue;
        };
        for privilege in normalized[6..grant_end].split(',').map(str::trim) {
            if privilege == "ALL" || privilege == "ALL PRIVILEGES" {
                for granted in granted.values_mut() {
                    *granted = true;
                }
            } else if let Some(granted) = granted.get_mut(privilege) {
                *granted = true;
            }
        }
    }

    !revoked && granted.values().all(|granted| *granted)
}

pub(super) fn mysql_view_dependencies_sql(name: &str, schema_filter: &str) -> String {
    format!(
        r#"
        WITH selected_views AS (
            SELECT TABLE_SCHEMA AS schema_name, TABLE_NAME AS object_name
            FROM information_schema.VIEWS
            WHERE TABLE_NAME = {name}{schema_filter}
        ),
        catalog_dependencies AS (
            SELECT selected.schema_name AS view_schema, selected.object_name AS view_name,
                CASE referenced.TABLE_TYPE WHEN 'VIEW' THEN 'view' ELSE 'table' END AS kind,
                view_usage.TABLE_SCHEMA AS dependency_schema, view_usage.TABLE_NAME AS name,
                NULL AS signature
            FROM selected_views AS selected
            JOIN information_schema.VIEW_TABLE_USAGE AS view_usage
              ON view_usage.VIEW_SCHEMA = selected.schema_name AND view_usage.VIEW_NAME = selected.object_name
            LEFT JOIN information_schema.TABLES AS referenced
              ON referenced.TABLE_SCHEMA = view_usage.TABLE_SCHEMA AND referenced.TABLE_NAME = view_usage.TABLE_NAME

            UNION

            SELECT selected.schema_name AS view_schema, selected.object_name AS view_name,
                'function' AS kind, routine.ROUTINE_SCHEMA AS dependency_schema,
                routine.ROUTINE_NAME AS name, NULL AS signature
            FROM selected_views AS selected
            JOIN information_schema.VIEW_ROUTINE_USAGE AS view_usage
              ON view_usage.TABLE_SCHEMA = selected.schema_name AND view_usage.TABLE_NAME = selected.object_name
            JOIN information_schema.ROUTINES AS routine
              ON routine.ROUTINE_SCHEMA = view_usage.SPECIFIC_SCHEMA
             AND routine.SPECIFIC_NAME = view_usage.SPECIFIC_NAME
             AND routine.ROUTINE_TYPE = 'FUNCTION'
        )
        SELECT (SELECT count(*) FROM selected_views) AS selected_count,
               0 AS unsupported_count,
               dependencies.kind, dependencies.dependency_schema, dependencies.name, dependencies.signature
        FROM (SELECT 1) AS seed
        LEFT JOIN catalog_dependencies AS dependencies ON TRUE
        ORDER BY dependencies.kind, dependencies.dependency_schema, dependencies.name
        "#
    )
}

/// Return exact outbound MySQL foreign-key table identities for a selected
/// base table. The status row remains present for empty results and records
/// malformed, unjoinable FK metadata as unsupported so an empty partial
/// catalog can never be treated as proof of no dependencies.
pub fn mysql_table_dependencies_sql(name: &str, schema: Option<&str>) -> String {
    let name = sql_string(name);
    let schema_filter = match schema {
        Some(schema) => format!("BINARY TABLE_SCHEMA = BINARY {}", sql_string(schema)),
        None => "BINARY TABLE_SCHEMA = BINARY DATABASE()".into(),
    };

    format!(
        r#"
        WITH selected_tables AS (
            SELECT TABLE_SCHEMA AS selected_schema, TABLE_NAME AS selected_name
            FROM information_schema.TABLES
            WHERE {schema_filter} AND BINARY TABLE_NAME = BINARY {name}
              AND TABLE_TYPE = 'BASE TABLE'
        ),
        selected_foreign_keys AS (
            SELECT constraint_row.CONSTRAINT_SCHEMA, constraint_row.TABLE_SCHEMA,
                   constraint_row.TABLE_NAME, constraint_row.CONSTRAINT_NAME
            FROM information_schema.TABLE_CONSTRAINTS AS constraint_row
            JOIN selected_tables AS selected
              ON BINARY selected.selected_schema = BINARY constraint_row.TABLE_SCHEMA
             AND BINARY selected.selected_name = BINARY constraint_row.TABLE_NAME
            WHERE constraint_row.CONSTRAINT_TYPE = 'FOREIGN KEY'
        ),
        foreign_key_metadata AS (
            SELECT foreign_key.CONSTRAINT_SCHEMA, foreign_key.TABLE_SCHEMA,
                   foreign_key.TABLE_NAME, foreign_key.CONSTRAINT_NAME,
                   MIN(key_column.REFERENCED_TABLE_SCHEMA) AS dependency_schema,
                   MIN(key_column.REFERENCED_TABLE_NAME) AS dependency_name,
                   COUNT(key_column.COLUMN_NAME) AS column_count,
                   COUNT(key_column.REFERENCED_TABLE_SCHEMA) AS reference_schema_count,
                   COUNT(key_column.REFERENCED_TABLE_NAME) AS reference_table_count,
                   COUNT(key_column.REFERENCED_COLUMN_NAME) AS reference_column_count,
                   COUNT(key_column.POSITION_IN_UNIQUE_CONSTRAINT) AS reference_position_count,
                   COUNT(DISTINCT key_column.ORDINAL_POSITION) AS distinct_source_positions,
                   MIN(key_column.ORDINAL_POSITION) AS first_source_position,
                   MAX(key_column.ORDINAL_POSITION) AS last_source_position,
                   MIN(key_column.POSITION_IN_UNIQUE_CONSTRAINT) AS first_reference_position,
                   MAX(key_column.POSITION_IN_UNIQUE_CONSTRAINT) AS last_reference_position,
                   COUNT(DISTINCT BINARY key_column.REFERENCED_TABLE_SCHEMA) AS distinct_reference_schemas,
                   COUNT(DISTINCT BINARY key_column.REFERENCED_TABLE_NAME) AS distinct_reference_tables,
                   COUNT(DISTINCT key_column.POSITION_IN_UNIQUE_CONSTRAINT) AS distinct_reference_positions,
                   COUNT(referential.CONSTRAINT_NAME) AS referential_rows,
                   COUNT(DISTINCT referential.CONSTRAINT_NAME) AS referential_constraints,
                   COUNT(DISTINCT referential.UNIQUE_CONSTRAINT_SCHEMA) AS unique_constraint_schemas,
                   COUNT(DISTINCT referential.UNIQUE_CONSTRAINT_NAME) AS unique_constraints,
                   MIN(referential.UNIQUE_CONSTRAINT_SCHEMA) AS unique_constraint_schema,
                   MIN(referential.UNIQUE_CONSTRAINT_NAME) AS unique_constraint_name,
                   COUNT(DISTINCT referenced_table.TABLE_NAME) AS visible_referenced_tables,
                   COUNT(referenced_table.TABLE_NAME) AS referenced_table_rows
            FROM selected_foreign_keys AS foreign_key
            LEFT JOIN information_schema.KEY_COLUMN_USAGE AS key_column
              ON BINARY key_column.CONSTRAINT_SCHEMA = BINARY foreign_key.CONSTRAINT_SCHEMA
             AND BINARY key_column.TABLE_SCHEMA = BINARY foreign_key.TABLE_SCHEMA
             AND BINARY key_column.TABLE_NAME = BINARY foreign_key.TABLE_NAME
             AND BINARY key_column.CONSTRAINT_NAME = BINARY foreign_key.CONSTRAINT_NAME
            LEFT JOIN information_schema.REFERENTIAL_CONSTRAINTS AS referential
              ON BINARY referential.CONSTRAINT_SCHEMA = BINARY foreign_key.CONSTRAINT_SCHEMA
             AND BINARY referential.CONSTRAINT_NAME = BINARY foreign_key.CONSTRAINT_NAME
             AND BINARY referential.TABLE_NAME = BINARY foreign_key.TABLE_NAME
            LEFT JOIN information_schema.TABLES AS referenced_table
              ON BINARY referenced_table.TABLE_SCHEMA = BINARY key_column.REFERENCED_TABLE_SCHEMA
             AND BINARY referenced_table.TABLE_NAME = BINARY key_column.REFERENCED_TABLE_NAME
             AND referenced_table.TABLE_TYPE = 'BASE TABLE'
            GROUP BY foreign_key.CONSTRAINT_SCHEMA, foreign_key.TABLE_SCHEMA,
                     foreign_key.TABLE_NAME, foreign_key.CONSTRAINT_NAME
        ),
        orphan_key_rows AS (
            SELECT COUNT(*) AS orphan_count
            FROM selected_tables AS selected
            JOIN information_schema.KEY_COLUMN_USAGE AS key_column
              ON BINARY key_column.TABLE_SCHEMA = BINARY selected.selected_schema
             AND BINARY key_column.TABLE_NAME = BINARY selected.selected_name
             AND key_column.REFERENCED_TABLE_NAME IS NOT NULL
            LEFT JOIN selected_foreign_keys AS foreign_key
              ON BINARY foreign_key.CONSTRAINT_SCHEMA = BINARY key_column.CONSTRAINT_SCHEMA
             AND BINARY foreign_key.TABLE_SCHEMA = BINARY key_column.TABLE_SCHEMA
             AND BINARY foreign_key.TABLE_NAME = BINARY key_column.TABLE_NAME
             AND BINARY foreign_key.CONSTRAINT_NAME = BINARY key_column.CONSTRAINT_NAME
            LEFT JOIN information_schema.REFERENTIAL_CONSTRAINTS AS referential
              ON BINARY referential.CONSTRAINT_SCHEMA = BINARY key_column.CONSTRAINT_SCHEMA
             AND BINARY referential.CONSTRAINT_NAME = BINARY key_column.CONSTRAINT_NAME
             AND BINARY referential.TABLE_NAME = BINARY key_column.TABLE_NAME
            WHERE foreign_key.CONSTRAINT_NAME IS NULL OR referential.CONSTRAINT_NAME IS NULL
        ),
        classified_foreign_keys AS (
            SELECT metadata.*,
                CASE WHEN BINARY metadata.CONSTRAINT_SCHEMA = BINARY metadata.TABLE_SCHEMA
                  AND metadata.column_count > 0
                  AND metadata.reference_schema_count = metadata.column_count
                  AND metadata.reference_table_count = metadata.column_count
                  AND metadata.reference_column_count = metadata.column_count
                  AND metadata.reference_position_count = metadata.column_count
                  AND metadata.distinct_source_positions = metadata.column_count
                  AND metadata.first_source_position = 1
                  AND metadata.last_source_position = metadata.column_count
                  AND metadata.first_reference_position = 1
                  AND metadata.last_reference_position = metadata.column_count
                  AND metadata.distinct_reference_schemas = 1
                  AND metadata.distinct_reference_tables = 1
                  AND metadata.distinct_reference_positions = metadata.column_count
                  AND metadata.referential_rows = metadata.column_count
                  AND metadata.referential_constraints = 1
                  AND metadata.unique_constraint_schemas = 1
                  AND metadata.unique_constraints = 1
                  AND BINARY metadata.unique_constraint_schema = BINARY metadata.dependency_schema
                  AND metadata.unique_constraint_name IS NOT NULL
                  AND metadata.visible_referenced_tables = 1
                  AND metadata.referenced_table_rows = metadata.column_count
                  AND metadata.dependency_schema IS NOT NULL
                  AND metadata.dependency_name IS NOT NULL
                THEN 0 ELSE 1 END AS unsupported
            FROM foreign_key_metadata AS metadata
        ),
        valid_dependencies AS (
            SELECT DISTINCT dependency_schema, dependency_name
            FROM classified_foreign_keys
            WHERE unsupported = 0
        ),
        status AS (
            SELECT
                (SELECT COUNT(*) FROM selected_tables) AS selected_count,
                (SELECT COUNT(*) FROM classified_foreign_keys WHERE unsupported <> 0)
                    + (SELECT orphan_count FROM orphan_key_rows) AS unsupported_count
        )
        SELECT status.selected_count, status.unsupported_count,
               CASE WHEN dependency.dependency_name IS NULL THEN NULL ELSE 'table' END AS kind,
               dependency.dependency_schema, dependency.dependency_name AS name,
               NULL AS signature
        FROM status
        LEFT JOIN valid_dependencies AS dependency
          ON NOT (BINARY dependency.dependency_schema = BINARY (SELECT selected_schema FROM selected_tables)
              AND BINARY dependency.dependency_name = BINARY (SELECT selected_name FROM selected_tables))
        ORDER BY dependency.dependency_schema, dependency.dependency_name
        "#
    )
}
