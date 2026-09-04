//! PostgreSQL catalog-backed DDL generation.

use crate::postgres::PostgresDriver;
use crate::sql::collect_named_ddl_column;
use crate::structure::{caps_for_version, plan_structure_changes_with_caps};
use datazen_driver_api::*;
use sqlx::{PgPool, Row};

/// One column line for CREATE TABLE assembly (catalog-backed DDL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PgColumnDdl {
    pub name: String,
    pub data_type: String,
    pub not_null: bool,
    pub default_expr: Option<String>,
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
                    line.push_str(&format!(" DEFAULT {def}"));
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
    pool: &PgPool,
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
          a.attname AS owned_column,
          pgs.last_value,
          pgs.is_called
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
        LEFT JOIN pg_sequences pgs
          ON pgs.schemaname = ns.nspname AND pgs.sequencename = seq_cls.relname
        WHERE tbl.oid = $1::regclass
        ORDER BY seq_cls.relname
        "#,
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    Ok(rows
        .iter()
        .map(|r| {
            let increment: i64 = r.get("seqincrement");
            let start: i64 = r.get("seqstart");
            PgSequenceDdl {
                qualified_name: r.get("qualified_name"),
                data_type: r.get("data_type"),
                increment,
                min_value: r.get("seqmin"),
                max_value: r.get("seqmax"),
                start: pg_sequence_start(
                    r.try_get::<Option<i64>, _>("last_value").ok().flatten(),
                    r.try_get::<Option<bool>, _>("is_called").ok().flatten(),
                    start,
                    increment,
                ),
                cache: r.get("seqcache"),
                cycle: r.get("seqcycle"),
                owned_column: r
                    .try_get::<Option<String>, _>("owned_column")
                    .ok()
                    .flatten(),
            }
        })
        .collect())
}

pub(crate) async fn fetch_pg_table_ddl_from_catalog(
    pool: &PgPool,
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
    .fetch_all(pool)
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
    .fetch_all(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let pk_columns: Vec<String> = pk_rows.iter().map(|r| r.get("attname")).collect();

    let sequences = fetch_pg_table_sequences(pool, table)
        .await
        .unwrap_or_default();
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

impl PostgresDriver {
    pub(crate) async fn dump_table_ddl_impl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<String, DriverError> {
        let catalog_result = {
            let pools = self.pools.read().await;
            let pool = Self::get_pool(&pools, handle)?;
            fetch_pg_table_ddl_from_catalog(pool, table, |n| self.quote_ident(n)).await
        };
        match catalog_result {
            Ok(ddl) => Ok(ddl),
            Err(_) => sql_dump::dump_table_ddl_from_schema(self, handle, table).await,
        }
    }

pub(crate) async fn dump_view_ddl_impl(
        &self,
        handle: &ConnectionHandle,
        view: &str,
    ) -> Result<String, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let row = sqlx::query("SELECT pg_get_viewdef($1::regclass, true) AS def")
            .bind(view)
            .fetch_one(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let def: String = row.try_get("def").unwrap_or_default();
        if def.trim().is_empty() {
            return Err(DriverError::QueryFailed(format!(
                "View definition not found: {view}"
            )));
        }
        Ok(format!(
            "CREATE OR REPLACE VIEW {} AS\n{};\n",
            self.quote_ident(view),
            def.trim().trim_end_matches(';')
        ))
    }

pub(crate) async fn dump_routines_impl(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        let result = self
            .query(
                handle,
                "SELECT n.nspname AS schema, p.proname AS name, \
                 pg_get_functiondef(p.oid) AS ddl \
                 FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
                 WHERE n.nspname NOT IN ('pg_catalog','information_schema') \
                   AND p.prokind IN ('f', 'p') \
                 ORDER BY 1, 2",
            )
            .await?;
        Ok(collect_named_ddl_column(&result, "ddl", "ROUTINE"))
    }

pub(crate) async fn dump_triggers_impl(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        let result = self
            .query(
                handle,
                "SELECT n.nspname AS schema, t.tgname AS name, \
                 pg_get_triggerdef(t.oid) AS ddl \
                 FROM pg_trigger t \
                 JOIN pg_class c ON c.oid = t.tgrelid \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE NOT t.tgisinternal \
                   AND n.nspname NOT IN ('pg_catalog','information_schema') \
                 ORDER BY 1, 2",
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
        let snapshot = match self.begin_transaction(handle).await {
            Ok(tx) => {
                let _ = self
                    .execute(
                        handle,
                        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY",
                    )
                    .await;
                Some(tx)
            }
            Err(_) => None,
        };
        let result = async {
            let mut out = String::new();
            if opts.create_database {
                // No `\connect` — restore runs against the existing session.
                out.push_str(&format!(
                    "CREATE DATABASE {};\n",
                    self.quote_ident(database)
                ));
            }
            out.push_str(
                &sql_dump::dump_sql_database_with_progress(
                    self,
                    handle,
                    database,
                    opts,
                    on_progress,
                )
                .await?,
            );
            Ok(out)
        }
        .await;
        if let Some(tx) = snapshot {
            if result.is_ok() {
                let _ = self.commit(tx).await;
            } else {
                let _ = self.rollback(tx).await;
            }
        }
        result
    }

pub(crate) async fn execute_command_impl(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        match execute_standard_sql_command(self, handle, command, input.clone()).await {
            Err(DriverError::Unsupported(_)) => {}
            other => return other,
        }
        if let Some(result) =
            try_execute_schema_catalog_command(self, handle, command, input.clone()).await?
        {
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
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        crate::admin_commands::execute_pg_admin_command(pool, command, input).await
    }

    pub(crate) async fn structure_capabilities_impl(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        let info = self.get_server_info(handle).await?;
        Ok(caps_for_version(&info.server_version))
    }

    pub(crate) async fn plan_structure_changes_impl(
        &self,
        handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        let caps = self.structure_capabilities_impl(handle).await?;
        plan_structure_changes_with_caps(&caps, request)
    }
}
