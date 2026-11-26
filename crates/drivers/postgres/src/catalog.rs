//! PostgreSQL catalog-backed DDL generation.

use crate::postgres::PostgresDriver;
use crate::sql::{collect_named_ddl_column, pg_regclass_name};
use crate::structure::{
    caps_for_version, plan_structure_changes_with_primary_key, primary_key_changed, qualified_table,
};
use datazen_driver_api::*;
use sqlx::Row;

/// One column line for CREATE TABLE assembly (catalog-backed DDL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PgColumnDdl {
    pub name: String,
    pub data_type: String,
    pub not_null: bool,
    pub default_expr: Option<String>,
}

/// The schema part of a qualified name, quoted or bare.
///
/// `quote_ident` only quotes what needs quoting, so the catalog hands back
/// `public.t` for an ordinary table and `"My Schema".t` otherwise; both are
/// split at the first dot that is outside quotes.
fn schema_of(qualified_name: &str) -> Option<&str> {
    let mut quoted = false;
    for (index, ch) in qualified_name.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '.' if !quoted => return Some(&qualified_name[..index]),
            _ => {}
        }
    }
    None
}

/// Qualify the `regclass` literal inside a default expression.
///
/// `nextval('t_id_seq'::regclass)` becomes `nextval('"public"."t_id_seq"'::regclass)`
/// when the table lives in `public` and the literal carries no schema of its
/// own. A literal that is already schema-qualified (`public.t_id_seq`) is left
/// alone, and so is every other default expression.
pub(crate) fn qualify_regclass_literals(expression: &str, schema: Option<&str>) -> String {
    const MARKER: &str = "::regclass";
    let Some(schema) = schema else {
        return expression.to_string();
    };
    let parts: Vec<&str> = expression.split(MARKER).collect();
    if parts.len() == 1 {
        return expression.to_string();
    }
    let mut out = String::with_capacity(expression.len() + schema.len() + 4);
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            out.push_str(MARKER);
        }
        // Only the segment that ends in a `regclass` cast has a literal to
        // qualify, and only when it is a bare name directly inside a call:
        // `nextval('<literal>'` → the trailing quote closes it, so the name is
        // what sits between the quote before it and that closer.
        let literal = if index + 1 < parts.len() {
            part.trim_end().strip_suffix('\'').and_then(|body| {
                let (head, literal) = body.rsplit_once('\'')?;
                (!literal.is_empty() && !literal.contains('.') && head.trim_end().ends_with('('))
                    .then_some((head, literal))
            })
        } else {
            None
        };
        match literal {
            Some((head, literal)) => {
                out.push_str(head);
                out.push('\'');
                out.push_str(schema);
                out.push('.');
                out.push_str(literal);
                out.push('\'');
            }
            None => out.push_str(part),
        }
    }
    out
}

/// Build `CREATE TABLE schema.table (...)` from catalog-derived column metadata.
pub(crate) fn build_pg_create_table_ddl(
    qualified_name: &str,
    columns: &[PgColumnDdl],
    pk_columns: &[String],
    quote_ident: &dyn Fn(&str) -> String,
) -> String {
    let mut parts: Vec<String> = columns
        .iter()
        .map(|c| {
            let mut line = format!("  {} {}", quote_ident(&c.name), c.data_type);
            if c.not_null {
                line.push_str(" NOT NULL");
            }
            if let Some(ref def) = c.default_expr {
                if !def.is_empty() {
                    // `pg_get_expr` returns the catalog's own text, whose
                    // `nextval('x'::regclass)` is written against the *dumping*
                    // session's `search_path`. A restore runs with a different
                    // one (often none), where that unqualified name resolves to
                    // nothing and the whole `CREATE TABLE` is rejected. Qualify
                    // the literal with the table's own schema, the way a real
                    // `pg_dump` does.
                    line.push_str(&format!(
                        " DEFAULT {}",
                        qualify_regclass_literals(def, schema_of(qualified_name))
                    ));
                }
            }
            line
        })
        .collect();

    if !pk_columns.is_empty() {
        let pk_list: Vec<String> = pk_columns.iter().map(|n| quote_ident(n)).collect();
        parts.push(format!("  PRIMARY KEY ({})", pk_list.join(", ")));
    }

    format!(
        "CREATE TABLE {qualified_name} (\n{}\n);\n",
        parts.join(",\n")
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PgSequenceDdl {
    pub qualified_name: String,
    pub data_type: String,
    pub increment: i64,
    pub min_value: i64,
    pub max_value: i64,
    pub start: i64,
    pub cache: i64,
    pub cycle: bool,
    pub owned_column: Option<String>,
}

pub(crate) fn build_pg_create_sequence_sql(seq: &PgSequenceDdl) -> String {
    let cycle = if seq.cycle { "CYCLE" } else { "NO CYCLE" };
    format!(
        "CREATE SEQUENCE IF NOT EXISTS {}\n    AS {}\n    INCREMENT BY {}\n    MINVALUE {}\n    MAXVALUE {}\n    START WITH {}\n    CACHE {}\n    {cycle};\n",
        seq.qualified_name,
        seq.data_type,
        seq.increment,
        seq.min_value,
        seq.max_value,
        seq.start,
        seq.cache,
    )
}

pub(crate) fn build_pg_alter_sequence_owned_by(
    seq: &PgSequenceDdl,
    table_qualified: &str,
    quote_ident: &dyn Fn(&str) -> String,
) -> Option<String> {
    seq.owned_column.as_ref().map(|col| {
        format!(
            "ALTER SEQUENCE {} OWNED BY {}.{};\n",
            seq.qualified_name,
            table_qualified,
            quote_ident(col)
        )
    })
}

pub(crate) fn pg_sequence_start(
    last_value: Option<i64>,
    is_called: Option<bool>,
    start: i64,
    increment: i64,
) -> i64 {
    match (last_value, is_called) {
        (Some(last), Some(true)) => last.saturating_add(increment),
        (Some(last), Some(false)) => last,
        _ => start,
    }
}

async fn fetch_pg_table_sequences(
    connection: &mut sqlx::PgConnection,
    table: &str,
) -> Result<Vec<PgSequenceDdl>, DriverError> {
    let rows = sqlx::query(
        r#"
        SELECT
          quote_ident(ns.nspname) || '.' || quote_ident(seq_cls.relname) AS qualified_name,
          format_type(s.seqtypid, NULL) AS data_type,
          s.seqstart,
          s.seqincrement,
          s.seqmin,
          s.seqmax,
          s.seqcache,
          s.seqcycle,
          a.attname AS owned_column
        FROM pg_class tbl
        JOIN pg_depend d
          ON d.refobjid = tbl.oid
         AND d.classid = 'pg_class'::regclass
         AND d.deptype IN ('a', 'i')
        JOIN pg_class seq_cls
          ON seq_cls.oid = d.objid AND seq_cls.relkind = 'S'
        JOIN pg_namespace ns ON ns.oid = seq_cls.relnamespace
        JOIN pg_sequence s ON s.seqrelid = seq_cls.oid
        LEFT JOIN pg_attribute a
          ON a.attrelid = tbl.oid AND a.attnum = d.refobjsubid AND NOT a.attisdropped
        WHERE tbl.oid = $1::regclass
        ORDER BY seq_cls.relname
        "#,
    )
    .bind(table)
    .fetch_all(&mut *connection)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let mut sequences = Vec::with_capacity(rows.len());
    for row in &rows {
        let qualified_name: String = row
            .try_get("qualified_name")
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        // pg_sequences has no is_called column. Read each quoted sequence on
        // this same retained connection to preserve its actual next-value state.
        let state = sqlx::query(&format!(
            "SELECT last_value, is_called FROM {qualified_name}"
        ))
        .fetch_one(&mut *connection)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let increment: i64 = row.get("seqincrement");
        let start: i64 = row.get("seqstart");
        sequences.push(PgSequenceDdl {
            qualified_name,
            data_type: row.get("data_type"),
            increment,
            min_value: row.get("seqmin"),
            max_value: row.get("seqmax"),
            start: pg_sequence_start(
                Some(
                    state
                        .try_get("last_value")
                        .map_err(|e| DriverError::QueryFailed(e.to_string()))?,
                ),
                Some(
                    state
                        .try_get("is_called")
                        .map_err(|e| DriverError::QueryFailed(e.to_string()))?,
                ),
                start,
                increment,
            ),
            cache: row.get("seqcache"),
            cycle: row.get("seqcycle"),
            owned_column: row
                .try_get::<Option<String>, _>("owned_column")
                .ok()
                .flatten(),
        });
    }
    Ok(sequences)
}

pub(crate) async fn fetch_pg_table_ddl_on_connection(
    connection: &mut sqlx::PgConnection,
    table: &str,
    quote_ident: impl Fn(&str) -> String,
) -> Result<String, DriverError> {
    let col_rows = sqlx::query(
        r#"
        SELECT
          quote_ident(n.nspname) || '.' || quote_ident(c.relname) AS qualified_name,
          a.attname,
          format_type(a.atttypid, a.atttypmod) AS col_type,
          a.attnotnull AS not_null,
          pg_get_expr(d.adbin, d.adrelid) AS col_default
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_attribute a ON a.attrelid = c.oid
        LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
        WHERE c.oid = $1::regclass
          AND a.attnum > 0
          AND NOT a.attisdropped
        ORDER BY a.attnum
        "#,
    )
    .bind(table)
    .fetch_all(&mut *connection)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    if col_rows.is_empty() {
        return Err(DriverError::QueryFailed(format!(
            "Table not found or has no columns: {table}"
        )));
    }

    let qualified_name: String = col_rows[0].get("qualified_name");
    let columns: Vec<PgColumnDdl> = col_rows
        .iter()
        .map(|r| PgColumnDdl {
            name: r.get("attname"),
            data_type: r.get("col_type"),
            not_null: r.get("not_null"),
            default_expr: r.try_get("col_default").ok(),
        })
        .collect();

    let pk_rows = sqlx::query(
        r#"
        SELECT a.attname
        FROM pg_constraint con
        JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = ANY(con.conkey)
        WHERE con.contype = 'p'
          AND con.conrelid = $1::regclass
        ORDER BY array_position(con.conkey, a.attnum)
        "#,
    )
    .bind(table)
    .fetch_all(&mut *connection)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let pk_columns: Vec<String> = pk_rows.iter().map(|r| r.get("attname")).collect();

    let sequences = fetch_pg_table_sequences(connection, table).await?;
    let mut out = String::new();
    for seq in &sequences {
        out.push_str(&build_pg_create_sequence_sql(seq));
        out.push('\n');
    }

    out.push_str(&build_pg_create_table_ddl(
        &qualified_name,
        &columns,
        &pk_columns,
        &quote_ident,
    ));

    for seq in &sequences {
        if let Some(sql) = build_pg_alter_sequence_owned_by(seq, &qualified_name, &quote_ident) {
            out.push_str(&sql);
        }
    }

    Ok(out)
}

/// Retrieve a portable view query on the already-retained backend.
pub(crate) async fn fetch_pg_view_definition(
    connection: &mut sqlx::PgConnection,
    qualified: &str,
) -> Result<(String, bool), DriverError> {
    // Deparsing with the source session's search_path can omit schema
    // qualifiers that a different restore session needs. Resolve the
    // intended view first, then deparse with only pg_catalog visible.
    // OFFSET 0 keeps the two read CTEs separate when PostgreSQL inlines
    // CTEs; the config setters themselves are volatile.
    let row = sqlx::query(
        "WITH saved AS ( \
           SELECT pg_catalog.current_setting('search_path') AS path, $1::pg_catalog.regclass AS view_oid OFFSET 0 \
         ), restricted AS ( \
           SELECT path, view_oid, pg_catalog.set_config('search_path', 'pg_catalog', true) FROM saved \
         ), definition AS ( \
           SELECT path, view_oid, pg_catalog.pg_get_viewdef(view_oid, true) AS def FROM restricted OFFSET 0 \
         ), restored AS ( \
           SELECT def, view_oid, pg_catalog.set_config('search_path', path, true) FROM definition \
         ) SELECT def, (SELECT c.relkind = 'm' FROM pg_catalog.pg_class c WHERE c.oid = restored.view_oid) AS materialized FROM restored",
    )
    .bind(qualified)
    .fetch_one(connection)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    let def: String = row.try_get("def").unwrap_or_default();
    if def.trim().is_empty() {
        return Err(DriverError::QueryFailed(format!(
            "View definition not found: {qualified}"
        )));
    }
    let materialized = row
        .try_get("materialized")
        .map_err(|error| DriverError::QueryFailed(error.to_string()))?;
    Ok((def, materialized))
}

// Deparse trigger functions against the catalog-only path and restore it even
// for an empty catalog. DDL is the first column for the retained stream.
pub(crate) const PG_TRIGGER_DDL_SQL: &str = "WITH saved AS ( \
   SELECT pg_catalog.current_setting('search_path') AS path OFFSET 0 \
 ), restricted AS ( \
   SELECT path, pg_catalog.set_config('search_path', 'pg_catalog', true) FROM saved \
 ), definitions AS ( \
   SELECT path, (SELECT pg_catalog.jsonb_agg(pg_catalog.jsonb_build_object( \
       'schema', n.nspname, 'name', t.tgname, 'ddl', pg_catalog.pg_get_triggerdef(t.oid)) \
     ORDER BY n.nspname, c.relname, t.tgname) \
     FROM pg_catalog.pg_trigger t \
     JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid \
     JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
     WHERE NOT t.tgisinternal \
     AND n.nspname NOT IN ('pg_catalog','information_schema')) AS entries \
   FROM restricted OFFSET 0 \
 ), restored AS ( \
   SELECT entries, pg_catalog.set_config('search_path', path, true) FROM definitions \
 ) SELECT trigger.ddl, trigger.schema, trigger.name FROM restored \
   LEFT JOIN LATERAL pg_catalog.jsonb_array_elements(restored.entries) \
     WITH ORDINALITY AS item(record, ord) ON true \
   LEFT JOIN LATERAL pg_catalog.jsonb_to_record(item.record) \
     AS trigger(schema text, name text, ddl text) ON true \
   WHERE trigger.ddl IS NOT NULL ORDER BY item.ord";

impl PostgresDriver {
    pub(crate) async fn dump_table_ddl_impl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        let (schema, bare_table) = crate::sql::resolve_pg_table_schema(table, schema);
        let qualified = pg_regclass_name(schema, bare_table);
        let catalog_result = {
            let mut connection =
                crate::session_connection::for_session(self, handle, database).await?;
            fetch_pg_table_ddl_on_connection(&mut connection, &qualified, |n| self.quote_ident(n))
                .await
        };
        match catalog_result {
            Ok(ddl) => Ok(ddl),
            Err(_) => {
                sql_dump::dump_table_ddl_from_schema(self, handle, table, database, schema).await
            }
        }
    }

    pub(crate) async fn dump_view_ddl_impl(
        &self,
        handle: &ConnectionHandle,
        view: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        let (schema, bare_view) = crate::sql::resolve_pg_table_schema(view, schema);
        let qualified = pg_regclass_name(schema, bare_view);
        let mut connection = crate::session_connection::for_session(self, handle, database).await?;
        let (def, materialized) = fetch_pg_view_definition(&mut connection, &qualified).await?;
        let command = if materialized {
            "CREATE MATERIALIZED VIEW"
        } else {
            "CREATE OR REPLACE VIEW"
        };
        Ok(format!(
            "{command} {qualified} AS\n{};\n",
            def.trim().trim_end_matches(';')
        ))
    }

    pub(crate) async fn dump_routines_impl(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<String, DriverError> {
        let result = self
            .query_at(
                handle,
                "SELECT n.nspname AS schema, p.proname AS name, \
                 pg_get_functiondef(p.oid) AS ddl \
                 FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
                 WHERE n.nspname NOT IN ('pg_catalog','information_schema') \
                   AND p.prokind IN ('f', 'p') \
                 ORDER BY 1, 2",
                SqlTarget::new(Some(database), None),
            )
            .await?;
        Ok(collect_named_ddl_column(&result, "ddl", "ROUTINE"))
    }

    pub(crate) async fn dump_triggers_impl(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<String, DriverError> {
        let result = self
            .query_at(
                handle,
                PG_TRIGGER_DDL_SQL,
                SqlTarget::new(Some(database), None),
            )
            .await?;
        Ok(collect_named_ddl_column(&result, "ddl", "TRIGGER"))
    }

    pub(crate) async fn dump_database_with_progress_impl(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        // Do NOT wrap the dump in a REPEATABLE READ transaction.  A single
        // failed query (e.g. a table the user lacks SELECT on) would poison
        // the entire transaction with "current transaction is aborted",
        // preventing the rest of the backup from completing.  Running in
        // auto-commit mode lets each object succeed or fail independently.
        let mut out = String::new();
        if opts.create_database {
            out.push_str(&format!(
                "CREATE DATABASE {};\n",
                self.quote_ident(database)
            ));
        }
        out.push_str(
            &sql_dump::dump_sql_database_with_progress(self, handle, database, opts, on_progress)
                .await?,
        );
        Ok(out)
    }

    pub(crate) async fn execute_command_impl(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        mut input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        if command == "get_object_ddl"
            && input.get("kind").and_then(serde_json::Value::as_str)
                .and_then(datazen_driver_api::schema_objects::ObjectKind::parse)
                == Some(datazen_driver_api::schema_objects::ObjectKind::Table)
        {
            // PostgreSQL table definitions need bound catalog lookups rather
            // than the legacy text-only object SQL provider.
            return crate::table_object_ddl::table_object_ddl(self, handle, &input).await;
        }
        if command == "drop_database" {
            // PostgreSQL cannot drop the currently-open database, nor one that still
            // has active backends. Handle it on the driver so we can move our own pool
            // off the target and terminate remaining connections first.
            return crate::admin_commands::drop_database_impl(self, handle, &input).await;
        }
        match execute_standard_sql_command(self, handle, command, input.clone()).await {
            Err(DriverError::Unsupported(_)) => {}
            other => return other,
        }
        if command == "get_table_schema" {
            if let Some(raw) = input.get("relationSql").and_then(serde_json::Value::as_str) {
                let parsed = sqlparser::parser::Parser::parse_sql(
                    &sqlparser::dialect::PostgreSqlDialect {},
                    &format!("SELECT * FROM {raw}"),
                )
                .map_err(|_| DriverError::InvalidConfig("invalid relation identifier".into()))?;
                let [sqlparser::ast::Statement::Query(query)] = parsed.as_slice() else {
                    return Err(DriverError::InvalidConfig(
                        "one relation is required".into(),
                    ));
                };
                let sqlparser::ast::SetExpr::Select(select) = &*query.body else {
                    return Err(DriverError::InvalidConfig(
                        "one relation is required".into(),
                    ));
                };
                let [relation] = select.from.as_slice() else {
                    return Err(DriverError::InvalidConfig(
                        "one relation is required".into(),
                    ));
                };
                if !relation.joins.is_empty() {
                    return Err(DriverError::InvalidConfig(
                        "one named relation is required; joins have no single table schema".into(),
                    ));
                }
                let sqlparser::ast::TableFactor::Table {
                    name, args: None, ..
                } = &relation.relation
                else {
                    return Err(DriverError::InvalidConfig(
                        "one named relation is required".into(),
                    ));
                };
                let parts = name
                    .0
                    .iter()
                    .map(|part| {
                        part.as_ident()
                            .map(|ident| {
                                if ident.quote_style.is_none() {
                                    ident.value.to_lowercase()
                                } else {
                                    ident.value.clone()
                                }
                            })
                            .ok_or_else(|| {
                                DriverError::InvalidConfig("named relation is required".into())
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if input
                    .get("database")
                    .and_then(serde_json::Value::as_str)
                    .is_none()
                {
                    let context = self.read_fixed_session(handle).await?;
                    input["database"] =
                        serde_json::json!(context.namespace.database.ok_or_else(|| {
                            DriverError::InvalidConfig("database is required".into())
                        })?);
                }
                match parts.as_slice() {
                    [table] => {
                        input["table"] = serde_json::json!(table);
                        input["schema"] = serde_json::Value::Null;
                    }
                    [schema, table] => {
                        input["table"] = serde_json::json!(table);
                        input["schema"] = serde_json::json!(schema);
                    }
                    [database, schema, table] => {
                        input["database"] = serde_json::json!(database);
                        input["table"] = serde_json::json!(table);
                        input["schema"] = serde_json::json!(schema);
                    }
                    _ => {
                        return Err(DriverError::InvalidConfig(
                            "unsupported relation namespace".into(),
                        ))
                    }
                }
            }
        }
        let raw_relation_result = if command == "get_table_schema"
            && input.get("relationSql").is_some()
            && self.fixed_sessions.read().await.contains_key(&handle.id)
        {
            let database = input["database"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("database is required".into()))?;
            self.ensure_transaction_reaches(handle, SqlTarget::new(Some(database), None))
                .await?;
            let table = input["table"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("table is required".into()))?;
            let schema = input.get("schema").and_then(serde_json::Value::as_str);
            let pinned = self
                .transactions
                .lock()
                .await
                .get(&handle.id)
                .cloned()
                .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
            let mut conn = pinned.lock().await;
            let schema = self
                .table_schema_exact_on_connection(&mut **conn, table, schema)
                .await?;
            Some(CommandResult::new(serde_json::json!({"schema": schema})))
        } else {
            None
        };
        if let Some(mut result) = match raw_relation_result {
            Some(result) => Some(result),
            None => {
                try_execute_schema_catalog_command(self, handle, command, input.clone()).await?
            }
        } {
            if command == "get_table_schema"
                && self.fixed_sessions.read().await.contains_key(&handle.id)
            {
                let table = input["table"]
                    .as_str()
                    .ok_or_else(|| DriverError::InvalidConfig("table is required".into()))?;
                let schema = input.get("schema").and_then(serde_json::Value::as_str);
                let (schema, table) = if input.get("relationSql").is_some() {
                    (schema, table)
                } else {
                    crate::sql::resolve_pg_table_schema(table, schema)
                };
                let relation = crate::sql::pg_regclass_name(schema, table);
                let pinned = self
                    .transactions
                    .lock()
                    .await
                    .get(&handle.id)
                    .cloned()
                    .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
                let mut conn = pinned.lock().await;
                let row = sqlx::query("SELECT current_database() AS database, n.nspname::text AS schema, c.relname::text AS table, c.relpersistence = 't' AS session_scoped FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace WHERE c.oid = to_regclass($1)")
                    .bind(relation).fetch_one(&mut **conn).await.map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                result.data["resolvedTarget"] = serde_json::json!({ "database": row.try_get::<String,_>("database").map_err(|e| DriverError::QueryFailed(e.to_string()))?, "catalog": null, "schema": row.try_get::<String,_>("schema").map_err(|e| DriverError::QueryFailed(e.to_string()))?, "table": row.try_get::<String,_>("table").map_err(|e| DriverError::QueryFailed(e.to_string()))?, "path": [], "sessionScoped": row.try_get::<bool,_>("session_scoped").map_err(|e| DriverError::QueryFailed(e.to_string()))? });
            }
            return Ok(result);
        }
        if is_schema_object_command(command) {
            return execute_schema_object_command(
                self,
                &self.driver_type(),
                handle,
                command,
                input,
            )
            .await;
        }
        // Admin commands run on the **session's own** connection: a retained
        // session pins the pool's only connection, so borrowing a second one
        // here would wait for a checkout that never returns. `for_session`
        // hands back the pinned connection when there is one (and a separate
        // per-database pool when the command names a foreign database), which
        // keeps `estimate_table_rows` and the `GRANT`/`CREATE` family scoped to
        // the database the user is actually looking at.
        let database = input["database"].as_str().unwrap_or_default().to_owned();
        let mut conn = crate::session_connection::for_session(self, handle, &database).await?;
        crate::admin_commands::execute_pg_admin_command(&mut conn, command, input).await
    }

    pub(crate) async fn structure_capabilities_impl(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        // Capabilities depend only on server version. Read this server-global
        // fact through the shared control connection so opening the structure
        // editor cannot stall behind table browsing or query streams that have
        // saturated the user's data pool.
        let pool = self
            .control_pool_for(handle)
            .await
            .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
        let row = sqlx::query("SELECT pg_catalog.version()")
            .fetch_one(&pool)
            .await
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?;
        let version: String = row.try_get(0).unwrap_or_default();
        Ok(caps_for_version(&version))
    }

    pub(crate) async fn plan_structure_changes_impl(
        &self,
        handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        let caps = self.structure_capabilities_impl(handle).await?;
        let primary_key_name = if request.mode == StructureChangeMode::Alter
            && primary_key_changed(request)
            && request
                .original_columns
                .iter()
                .any(|column| column.is_primary_key)
        {
            let mut connection = crate::session_connection::for_session(self, handle, "").await?;
            let relation = qualified_table(request.schema.as_deref(), &request.table);
            let key = sqlx::query_as::<_, (String, Vec<String>)>(
                r#"SELECT k.conname::text, ARRAY(
                       SELECT a.attname::text FROM pg_catalog.pg_attribute a
                       WHERE a.attrelid = k.conrelid AND a.attnum = ANY(k.conkey)
                   )
                   FROM pg_catalog.pg_constraint k
                   WHERE k.conrelid = pg_catalog.to_regclass($1) AND k.contype = 'p'"#,
            )
            .bind(&relation)
            .fetch_optional(&mut *connection)
            .await
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?;
            let expected = request
                .original_columns
                .iter()
                .filter(|c| c.is_primary_key)
                .map(|c| c.name.as_str())
                .collect::<std::collections::HashSet<_>>();
            let Some((name, _)) = key.filter(|(_, actual)| {
                actual
                    .iter()
                    .map(String::as_str)
                    .collect::<std::collections::HashSet<_>>() == expected
            }) else {
                return Err(DriverError::InvalidConfig(
                    "PostgreSQL primary key changed; reload the structure before applying this plan".into(),
                ));
            };
            Some(name)
        } else {
            None
        };
        plan_structure_changes_with_primary_key(&caps, request, primary_key_name.as_deref())
    }
}
