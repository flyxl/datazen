//! Complete table introspection on one borrowed physical connection.
use super::*;
impl PostgresDriver {
    pub(super) async fn table_schema_on_connection(
        &self,
        conn: &mut sqlx::PgConnection,
        table: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        let (requested_schema, requested_table) = resolve_pg_table_schema(table, schema);
        self.table_schema_exact_on_connection(conn, requested_table, requested_schema)
            .await
    }

    pub(crate) async fn table_schema_exact_on_connection(
        &self,
        conn: &mut sqlx::PgConnection,
        requested_table: &str,
        requested_schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        let requested = pg_regclass_name(requested_schema, requested_table);
        let relation = sqlx::query("SELECT n.nspname::text AS schema_name, c.relname::text AS table_name FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace WHERE c.oid = to_regclass($1)")
            .bind(&requested).fetch_optional(&mut *conn).await.map_err(|e| DriverError::QueryFailed(e.to_string()))?
            .ok_or_else(|| DriverError::QueryFailed("relation does not exist in this physical session".into()))?;
        let schema_name: String = relation
            .try_get("schema_name")
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let table_name: String = relation
            .try_get("table_name")
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let schema = Some(schema_name.as_str());
        let bare_table = table_name.as_str();
        let regclass = pg_regclass_name(schema, bare_table);

        let cols = sqlx::query(
            r#"
            SELECT column_name,
                   CASE WHEN data_type = 'USER-DEFINED'
                        THEN quote_ident(udt_schema) || '.' || quote_ident(udt_name)
                        ELSE data_type
                   END AS migration_data_type,
                   is_nullable, column_default, is_identity,
                   col_description((quote_ident(table_schema)||'.'||quote_ident(table_name))::regclass, ordinal_position) as comment
            FROM information_schema.columns
            WHERE table_name = $1
              AND ($2::text IS NULL OR table_schema = $2)
            ORDER BY ordinal_position
            "#,
        )
        .bind(bare_table)
        .bind(schema)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        if cols.is_empty() {
            return Ok(TableSchema {
                table_name: bare_table.to_string(),
                columns: Vec::new(),
                primary_keys: Vec::new(),
                indexes: Vec::new(),
                foreign_keys: Vec::new(),
                check_constraints: Vec::new(),
                table_options: TableOptions::default(),
            });
        }

        let relation_snapshot_safe = sqlx::query(
            "SELECT c.relkind::text AS relkind, c.relispartition AS is_partition, \
                    EXISTS (SELECT 1 FROM pg_catalog.pg_inherits i \
                            WHERE i.inhrelid = c.oid OR i.inhparent = c.oid) \
                        AS participates_in_inheritance \
             FROM pg_catalog.pg_class c WHERE c.oid = $1::regclass",
        )
        .bind(&regclass)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?
        .is_some_and(|row| {
            let relkind: String = row.get("relkind");
            let is_partition: bool = row.get("is_partition");
            let participates_in_inheritance: bool = row.get("participates_in_inheritance");
            relation_supports_consistent_snapshot(
                &relkind,
                is_partition,
                participates_in_inheritance,
            )
        });

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
        .fetch_all(&mut *conn)
        .await
        .unwrap_or_default();

        let pk_names: Vec<String> = pk_rows.iter().map(|r| r.get::<String, _>(0)).collect();

        let columns: Vec<ColumnSchema> = cols
            .iter()
            .map(|r| {
                let name: String = r.get("column_name");
                let nullable: String = r.get("is_nullable");
                let default_value: Option<String> = r.get("column_default");
                let is_auto_increment = r.get::<String, _>("is_identity") == "YES"
                    || default_value
                        .as_deref()
                        .map(|d| d.contains("nextval("))
                        .unwrap_or(false);
                ColumnSchema {
                    is_primary_key: pk_names.contains(&name),
                    name,
                    data_type: r.get("migration_data_type"),
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
        .fetch_all(&mut *conn)
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
        // Read both sides from pg_constraint's ordinal arrays. Joining the two
        // information_schema column views independently can multiply composite
        // keys, and joining only by constraint name can silently mix same-named
        // constraints from different tables. Catalog OIDs and matching ordinal
        // positions keep each FK tied to its owning table and preserve pairs.
        let fk_rows = sqlx::query(
            r#"
            SELECT
                con.conname::text AS fk_name,
                array_agg(src_att.attname::text ORDER BY src_key.ordinality) AS columns,
                ref_class.relname::text AS ref_table,
                ref_ns.nspname::text AS ref_schema,
                array_agg(ref_att.attname::text ORDER BY src_key.ordinality) AS ref_columns,
                CASE con.confupdtype
                    WHEN 'a' THEN 'NO ACTION'
                    WHEN 'r' THEN 'RESTRICT'
                    WHEN 'c' THEN 'CASCADE'
                    WHEN 'n' THEN 'SET NULL'
                    WHEN 'd' THEN 'SET DEFAULT'
                END::text AS update_rule,
                CASE con.confdeltype
                    WHEN 'a' THEN 'NO ACTION'
                    WHEN 'r' THEN 'RESTRICT'
                    WHEN 'c' THEN 'CASCADE'
                    WHEN 'n' THEN 'SET NULL'
                    WHEN 'd' THEN 'SET DEFAULT'
                END::text AS delete_rule,
                CASE WHEN con.condeferrable THEN 'YES' ELSE 'NO' END::text AS is_deferrable,
                CASE WHEN con.condeferred THEN 'YES' ELSE 'NO' END::text AS initially_deferred
            FROM pg_catalog.pg_constraint con
            JOIN pg_catalog.pg_class src_class ON src_class.oid = con.conrelid
            JOIN pg_catalog.pg_namespace src_ns ON src_ns.oid = src_class.relnamespace
            JOIN pg_catalog.pg_class ref_class ON ref_class.oid = con.confrelid
            JOIN pg_catalog.pg_namespace ref_ns ON ref_ns.oid = ref_class.relnamespace
            JOIN LATERAL unnest(con.conkey) WITH ORDINALITY AS src_key(attnum, ordinality)
              ON true
            JOIN LATERAL unnest(con.confkey) WITH ORDINALITY AS ref_key(attnum, ordinality)
              ON ref_key.ordinality = src_key.ordinality
            JOIN pg_catalog.pg_attribute src_att
              ON src_att.attrelid = src_class.oid AND src_att.attnum = src_key.attnum
            JOIN pg_catalog.pg_attribute ref_att
              ON ref_att.attrelid = ref_class.oid AND ref_att.attnum = ref_key.attnum
            WHERE con.contype = 'f'
              AND src_class.relname = $1
              AND ($2::text IS NULL OR src_ns.nspname = $2)
            GROUP BY con.oid, con.conname, ref_class.relname, ref_ns.nspname, con.confupdtype,
                     con.confdeltype, con.condeferrable, con.condeferred
            ORDER BY con.conname
            "#,
        )
        .bind(bare_table)
        .bind(schema)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let foreign_keys: Vec<ForeignKeyInfo> = fk_rows
            .iter()
            .filter_map(|r| {
                let columns = r.get::<Vec<String>, _>("columns");
                let referenced_columns = r.get::<Vec<String>, _>("ref_columns");
                // Defend the typed schema boundary even though the catalog
                // query pairs each referenced column by ordinality.
                let (columns, referenced_columns) =
                    normalise_fk_columns(columns, referenced_columns)?;
                Some(ForeignKeyInfo {
                    name: r.get("fk_name"),
                    columns,
                    referenced_table: qualified_pg_table_identity(
                        &r.get::<String, _>("ref_schema"),
                        &r.get::<String, _>("ref_table"),
                    ),
                    referenced_columns,
                    on_update: r.get("update_rule"),
                    on_delete: r.get("delete_rule"),
                    deferrability: parse_pg_fk_deferrability(
                        &r.get::<String, _>("is_deferrable"),
                        &r.get::<String, _>("initially_deferred"),
                    ),
                })
            })
            .collect();

        let check_rows = sqlx::query(
            r#"
            SELECT conname::text AS name, pg_get_constraintdef(oid)::text AS definition
            FROM pg_constraint
            WHERE conrelid = $1::regclass AND contype = 'c'
            ORDER BY conname
            "#,
        )
        .bind(&regclass)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let check_constraints = check_rows
            .iter()
            .filter_map(|row| {
                let definition: String = row.get("definition");
                parse_pg_check_definition(&definition).map(|expression| CheckConstraint {
                    name: row.get("name"),
                    expression,
                })
            })
            .collect();

        Ok(TableSchema {
            table_name: bare_table.to_string(),
            columns,
            primary_keys: pk_names,
            indexes,
            foreign_keys,
            check_constraints,
            table_options: TableOptions {
                supports_consistent_snapshot: Some(relation_snapshot_safe),
                ..TableOptions::default()
            },
        })
    }
}
