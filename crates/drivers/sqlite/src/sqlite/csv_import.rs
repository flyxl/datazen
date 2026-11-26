//! Atomic, parameter-bound append to one reviewed ordinary SQLite table.
use super::{db_path, sqlite_file_identity, SqliteDriver};
use datazen_driver_api::sql_identifiers::quote_ident_sql;
use datazen_driver_api::{ConnectionConfig, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub struct CsvImportTarget {
    pub identity: String,
    pub ddl: String,
    pub columns: Vec<String>,
}

impl SqliteDriver {
    pub async fn inspect_csv_import_target(
        config: &ConnectionConfig,
        table: &str,
    ) -> Result<CsvImportTarget, String> {
        let (pool, path, identity) = open(config, true).await?;
        let result = async {
            let mut tx = pool
                .begin()
                .await
                .map_err(|_| "Could not start the SQLite preview.")?;
            expect_identity(&path, &identity)?;
            inspect(&mut tx, table, identity).await
        }
        .await;
        pool.close().await;
        result
    }

    pub async fn append_reviewed_csv(
        config: &ConnectionConfig,
        table: &str,
        reviewed: &CsvImportTarget,
        headers: &[String],
        rows: &[Vec<Value>],
    ) -> Result<usize, String> {
        if config.read_only {
            return Err("The CSV import target is read-only.".into());
        }
        let columns: HashSet<_> = headers.iter().collect();
        if headers.is_empty()
            || columns.len() != headers.len()
            || headers.iter().any(|name| !reviewed.columns.contains(name))
            || rows.is_empty()
            || rows.iter().any(|row| row.len() != headers.len())
        {
            return Err("The CSV mapping is not valid for the reviewed table.".into());
        }
        let (pool, path, identity) = open(config, false).await?;
        let result = async {
            if identity != reviewed.identity {
                return Err("The SQLite file changed. Preview the CSV again.".into());
            }
            // Snapshot validation and all inserts are pinned to one SQLx
            // connection. No external SQLite writer can change the reviewed
            // catalog between validation and append.
            let mut tx = pool.begin_with("BEGIN IMMEDIATE").await
                .map_err(|_| "Could not start the SQLite import transaction.")?;
            let inserted = async {
                expect_identity(&path, &identity)?;
                if inspect(&mut tx, table, identity).await? != *reviewed {
                    return Err("The SQLite table changed. Preview the CSV again.".into());
                }
                let columns = headers.iter().map(|name| quote_ident_sql(name, '"'))
                    .collect::<Vec<_>>().join(", ");
                let params = vec!["?"; headers.len()].join(", ");
                let sql = format!(
                    "INSERT OR ABORT INTO main.{} ({columns}) VALUES ({params})",
                    quote_ident_sql(table, '"'),
                );
                for row in rows {
                    let result = Self::bind_values(sqlx::query(&sql), row).execute(&mut *tx).await
                        .map_err(|_| "A CSV row violates the target table constraints.")?;
                    if result.rows_affected() != 1 {
                        return Err("SQLite did not report one inserted row.".into());
                    }
                }
                Ok(rows.len())
            }.await;
            match inserted {
                Ok(count) => {
                    tx.commit().await.map_err(|_| {
                        "SQLite could not confirm the import outcome. Check the table before retrying."
                    })?;
                    Ok(count)
                }
                Err(error) => {
                    tx.rollback().await.map_err(|_| {
                        "SQLite could not confirm rollback. Check the table before retrying."
                    })?;
                    Err(error)
                }
            }
        }.await;
        pool.close().await;
        result
    }
}

async fn open(
    config: &ConnectionConfig,
    read_only: bool,
) -> Result<(SqlitePool, String, String), String> {
    let path = db_path(config).map_err(|_| "Choose an existing SQLite file.")?;
    if config.database_type != "sqlite"
        || path.chars().any(|ch| matches!(ch, '?' | '#' | '%'))
        || !std::path::Path::new(&path).is_file()
    {
        return Err("Choose an existing SQLite file without URI options.".into());
    }
    let identity =
        sqlite_file_identity(&path).ok_or("Could not verify the SQLite file identity.")?;
    let options = SqliteConnectOptions::from_str(&format!("sqlite:{path}"))
        .map_err(|_| "Could not open the SQLite file.")?
        .create_if_missing(false)
        .read_only(read_only)
        .foreign_keys(true);
    // Intentionally do not change persistent journal mode, including on a
    // cancelled/rejected import or a profile whose filename was replaced.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(config.connection_timeout.into()))
        .connect_with(options)
        .await
        .map_err(|_| "Could not open the SQLite file.")?;
    Ok((pool, path, identity))
}

fn expect_identity(path: &str, expected: &str) -> Result<(), String> {
    if sqlite_file_identity(path).as_deref() == Some(expected) {
        Ok(())
    } else {
        Err("The SQLite file changed. Preview the CSV again.".into())
    }
}

async fn inspect(
    connection: &mut SqliteConnection,
    table: &str,
    identity: String,
) -> Result<CsvImportTarget, String> {
    let ddl: String = sqlx::query_scalar(
        "SELECT s.sql FROM main.sqlite_schema s
         WHERE s.type = 'table' AND s.name = ? COLLATE BINARY
           AND substr(s.name, 1, 7) COLLATE NOCASE <> 'sqlite_'
           AND EXISTS (SELECT 1 FROM pragma_table_list p
                       WHERE p.schema = 'main' AND p.name = s.name AND p.type = 'table')
           AND NOT EXISTS (SELECT 1 FROM main.sqlite_schema t
                           WHERE t.type = 'trigger' AND t.tbl_name = s.name COLLATE NOCASE)",
    )
    .bind(table)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| "Could not read the SQLite target catalog.")?
    .ok_or("Choose an existing ordinary SQLite table with no triggers.")?;
    let columns = sqlx::query_scalar(
        "SELECT name FROM pragma_table_xinfo(?, 'main') WHERE hidden = 0 ORDER BY cid",
    )
    .bind(table)
    .fetch_all(connection)
    .await
    .map_err(|_| "Could not read the SQLite target columns.")?;
    Ok(CsvImportTarget {
        identity,
        ddl,
        columns,
    })
}
