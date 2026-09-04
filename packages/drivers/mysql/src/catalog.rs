//! Routine/trigger DDL dump helpers and SHOW CREATE result extraction.

use datazen_driver_api::*;
use super::MysqlDriver;

/// Best-effort dump of stored procedures and functions via SHOW CREATE.
pub(crate) async fn dump_mysql_routines(driver: &MysqlDriver, handle: &ConnectionHandle) -> String {
    let mut out = String::new();
    for (show_status, kind) in [
        ("SHOW PROCEDURE STATUS WHERE Db = DATABASE()", "PROCEDURE"),
        ("SHOW FUNCTION STATUS WHERE Db = DATABASE()", "FUNCTION"),
    ] {
        let Ok(result) = driver.query(handle, show_status).await else {
            continue;
        };
        let name_idx = result
            .columns
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case("Name"));
        let Some(name_idx) = name_idx else {
            continue;
        };
        for row in &result.rows {
            let Some(Value::String(name)) = row.get(name_idx).and_then(|v| v.as_ref()) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let create_sql = format!("SHOW CREATE {kind} {}", driver.quote_ident(name));
            let Ok(create_result) = driver.query(handle, &create_sql).await else {
                out.push_str(&format!("-- Error dumping {kind} {name}\n"));
                continue;
            };
            let col_name = if kind == "PROCEDURE" {
                "Create Procedure"
            } else {
                "Create Function"
            };
            if let Some(ddl) = extract_named_create_column(&create_result, col_name) {
                out.push_str(&format!("-- {kind}: {name}\n"));
                out.push_str(&wrap_mysql_client_routine(&ddl));
                out.push('\n');
            }
        }
    }
    out
}

/// Best-effort dump of triggers via SHOW TRIGGERS + SHOW CREATE TRIGGER.
pub(crate) async fn dump_mysql_triggers(driver: &MysqlDriver, handle: &ConnectionHandle) -> String {
    let mut out = String::new();
    let Ok(result) = driver.query(handle, "SHOW TRIGGERS").await else {
        return out;
    };
    let name_idx = result
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("Trigger"));
    let Some(name_idx) = name_idx else {
        return out;
    };
    for row in &result.rows {
        let Some(Value::String(name)) = row.get(name_idx).and_then(|v| v.as_ref()) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let create_sql = format!("SHOW CREATE TRIGGER {}", driver.quote_ident(name));
        match driver.query(handle, &create_sql).await {
            Ok(create_result) => {
                if let Some(ddl) =
                    extract_named_create_column(&create_result, "SQL Original Statement")
                        .or_else(|| extract_named_create_column(&create_result, "Create Trigger"))
                {
                    out.push_str(&format!("-- TRIGGER: {name}\n"));
                    out.push_str(&wrap_mysql_client_routine(&ddl));
                    out.push('\n');
                }
            }
            Err(e) => {
                out.push_str(&format!("-- Error dumping trigger {name}: {e}\n"));
            }
        }
    }
    out
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
    match cell {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
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
