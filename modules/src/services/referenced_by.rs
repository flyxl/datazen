//! The reverse foreign-key catalog (DB-20): "which tables reference **this**
//! table?".
//!
//! The jump (DB-07) follows a foreign key forward, from the cell to the row
//! it points at. The reverse question — *who points here?* — has no cell to
//! answer for: the referencing constraints live in **other** tables' schemas,
//! so the only way to find them is to read those schemas. That is a scan, and
//! a scan needs a bound, or one wide database turns a right-click into a
//! minute of metadata reads.
//!
//! The rules that matter:
//!
//! * The scan is **bounded**: at most [`MAX_INBOUND_SCAN_TABLES`] table
//!   schemas are read, and `truncated` says so. A catalog that silently
//!   stopped at 200 tables would lie about "no table references this one".
//! * A table whose schema cannot be read is **skipped**, not fatal: the
//!   metadata may be unreadable for one table out of two hundred, and the
//!   other one hundred ninety-nine still answer the question.
//! * The referenced-table name is matched by its **last segment**: drivers
//!   report the reference differently (PostgreSQL prefixes the schema,
//!   MySQL/SQLite do not), so the comparison normalizes both sides the same
//!   way — and the front end still never parses a table name itself.

use std::sync::Arc;

use datazen_driver_api::TableSchema;
use serde::{Deserialize, Serialize};

use crate::services::fk_navigation::{driver_error_note, FkNavigationError};
use crate::services::ports::{FkSessionBinding, FkSessionError, ForeignKeyNavigationSource};
use crate::services::schema_scope::{embedded_schema_of, metadata_schema};

/// How many table schemas one catalog scan may read.
///
/// Two hundred schema reads is seconds on every driver we ship; two thousand
/// is a minute on some. Beyond the cap the answer says `truncated` and the
/// front end offers what it found — an honest partial answer beats an
/// unbounded read on the UI thread of a right-click.
pub const MAX_INBOUND_SCAN_TABLES: usize = 200;

/// The request: one table, as the grid's tab addresses it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListInboundForeignKeysRequest {
    /// The runtime session; never persisted.
    pub db_session_id: String,
    /// Database of the table; falls back to the connection profile.
    #[serde(default)]
    pub database: Option<String>,
    /// Namespace of the table; falls back to the embedded/config schema.
    #[serde(default)]
    pub schema: Option<String>,
    /// The table the references point at.
    pub table: String,
    /// An explicit qualified reference; wins over `table`.
    #[serde(default)]
    pub qualified_table: Option<String>,
}

/// One referencing constraint, as the front end's reverse jump needs it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundForeignKeyRef {
    /// The referencing table, in the driver's own naming.
    pub table: String,
    /// The referencing table's namespace; `None` when the driver has none.
    pub schema: Option<String>,
    /// The constraint's name, as the structure view shows it.
    pub constraint_name: String,
    /// The referencing table's own columns that carry the reference.
    pub columns: Vec<String>,
    /// **This** table's columns being referenced, in pairing order.
    pub referenced_columns: Vec<String>,
}

/// The catalog: every inbound constraint the scan found, plus how far it got.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundForeignKeyCatalog {
    /// The table the references point at, as the driver normalized it.
    pub table: String,
    /// The database the scan ran in.
    pub database: String,
    /// One entry per referencing constraint, ordered by table name.
    pub references: Vec<InboundForeignKeyRef>,
    /// How many table schemas the scan actually read.
    pub scanned_tables: usize,
    /// The scan hit the cap before the table list ran out.
    pub truncated: bool,
}

/// Whether `referenced_table` names the same table as `source_table`, once
/// the drivers' different qualification habits are normalized away.
fn same_table(referenced_table: &str, source_table: &str) -> bool {
    fn last_segment(name: &str) -> &str {
        name.rsplit('.')
            .next()
            .unwrap_or(name)
            .trim()
            .trim_matches('"')
            .trim_matches('`')
    }
    let left = last_segment(referenced_table);
    let right = last_segment(source_table);
    !left.is_empty() && left.eq_ignore_ascii_case(right)
}

/// Whether every referenced column exists on the source table.
///
/// A constraint whose referenced columns name columns the table does not have
/// cannot be turned into a filter — offering it would open a tab that cannot
/// render its own conditions.
fn referenced_columns_exist(source_meta: &TableSchema, referenced_columns: &[String]) -> bool {
    referenced_columns.iter().all(|name| {
        source_meta
            .columns
            .iter()
            .any(|column| &column.name == name)
    })
}

/// List the constraints of other tables that reference the request's table.
pub async fn list_inbound_foreign_keys(
    source: &dyn ForeignKeyNavigationSource,
    request: &ListInboundForeignKeysRequest,
) -> Result<InboundForeignKeyCatalog, FkNavigationError> {
    if request.table.trim().is_empty() {
        return Err(FkNavigationError::coded(
            crate::services::fk_navigation::FkErrorCode::ShapeMismatch,
            "table is required",
        ));
    }
    let binding = bind_session(source, request).await?;
    let driver = Arc::clone(&binding.driver);

    let source_table = request
        .qualified_table
        .as_deref()
        .map(str::trim)
        .filter(|table| !table.is_empty())
        .unwrap_or(request.table.as_str());
    let database = resolve_database(&binding, request);
    let schema = metadata_schema(
        driver.as_ref(),
        request.schema.as_deref(),
        embedded_schema_of(source_table),
        binding.schema.as_deref(),
    );

    // The source table's own metadata: it proves the table exists, and it
    // filters out constraints that reference columns the table does not have.
    let source_meta = driver
        .get_table_schema(&binding.handle, source_table, &database, schema.as_deref())
        .await
        .map_err(|error| {
            tracing::warn!(
                table = %source_table,
                error_kind = fk_navigation_error_kind(&error),
                "inbound foreign key source metadata is unreadable"
            );
            FkNavigationError::coded(
                crate::services::fk_navigation::FkErrorCode::TargetUnresolved,
                format!(
                    "table '{source_table}' cannot be resolved ({})",
                    fk_navigation_error_note(&error)
                ),
            )
        })?;

    // The candidates: every table of the same namespace, minus the source
    // itself (a self-reference is a jump the forward path already handles).
    let tables = driver
        .get_tables(&binding.handle, &database, schema.as_deref())
        .await
        .map_err(|error| {
            tracing::warn!(
                database = %database,
                error_kind = fk_navigation_error_kind(&error),
                "inbound foreign key table list is unreadable"
            );
            FkNavigationError::coded(
                crate::services::fk_navigation::FkErrorCode::ProbeFailed,
                format!(
                    "the tables of database '{database}' cannot be listed ({})",
                    fk_navigation_error_note(&error)
                ),
            )
        })?;

    let mut references: Vec<InboundForeignKeyRef> = Vec::new();
    let mut scanned: usize = 0;
    let mut truncated = false;
    for table in tables {
        let name = table.name;
        if name == source_meta.table_name {
            continue;
        }
        if scanned >= MAX_INBOUND_SCAN_TABLES {
            truncated = true;
            break;
        }
        scanned += 1;
        // One unreadable table is a skipped table, not a failed scan: the
        // catalog answers with what it could read.
        let Ok(meta) = driver
            .get_table_schema(&binding.handle, &name, &database, schema.as_deref())
            .await
        else {
            tracing::info!(table = %name, "inbound scan skips an unreadable table");
            continue;
        };
        for fk in &meta.foreign_keys {
            if !same_table(&fk.referenced_table, &source_meta.table_name) {
                continue;
            }
            if !referenced_columns_exist(&source_meta, &fk.referenced_columns) {
                continue;
            }
            references.push(InboundForeignKeyRef {
                table: meta.table_name.clone(),
                schema: schema.clone(),
                constraint_name: fk.name.clone(),
                columns: fk.columns.clone(),
                referenced_columns: fk.referenced_columns.clone(),
            });
        }
    }
    references.sort_by(|left, right| {
        left.table
            .cmp(&right.table)
            .then_with(|| left.constraint_name.cmp(&right.constraint_name))
    });

    Ok(InboundForeignKeyCatalog {
        table: source_meta.table_name,
        database,
        references,
        scanned_tables: scanned,
        truncated,
    })
}

/// The catalog's own database resolution: the request wins, then the
/// connection profile, then the host's default — the same ladder the jump
/// resolves by, so a table and its reverse catalog land on one database.
fn resolve_database(binding: &FkSessionBinding, request: &ListInboundForeignKeysRequest) -> String {
    request
        .database
        .as_deref()
        .map(str::trim)
        .filter(|database| !database.is_empty())
        .map(str::to_string)
        .or_else(|| {
            binding
                .database
                .as_deref()
                .map(str::trim)
                .filter(|database| !database.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| crate::services::fk_navigation::DEFAULT_DATABASE.to_string())
}

async fn bind_session(
    source: &dyn ForeignKeyNavigationSource,
    request: &ListInboundForeignKeysRequest,
) -> Result<FkSessionBinding, FkNavigationError> {
    if request.db_session_id.trim().is_empty() {
        return Err(FkNavigationError::Session(FkSessionError::NotFound(
            "dbSessionId is required".to_string(),
        )));
    }
    source
        .bind_session(request.db_session_id.as_str())
        .await
        .map_err(FkNavigationError::Session)
}

/// The kind of a driver error, without its content (the same rule the jump
/// follows: the text can quote a user value, the kind cannot).
fn fk_navigation_error_kind(error: &datazen_driver_api::DriverError) -> &'static str {
    crate::services::fk_navigation::driver_error_kind(error)
}

fn fk_navigation_error_note(error: &datazen_driver_api::DriverError) -> String {
    driver_error_note(error)
}

#[cfg(test)]
mod tests;
