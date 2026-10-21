//! Foreign-key navigation resolution (DB-07).
//!
//! The grid asks "where does this cell point?" and answering needs three reads
//! against one live session: the source table's foreign keys, the target
//! table's metadata, and one existence probe against the target table. All
//! three live here so that the host command stays a thin adapter and the rules
//! — which constraint matches, how a composite key pairs, what a NULL means —
//! have exactly one home.
//!
//! The rules that matter:
//!
//! * A composite foreign key is located **as a group**: the i-th local column
//!   pairs with the i-th referenced column, and a NULL in any member makes the
//!   whole group unlocatable.
//! * The target table is resolved by the **host**, never by the front end:
//!   [`ForeignKeyNavigationSource`] hands over the driver and the target
//!   metadata is read through the driver's own catalog contract.
//! * A missing referenced row is **not** an error: it is `exists = false`.

use std::sync::Arc;

use datazen_driver_api::{
    ColumnSchema, DatabaseDriver, DriverError, ForeignKeyInfo, LegacySqlDialect, SqlTarget,
    TableSchema, Value,
};
use serde::{Deserialize, Serialize};

use crate::services::ports::{FkSessionBinding, FkSessionError, ForeignKeyNavigationSource};
use crate::services::schema_scope::{embedded_schema_of, metadata_schema};

mod probe;

use probe::{build_probe_statement, ProbeStatement};

/// Default number of candidate rows returned for one navigation target.
pub const DEFAULT_CANDIDATE_LIMIT: u32 = 50;

/// Hard cap for the candidate rows; a request may ask for less, never more.
pub const MAX_CANDIDATE_LIMIT: u32 = 200;

/// Database used when neither the request nor the connection profile names one.
///
/// This mirrors the host's own table-reading path so a foreign-key target and
/// the grid it came from resolve to the same database.
const DEFAULT_DATABASE: &str = "default";

/// The `grid.fk.*` rejection codes (00-contracts §8.2).
///
/// These are the *only* codes this feature raises; the front end classifies on
/// the exact prefix, so the spelling is part of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FkErrorCode {
    /// The two sides of the key do not line up (lengths, or missing columns).
    ShapeMismatch,
    /// No foreign key in the source table matches the request.
    UnknownConstraint,
    /// A member of the key is NULL, so the group cannot be located.
    ValueNull,
    /// The target table/column cannot be resolved.
    TargetUnresolved,
    /// The target was resolved but could not be probed.
    ProbeFailed,
}

impl FkErrorCode {
    /// The wire prefix the front end matches on.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ShapeMismatch => "grid.fk.shapeMismatch",
            Self::UnknownConstraint => "grid.fk.unknownConstraint",
            Self::ValueNull => "grid.fk.valueNull",
            Self::TargetUnresolved => "grid.fk.targetUnresolved",
            Self::ProbeFailed => "grid.fk.probeFailed",
        }
    }
}

impl std::fmt::Display for FkErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every rejection the foreign-key resolver can raise.
#[derive(Debug, Clone)]
pub enum FkNavigationError {
    /// A `grid.fk.*` rejection: the message explains *which* constraint, table
    /// or column caused it and never echoes a key value.
    Coded {
        /// The classification prefix.
        code: FkErrorCode,
        /// The human-readable reason.
        message: String,
    },
    /// The live session could not be bound — not a navigation rule failure.
    Session(FkSessionError),
}

impl FkNavigationError {
    fn coded(code: FkErrorCode, message: impl Into<String>) -> Self {
        Self::Coded {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for FkNavigationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coded { code, message } => write!(f, "{code}: {message}"),
            Self::Session(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for FkNavigationError {}

/// The request: which local columns of the source table carry which values.
///
/// Shared by the grid jump and the future in-cell selector: both ask "where
/// does this group of local columns point?".
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveForeignKeyTargetsRequest {
    /// The runtime session; never persisted.
    pub db_session_id: String,
    /// Database of the source table; falls back to the connection profile.
    #[serde(default)]
    pub database: Option<String>,
    /// Namespace of the source table; falls back to the embedded/config schema.
    #[serde(default)]
    pub schema: Option<String>,
    /// The source table name.
    pub table: String,
    /// An explicit qualified reference; wins over `table`.
    #[serde(default)]
    pub qualified_table: Option<String>,
    /// The local columns of the foreign key, in `ForeignKeyInfo.columns` order.
    pub columns: Vec<String>,
    /// The constraint name; wins over the ordered-columns match.
    #[serde(default)]
    pub constraint_name: Option<String>,
    /// The current value of every local column, same length and order.
    pub values: Vec<Value>,
    /// How many candidate rows to return; clamped to the hard cap.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// The normalized target of a foreign key.
///
/// The front end consumes exactly this and never parses `referenced_table`
/// itself: the shape of that string differs per driver.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyTargetRef {
    /// Database of the target table.
    pub database: String,
    /// Namespace of the target table; `None` when the driver has no schema level.
    pub schema: Option<String>,
    /// The target table.
    pub table: String,
    /// The referenced columns, in the same order as the local columns.
    pub columns: Vec<String>,
    /// The referenced columns' data types, in the same order.
    pub data_types: Vec<String>,
    /// The target table's effective primary key.
    pub primary_keys: Vec<String>,
    /// Whether the referenced columns are covered by the primary key or by a
    /// unique index. `false` still allows the jump, but the result may hold
    /// more than one row.
    pub unique: bool,
    /// The candidate limit that was actually applied.
    pub candidate_limit: u32,
}

/// The resolution result.
///
/// `exists = false` is **not** an error: the target table is readable but that
/// row is gone.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyTargetResolution {
    /// The resolved target.
    pub target: ForeignKeyTargetRef,
    /// Whether at least one row satisfies all equality conditions.
    pub exists: bool,
    /// The candidate rows, each in `target.columns` order.
    pub candidates: Vec<Vec<Option<Value>>>,
    /// Whether the candidate list was cut off by the limit.
    pub truncated: bool,
}

/// Find the constraint a request refers to.
///
/// The constraint name wins when it matches; otherwise the ordered local
/// columns must match exactly, because the pairing between local and
/// referenced columns is positional.
pub fn find_fk<'a>(
    foreign_keys: &'a [ForeignKeyInfo],
    columns: &[String],
    constraint_name: Option<&str>,
) -> Option<&'a ForeignKeyInfo> {
    if let Some(name) = constraint_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        if let Some(found) = foreign_keys.iter().find(|fk| fk.name == name) {
            return Some(found);
        }
    }
    foreign_keys
        .iter()
        .find(|fk| fk.columns.as_slice() == columns)
}

/// Whether the referenced columns are covered by the primary key or by a unique
/// index of the target table.
///
/// "Covered" means the *same set* of columns, not merely a superset: a unique
/// index on `(x, y, z)` does not make `(x, y)` unique.
pub fn referenced_columns_are_unique(schema: &TableSchema, referenced_columns: &[String]) -> bool {
    if referenced_columns.is_empty() {
        return false;
    }
    let same_set = |candidate: &[String]| {
        candidate.len() == referenced_columns.len()
            && candidate
                .iter()
                .all(|name| referenced_columns.contains(name))
            && referenced_columns
                .iter()
                .all(|name| candidate.contains(name))
    };
    if same_set(&schema.effective_primary_keys()) {
        return true;
    }
    schema
        .indexes
        .iter()
        .any(|index| (index.is_unique || index.is_primary) && same_set(&index.columns))
}

/// Resolve the target of one foreign-key group and probe it for existence.
///
/// The returned [`ForeignKeyTargetResolution`] carries the normalized target, whether
/// the referenced row still exists, and the candidate rows that satisfy the
/// equality conditions.
pub async fn resolve_foreign_key_targets(
    source: &dyn ForeignKeyNavigationSource,
    request: &ResolveForeignKeyTargetsRequest,
) -> Result<ForeignKeyTargetResolution, FkNavigationError> {
    validate_request(request)?;
    let binding = bind_session(source, request).await?;
    let driver = Arc::clone(&binding.driver);

    // The source table: the explicit qualified reference wins over the plain
    // name, exactly like the host's own table-reading path.
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

    let source_schema = driver
        .get_table_schema(&binding.handle, source_table, &database, schema.as_deref())
        .await
        .map_err(|error| {
            // The driver's text is not part of the message: it is the one
            // place a user value can leak. The kind goes to the log.
            tracing::warn!(
                table = %source_table,
                error_kind = driver_error_kind(&error),
                "foreign key source metadata is unreadable"
            );
            FkNavigationError::coded(
                FkErrorCode::UnknownConstraint,
                format!(
                    "source table '{source_table}' has no readable foreign key metadata ({})",
                    driver_error_note(&error)
                ),
            )
        })?;

    let fk = find_fk(
        &source_schema.foreign_keys,
        &request.columns,
        request.constraint_name.as_deref(),
    )
    .ok_or_else(|| {
        FkNavigationError::coded(
            FkErrorCode::UnknownConstraint,
            format!(
                "source table '{source_table}' has no foreign key for columns [{}]{}",
                request.columns.join(", "),
                match request.constraint_name.as_deref() {
                    Some(name) => format!(" named '{name}'"),
                    None => String::new(),
                }
            ),
        )
    })?;

    // The request addresses the key by its ordered local columns; a constraint
    // with a different column list cannot be paired with the given values.
    if fk.columns.as_slice() != request.columns.as_slice() {
        return Err(FkNavigationError::coded(
            FkErrorCode::ShapeMismatch,
            format!(
                "constraint '{}' covers columns [{}] but the request addresses [{}]",
                fk.name,
                fk.columns.join(", "),
                request.columns.join(", ")
            ),
        ));
    }
    if fk.columns.is_empty() || fk.referenced_columns.is_empty() {
        return Err(FkNavigationError::coded(
            FkErrorCode::ShapeMismatch,
            format!("constraint '{}' declares no columns", fk.name),
        ));
    }
    if fk.columns.len() != fk.referenced_columns.len() {
        return Err(FkNavigationError::coded(
            FkErrorCode::ShapeMismatch,
            format!(
                "constraint '{}' declares {} local columns but {} referenced columns",
                fk.name,
                fk.columns.len(),
                fk.referenced_columns.len()
            ),
        ));
    }

    let (target_table, target_schema, target_meta) = resolve_target_table(
        &driver,
        &binding,
        &database,
        schema.as_deref(),
        &fk.referenced_table,
    )
    .await?;

    let referenced_columns = collect_referenced_columns(&target_meta, &fk, &target_table)?;
    let unique = referenced_columns_are_unique(&target_meta, &fk.referenced_columns);
    let primary_keys = target_meta.effective_primary_keys();
    let data_types = referenced_columns
        .iter()
        .map(|column| column.data_type.clone())
        .collect();

    let limit = request
        .limit
        .unwrap_or(DEFAULT_CANDIDATE_LIMIT)
        .clamp(1, MAX_CANDIDATE_LIMIT);
    let rows = probe_target(
        &driver,
        &binding,
        &database,
        target_schema.as_deref(),
        &target_table,
        &referenced_columns,
        &fk.referenced_columns,
        &request.values,
        limit,
    )
    .await?;

    let exists = !rows.is_empty();
    let truncated = rows.len() as u32 > limit;
    let candidates = rows.into_iter().take(limit as usize).collect();

    Ok(ForeignKeyTargetResolution {
        target: ForeignKeyTargetRef {
            database,
            schema: target_schema,
            table: target_table,
            columns: fk.referenced_columns.clone(),
            data_types,
            primary_keys,
            unique,
            candidate_limit: limit,
        },
        exists,
        candidates,
        truncated,
    })
}

/// Reject a request whose shape cannot describe a key at all.
fn validate_request(request: &ResolveForeignKeyTargetsRequest) -> Result<(), FkNavigationError> {
    if request.columns.is_empty() || request.values.is_empty() {
        return Err(FkNavigationError::coded(
            FkErrorCode::ShapeMismatch,
            "a foreign key needs at least one local column and one value",
        ));
    }
    if request.columns.len() != request.values.len() {
        return Err(FkNavigationError::coded(
            FkErrorCode::ShapeMismatch,
            format!(
                "the request gives {} local columns but {} values",
                request.columns.len(),
                request.values.len()
            ),
        ));
    }
    if let Some(index) = request
        .values
        .iter()
        .position(|value| matches!(value, Value::Null))
    {
        return Err(FkNavigationError::coded(
            FkErrorCode::ValueNull,
            format!(
                "column '{}' is NULL, so the key cannot be located by equality",
                request
                    .columns
                    .get(index)
                    .map(String::as_str)
                    .unwrap_or("<unknown>")
            ),
        ));
    }
    Ok(())
}

/// Bind the live session the request addresses.
async fn bind_session(
    source: &dyn ForeignKeyNavigationSource,
    request: &ResolveForeignKeyTargetsRequest,
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

/// The database of the source table: the request wins, then the connection
/// profile, then the host's own default.
fn resolve_database(
    binding: &FkSessionBinding,
    request: &ResolveForeignKeyTargetsRequest,
) -> String {
    request
        .database
        .as_deref()
        .map(str::trim)
        .filter(|database| !database.is_empty())
        .or_else(|| {
            binding
                .database
                .as_deref()
                .map(str::trim)
                .filter(|database| !database.is_empty())
        })
        .unwrap_or(DEFAULT_DATABASE)
        .to_string()
}

/// The namespace the referenced table names itself, falling back to the source
/// table's namespace.
///
/// A driver that folds the namespace into the table name (`public.parents`)
/// says where the target lives; one that reports a bare name leaves the target
/// in the source table's namespace.
fn target_namespace(referenced_table: &str, source_schema: Option<&str>) -> Option<String> {
    embedded_schema_of(referenced_table)
        .map(str::to_string)
        .or_else(|| source_schema.map(str::to_string))
}

/// Resolve the target table of a foreign key.
///
/// Two attempts, in this order, and both are driver-agnostic:
///
/// 1. the referenced table as the driver reported it, in the namespace **it
///    names itself** — a driver that reports `public.parents` means the
///    `public` namespace, not the source table's;
/// 2. the referenced table split at its **last** `.`, so a driver that folds
///    the namespace into the table name still resolves.
///
/// There is no database-name branch here: a cross-database foreign key simply
/// fails to resolve, which is safer than jumping into the wrong database.
async fn resolve_target_table(
    driver: &Arc<dyn DatabaseDriver>,
    binding: &FkSessionBinding,
    database: &str,
    source_schema: Option<&str>,
    referenced_table: &str,
) -> Result<(String, Option<String>, TableSchema), FkNavigationError> {
    // The namespace the referenced table names itself comes first; the source
    // table's namespace is only the fallback.
    let namespace = target_namespace(referenced_table, source_schema);
    // The read respects the driver's schema level: a driver without one must
    // not be handed a namespace at all.
    let read_schema = metadata_schema(
        driver.as_ref(),
        None,
        embedded_schema_of(referenced_table),
        source_schema,
    );

    let mut candidates: Vec<(String, Option<String>)> =
        vec![(referenced_table.to_string(), read_schema.clone())];
    if let Some((head, tail)) = referenced_table.rsplit_once('.') {
        let head = head.trim().trim_matches('"');
        let tail = tail.trim().trim_matches('"');
        if !head.is_empty() && !tail.is_empty() {
            candidates.push((tail.to_string(), read_schema.clone()));
        }
    }

    // The note carries the driver's **kind**, never its text: the text is the
    // one place a user value can leak.
    let mut last_note = "the driver returned no column metadata".to_string();
    for (table, schema) in candidates {
        match driver
            .get_table_schema(&binding.handle, &table, database, schema.as_deref())
            .await
        {
            Ok(meta) if !meta.columns.is_empty() => {
                // The **driver's** name for the table, not the raw reference:
                // a driver that reports `public.parents` answers with the bare
                // `parents`, and the probe has to address it by that name.
                return Ok((meta.table_name.clone(), namespace, meta));
            }
            Ok(_) => last_note = "the driver returned no column metadata".to_string(),
            Err(error) => {
                tracing::warn!(
                    table = %table,
                    error_kind = driver_error_kind(&error),
                    "foreign key target metadata is unreadable"
                );
                last_note = driver_error_note(&error);
            }
        }
    }

    Err(FkNavigationError::coded(
        FkErrorCode::TargetUnresolved,
        format!("referenced table '{referenced_table}' cannot be resolved ({last_note})"),
    ))
}

/// Collect the target's referenced columns, in the constraint's order.
fn collect_referenced_columns(
    target_meta: &TableSchema,
    fk: &ForeignKeyInfo,
    target_table: &str,
) -> Result<Vec<ColumnSchema>, FkNavigationError> {
    let mut columns = Vec::with_capacity(fk.referenced_columns.len());
    for name in &fk.referenced_columns {
        let column = target_meta
            .columns
            .iter()
            .find(|column| &column.name == name)
            .cloned()
            .ok_or_else(|| {
                FkNavigationError::coded(
                    FkErrorCode::ShapeMismatch,
                    format!(
                        "target table '{target_table}' has no column '{name}' referenced by constraint '{}'",
                        fk.name
                    ),
                )
            })?;
        columns.push(column);
    }
    Ok(columns)
}

/// Probe the target table for rows that satisfy the equality conditions.
///
/// One `SELECT … LIMIT limit + 1` and no `COUNT(*)`: the existence check must
/// not cost a full-table count on a large target.
#[allow(clippy::too_many_arguments)]
async fn probe_target(
    driver: &Arc<dyn DatabaseDriver>,
    binding: &FkSessionBinding,
    database: &str,
    target_schema: Option<&str>,
    target_table: &str,
    referenced_columns: &[ColumnSchema],
    referenced_names: &[String],
    values: &[Value],
    limit: u32,
) -> Result<Vec<Vec<Option<Value>>>, FkNavigationError> {
    let dialect = LegacySqlDialect::new(driver.as_ref());
    let statement = build_probe_statement(
        &dialect,
        target_table,
        referenced_columns,
        referenced_names,
        values,
        limit,
    )
    .map_err(|error| {
        // The statement could not be expressed: the metadata was readable, so
        // this is a failed probe, not a shape fault.
        tracing::warn!(
            table = %target_table,
            error_kind = driver_error_kind(&error),
            "foreign key probe statement could not be built"
        );
        FkNavigationError::coded(
            FkErrorCode::ProbeFailed,
            format!(
                "probing target table '{target_table}' cannot be expressed ({})",
                driver_error_note(&error)
            ),
        )
    })?;
    let target = SqlTarget::new(Some(database), target_schema);
    let result = match statement {
        ProbeStatement::Bound { sql, params } => {
            driver
                .query_with_params_at(&binding.handle, &sql, &params, target)
                .await
        }
        ProbeStatement::Inlined { sql } => driver.query_at(&binding.handle, &sql, target).await,
    }
    .map_err(|error| {
        // A driver error can quote the value it choked on (`Incorrect integer
        // value: 'abc'`), and that value is the user's own data. Neither the
        // IPC message nor the log carries the driver's text: the message
        // carries the error's **kind**, the log the same.
        tracing::warn!(
            table = %target_table,
            error_kind = driver_error_kind(&error),
            "foreign key probe failed"
        );
        FkNavigationError::coded(
            FkErrorCode::ProbeFailed,
            format!(
                "probing target table '{target_table}' failed ({})",
                driver_error_note(&error)
            ),
        )
    })?;
    Ok(result.rows)
}

/// The kind of a driver error, without its content.
///
/// The content can quote a user value; the kind cannot, and the kind is all
/// a rejection message and a log line need to say "the probe failed because
/// the statement was refused" or "because the connection is gone".
fn driver_error_kind(error: &DriverError) -> &'static str {
    match error {
        DriverError::SessionLost => "sessionLost",
        DriverError::ConnectionFailed(_) => "connectionFailed",
        DriverError::QueryFailed(_) => "queryFailed",
        DriverError::ConnectionTimeout => "connectionTimeout",
        DriverError::AuthenticationFailed(_) => "authenticationFailed",
        DriverError::InvalidConfig(_) => "invalidConfig",
        DriverError::PoolExhausted => "poolExhausted",
        DriverError::NotSupported(_) => "notSupported",
        DriverError::Unsupported(_) => "unsupported",
        DriverError::QueryCancelled => "queryCancelled",
        _ => "other",
    }
}

/// The driver-error part of a rejection message: the **kind**, never the
/// text. The text is the one place a user value can leak (a server-side type
/// error quotes the value it choked on), so it is not part of any message.
fn driver_error_note(error: &DriverError) -> String {
    format!("driver error kind: {}", driver_error_kind(error))
}

#[cfg(test)]
mod tests;
