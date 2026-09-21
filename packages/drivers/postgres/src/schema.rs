//! Schema introspection (columns, table metadata).

use crate::postgres::PostgresDriver;
use crate::sql::{pg_regclass_name, resolve_pg_table_schema};
use datazen_driver_api::*;
use sqlx::{PgPool, Row};
use std::collections::HashMap;

/// Relation kinds that own columns (mirrors the table listing query).
const COLUMN_BEARING_RELKINDS: &str = "'r', 'v', 'm', 'f', 'p'";

/// Fail loudly when `schema.table` does not resolve in the *current* database.
///
/// `information_schema.columns` is resolved against the session's active
/// database, so reading a table that lives in another catalog returns no rows.
/// That used to be reported as a successful, column-less table — which callers
/// then cached, hiding the mistake for the whole cache TTL. A relation that
/// genuinely exists but whose columns are privilege-filtered still resolves
/// here, so a real (if unusual) zero-column table keeps its old behavior.
async fn ensure_pg_relation_exists(
    pool: &PgPool,
    schema: Option<&str>,
    table: &str,
    display: &str,
) -> Result<(), DriverError> {
    let exists: Option<i32> = sqlx::query_scalar(&format!(
        "SELECT 1 FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE c.relname = $1 AND c.relkind IN ({COLUMN_BEARING_RELKINDS}) \
           AND ($2::text IS NULL OR n.nspname = $2) \
         LIMIT 1"
    ))
    .bind(table)
    .bind(schema)
    .fetch_optional(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    if exists.is_some() {
        return Ok(());
    }

    // Not found under the requested schema. Report where the relation *does*
    // live, if anywhere: this distinguishes a mis-targeted read (the relation
    // is in another schema) from a genuinely absent table, without needing a
    // manual psql session against the same database.
    let elsewhere: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT n.nspname FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE c.relname = $1 AND c.relkind IN ({COLUMN_BEARING_RELKINDS}) \
         ORDER BY n.nspname"
    ))
    .bind(table)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    if !elsewhere.is_empty() {
        tracing::warn!(
            %table,
            requested_schema = ?schema,
            present_in = ?elsewhere,
            "relation exists, but not under the requested schema"
        );
    }

    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await
        .unwrap_or_default();
    Err(DriverError::QueryFailed(format!(
        "Table '{display}' does not exist in the current database '{database}'"
    )))
}

impl PostgresDriver {
    pub(crate) async fn get_columns_impl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        let (schema, bare_table) = resolve_pg_table_schema(table, schema);
        let pool = self.pool_for_target(handle, database).await?;
        let regclass = pg_regclass_name(schema, bare_table);

        let cols = sqlx::query(
            r#"
                    SELECT column_name, data_type, is_nullable, column_default,
                           col_description((quote_ident(table_schema)||'.'||quote_ident(table_name))::regclass, ordinal_position) as comment
                    FROM information_schema.columns
                    WHERE table_name = $1
                      AND ($2::text IS NULL OR table_schema = $2)
                    ORDER BY ordinal_position
                    "#,
        )
        .bind(bare_table)
        .bind(schema)
        .fetch_all(&pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        // No columns for a relation that does not exist here: the caller asked
        // for a table of another database (or a dropped one). Report it instead
        // of handing out an empty column list.
        if cols.is_empty() {
            ensure_pg_relation_exists(&pool, schema, bare_table, table).await?;
        }

        // `quote_ident($1)::regclass` fails when the table is not on search_path.
        // Columns must still load so SQL autocomplete can list fields.
        let pk_rows = sqlx::query(
            r#"
                    SELECT a.attname
                    FROM pg_index i
                    JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
                    WHERE i.indrelid = $1::regclass AND i.indisprimary
                    "#,
        )
        .bind(&regclass)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        let pk_names: Vec<String> = pk_rows.iter().map(|r| r.get::<String, _>(0)).collect();
        let columns: Vec<ColumnSchema> = cols
            .iter()
            .map(|r| {
                let name: String = r.get("column_name");
                let nullable: String = r.get("is_nullable");
                ColumnSchema {
                    is_primary_key: pk_names.contains(&name),
                    name,
                    data_type: r.get("data_type"),
                    nullable: nullable == "YES",
                    default_value: r.get("column_default"),
                    comment: r.get("comment"),
                    is_auto_increment: false,
                }
            })
            .collect();

        Ok((columns, pk_names))
    }

    pub(crate) async fn get_table_schema_impl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        let (schema, bare_table) = resolve_pg_table_schema(table, schema);
        let pool = self.pool_for_target(handle, database).await?;
        let regclass = pg_regclass_name(schema, bare_table);

        let cols = sqlx::query(
            r#"
            SELECT column_name, data_type, is_nullable, column_default,
                   col_description((quote_ident(table_schema)||'.'||quote_ident(table_name))::regclass, ordinal_position) as comment
            FROM information_schema.columns
            WHERE table_name = $1
              AND ($2::text IS NULL OR table_schema = $2)
            ORDER BY ordinal_position
            "#,
        )
        .bind(bare_table)
        .bind(schema)
        .fetch_all(&pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        if cols.is_empty() {
            ensure_pg_relation_exists(&pool, schema, bare_table, table).await?;
            return Ok(TableSchema {
                table_name: table.to_string(),
                columns: Vec::new(),
                primary_keys: Vec::new(),
                indexes: Vec::new(),
                foreign_keys: Vec::new(),
            });
        }

        let pk_rows = sqlx::query(
            r#"
            SELECT a.attname
            FROM pg_index i
            JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS k(attnum, n) ON true
            JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.attnum
            WHERE i.indrelid = $1::regclass AND i.indisprimary
            ORDER BY k.n
            "#,
        )
        .bind(&regclass)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        let pk_names: Vec<String> = pk_rows.iter().map(|r| r.get::<String, _>(0)).collect();

        let columns: Vec<ColumnSchema> = cols
            .iter()
            .map(|r| {
                let name: String = r.get("column_name");
                let nullable: String = r.get("is_nullable");
                let default_value: Option<String> = r.get("column_default");
                let is_auto_increment = default_value
                    .as_deref()
                    .map(|d| d.contains("nextval("))
                    .unwrap_or(false);
                ColumnSchema {
                    is_primary_key: pk_names.contains(&name),
                    name,
                    data_type: r.get("data_type"),
                    nullable: nullable == "YES",
                    default_value,
                    comment: r.get("comment"),
                    is_auto_increment,
                }
            })
            .collect();

        // ── indexes ──
        let idx_rows = sqlx::query(
            r#"
            SELECT i.relname::text                                AS index_name,
                   array_agg(a.attname::text ORDER BY k.n)        AS columns,
                   ix.indisunique                                  AS is_unique,
                   ix.indisprimary                                 AS is_primary,
                   am.amname::text                                 AS index_type
            FROM pg_index ix
            JOIN pg_class i  ON i.oid  = ix.indexrelid
            JOIN pg_class t  ON t.oid  = ix.indrelid
            JOIN pg_am   am ON am.oid  = i.relam
            JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS k(attnum, n) ON true
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = k.attnum
            WHERE ix.indrelid = $1::regclass
            GROUP BY i.relname, ix.indisunique, ix.indisprimary, am.amname
            ORDER BY ix.indisprimary DESC, i.relname
            "#,
        )
        .bind(&regclass)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        let indexes: Vec<IndexInfo> = idx_rows
            .iter()
            .map(|r| IndexInfo {
                name: r.get("index_name"),
                columns: r.get::<Vec<String>, _>("columns"),
                is_unique: r.get("is_unique"),
                is_primary: r.get("is_primary"),
                index_type: r.get("index_type"),
            })
            .collect();

        // ── foreign keys ──
        //
        // NOTE: the query below aggregates the referencing columns
        // (`key_column_usage`) and the referenced columns
        // (`constraint_column_usage`) *independently*, joined only by constraint
        // name. A composite key therefore arrives with N x N entries —
        // `(pa, pb) -> (a, b)` comes back as `[pa, pa, pb, pb]` against
        // `[a, b, a, b]` — which is why the result rows are normalised through
        // `normalise_fk_columns` before being handed out.
        let fk_rows = sqlx::query(
            r#"
            SELECT
                tc.constraint_name::text                                             AS fk_name,
                array_agg(kcu.column_name::text ORDER BY kcu.ordinal_position)       AS columns,
                ccu.table_name::text                                                 AS ref_table,
                array_agg(ccu.column_name::text ORDER BY kcu.ordinal_position)       AS ref_columns,
                rc.update_rule::text,
                rc.delete_rule::text
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
              ON kcu.constraint_name = tc.constraint_name
             AND kcu.table_schema   = tc.table_schema
            JOIN information_schema.constraint_column_usage ccu
              ON ccu.constraint_name = tc.constraint_name
             AND ccu.table_schema   = tc.table_schema
            JOIN information_schema.referential_constraints rc
              ON rc.constraint_name = tc.constraint_name
             AND rc.constraint_schema = tc.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
              AND tc.table_name = $1
              AND ($2::text IS NULL OR tc.table_schema = $2)
            GROUP BY tc.constraint_name, ccu.table_name, rc.update_rule, rc.delete_rule
            ORDER BY tc.constraint_name
            "#,
        )
        .bind(bare_table)
        .bind(schema)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        let foreign_keys: Vec<ForeignKeyInfo> = fk_rows
            .iter()
            .filter_map(|r| {
                let columns = r.get::<Vec<String>, _>("columns");
                let referenced_columns = r.get::<Vec<String>, _>("ref_columns");
                // Composite keys come back multiplied (see the helper) — a
                // constraint whose sides cannot be reconciled is skipped rather
                // than reported with an invented column pairing.
                let (columns, referenced_columns) =
                    normalise_fk_columns(columns, referenced_columns)?;
                Some(ForeignKeyInfo {
                    name: r.get("fk_name"),
                    columns,
                    referenced_table: r.get("ref_table"),
                    referenced_columns,
                    on_update: r.get("update_rule"),
                    on_delete: r.get("delete_rule"),
                })
            })
            .collect();

        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys: pk_names,
            indexes,
            foreign_keys,
        })
    }

    /// Batch-fetch columns for ALL tables in the connected database/schema
    /// using a single SQL query, avoiding N per-table round-trips.
    pub(crate) async fn get_all_columns_impl(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        let pool = self.pool_for_target(handle, database).await?;
        let schema_ref = schema.map(str::trim).filter(|s| !s.is_empty());

        // `$1 IS NULL` means "every user schema", matching `get_tables` so the
        // ER diagram and the table list always describe the same set.
        let rows = sqlx::query(
            r#"
            SELECT
                c.table_schema,
                c.table_name,
                c.column_name,
                c.data_type,
                c.is_nullable,
                c.column_default,
                col_description(
                    (quote_ident(c.table_schema)||'.'||quote_ident(c.table_name))::regclass,
                    c.ordinal_position
                ) AS comment,
                CASE WHEN pk.column_name IS NOT NULL THEN true ELSE false END AS is_primary_key
            FROM information_schema.columns c
            LEFT JOIN (
                SELECT ku.table_schema, ku.table_name, ku.column_name
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage ku
                  ON ku.constraint_name = tc.constraint_name
                 AND ku.table_schema = tc.table_schema
                WHERE tc.constraint_type = 'PRIMARY KEY'
            ) pk ON c.table_schema = pk.table_schema
                AND c.table_name = pk.table_name
                AND c.column_name = pk.column_name
            WHERE ($1::text IS NULL OR c.table_schema = $1)
              AND c.table_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY c.table_schema, c.table_name, c.ordinal_position
            "#,
        )
        .bind(schema_ref)
        .fetch_all(&pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let mut result: HashMap<String, (Vec<ColumnSchema>, Vec<String>)> = HashMap::new();
        let mut owners: HashMap<String, String> = HashMap::new();

        for row in &rows {
            let table_name: String = row.get("table_name");
            let table_schema: String = row.get("table_schema");
            let col_name: String = row.get("column_name");
            let nullable: String = row.get("is_nullable");
            let is_pk: bool = row.get("is_primary_key");

            // The payload is keyed by bare table name, so two same-named tables
            // in different schemas cannot both be represented. Keep the first
            // and say so rather than merging their columns into one table.
            if let Some(existing) = owners.get(&table_name) {
                if existing != &table_schema {
                    tracing::warn!(
                        table = %table_name,
                        kept = %existing,
                        skipped = %table_schema,
                        "get_all_columns: same-named table in another schema skipped"
                    );
                }
                continue;
            }
            owners.insert(table_name.clone(), table_schema);

            let column = ColumnSchema {
                name: col_name.clone(),
                data_type: row.get("data_type"),
                nullable: nullable == "YES",
                default_value: row.get("column_default"),
                comment: row.get("comment"),
                is_primary_key: is_pk,
                is_auto_increment: false,
            };

            let entry = result.entry(table_name).or_default();
            entry.0.push(column);
            if is_pk {
                entry.1.push(col_name);
            }
        }

        Ok(result)
    }
}

/// Collapse a foreign key's column arrays down to their ordered distinct columns.
///
/// `information_schema` exposes the two sides of a foreign key as independent
/// aggregates, so a composite key is reported multiplied: a two-column key
/// arrives as `["pa", "pa", "pb", "pb"]` against `["a", "b", "a", "b"]`.
/// Consumers pair the arrays positionally, so that bloat silently becomes a
/// cartesian product — four predicates for a two-column key, i.e. a wrong JOIN.
///
/// Returns `None` when the sides cannot be reconciled (a different number of
/// distinct columns), letting the caller skip the constraint instead of
/// inventing a pairing.
pub(crate) fn normalise_fk_columns(
    columns: Vec<String>,
    referenced_columns: Vec<String>,
) -> Option<(Vec<String>, Vec<String>)> {
    use std::collections::HashSet;

    let distinct = |values: Vec<String>| -> Vec<String> {
        let mut seen: HashSet<String> = HashSet::new();
        values
            .into_iter()
            .filter(|value| seen.insert(value.clone()))
            .collect()
    };

    let from = distinct(columns);
    let to = distinct(referenced_columns);
    if from.is_empty() || from.len() != to.len() {
        return None;
    }
    Some((from, to))
}

#[cfg(test)]
mod schema_tests {
    use super::normalise_fk_columns;

    fn owned(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn composite_key_is_collapsed_from_the_cartesian_bloat() {
        // What information_schema actually returns for (pa, pb) -> (a, b).
        let columns = owned(&["pa", "pa", "pb", "pb"]);
        let referenced = owned(&["a", "b", "a", "b"]);

        let (from, to) = normalise_fk_columns(columns, referenced).expect("reconcilable");

        assert_eq!(from, owned(&["pa", "pb"]));
        assert_eq!(to, owned(&["a", "b"]));
    }

    #[test]
    fn three_column_key_is_collapsed() {
        let columns = owned(&["c1", "c1", "c1", "c2", "c2", "c2", "c3", "c3", "c3"]);
        let referenced = owned(&["p1", "p2", "p3", "p1", "p2", "p3", "p1", "p2", "p3"]);

        let (from, to) = normalise_fk_columns(columns, referenced).expect("reconcilable");

        assert_eq!(from, owned(&["c1", "c2", "c3"]));
        assert_eq!(to, owned(&["p1", "p2", "p3"]));
    }

    #[test]
    fn single_column_key_is_unchanged() {
        let (from, to) =
            normalise_fk_columns(owned(&["customer_id"]), owned(&["id"])).expect("reconcilable");
        assert_eq!(from, owned(&["customer_id"]));
        assert_eq!(to, owned(&["id"]));
    }

    #[test]
    fn preserves_positional_order() {
        let (from, to) =
            normalise_fk_columns(owned(&["b", "a"]), owned(&["pb", "pa"])).expect("reconcilable");
        assert_eq!(from, owned(&["b", "a"]));
        assert_eq!(to, owned(&["pb", "pa"]));
    }

    #[test]
    fn unreconcilable_sides_are_rejected_rather_than_guessed() {
        assert!(normalise_fk_columns(owned(&["a", "b"]), owned(&["x"])).is_none());
        assert!(normalise_fk_columns(Vec::new(), Vec::new()).is_none());
    }
}
