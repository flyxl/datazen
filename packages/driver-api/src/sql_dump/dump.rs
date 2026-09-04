//! SQL dump generation: DDL, INSERT batches, and full-database export.

use crate::traits::DatabaseDriver;
use crate::types::*;

use super::parser::take_sql_string_literal;

/// Build `CREATE TABLE IF NOT EXISTS` DDL from a table schema (host-compatible).
pub fn build_create_table_sql(
    quote_ident: &dyn Fn(&str) -> String,
    schema: &TableSchema,
) -> String {
    let tname = &schema.table_name;
    let cols_sql: Vec<String> = schema
        .columns
        .iter()
        .map(|c| {
            let mut def = format!("  {} {}", quote_ident(&c.name), c.data_type);
            if !c.nullable {
                def.push_str(" NOT NULL");
            }
            if let Some(ref dv) = c.default_value {
                def.push_str(&format!(" DEFAULT {}", dv));
            }
            def
        })
        .collect();

    let mut create = format!(
        "CREATE TABLE IF NOT EXISTS {} (\n{}",
        quote_ident(tname),
        cols_sql.join(",\n")
    );
    if !schema.primary_keys.is_empty() {
        let pks: Vec<String> = schema.primary_keys.iter().map(|k| quote_ident(k)).collect();
        create.push_str(&format!(",\n  PRIMARY KEY ({})", pks.join(", ")));
    }
    create.push_str("\n);\n");
    create
}

/// Default `dump_table_ddl`: load schema then build CREATE TABLE.
pub async fn dump_table_ddl_from_schema<D>(
    driver: &D,
    handle: &ConnectionHandle,
    table: &str,
) -> Result<String, DriverError>
where
    D: DatabaseDriver + ?Sized,
{
    let schema = driver.get_table_schema(handle, table).await?;
    Ok(build_create_table_sql(&|n| driver.quote_ident(n), &schema))
}

pub fn is_view_like(table_type: &TableType) -> bool {
    matches!(table_type, TableType::View | TableType::MaterializedView)
}

/// Tables first, then views — restore must create base relations before
/// `CREATE VIEW ... AS SELECT FROM <table>`.
pub fn partition_dump_objects(tables: Vec<TableInfo>) -> (Vec<TableInfo>, Vec<TableInfo>) {
    let mut base = Vec::new();
    let mut views = Vec::new();
    for table in tables {
        if is_view_like(&table.table_type) {
            views.push(table);
        } else {
            base.push(table);
        }
    }
    (base, views)
}

/// True when a listed relation may enter the dump pipeline.
///
/// Drivers surface navigation-only rows in [`DatabaseDriver::get_tables`]:
/// PostgreSQL reports one entry with a **blank** name per empty schema (the
/// schema marker, typed [`TableType::SystemTable`]), and MySQL maps catalog
/// `SYSTEM VIEW`s to the same type. Dumping such rows would fabricate
/// statements containing zero-length identifiers
/// (`CREATE TABLE IF NOT EXISTS ""`) that restore then rejects — and system
/// catalog objects are never business data. Skip them before any statement
/// (CREATE / clean DROP / INSERT) is generated.
pub fn is_dumpable_object(table: &TableInfo) -> bool {
    !table.name.trim().is_empty() && !matches!(table.table_type, TableType::SystemTable)
}

/// `schema.name` when `schema` is a real grouping label; otherwise just `name`.
pub fn qualified_ident(
    quote_ident: &dyn Fn(&str) -> String,
    schema: Option<&str>,
    name: &str,
) -> String {
    match schema
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "CATALOG" && *s != "SCHEMA")
    {
        Some(schema) => format!("{}.{}", quote_ident(schema), quote_ident(name)),
        None => quote_ident(name),
    }
}

pub fn drop_object_sql(
    quote_ident: &dyn Fn(&str) -> String,
    name: &str,
    table_type: &TableType,
    schema: Option<&str>,
) -> String {
    let keyword = if is_view_like(table_type) {
        "VIEW"
    } else {
        "TABLE"
    };
    format!(
        "DROP {keyword} IF EXISTS {};\n",
        qualified_ident(quote_ident, schema, name)
    )
}

/// Sequence names from `nextval('categories_id_seq'::regclass)` defaults.
pub fn extract_nextval_sequence_names(sql: &str) -> Vec<String> {
    let mut names = Vec::new();
    let lower = sql.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("nextval") {
        let abs = from + rel + 7;
        let after = sql[abs..].trim_start();
        from = abs;
        let Some(rest) = after.strip_prefix('(') else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(name) = take_sql_string_literal(rest) else {
            continue;
        };
        if !names.iter().any(|n| n == &name) {
            names.push(name);
        }
    }
    names
}

/// Quote a dump/restore sequence ident (`categories_id_seq` or `public.foo_seq`).
pub fn quote_sequence_ident(name: &str) -> String {
    name.split('.')
        .map(|part| {
            let part = part.trim().trim_matches('"');
            format!("\"{}\"", part.replace('"', "\"\""))
        })
        .collect::<Vec<_>>()
        .join(".")
}

async fn dump_one_object<D, F>(
    driver: &D,
    handle: &ConnectionHandle,
    table: &TableInfo,
    opts: &BackupDumpOptions,
    current: u32,
    total: u32,
    out: &mut String,
    on_progress: &mut F,
) -> Result<(), DriverError>
where
    D: DatabaseDriver + ?Sized,
    F: FnMut(DumpProgress),
{
    let tname = &table.name;
    on_progress(DumpProgress {
        current,
        total,
        object_name: tname.clone(),
        phase: DumpPhase::Object,
    });
    let view_like = is_view_like(&table.table_type);
    out.push_str(&format!(
        "-- {}: {}\n",
        if view_like { "View" } else { "Table" },
        tname
    ));

    if !opts.data_only {
        let ddl = if view_like {
            match driver.dump_view_ddl(handle, tname).await {
                Ok(sql) => sql,
                Err(e) => format!("-- View {tname}: skipped DDL ({e})\n"),
            }
        } else {
            driver.dump_table_ddl(handle, tname).await?
        };
        out.push_str(&ddl);
        if !ddl.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }

    if view_like || opts.schema_only {
        return Ok(());
    }

    let schema = driver.get_table_schema(handle, tname).await?;
    let col_names: Vec<String> = schema
        .columns
        .iter()
        .map(|c| driver.quote_ident(&c.name))
        .collect();
    let rel = qualified_ident(&|n| driver.quote_ident(n), table.schema.as_deref(), tname);
    let select_sql = format!("SELECT {} FROM {}", col_names.join(", "), rel);

    match driver.query(handle, &select_sql).await {
        Ok(result) => {
            let tuples: Vec<String> = result
                .rows
                .iter()
                .map(|row| {
                    let vals: Vec<String> =
                        row.iter().map(|v| driver.format_sql_literal(v)).collect();
                    format!("({})", vals.join(", "))
                })
                .collect();
            append_batched_inserts(
                out,
                &rel,
                &col_names.join(", "),
                &tuples,
                INSERT_BATCH_MAX_ROWS,
                INSERT_BATCH_MAX_BYTES,
            );
            if !tuples.is_empty() {
                out.push('\n');
            }
        }
        Err(e) => {
            out.push_str(&format!("-- Error dumping data for {tname}: {e}\n\n"));
        }
    }
    Ok(())
}

/// Rows per multi-value `INSERT` (mysqldump-style extended insert).
pub const INSERT_BATCH_MAX_ROWS: usize = 250;
/// Flush a batch before the statement grows past this (packet / parser safety).
pub const INSERT_BATCH_MAX_BYTES: usize = 512 * 1024;

/// `INSERT INTO rel (cols) VALUES (...), (...);` in row/size-limited batches.
pub fn append_batched_inserts(
    out: &mut String,
    rel: &str,
    columns_sql: &str,
    value_tuples: &[String],
    max_rows: usize,
    max_bytes: usize,
) {
    if value_tuples.is_empty() {
        return;
    }
    let prefix = format!("INSERT INTO {rel} ({columns_sql}) VALUES ");
    let max_rows = max_rows.max(1);
    let max_bytes = max_bytes.max(prefix.len() + 2);
    let mut batch = String::new();
    let mut count = 0usize;
    let flush = |out: &mut String, batch: &mut String, count: &mut usize| {
        if *count == 0 {
            return;
        }
        out.push_str(&prefix);
        out.push_str(batch);
        out.push_str(";\n");
        batch.clear();
        *count = 0;
    };
    for tuple in value_tuples {
        let extra = if count == 0 {
            tuple.len()
        } else {
            2 + tuple.len()
        };
        if count > 0 && (count >= max_rows || prefix.len() + batch.len() + extra + 2 > max_bytes) {
            flush(out, &mut batch, &mut count);
        }
        if count > 0 {
            batch.push_str(", ");
        }
        batch.push_str(tuple);
        count += 1;
    }
    flush(out, &mut batch, &mut count);
}

/// Dump a database to SQL (header, optional DROP, DDL via `dump_table_ddl`, INSERTs).
///
/// Views / materialized views emit `CREATE VIEW` (when available) and **never**
/// `INSERT INTO`. Does not emit `CREATE DATABASE`. Dump order is tables then
/// views so restore can create base relations before dependent views.
pub async fn dump_sql_database<D>(
    driver: &D,
    handle: &ConnectionHandle,
    database: &str,
    opts: &BackupDumpOptions,
) -> Result<String, DriverError>
where
    D: DatabaseDriver + ?Sized,
{
    dump_sql_database_with_progress(driver, handle, database, opts, |_| {}).await
}

pub async fn dump_sql_database_with_progress<D, F>(
    driver: &D,
    handle: &ConnectionHandle,
    database: &str,
    opts: &BackupDumpOptions,
    mut on_progress: F,
) -> Result<String, DriverError>
where
    D: DatabaseDriver + ?Sized,
    F: FnMut(DumpProgress),
{
    let tables: Vec<TableInfo> = driver
        .get_tables(handle, database)
        .await?
        .into_iter()
        .filter(is_dumpable_object)
        .collect();
    let (base_tables, views) = partition_dump_objects(tables);

    let mut out = String::new();
    out.push_str(&format!("-- DataZen backup: {}\n", database));
    out.push_str(&format!("-- Date: {}\n", chrono::Utc::now().to_rfc3339()));
    let mut opt_flags = Vec::new();
    if opts.schema_only {
        opt_flags.push("schema-only");
    }
    if opts.data_only {
        opt_flags.push("data-only");
    }
    if opts.clean {
        opt_flags.push("clean");
    }
    if opts.create_database {
        opt_flags.push("create");
    }
    if opts.no_owner {
        opt_flags.push("no-owner");
    }
    if opts.single_transaction {
        opt_flags.push("single-transaction");
    }
    if opts.routines {
        opt_flags.push("routines");
    }
    if opts.triggers {
        opt_flags.push("triggers");
    }
    if !opt_flags.is_empty() {
        out.push_str(&format!("-- Options: {}\n", opt_flags.join(", ")));
    }
    if opts.no_owner {
        out.push_str("-- no-owner: OWNER clauses omitted\n");
    }
    out.push('\n');

    // DROP VIEW before DROP TABLE so dependent views do not block table drops.
    if opts.clean {
        for table in views.iter().chain(base_tables.iter()) {
            out.push_str(&drop_object_sql(
                &|n| driver.quote_ident(n),
                &table.name,
                &table.table_type,
                table.schema.as_deref(),
            ));
        }
        if !views.is_empty() || !base_tables.is_empty() {
            out.push('\n');
        }
    }

    let total = (base_tables.len() + views.len()) as u32;
    let mut current = 0u32;
    for table in base_tables.iter().chain(views.iter()) {
        current += 1;
        dump_one_object(
            driver,
            handle,
            table,
            opts,
            current,
            total,
            &mut out,
            &mut on_progress,
        )
        .await?;
    }

    if !opts.data_only && opts.routines {
        on_progress(DumpProgress {
            current: total.saturating_add(1),
            total: total.saturating_add(1),
            object_name: "routines".into(),
            phase: DumpPhase::Object,
        });
        let routines = driver.dump_routines(handle, database).await?;
        if !routines.trim().is_empty() {
            out.push_str("-- Routines (functions / procedures)\n");
            out.push_str(&routines);
            if !routines.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
    }

    if !opts.data_only && opts.triggers {
        on_progress(DumpProgress {
            current: total.saturating_add(2),
            total: total.saturating_add(2),
            object_name: "triggers".into(),
            phase: DumpPhase::Object,
        });
        let triggers = driver.dump_triggers(handle, database).await?;
        if !triggers.trim().is_empty() {
            out.push_str("-- Triggers\n");
            out.push_str(&triggers);
            if !triggers.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
    }

    Ok(out)
}
