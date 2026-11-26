//! Routine/trigger DDL dump helpers and SHOW CREATE result extraction.

use super::MysqlDriver;
use datazen_driver_api::*;

/// Decode values from a text-only catalog column, even when MySQL tags
/// the column as binary on the wire. Invalid UTF-8 is not an identifier.
fn catalog_text(value: &Value) -> Option<&str> {
    let text = match value {
        Value::String(text) => text.as_str(),
        Value::Bytes(bytes) => std::str::from_utf8(bytes).ok()?,
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

pub(crate) async fn dump_database_name(
    driver: &MysqlDriver,
    handle: &ConnectionHandle,
    requested: &str,
) -> Result<String, DriverError> {
    if !requested.trim().is_empty() {
        return Ok(requested.to_owned());
    }
    let result = driver.query(handle, "SELECT DATABASE()").await?;
    result.rows.first().and_then(|row| row.first())
        .and_then(Option::as_ref).and_then(catalog_text).map(str::to_owned)
        .ok_or_else(|| DriverError::InvalidConfig("backup database is required".into()))
}

/// Read and dump exactly the selected catalog; never USE another database
/// on the retained connection to discover its routines.
pub(crate) async fn dump_mysql_routines(
    driver: &MysqlDriver,
    handle: &ConnectionHandle,
    database: &str,
) -> Result<String, DriverError> {
    let mut out = String::new();
    for (kind, ddl_column) in [
        ("PROCEDURE", "Create Procedure"),
        ("FUNCTION", "Create Function"),
    ] {
        let result = driver.query_with_params(
            handle,
            "SELECT ROUTINE_NAME AS name FROM information_schema.ROUTINES \
             WHERE ROUTINE_SCHEMA = ? AND ROUTINE_TYPE = ? ORDER BY ROUTINE_NAME",
            &[Value::String(database.to_owned()), Value::String(kind.to_owned())],
        ).await?;
        for row in &result.rows {
            let name = row.first().and_then(Option::as_ref).and_then(catalog_text)
                .ok_or_else(|| DriverError::QueryFailed("missing routine catalog name".into()))?;
            let sql = format!(
                "SHOW CREATE {kind} {}.{}",
                driver.quote_ident(database),
                driver.quote_ident(name)
            );
            let create = driver.query(handle, &sql).await?;
            append_mysql_routine(&mut out, kind, name, extract_named_create_column(&create, ddl_column))?;
        }
    }
    Ok(out)
}

pub(crate) async fn dump_mysql_triggers(
    driver: &MysqlDriver,
    handle: &ConnectionHandle,
    database: &str,
) -> Result<String, DriverError> {
    let mut out = String::new();
    let result = driver.query_with_params(
        handle,
        "SELECT TRIGGER_NAME AS name FROM information_schema.TRIGGERS \
         WHERE TRIGGER_SCHEMA = ? ORDER BY TRIGGER_NAME",
        &[Value::String(database.to_owned())],
    ).await?;
    for row in &result.rows {
        let name = row.first().and_then(Option::as_ref).and_then(catalog_text)
            .ok_or_else(|| DriverError::QueryFailed("missing trigger catalog name".into()))?;
        let sql = format!(
            "SHOW CREATE TRIGGER {}.{}",
            driver.quote_ident(database),
            driver.quote_ident(name)
        );
        let create = driver.query(handle, &sql).await?;
        let ddl = extract_named_create_column(&create, "SQL Original Statement")
            .or_else(|| extract_named_create_column(&create, "Create Trigger"));
        append_mysql_routine(&mut out, "TRIGGER", name, ddl)?;
    }
    Ok(out)
}

fn append_mysql_routine(
    output: &mut String,
    kind: &str,
    name: &str,
    ddl: Option<String>,
) -> Result<(), DriverError> {
    let ddl = ddl.ok_or_else(|| DriverError::QueryFailed(format!("{kind} definition is unavailable")))?;
    let label = datazen_driver_api::sql_dump::stream::comment_label(name);
    output.push_str(&format!("-- {kind}: {label}\n"));
    output.push_str(&wrap_mysql_client_routine(&ddl));
    output.push('\n');
    Ok(())
}

pub(crate) fn wrap_mysql_client_routine(ddl: &str) -> String {
    let delim = if ddl.contains("$$") { "//" } else { "$$" };
    let body = ddl.trim().trim_end_matches(';');
    format!("DELIMITER {delim}\n{body}{delim}\nDELIMITER ;\n")
}

pub(crate) fn extract_named_create_column(result: &QueryResult, col_name: &str) -> Option<String> {
    let col_idx = result
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(col_name))?;
    let row = result.rows.first()?;
    let cell = row.get(col_idx)?.as_ref()?;
    catalog_text(cell).map(str::to_owned)
}

/// Extract the `Create Table` column from `SHOW CREATE TABLE` result rows.
pub(crate) fn extract_show_create_table(result: &QueryResult) -> Option<String> {
    extract_named_create_column(result, "Create Table").or_else(|| {
        if result.columns.len() >= 2 {
            let row = result.rows.first()?;
            let cell = row.get(1)?.as_ref()?;
            match cell {
                Value::String(s) if !s.is_empty() => Some(s.clone()),
                _ => None,
            }
        } else {
            None
        }
    })
}
