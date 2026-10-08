//! Server-owned SQL-file output for Data Transfer.
//!
//! The native save dialog registers a destination path and returns only an
//! opaque token to the webview. Execution writes a sibling temporary file and
//! publishes it with a single rename after every selected table has rendered
//! successfully. A failed or cancelled transfer therefore never leaves a
//! partially written SQL file at the requested destination.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::{LazyLock, Mutex};

use datazen_driver_api::{DatabaseDriver, TableSchema};
use flate2::write::GzEncoder;
use flate2::Compression;
use uuid::Uuid;

use super::error::TransferError;
use super::execute::active_column_mappings;
use super::model::{
    ColumnMapping, DdlPreviewItem, DdlPreviewKind, SqlFileCompression, SqlFileEncoding,
    TableExecutionResult, TableInspectResult, TransferExecutionResult, TransferJob, TransferMode,
    WriteMode,
};
use crate::transfer::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::transfer::ir::{IRDefault, IRType};
use datazen_data_sync::sql::{qualify_relation_sql, quote_ident_sql};
use datazen_driver_api::{ConnectionHandle, Value};

pub use super::sql_structure::build_structure_plan;

static PATHS: LazyLock<Mutex<HashMap<String, PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a native-dialog-selected SQL path and return its opaque handle.
pub fn register_path(path: PathBuf) -> Result<String, TransferError> {
    validate_path(&path)?;
    let token = Uuid::new_v4().to_string();
    PATHS
        .lock()
        .map_err(|_| TransferError::validation("SQL file destination registry is unavailable"))?
        .insert(token.clone(), path);
    Ok(token)
}

pub fn resolve_path(token: &str) -> Result<PathBuf, TransferError> {
    if token.trim().is_empty() {
        return Err(TransferError::validation(
            "SQL file destination token is empty",
        ));
    }
    let path = PATHS
        .lock()
        .map_err(|_| TransferError::validation("SQL file destination registry is unavailable"))?
        .get(token)
        .cloned()
        .ok_or_else(|| TransferError::validation("SQL file destination is unknown or expired"))?;
    validate_path(&path)?;
    Ok(path)
}

/// Resolve a SQL-file rendering dialect from the registered driver factories.
/// A missing selection deliberately returns the live source driver so older
/// plans keep their source-dialect behavior.
pub fn resolve_target_driver(
    source_driver: Arc<dyn DatabaseDriver>,
    target: &super::model::SqlFileTarget,
) -> Result<Arc<dyn DatabaseDriver>, TransferError> {
    let Some(database_type) = target.normalized_database_type() else {
        return Ok(source_driver);
    };
    let driver = datazen_driver_api::create_driver(database_type).ok_or_else(|| {
        TransferError::validation(format!(
            "SQL file target dialect '{database_type}' is not registered in this build"
        ))
    })?;
    if !matches!(
        driver.driver_category(),
        datazen_driver_api::DriverCategory::Sql
    ) {
        return Err(TransferError::validation(format!(
            "SQL file target dialect '{database_type}' is not a SQL driver"
        )));
    }
    if driver.driver_type() != database_type {
        return Err(TransferError::validation(format!(
            "SQL file target dialect '{database_type}' resolved to '{}', which is not a stable driver id",
            driver.driver_type()
        )));
    }
    Ok(driver)
}

/// Validate that the requested catalog/schema pair has an unambiguous
/// meaning in the selected SQL dialect. Silently dropping one qualifier
/// would produce a file that looks scoped while writing into the session's
/// default namespace.
pub fn validate_target_scope_for_driver(
    driver: &dyn DatabaseDriver,
    target: &super::model::SqlFileTarget,
) -> Result<(), TransferError> {
    let driver_type = driver.driver_type();
    validate_target_scope_for_family(&driver_type, target)
}

fn validate_target_scope_for_family(
    driver_type: &str,
    target: &super::model::SqlFileTarget,
) -> Result<(), TransferError> {
    target.validate_qualifiers()?;
    let family = driver_type.to_ascii_lowercase();
    let database = target.normalized_database();
    let schema = target.normalized_schema();
    match family.as_str() {
        "mysql" | "mariadb" | "clickhouse" => {
            if schema.is_some() {
                return Err(TransferError::validation(format!(
                    "SQL file target dialect '{}' accepts a database/catalog qualifier but not a separate schema",
                    driver_type
                )));
            }
        }
        "sqlserver" => {}
        "postgresql" | "sqlite" | "duckdb" => {
            if database.is_some() {
                return Err(TransferError::validation(format!(
                    "SQL file target dialect '{}' cannot qualify a relation with a database/catalog; use schema",
                    driver_type
                )));
            }
        }
        _ => {
            if database.is_some() || schema.is_some() {
                return Err(TransferError::validation(format!(
                    "SQL file target dialect '{}' does not advertise database/catalog or schema qualification",
                    driver_type
                )));
            }
        }
    }
    Ok(())
}

fn validate_path(path: &Path) -> Result<(), TransferError> {
    if !path.is_absolute() {
        return Err(TransferError::validation(
            "SQL file destination must be an absolute path",
        ));
    }
    if !has_supported_sql_suffix(path) {
        return Err(TransferError::validation(
            "SQL file destination must use the .sql or .sql.gz extension",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| TransferError::validation("SQL file destination has no parent directory"))?;
    let metadata = fs::metadata(parent).map_err(|error| {
        TransferError::validation(format!(
            "SQL file destination directory is unavailable: {error}"
        ))
    })?;
    if !metadata.is_dir() {
        return Err(TransferError::validation(
            "SQL file destination parent is not a directory",
        ));
    }
    if path.file_name().is_none() {
        return Err(TransferError::validation(
            "SQL file destination has no file name",
        ));
    }
    Ok(())
}

fn has_supported_sql_suffix(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    name.to_ascii_lowercase().ends_with(".sql") || name.to_ascii_lowercase().ends_with(".sql.gz")
}

pub fn validate_output_path(
    path: &Path,
    compression: SqlFileCompression,
) -> Result<(), TransferError> {
    validate_path(path)?;
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return Err(TransferError::validation(
            "SQL file destination has no valid UTF-8 file name",
        ));
    };
    let lower = name.to_ascii_lowercase();
    let expected = match compression {
        SqlFileCompression::None => lower.ends_with(".sql") && !lower.ends_with(".sql.gz"),
        SqlFileCompression::Gzip => lower.ends_with(".sql.gz"),
    };
    if !expected {
        return Err(TransferError::validation(match compression {
            SqlFileCompression::None => "uncompressed SQL output must use the .sql extension",
            SqlFileCompression::Gzip => "gzip SQL output must use the .sql.gz extension",
        }));
    }
    Ok(())
}

enum SqlFileWriter {
    Plain(BufWriter<File>),
    Gzip(GzEncoder<BufWriter<File>>),
}

impl SqlFileWriter {
    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            Self::Plain(writer) => writer.write_all(bytes),
            Self::Gzip(writer) => writer.write_all(bytes),
        }
    }

    fn finish(self) -> std::io::Result<()> {
        let mut writer = match self {
            Self::Plain(writer) => writer,
            Self::Gzip(writer) => writer.finish()?,
        };
        writer.flush()?;
        writer.get_ref().sync_all()
    }
}

struct AtomicSqlFile {
    destination: PathBuf,
    temporary: PathBuf,
    encoding: SqlFileEncoding,
    writer: Option<SqlFileWriter>,
}

#[cfg(not(windows))]
fn publish_staged_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    // POSIX rename replaces an existing destination as one atomic operation.
    fs::rename(temporary, destination)
}

#[cfg(windows)]
fn publish_staged_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    // Windows' std::fs::rename does not replace an existing file. MoveFileExW
    // with REPLACE_EXISTING keeps the sibling staging file and destination on
    // the same volume while providing the corresponding atomic replacement.
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let temporary: Vec<u16> = temporary.as_os_str().encode_wide().chain(once(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect();
    let replaced = unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

impl AtomicSqlFile {
    fn create(destination: PathBuf) -> Result<Self, TransferError> {
        Self::create_with_format(destination, SqlFileEncoding::Utf8, SqlFileCompression::None)
    }

    fn create_with_encoding(
        destination: PathBuf,
        encoding: SqlFileEncoding,
    ) -> Result<Self, TransferError> {
        Self::create_with_format(destination, encoding, SqlFileCompression::None)
    }

    fn create_with_format(
        destination: PathBuf,
        encoding: SqlFileEncoding,
        compression: SqlFileCompression,
    ) -> Result<Self, TransferError> {
        validate_path(&destination)?;
        validate_output_path(&destination, compression)?;
        let parent = destination
            .parent()
            .ok_or_else(|| TransferError::validation("SQL file destination has no parent"))?;
        for _ in 0..8 {
            let temporary = parent.join(format!(
                ".{}.{}.tmp",
                destination
                    .file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("datazen"),
                Uuid::new_v4()
            ));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&temporary) {
                Ok(file) => {
                    let mut writer = match compression {
                        SqlFileCompression::None => SqlFileWriter::Plain(BufWriter::new(file)),
                        SqlFileCompression::Gzip => SqlFileWriter::Gzip(GzEncoder::new(
                            BufWriter::new(file),
                            Compression::default(),
                        )),
                    };
                    let marker = match encoding {
                        SqlFileEncoding::Utf8 => None,
                        SqlFileEncoding::Utf8Bom => Some([0xEF, 0xBB, 0xBF].as_slice()),
                        SqlFileEncoding::Utf16Le => Some([0xFF, 0xFE].as_slice()),
                        SqlFileEncoding::Utf16Be => Some([0xFE, 0xFF].as_slice()),
                    };
                    if let Some(marker) = marker {
                        if let Err(error) = writer.write_all(marker) {
                            let _ = fs::remove_file(&temporary);
                            return Err(TransferError::validation(format!(
                                "cannot write SQL file encoding marker: {error}"
                            )));
                        }
                    }
                    return Ok(Self {
                        destination,
                        temporary,
                        encoding,
                        writer: Some(writer),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(TransferError::validation(format!(
                        "cannot create SQL file staging file: {error}"
                    )));
                }
            }
        }
        Err(TransferError::validation(
            "cannot allocate SQL file staging path",
        ))
    }

    fn line(&mut self, text: &str) -> Result<(), TransferError> {
        let bytes = encode_line(text, self.encoding);
        self.writer
            .as_mut()
            .ok_or_else(|| TransferError::validation("SQL file writer is already finished"))?
            .write_all(&bytes)
            .map_err(|error| TransferError::validation(format!("cannot write SQL file: {error}")))
    }

    fn finish(mut self) -> Result<(), TransferError> {
        let writer = self
            .writer
            .take()
            .ok_or_else(|| TransferError::validation("SQL file writer is already finished"))?;
        writer.finish().map_err(|error| {
            TransferError::validation(format!("cannot finalize SQL file: {error}"))
        })?;
        publish_staged_file(&self.temporary, &self.destination)
            .map_err(|error| TransferError::validation(format!("cannot publish SQL file: {error}")))
    }
}

fn encode_line(text: &str, encoding: SqlFileEncoding) -> Vec<u8> {
    match encoding {
        SqlFileEncoding::Utf8 | SqlFileEncoding::Utf8Bom => {
            let mut bytes = text.as_bytes().to_vec();
            bytes.push(b'\n');
            bytes
        }
        SqlFileEncoding::Utf16Le | SqlFileEncoding::Utf16Be => {
            let mut bytes = Vec::with_capacity((text.len() + 1) * 2);
            for unit in text.encode_utf16().chain(std::iter::once('\n' as u16)) {
                let encoded = if matches!(encoding, SqlFileEncoding::Utf16Le) {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                };
                bytes.extend_from_slice(&encoded);
            }
            bytes
        }
    }
}

impl Drop for AtomicSqlFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.temporary);
    }
}

fn qualify_target_relation(
    driver: &dyn DatabaseDriver,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> String {
    let family = driver.driver_type().to_ascii_lowercase();
    if family == "sqlserver" {
        return qualify_sqlserver_target_relation(database, schema, table);
    }
    qualify_relation_sql(&family, database, schema, table, driver.quote_char())
}

fn qualify_sqlserver_target_relation(
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> String {
    let effective_schema = schema.or_else(|| database.map(|_| "dbo"));
    [database, effective_schema, Some(table)]
        .into_iter()
        .flatten()
        .map(quote_sqlserver_ident)
        .collect::<Vec<_>>()
        .join(".")
}

fn quote_sqlserver_ident(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

fn effective_target_scope(job: &TransferJob) -> (Option<&str>, Option<&str>) {
    let (database, schema) = match job.sql_file_target.as_ref() {
        Some(target)
            if target.normalized_database().is_some() || target.normalized_schema().is_some() =>
        {
            (target.normalized_database(), target.normalized_schema())
        }
        // Preserve the legacy source-schema fallback for old SQL-file plans
        // that did not carry an explicit target scope.
        _ => (None, job.source.schema.as_deref()),
    };
    (database, schema)
}

fn effective_target_schema(job: &TransferJob) -> Option<&str> {
    effective_target_scope(job).1
}

pub fn target_table_ref(driver: &dyn DatabaseDriver, job: &TransferJob, table: &str) -> String {
    let (database, schema) = effective_target_scope(job);
    qualify_target_relation(driver, database, schema, table)
}

/// A custom DDL string is an opaque dialect-specific escape hatch. It cannot
/// be safely re-rendered when the SQL-file target dialect differs from the
/// source, so reject it before preview and execution instead of emitting a
/// source-dialect statement into a target-dialect artifact.
pub fn validate_target_dialect_job(job: &TransferJob) -> Result<(), TransferError> {
    let Some(target) = job.sql_file_target.as_ref() else {
        return Ok(());
    };
    target.validate_qualifiers()?;
    if job.mode == TransferMode::Data && job.write_mode == WriteMode::DropCreateInsert {
        return Err(TransferError::unsupported(
            "SQL-file Data-only transfer cannot use Drop + Create + Insert because Data mode does not emit CREATE TABLE; select Structure + Data or choose Insert/Truncate + Insert",
        ));
    }
    if target.has_explicit_scope()
        && job.tables.iter().any(|mapping| {
            mapping.enabled
                && mapping
                    .ddl_override
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|ddl| !ddl.is_empty())
        })
    {
        return Err(TransferError::validation(
            "custom SQL-file DDL cannot be combined with an explicit target database/schema; clear the DDL override and preview again",
        ));
    }
    if target.normalized_database_type().is_none() {
        return Ok(());
    }
    if job.tables.iter().any(|mapping| {
        mapping.enabled
            && mapping
                .ddl_override
                .as_deref()
                .map(str::trim)
                .is_some_and(|ddl| !ddl.is_empty())
    }) {
        return Err(TransferError::validation(
            "custom SQL-file DDL is unavailable with an explicit target dialect; clear the DDL override and preview again",
        ));
    }
    Ok(())
}

fn source_table_ref(driver: &dyn DatabaseDriver, job: &TransferJob, table: &str) -> String {
    qualify_relation_sql(
        &driver.driver_type(),
        Some(&job.source.database),
        job.source.schema.as_deref(),
        table,
        driver.quote_char(),
    )
}

pub fn create_table_sql(
    driver: &dyn DatabaseDriver,
    job: &TransferJob,
    table: &TableInspectResult,
    schema: &TableSchema,
) -> Result<String, TransferError> {
    let mappings = active_column_mappings(&table.column_mappings);
    if mappings.is_empty() {
        return Err(TransferError::validation(format!(
            "table '{}' has no active column mappings",
            table.source_table
        )));
    }
    let columns = mappings
        .iter()
        .map(|mapping| {
            let source = schema
                .columns
                .iter()
                .find(|column| column.name == mapping.source_column)
                .ok_or_else(|| {
                    TransferError::validation(format!(
                        "source column '{}' not found",
                        mapping.source_column
                    ))
                })?;
            let mut ddl = format!(
                "{} {}",
                quote_ident_sql(&mapping.target_column, driver.quote_char()),
                mapping
                    .target_native_type
                    .as_deref()
                    .unwrap_or(&source.data_type)
            );
            if !source.nullable {
                ddl.push_str(" NOT NULL");
            }
            if let Some(default) = source.default_value.as_deref() {
                if !default.trim().is_empty() {
                    ddl.push_str(" DEFAULT ");
                    ddl.push_str(default);
                }
            }
            Ok(ddl)
        })
        .collect::<Result<Vec<_>, TransferError>>()?;
    let mut parts = columns;
    let mapped_primary_keys: Vec<String> = schema
        .effective_primary_keys()
        .into_iter()
        .filter_map(|key| mappings.iter().find(|mapping| mapping.source_column == key))
        .map(|mapping| quote_ident_sql(&mapping.target_column, driver.quote_char()))
        .collect();
    if !mapped_primary_keys.is_empty() {
        parts.push(format!("PRIMARY KEY ({})", mapped_primary_keys.join(", ")));
    }
    Ok(format!(
        "CREATE TABLE IF NOT EXISTS {} ({})",
        target_table_ref(driver, job, &table.target_table),
        parts.join(", ")
    ))
}

/// Render a CREATE statement through the target dialect's IR adapter. Source
/// metadata and defaults are interpreted by the source adapter; identifiers,
/// native types, defaults, and constraints are emitted by the target adapter.
pub fn create_table_sql_with_target(
    source_adapter: &dyn SyncSourceAdapter,
    target_adapter: &dyn SyncTargetAdapter,
    target_driver: &dyn DatabaseDriver,
    job: &TransferJob,
    table: &TableInspectResult,
    schema: &TableSchema,
) -> Result<String, TransferError> {
    let mappings = active_column_mappings(&table.column_mappings);
    if mappings.is_empty() {
        return Err(TransferError::validation(format!(
            "table '{}' has no active column mappings",
            table.source_table
        )));
    }
    let mapping = job
        .tables
        .iter()
        .find(|mapping| mapping.source_table == table.source_table);
    let mut ir = source_adapter.table_to_ir(schema, None);
    ir.name = table.target_table.clone();
    if let Some(mapping) = mapping {
        super::structure::apply_column_type_overrides(&mut ir, mapping, target_adapter)?;
    }
    for column in &ir.columns {
        if let Some(default) = &column.default_expr {
            if matches!(default, IRDefault::RawExpression(_))
                || target_adapter.format_default(default).is_none()
            {
                return Err(TransferError::unsupported(format!(
                    "default on '{}.{}' cannot be rendered by the SQL-file target without loss",
                    table.source_table, column.name
                )));
            }
            if !crate::transfer::ddl::transfer_column_default_is_supported(
                &column.ir_type,
                target_adapter,
            ) {
                return Err(TransferError::unsupported(format!(
                    "default on '{}.{}' cannot be represented by the SQL-file target type",
                    table.source_table, column.name
                )));
            }
        }
    }
    Ok(crate::transfer::ddl::build_transfer_create_table_ddl_ref(
        &ir,
        target_adapter,
        &target_table_ref(target_driver, job, &table.target_table),
    ))
}

/// Reject source native types for which the registered IR bridge has no safe
/// cross-dialect representation. Falling back to the source type would emit
/// SQL that looks valid while changing the target schema semantics.
pub fn validate_target_ir(
    source_adapter: &dyn SyncSourceAdapter,
    source_driver: &dyn DatabaseDriver,
    target_driver: &dyn DatabaseDriver,
    schemas: &HashMap<String, TableSchema>,
    inspected: &[TableInspectResult],
) -> Result<(), TransferError> {
    if source_driver.sync_family() == target_driver.sync_family() {
        return Ok(());
    }
    let selected: std::collections::HashSet<&str> = inspected
        .iter()
        .filter(|table| table.enabled)
        .map(|table| table.source_table.as_str())
        .collect();
    for (table, schema) in schemas
        .iter()
        .filter(|(table, _)| selected.contains(table.as_str()))
    {
        for column in &source_adapter.table_to_ir(schema, None).columns {
            if let IRType::Other(native) = &column.ir_type {
                return Err(TransferError::unsupported(format!(
                    "target dialect '{}' has no safe IR mapping for {}.{} ({native})",
                    target_driver.driver_type(),
                    table,
                    column.name
                )));
            }
            if let Some(IRDefault::RawExpression(expression)) = &column.default_expr {
                return Err(TransferError::unsupported(format!(
                    "target dialect '{}' cannot safely preserve default expression on {}.{} ({expression})",
                    target_driver.driver_type(),
                    table,
                    column.name
                )));
            }
        }
    }
    Ok(())
}

fn insert_sql(
    driver: &dyn DatabaseDriver,
    job: &TransferJob,
    table: &TableInspectResult,
    mappings: &[&ColumnMapping],
    row: &[Option<Value>],
) -> Result<String, TransferError> {
    insert_sql_batch(driver, job, table, mappings, &[row.to_vec()])
}

fn mapped_insert_target_columns(mappings: &[&ColumnMapping]) -> Vec<String> {
    mappings
        .iter()
        .map(|mapping| mapping.target_column.clone())
        .collect()
}

fn insert_sql_batch(
    driver: &dyn DatabaseDriver,
    job: &TransferJob,
    table: &TableInspectResult,
    mappings: &[&ColumnMapping],
    rows: &[Vec<Option<Value>>],
) -> Result<String, TransferError> {
    let columns = mappings
        .iter()
        .map(|mapping| quote_ident_sql(&mapping.target_column, driver.quote_char()))
        .collect::<Vec<_>>();
    let values = rows
        .iter()
        .map(|row| {
            if mappings.len() != row.len() {
                return Err(TransferError::validation(format!(
                    "projected row has {} values, expected {}",
                    row.len(),
                    mappings.len()
                )));
            }
            Ok(format!(
                "({})",
                row.iter()
                    .map(|value| driver.format_sql_literal(value))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
        .collect::<Result<Vec<_>, TransferError>>()?;
    render_sql_file_insert(
        driver,
        &target_table_ref(driver, job, &table.target_table),
        &columns.join(", "),
        &values.join(", "),
    )
}

fn render_sql_file_insert(
    driver: &dyn DatabaseDriver,
    target_table: &str,
    columns: &str,
    values: &str,
) -> Result<String, TransferError> {
    let prefix = format!("INSERT INTO {target_table} ({columns})");
    let marker = loop {
        let candidate = format!(
            "/*DATAZEN_TRANSFER_IDENTITY_OVERRIDE_{}*/",
            Uuid::new_v4().simple()
        );
        if !prefix.contains(&candidate) && !values.contains(&candidate) {
            break candidate;
        }
    };
    let template = format!("{prefix} {marker} VALUES {values}");
    driver
        .render_transfer_sql_file_insert(&template, &marker)
        .map_err(|error| TransferError::unsupported(error.to_string()))
}

fn insert_sql_with_target(
    target_driver: &dyn DatabaseDriver,
    target_adapter: &dyn SyncTargetAdapter,
    source_column_ir_types: &HashMap<String, IRType>,
    job: &TransferJob,
    table: &TableInspectResult,
    mappings: &[&ColumnMapping],
    row: &[Option<Value>],
) -> Result<String, TransferError> {
    insert_sql_with_target_batch(
        target_driver,
        target_adapter,
        source_column_ir_types,
        job,
        table,
        mappings,
        &[row.to_vec()],
    )
}

fn insert_sql_with_target_batch(
    target_driver: &dyn DatabaseDriver,
    target_adapter: &dyn SyncTargetAdapter,
    source_column_ir_types: &HashMap<String, IRType>,
    job: &TransferJob,
    table: &TableInspectResult,
    mappings: &[&ColumnMapping],
    rows: &[Vec<Option<Value>>],
) -> Result<String, TransferError> {
    let columns = mappings
        .iter()
        .map(|mapping| target_adapter.quote_ident(&mapping.target_column))
        .collect::<Vec<_>>();
    let values = rows
        .iter()
        .map(|row| {
            if mappings.len() != row.len() {
                return Err(TransferError::validation(format!(
                    "projected row has {} values, expected {}",
                    row.len(),
                    mappings.len()
                )));
            }
            let row_values = mappings
                .iter()
                .zip(row.iter())
                .map(|(mapping, value)| {
                    let ir_type = source_column_ir_types
                        .get(&mapping.source_column)
                        .ok_or_else(|| {
                            TransferError::validation(format!(
                                "missing IR type for {}.{}",
                                table.source_table, mapping.source_column
                            ))
                        })?;
                    let transformed = target_adapter.transform_value(value, ir_type);
                    if value.is_some() && transformed.is_none() {
                        return Err(TransferError::validation(format!(
                            "target dialect '{}' cannot represent {}.{}",
                            target_driver.driver_type(),
                            table.source_table,
                            mapping.source_column
                        )));
                    }
                    Ok(target_adapter.format_literal(&transformed, ir_type))
                })
                .collect::<Result<Vec<_>, TransferError>>()?;
            Ok(format!("({})", row_values.join(", ")))
        })
        .collect::<Result<Vec<_>, TransferError>>()?;
    render_sql_file_insert(
        target_driver,
        &target_table_ref(target_driver, job, &table.target_table),
        &columns.join(", "),
        &values.join(", "),
    )
}

/// Execute a transfer into one atomically published SQL file using the source
/// dialect for identifiers, DDL, and literal formatting. This wrapper keeps
/// the original source-dialect contract for old callers and plans.
pub async fn execute(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    destination: PathBuf,
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<TransferExecutionResult, TransferError> {
    execute_with_target(
        driver,
        driver,
        None,
        None,
        handle,
        job,
        inspected,
        source_schemas,
        destination,
        cancelled,
        None,
    )
    .await
}

/// Execute a SQL-file transfer with independent source and target dialects.
/// Source scanning remains owned by `source_driver`; the target driver and IR
/// adapters own every emitted identifier, type, default, and literal.
pub async fn execute_with_target(
    source_driver: &dyn DatabaseDriver,
    target_driver: &dyn DatabaseDriver,
    source_adapter: Option<&dyn SyncSourceAdapter>,
    target_adapter: Option<&dyn SyncTargetAdapter>,
    handle: &ConnectionHandle,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    destination: PathBuf,
    cancelled: Option<Arc<AtomicBool>>,
    immutable_structure: Option<&[DdlPreviewItem]>,
) -> Result<TransferExecutionResult, TransferError> {
    let ir_rendering = source_adapter.zip(target_adapter);
    if (source_adapter.is_some()) != (target_adapter.is_some()) {
        return Err(TransferError::validation(
            "SQL file target requires both source and target IR adapters",
        ));
    }
    if let Some(source_adapter) = source_adapter {
        super::structure::validate_transfer_source_columns(
            job,
            inspected,
            source_schemas,
            source_adapter,
        )?;
        if let Some(target_adapter) = target_adapter {
            super::structure::validate_transfer_column_types(
                job,
                inspected,
                source_schemas,
                source_adapter,
                target_adapter,
            )?;
        }
        validate_target_ir(
            source_adapter,
            source_driver,
            target_driver,
            source_schemas,
            inspected,
        )?;
    }
    validate_target_dialect_job(job)?;
    if let Some(target) = job.sql_file_target.as_ref() {
        validate_target_scope_for_driver(target_driver, target)?;
    }
    job.options.validate()?;
    let encoding = job
        .sql_file_target
        .as_ref()
        .map(|target| target.normalized_encoding())
        .unwrap_or_default();
    let compression = job
        .sql_file_target
        .as_ref()
        .map(|target| target.normalized_compression())
        .unwrap_or_default();
    let mut output = AtomicSqlFile::create_with_format(destination, encoding, compression)?;
    output.line("-- DataZen Data Transfer SQL export")?;
    output.line(target_driver.transfer_sql_file_begin_transaction())?;
    let mut results = Vec::new();
    let mut total = 0u64;
    let mut partial = false;

    let structure_mode = matches!(
        job.mode,
        TransferMode::Structure | TransferMode::StructureAndData
    );
    let structure_plan = if structure_mode {
        match immutable_structure {
            Some(statements) => Some(statements.to_vec()),
            None => Some(build_structure_plan(
                source_adapter,
                target_adapter,
                target_driver,
                job,
                inspected,
                source_schemas,
            )?),
        }
    } else {
        None
    };

    // Destructive table preambles are emitted before any CREATE statement so
    // the structure sequence remains tables-first and deterministic.
    for table in inspected
        .iter()
        .filter(|table| table.enabled && !table.source_table.is_empty())
    {
        let target_ref = target_table_ref(target_driver, job, &table.target_table);
        if matches!(job.write_mode, WriteMode::DropCreateInsert) {
            output.line(&format!("DROP TABLE IF EXISTS {target_ref};"))?;
        } else if matches!(job.write_mode, WriteMode::TruncateInsert) {
            output.line(&format!("TRUNCATE TABLE {target_ref};"))?;
        }
    }

    if let Some(statements) = &structure_plan {
        let enabled: std::collections::HashSet<&str> = inspected
            .iter()
            .filter(|table| table.enabled)
            .map(|table| table.source_table.as_str())
            .collect();
        for statement in statements {
            if !enabled.contains(statement.source_table.as_str())
                || statement
                    .depends_on
                    .iter()
                    .any(|dependency| !enabled.contains(dependency.as_str()))
            {
                return Err(TransferError::validation(format!(
                    "SQL-file structure statement for '{}' depends on an unselected table",
                    statement.source_table
                )));
            }
        }
        // CREATE TABLE statements must precede data. Secondary indexes and
        // foreign keys are deliberately deferred until after the data loop so
        // child/parent inserts and cyclic graphs do not fail during loading.
        for statement in statements
            .iter()
            .filter(|statement| matches!(statement.kind, DdlPreviewKind::Table))
        {
            output.line(&format!("{};", statement.ddl))?;
        }
    }

    for table in inspected
        .iter()
        .filter(|table| table.enabled && !table.source_table.is_empty())
    {
        if cancelled
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
        {
            partial = true;
            break;
        }
        let mappings = active_column_mappings(&table.column_mappings);
        let Some(schema) = source_schemas.get(&table.source_table) else {
            partial = true;
            results.push(TableExecutionResult {
                source_table: table.source_table.clone(),
                target_table: table.target_table.clone(),
                rows_inserted: Some(0),
                success: false,
                error: Some("source schema not loaded".into()),
                outcome: None,
            });
            if job.options.stop_on_error {
                break;
            }
            continue;
        };
        let mapping = job
            .tables
            .iter()
            .find(|mapping| mapping.source_table == table.source_table);
        if !matches!(
            job.mode,
            TransferMode::Data | TransferMode::StructureAndData
        ) {
            results.push(TableExecutionResult {
                source_table: table.source_table.clone(),
                target_table: table.target_table.clone(),
                rows_inserted: Some(0),
                success: true,
                error: None,
                outcome: None,
            });
            continue;
        }
        if mappings.is_empty() {
            partial = true;
            results.push(TableExecutionResult {
                source_table: table.source_table.clone(),
                target_table: table.target_table.clone(),
                rows_inserted: Some(0),
                success: false,
                error: Some("no column mappings".into()),
                outcome: None,
            });
            if job.options.stop_on_error {
                break;
            }
            continue;
        }
        let mut query = format!(
            "SELECT {} FROM {}",
            mappings
                .iter()
                .map(|mapping| quote_ident_sql(&mapping.source_column, source_driver.quote_char()))
                .collect::<Vec<_>>()
                .join(", "),
            source_table_ref(source_driver, job, &table.source_table)
        );
        let scope = super::recordset::build_source_scope(
            schema,
            mapping.and_then(|mapping| mapping.source_filter.as_ref()),
            mapping.and_then(|mapping| mapping.recordset.as_ref()),
            source_driver.quote_char(),
            &source_driver.driver_type(),
            |index, data_type| {
                source_driver
                    .parameter_placeholder(index, data_type)
                    .map_err(|error| TransferError::unsupported(error.to_string()))
            },
            |column| {
                schema
                    .columns
                    .iter()
                    .find(|candidate| candidate.name == column)
                    .map(|column| column.data_type.clone())
            },
        )?;
        scope.append_to(&mut query);
        let ir_types = match ir_rendering {
            Some((src_adapter, _)) => Some(
                src_adapter
                    .table_to_ir(schema, None)
                    .columns
                    .into_iter()
                    .map(|column| (column.name, column.ir_type))
                    .collect::<HashMap<_, _>>(),
            ),
            None => None,
        };
        let target_relation = target_table_ref(target_driver, job, &table.target_table);
        let mapped_target_columns = mapped_insert_target_columns(&mappings);
        let mut scan = super::scan::scan_rows_with_params(
            source_driver,
            handle,
            &query,
            &scope.params,
            mappings
                .iter()
                .map(|mapping| mapping.source_column.clone())
                .collect(),
            cancelled.clone(),
        )
        .await?;
        let mut rows = 0u64;
        let mut error = None;
        let insert_batch_size = target_driver.transfer_sql_file_insert_batch_size().max(1);
        let scan_batch_size = (job.options.batch_size as usize).max(insert_batch_size);
        loop {
            if cancelled
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::SeqCst))
            {
                error = Some("transfer cancelled; SQL file was not published".into());
                break;
            }
            let batch = scan.next_batch(scan_batch_size)?;
            if batch.is_empty() {
                break;
            }
            for row_chunk in batch.chunks(insert_batch_size) {
                let rendered = match ir_rendering {
                    Some((_src_adapter, tgt_adapter)) => {
                        let Some(ir_types) = ir_types.as_ref() else {
                            return Err(TransferError::validation(
                                "source IR adapter is unavailable",
                            ));
                        };
                        insert_sql_with_target_batch(
                            target_driver,
                            tgt_adapter,
                            &ir_types,
                            job,
                            table,
                            &mappings,
                            row_chunk,
                        )
                    }
                    None => insert_sql_batch(target_driver, job, table, &mappings, row_chunk),
                };
                let rendered = rendered.and_then(|sql| {
                    target_driver
                        .render_transfer_sql_file_identity_insert(
                            &sql,
                            &target_relation,
                            &mapped_target_columns,
                        )
                        .map_err(|error| TransferError::unsupported(error.to_string()))
                });
                match rendered {
                    Ok(sql) => {
                        output.line(&format!("{sql};"))?;
                        rows += row_chunk.len() as u64;
                    }
                    Err(err) => {
                        error = Some(err.to_string());
                        break;
                    }
                }
            }
            if error.is_some() {
                break;
            }
        }
        if let Some(err) = error {
            partial = true;
            results.push(TableExecutionResult {
                source_table: table.source_table.clone(),
                target_table: table.target_table.clone(),
                rows_inserted: Some(0),
                success: false,
                error: Some(err),
                outcome: None,
            });
            if job.options.stop_on_error {
                break;
            }
        } else {
            total += rows;
            if rows > 0 {
                let target_columns = mappings
                    .iter()
                    .map(|mapping| mapping.target_column.clone())
                    .collect::<Vec<_>>();
                for sql in target_driver
                    .render_transfer_identity_sequence_sync_sql(
                        effective_target_schema(job),
                        &table.target_table,
                        &target_columns,
                    )
                    .map_err(|error| TransferError::unsupported(error.to_string()))?
                {
                    output.line(&format!("{sql};"))?;
                }
            }
            results.push(TableExecutionResult {
                source_table: table.source_table.clone(),
                target_table: table.target_table.clone(),
                rows_inserted: Some(rows),
                success: true,
                error: None,
                outcome: None,
            });
        }
    }
    if !partial {
        if let Some(statements) = &structure_plan {
            for statement in statements.iter().filter(|statement| {
                matches!(
                    statement.kind,
                    DdlPreviewKind::Index | DdlPreviewKind::ForeignKey
                )
            }) {
                output.line(&format!("{};", statement.ddl))?;
            }
        }
    }
    if !partial
        && !cancelled
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
    {
        output.line(target_driver.transfer_sql_file_commit_transaction())?;
        output.finish()?;
        Ok(TransferExecutionResult {
            tables: results,
            rows_inserted: total,
            cancelled: false,
            partial: false,
            resume_token: None,
        })
    } else {
        Ok(TransferExecutionResult {
            tables: results,
            rows_inserted: 0,
            cancelled: cancelled
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::SeqCst)),
            partial: true,
            resume_token: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TableMapping;
    use crate::transfer::adapter::SyncSourceAdapter;
    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
    use datazen_driver_api::{ColumnSchema, TableSchema};
    use datazen_driver_mysql::MysqlSyncAdapter;
    use datazen_driver_postgres::PgSyncAdapter;
    use flate2::read::GzDecoder;
    use std::io::Read;

    #[test]
    fn path_registry_rejects_non_sql_and_resolves_opaque_token() {
        let dir = tempfile::tempdir().unwrap();
        assert!(register_path(dir.path().join("out.txt")).is_err());
        let token = register_path(dir.path().join("out.sql")).unwrap();
        assert_eq!(resolve_path(&token).unwrap(), dir.path().join("out.sql"));
        assert!(resolve_path("/absolute-path-is-not-a-token").is_err());

        let gzip_token = register_path(dir.path().join("out.sql.gz")).unwrap();
        assert_eq!(
            resolve_path(&gzip_token).unwrap(),
            dir.path().join("out.sql.gz")
        );
    }

    #[test]
    fn output_suffix_is_bound_to_compression_and_unknown_enums_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let sql = dir.path().join("out.sql");
        let gzip = dir.path().join("out.sql.gz");
        assert!(validate_output_path(&sql, SqlFileCompression::None).is_ok());
        assert!(validate_output_path(&sql, SqlFileCompression::Gzip).is_err());
        assert!(validate_output_path(&gzip, SqlFileCompression::Gzip).is_ok());
        assert!(validate_output_path(&gzip, SqlFileCompression::None).is_err());
        assert!(serde_json::from_str::<SqlFileEncoding>(r#""utf16Le""#).is_ok());
        assert!(serde_json::from_str::<SqlFileEncoding>(r#""cp936""#).is_err());
        assert!(serde_json::from_str::<SqlFileCompression>(r#""brotli""#).is_err());
    }

    #[test]
    fn sql_file_identity_wrapper_receives_actual_mapped_target_columns() {
        let mappings = vec![
            ColumnMapping {
                source_column: "ordinary_source_column".into(),
                target_column: "target_identity".into(),
                skip: false,
                target_native_type: None,
            },
            ColumnMapping {
                source_column: "source_identity".into(),
                target_column: "ordinary_target_column".into(),
                skip: false,
                target_native_type: None,
            },
            ColumnMapping {
                source_column: "unused_source_column".into(),
                target_column: "unmapped_target_identity".into(),
                skip: true,
                target_native_type: None,
            },
        ];
        let active_mappings = active_column_mappings(&mappings);
        assert_eq!(
            mapped_insert_target_columns(&active_mappings),
            vec![
                "target_identity".to_string(),
                "ordinary_target_column".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn postgres_sql_file_emits_identity_sequence_sync_after_data_inserts() {
        let source_schema = TableSchema {
            table_name: "source".into(),
            columns: vec![ColumnSchema {
                name: "id".into(),
                data_type: "BIGINT".into(),
                nullable: false,
                default_value: Some("nextval('source_id_seq'::regclass)".into()),
                comment: None,
                is_primary_key: true,
                is_auto_increment: true,
            }],
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        };
        let source = MockDriver::new(
            "postgresql",
            MockDriverOptions {
                columns: source_schema.columns.clone(),
                query_rows: vec![
                    vec![Some(Value::Integer(41))],
                    vec![Some(Value::Integer(42))],
                ],
                ..Default::default()
            },
        );
        let target = datazen_driver_postgres::PostgresDriver::new();
        let adapter = PgSyncAdapter;
        let table = TableInspectResult {
            source_table: "source".into(),
            target_table: "table.with\"quote".into(),
            status: super::super::model::TableMappingStatus::Matched,
            create_new: false,
            enabled: true,
            column_mappings: vec![ColumnMapping {
                source_column: "id".into(),
                target_column: "id.with\"quote".into(),
                skip: false,
                target_native_type: None,
            }],
            source_columns: vec!["id".into()],
            source_primary_keys: vec!["id".into()],
            target_columns: vec!["id.with\"quote".into()],
            source_column_types: HashMap::from([("id".into(), "BIGINT".into())]),
            target_column_types: HashMap::from([(
                "id.with\"quote".into(),
                crate::model::TransferTargetColumnType {
                    native_type: "BIGINT".into(),
                    character_set: None,
                    collation: None,
                },
            )]),
            incompatible_reason: None,
            source_row_count: Some(2),
            recordset: None,
        };
        let suffix = Uuid::new_v4().simple().to_string();
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join(format!("identity-{suffix}.sql"));
        let job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "source".into(),
                database: "source_db".into(),
                schema: Some("legacy_source_schema".into()),
            },
            target: None,
            sql_file_target: Some(super::super::model::SqlFileTarget {
                file_token: "token".into(),
                database_type: Some("postgresql".into()),
                database: None,
                schema: None,
                encoding: None,
                compression: None,
            }),
            mode: TransferMode::Data,
            write_mode: WriteMode::Insert,
            tables: vec![TableMapping {
                source_table: "source".into(),
                target_table: table.target_table.clone(),
                create_new: false,
                enabled: true,
                column_mappings: table.column_mappings.clone(),
                ddl_override: None,
                source_filter: None,
                recordset: None,
            }],
            options: Default::default(),
        };
        let schemas = HashMap::from([("source".into(), source_schema)]);
        let result = execute_with_target(
            source.as_ref(),
            &target,
            Some(&adapter),
            Some(&adapter),
            &ConnectionHandle {
                id: "source".into(),
                pool_id: "source".into(),
            },
            &job,
            &[table],
            &schemas,
            destination.clone(),
            None,
            None,
        )
        .await
        .expect("SQL-file transfer should finish");

        assert!(!result.partial);
        let script = fs::read_to_string(destination).unwrap();
        let insert_at = script.find("INSERT INTO").unwrap();
        let sync_at = script.find("pg_get_serial_sequence").unwrap();
        let commit_at = script.rfind("COMMIT;").unwrap();
        assert!(insert_at < sync_at && sync_at < commit_at, "{script}");
        assert!(script.contains(
            "INSERT INTO \"legacy_source_schema\".\"table.with\"\"quote\" (\"id.with\"\"quote\") /*DATAZEN_TRANSFER_IDENTITY_OVERRIDE_"
        ));
        assert!(script.contains("*/ VALUES (41), (42)"));
        assert_eq!(script.matches("INSERT INTO").count(), 1, "{script}");
        assert!(script.contains("pg_catalog.replace(v_insert_sql"));
        assert!(script.contains("OVERRIDING SYSTEM VALUE"));
        assert!(script
            .contains("v_relation text := '\"legacy_source_schema\".\"table.with\"\"quote\"'"));
        assert!(script.contains("'id.with\"quote'"));
        assert!(script.contains("ALTER SEQUENCE %s RESTART WITH %s"));
    }

    #[test]
    fn generated_insert_uses_driver_literals_and_mapped_columns() {
        let driver = MockDriver::new("postgres", Default::default());
        let job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "s".into(),
                database: "db".into(),
                schema: Some("public".into()),
            },
            target: None,
            sql_file_target: Some(super::super::model::SqlFileTarget {
                file_token: "token".into(),
                database_type: None,
                database: None,
                schema: None,
                encoding: None,
                compression: None,
            }),
            mode: TransferMode::Data,
            write_mode: WriteMode::Insert,
            tables: vec![],
            options: Default::default(),
        };
        let table = TableInspectResult {
            source_table: "users".into(),
            target_table: "people".into(),
            status: super::super::model::TableMappingStatus::CreateNew,
            create_new: true,
            enabled: true,
            column_mappings: vec![ColumnMapping {
                source_column: "name".into(),
                target_column: "display_name".into(),
                skip: false,
                target_native_type: None,
            }],
            source_columns: vec!["name".into()],
            source_primary_keys: vec![],
            target_columns: vec![],
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: None,
            recordset: None,
        };
        let mappings = active_column_mappings(&table.column_mappings);
        let sql = insert_sql(
            driver.as_ref(),
            &job,
            &table,
            &mappings,
            &[Some(Value::String("O'Reilly".into()))],
        )
        .unwrap();
        assert!(sql.contains("\"display_name\""));
        assert!(sql.contains("'O''Reilly'"));
    }

    #[test]
    fn explicit_mysql_target_changes_identifiers_literals_and_ddl() {
        let target_driver = datazen_driver_mysql::MysqlDriver::new(false);
        let source_adapter = PgSyncAdapter;
        let target_adapter = MysqlSyncAdapter { is_mariadb: false };
        let job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "s".into(),
                database: "db".into(),
                schema: Some("public".into()),
            },
            target: None,
            sql_file_target: Some(super::super::model::SqlFileTarget {
                file_token: "token".into(),
                database_type: Some("mysql".into()),
                database: None,
                schema: None,
                encoding: None,
                compression: None,
            }),
            mode: TransferMode::Structure,
            write_mode: WriteMode::Insert,
            tables: vec![TableMapping {
                source_table: "users".into(),
                target_table: "users".into(),
                create_new: true,
                enabled: true,
                column_mappings: vec![
                    ColumnMapping {
                        source_column: "id".into(),
                        target_column: "id".into(),
                        skip: false,
                        target_native_type: None,
                    },
                    ColumnMapping {
                        source_column: "enabled".into(),
                        target_column: "is_enabled".into(),
                        skip: false,
                        target_native_type: None,
                    },
                ],
                ddl_override: None,
                source_filter: None,
                recordset: None,
            }],
            options: Default::default(),
        };
        let schema = TableSchema {
            table_name: "users".into(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                },
                ColumnSchema {
                    name: "enabled".into(),
                    data_type: "boolean".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        };
        let table = TableInspectResult {
            source_table: "users".into(),
            target_table: "users".into(),
            status: super::super::model::TableMappingStatus::CreateNew,
            create_new: true,
            enabled: true,
            column_mappings: vec![
                ColumnMapping {
                    source_column: "id".into(),
                    target_column: "id".into(),
                    skip: false,
                    target_native_type: None,
                },
                ColumnMapping {
                    source_column: "enabled".into(),
                    target_column: "is_enabled".into(),
                    skip: false,
                    target_native_type: None,
                },
            ],
            source_columns: vec!["id".into(), "enabled".into()],
            source_primary_keys: vec!["id".into()],
            target_columns: vec![],
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: None,
            recordset: None,
        };
        let ddl = create_table_sql_with_target(
            &source_adapter,
            &target_adapter,
            &target_driver,
            &job,
            &table,
            &schema,
        )
        .unwrap();
        assert!(ddl.contains("CREATE TABLE `users`"), "{ddl}");
        assert!(ddl.contains("`id` INT NOT NULL"), "{ddl}");
        assert!(ddl.contains("`is_enabled` TINYINT(1) NOT NULL"), "{ddl}");
        assert!(ddl.contains("PRIMARY KEY (`id`)"), "{ddl}");

        let ir_types = source_adapter
            .table_to_ir(&schema, None)
            .columns
            .into_iter()
            .map(|column| (column.name, column.ir_type))
            .collect::<HashMap<_, _>>();
        let mappings = active_column_mappings(&table.column_mappings);
        let sql = insert_sql_with_target(
            &target_driver,
            &target_adapter,
            &ir_types,
            &job,
            &table,
            &mappings,
            &[Some(Value::Integer(7)), Some(Value::Bool(true))],
        )
        .unwrap();
        assert!(
            sql.contains("INSERT INTO `users` (`id`, `is_enabled`) VALUES (7, 1)"),
            "{sql}"
        );
    }

    #[test]
    fn mysql_sql_file_ddl_preserves_varchar_override_default_and_rejects_longtext() {
        let target_driver = datazen_driver_mysql::MysqlDriver::new(false);
        let source_adapter = PgSyncAdapter;
        let target_adapter = MysqlSyncAdapter { is_mariadb: false };
        let (table, mut schema) = structure_table("records", "records_copy", &[("id", "date")]);
        schema.columns[0].default_value = Some("'2024-01-01'::date".into());

        let make_job = |native_type: &str| {
            let table_mapping = TableMapping {
                source_table: "records".into(),
                target_table: "records_copy".into(),
                create_new: true,
                enabled: true,
                column_mappings: vec![ColumnMapping {
                    source_column: "id".into(),
                    target_column: "id".into(),
                    skip: false,
                    target_native_type: Some(native_type.into()),
                }],
                ddl_override: None,
                source_filter: None,
                recordset: None,
            };
            TransferJob {
                source: super::super::model::Endpoint {
                    db_session_id: "source".into(),
                    database: "source_db".into(),
                    schema: Some("public".into()),
                },
                target: None,
                sql_file_target: Some(super::super::model::SqlFileTarget {
                    file_token: "opaque".into(),
                    database_type: Some("mysql".into()),
                    database: None,
                    schema: None,
                    encoding: None,
                    compression: None,
                }),
                mode: TransferMode::Structure,
                write_mode: WriteMode::Insert,
                tables: vec![table_mapping],
                options: Default::default(),
            }
        };

        let error = create_table_sql_with_target(
            &source_adapter,
            &target_adapter,
            &target_driver,
            &make_job("LONGTEXT"),
            &table,
            &schema,
        )
        .expect_err("SQL-file CREATE must reject a default that LONGTEXT cannot store");
        assert!(error.to_string().contains("default on 'records.id'"));

        let ddl = create_table_sql_with_target(
            &source_adapter,
            &target_adapter,
            &target_driver,
            &make_job("VARCHAR(64)"),
            &table,
            &schema,
        )
        .expect("SQL-file CREATE can preserve the VARCHAR default");
        assert!(
            ddl.contains("`id` VARCHAR(64) NOT NULL DEFAULT '2024-01-01'"),
            "{ddl}"
        );
    }

    #[test]
    fn unregistered_sql_file_dialect_is_rejected() {
        let source: std::sync::Arc<dyn DatabaseDriver> =
            MockDriver::new("postgresql", Default::default());
        let target = super::super::model::SqlFileTarget {
            file_token: "token".into(),
            database_type: Some("not-a-registered-driver".into()),
            database: None,
            schema: None,
            encoding: None,
            compression: None,
        };
        let result = resolve_target_driver(source, &target);
        assert!(result.is_err(), "unknown dialect must fail");
        let error = result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(error.contains("not registered"));
    }

    #[test]
    fn target_scope_overrides_source_schema_for_mysql_dml_and_ddl() {
        let driver = datazen_driver_mysql::MysqlDriver::new(false);
        let mut job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "s".into(),
                database: "source_catalog".into(),
                schema: Some("source_schema".into()),
            },
            target: None,
            sql_file_target: Some(super::super::model::SqlFileTarget {
                file_token: "token".into(),
                database_type: Some("mysql".into()),
                database: Some("target_catalog".into()),
                schema: None,
                encoding: None,
                compression: None,
            }),
            mode: TransferMode::Data,
            write_mode: WriteMode::Insert,
            tables: vec![],
            options: Default::default(),
        };
        job.sql_file_target
            .as_mut()
            .unwrap()
            .normalize_qualifiers()
            .unwrap();
        validate_target_scope_for_driver(&driver, job.sql_file_target.as_ref().unwrap()).unwrap();
        let table = TableInspectResult {
            source_table: "users".into(),
            target_table: "people".into(),
            status: super::super::model::TableMappingStatus::Matched,
            create_new: false,
            enabled: true,
            column_mappings: vec![ColumnMapping {
                source_column: "id".into(),
                target_column: "id".into(),
                skip: false,
                target_native_type: None,
            }],
            source_columns: vec!["id".into()],
            source_primary_keys: vec!["id".into()],
            target_columns: vec!["id".into()],
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: None,
            recordset: None,
        };
        let mappings = active_column_mappings(&table.column_mappings);
        let insert =
            insert_sql(&driver, &job, &table, &mappings, &[Some(Value::Integer(1))]).unwrap();
        assert!(
            insert.contains("INSERT INTO `target_catalog`.`people`"),
            "{insert}"
        );
        assert!(!insert.contains("source_schema"), "{insert}");
        assert_eq!(
            target_table_ref(&driver, &job, "people"),
            "`target_catalog`.`people`"
        );
    }

    #[test]
    fn target_scope_overrides_source_catalog_for_postgres_ddl() {
        let driver = datazen_driver_postgres::PostgresDriver::new();
        let job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "s".into(),
                database: "source_catalog".into(),
                schema: Some("source_schema".into()),
            },
            target: None,
            sql_file_target: Some(super::super::model::SqlFileTarget {
                file_token: "token".into(),
                database_type: Some("postgresql".into()),
                database: None,
                schema: Some("target_schema".into()),
                encoding: None,
                compression: None,
            }),
            mode: TransferMode::Structure,
            write_mode: WriteMode::Insert,
            tables: vec![],
            options: Default::default(),
        };
        validate_target_scope_for_driver(&driver, job.sql_file_target.as_ref().unwrap()).unwrap();
        assert_eq!(
            target_table_ref(&driver, &job, "people"),
            "\"target_schema\".\"people\""
        );
        assert!(!target_table_ref(&driver, &job, "people").contains("source_schema"));
    }

    #[test]
    fn sqlserver_target_relation_always_uses_database_schema_table_order() {
        assert_eq!(
            qualify_sqlserver_target_relation(Some("archive"), Some("sales"), "people"),
            "[archive].[sales].[people]"
        );
        assert_eq!(
            qualify_sqlserver_target_relation(Some("archive"), None, "people"),
            "[archive].[dbo].[people]"
        );
        assert_eq!(quote_sqlserver_ident("peo]ple"), "[peo]]ple]");
    }

    #[test]
    fn target_scope_rejects_dotted_or_dialect_ambiguous_qualifiers() {
        let driver = datazen_driver_mysql::MysqlDriver::new(false);
        let dotted = super::super::model::SqlFileTarget {
            file_token: "token".into(),
            database_type: Some("mysql".into()),
            database: Some("tenant.public".into()),
            schema: None,
            encoding: None,
            compression: None,
        };
        assert!(dotted.validate_qualifiers().is_err());

        let schema = super::super::model::SqlFileTarget {
            file_token: "token".into(),
            database_type: Some("mysql".into()),
            database: None,
            schema: Some("public".into()),
            encoding: None,
            compression: None,
        };
        let error = validate_target_scope_for_driver(&driver, &schema).unwrap_err();
        assert!(error.to_string().contains("not a separate schema"));
    }

    #[test]
    fn target_scope_rejects_unknown_driver_qualifiers_instead_of_dropping_them() {
        let target = super::super::model::SqlFileTarget {
            file_token: "token".into(),
            database_type: Some("oracle".into()),
            database: Some("catalog".into()),
            schema: None,
            encoding: None,
            compression: None,
        };
        let error = validate_target_scope_for_family("oracle", &target).unwrap_err();
        assert!(error.to_string().contains("does not advertise"));
    }

    #[test]
    fn explicit_target_scope_rejects_custom_ddl_overrides() {
        let mut job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "s".into(),
                database: "source".into(),
                schema: Some("public".into()),
            },
            target: None,
            sql_file_target: Some(super::super::model::SqlFileTarget {
                file_token: "token".into(),
                database_type: None,
                database: None,
                schema: Some("target".into()),
                encoding: None,
                compression: None,
            }),
            mode: TransferMode::Structure,
            write_mode: WriteMode::Insert,
            tables: vec![TableMapping {
                source_table: "users".into(),
                target_table: "users".into(),
                create_new: true,
                enabled: true,
                column_mappings: Vec::new(),
                ddl_override: Some("CREATE TABLE users (id INTEGER)".into()),
                source_filter: None,
                recordset: None,
            }],
            options: Default::default(),
        };
        job.sql_file_target
            .as_mut()
            .unwrap()
            .normalize_qualifiers()
            .unwrap();
        let error = validate_target_dialect_job(&job).unwrap_err();
        assert!(error
            .to_string()
            .contains("explicit target database/schema"));
    }

    #[test]
    fn sql_file_data_only_drop_create_is_rejected_before_export() {
        let job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "s".into(),
                database: "source".into(),
                schema: None,
            },
            target: None,
            sql_file_target: Some(super::super::model::SqlFileTarget {
                file_token: "token".into(),
                database_type: Some("postgresql".into()),
                database: None,
                schema: None,
                encoding: None,
                compression: None,
            }),
            mode: TransferMode::Data,
            write_mode: WriteMode::DropCreateInsert,
            tables: vec![],
            options: Default::default(),
        };

        let error = validate_target_dialect_job(&job).unwrap_err();
        assert!(error.to_string().contains("does not emit CREATE TABLE"));
        assert!(error.to_string().contains("Structure + Data"));
    }

    #[test]
    fn atomic_writer_replaces_existing_destination_on_supported_platforms() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("out.sql");
        fs::write(&destination, "old").unwrap();
        let mut output = AtomicSqlFile::create(destination.clone()).unwrap();
        output.line("new").unwrap();
        assert_eq!(fs::read_to_string(&destination).unwrap(), "old");
        output.finish().unwrap();
        assert_eq!(fs::read_to_string(destination).unwrap(), "new\n");
    }

    #[test]
    fn atomic_writer_drops_staging_without_touching_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("out.sql");
        fs::write(&destination, "old").unwrap();
        let mut output = AtomicSqlFile::create(destination.clone()).unwrap();
        output.line("partial").unwrap();
        drop(output);
        assert_eq!(fs::read_to_string(destination).unwrap(), "old");
    }

    #[test]
    fn atomic_writer_emits_utf8_bom_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("out.sql");
        let mut output =
            AtomicSqlFile::create_with_encoding(destination.clone(), SqlFileEncoding::Utf8Bom)
                .unwrap();
        output.line("SELECT 1;").unwrap();
        output.finish().unwrap();
        let bytes = fs::read(destination).unwrap();
        assert!(bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
        assert!(bytes.ends_with(b"SELECT 1;\n"));
    }

    #[test]
    fn atomic_writer_round_trips_utf16le_and_utf16be_text() {
        let dir = tempfile::tempdir().unwrap();
        let text = "SELECT '中文 😀';";
        for (name, encoding, marker) in [
            ("le.sql", SqlFileEncoding::Utf16Le, [0xFF, 0xFE]),
            ("be.sql", SqlFileEncoding::Utf16Be, [0xFE, 0xFF]),
        ] {
            let destination = dir.path().join(name);
            let mut output = AtomicSqlFile::create_with_format(
                destination.clone(),
                encoding,
                SqlFileCompression::None,
            )
            .unwrap();
            output.line(text).unwrap();
            output.finish().unwrap();
            let bytes = fs::read(destination).unwrap();
            assert_eq!(&bytes[..2], &marker);
            let units = bytes[2..]
                .chunks_exact(2)
                .map(|chunk| match encoding {
                    SqlFileEncoding::Utf16Le => u16::from_le_bytes([chunk[0], chunk[1]]),
                    SqlFileEncoding::Utf16Be => u16::from_be_bytes([chunk[0], chunk[1]]),
                    _ => 0,
                })
                .collect::<Vec<_>>();
            assert_eq!(String::from_utf16(&units).unwrap(), format!("{text}\n"));
        }
    }

    #[test]
    fn atomic_writer_round_trips_gzip_without_touching_destination_until_finish() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("out.sql.gz");
        fs::write(&destination, b"old").unwrap();
        let mut output = AtomicSqlFile::create_with_format(
            destination.clone(),
            SqlFileEncoding::Utf8,
            SqlFileCompression::Gzip,
        )
        .unwrap();
        let statement = "INSERT INTO t VALUES ('O''Reilly', '中文 😀', X'00FF', 'AAE=', 12345678901234567890.123456789, '2026-09-22T12:34:56Z', '{\"emoji\":\"😀\",\"value\":42}');";
        output.line(statement).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        output.finish().unwrap();
        let compressed = fs::read(destination).unwrap();
        let mut decoder = GzDecoder::new(compressed.as_slice());
        let mut decoded = String::new();
        decoder.read_to_string(&mut decoded).unwrap();
        assert_eq!(decoded, format!("{statement}\n"));
    }

    #[test]
    fn test_tester_atomic_writer_round_trips_utf16_bom_inside_gzip() {
        let dir = tempfile::tempdir().unwrap();
        let text = "INSERT INTO t VALUES ('中文 😀', X'00FF', 12345678901234567890.123456789, '{\"ok\":true}');";
        for (name, encoding, marker) in [
            ("le.sql.gz", SqlFileEncoding::Utf16Le, [0xFF, 0xFE]),
            ("be.sql.gz", SqlFileEncoding::Utf16Be, [0xFE, 0xFF]),
        ] {
            let destination = dir.path().join(name);
            let mut output = AtomicSqlFile::create_with_format(
                destination.clone(),
                encoding,
                SqlFileCompression::Gzip,
            )
            .unwrap();
            output.line(text).unwrap();
            output.finish().unwrap();

            let compressed = fs::read(destination).unwrap();
            let mut decoder = GzDecoder::new(compressed.as_slice());
            let mut decoded = Vec::new();
            decoder.read_to_end(&mut decoded).unwrap();
            assert!(decoded.starts_with(&marker));
            let units = decoded[2..]
                .chunks_exact(2)
                .map(|chunk| match encoding {
                    SqlFileEncoding::Utf16Le => u16::from_le_bytes([chunk[0], chunk[1]]),
                    SqlFileEncoding::Utf16Be => u16::from_be_bytes([chunk[0], chunk[1]]),
                    _ => unreachable!(),
                })
                .collect::<Vec<_>>();
            assert_eq!(String::from_utf16(&units).unwrap(), format!("{text}\n"));
        }
    }

    fn structure_table(
        source: &str,
        target: &str,
        columns: &[(&str, &str)],
    ) -> (TableInspectResult, TableSchema) {
        let mappings = columns
            .iter()
            .map(|(source_column, target_column)| ColumnMapping {
                source_column: (*source_column).into(),
                target_column: (*target_column).into(),
                skip: false,
                target_native_type: None,
            })
            .collect::<Vec<_>>();
        let schema = TableSchema {
            table_name: source.into(),
            columns: columns
                .iter()
                .map(|(name, data_type)| ColumnSchema {
                    name: (*name).into(),
                    data_type: (*data_type).into(),
                    nullable: *name != "id",
                    default_value: None,
                    comment: None,
                    is_primary_key: *name == "id",
                    is_auto_increment: false,
                })
                .collect(),
            primary_keys: vec!["id".into()],
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            check_constraints: Vec::new(),
            table_options: Default::default(),
        };
        let inspected = TableInspectResult {
            source_table: source.into(),
            target_table: target.into(),
            status: super::super::model::TableMappingStatus::CreateNew,
            create_new: true,
            enabled: true,
            column_mappings: mappings,
            source_columns: columns.iter().map(|(name, _)| (*name).into()).collect(),
            source_primary_keys: vec!["id".into()],
            target_columns: Vec::new(),
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: None,
            recordset: None,
        };
        (inspected, schema)
    }

    #[test]
    fn cross_dialect_sql_file_rejects_raw_defaults_only_for_selected_tables() {
        let (table, mut schema) = structure_table("orders", "orders_copy", &[("id", "integer")]);
        schema.columns[0].default_value = Some("tenant_sequence.next_value()".into());
        let schemas = HashMap::from([("orders".into(), schema)]);
        let source = datazen_driver_postgres::PostgresDriver::new();
        let target = datazen_driver_mysql::MysqlDriver::new(false);
        let adapter = PgSyncAdapter;

        let error = validate_target_ir(&adapter, &source, &target, &schemas, &[table.clone()])
            .expect_err("a source-only expression must fail before SQL-file publication");
        assert!(error.to_string().contains("default expression"));

        validate_target_ir(&adapter, &source, &target, &schemas, &[])
            .expect("an unselected source table must not block this export");
    }

    #[test]
    fn structure_plan_orders_tables_then_indexes_then_foreign_keys_in_target_dialect() {
        let (parent, mut parent_schema) =
            structure_table("accounts", "accounts_copy", &[("id", "account_id")]);
        parent_schema.indexes.push(datazen_driver_api::IndexInfo {
            name: "accounts_name_idx".into(),
            columns: vec!["id".into()],
            is_unique: true,
            is_primary: false,
            index_type: "btree".into(),
        });
        let (child, mut child_schema) = structure_table(
            "invoices",
            "invoices_copy",
            &[("id", "invoice_id"), ("account_id", "account_ref")],
        );
        child_schema
            .foreign_keys
            .push(datazen_driver_api::ForeignKeyInfo {
                name: "invoice_account_fk".into(),
                columns: vec!["account_id".into()],
                referenced_table: "public.accounts".into(),
                referenced_columns: vec!["id".into()],
                on_update: "NO ACTION".into(),
                on_delete: "CASCADE".into(),
                deferrability: datazen_driver_api::ForeignKeyDeferrability::Unknown,
            });
        let mut schemas = HashMap::new();
        schemas.insert("accounts".into(), parent_schema);
        schemas.insert("invoices".into(), child_schema);
        let job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "source".into(),
                database: "source_catalog".into(),
                schema: Some("public".into()),
            },
            target: None,
            sql_file_target: Some(super::super::model::SqlFileTarget {
                file_token: "opaque".into(),
                database_type: Some("mysql".into()),
                database: None,
                schema: None,
                encoding: None,
                compression: None,
            }),
            mode: TransferMode::Structure,
            write_mode: WriteMode::Insert,
            tables: vec![
                TableMapping {
                    source_table: "invoices".into(),
                    target_table: "invoices_copy".into(),
                    create_new: true,
                    enabled: true,
                    column_mappings: vec![
                        ColumnMapping {
                            source_column: "id".into(),
                            target_column: "invoice_id".into(),
                            skip: false,
                            target_native_type: None,
                        },
                        ColumnMapping {
                            source_column: "account_id".into(),
                            target_column: "account_ref".into(),
                            skip: false,
                            target_native_type: None,
                        },
                    ],
                    ddl_override: None,
                    source_filter: None,
                    recordset: None,
                },
                TableMapping {
                    source_table: "accounts".into(),
                    target_table: "accounts_copy".into(),
                    create_new: true,
                    enabled: true,
                    column_mappings: vec![ColumnMapping {
                        source_column: "id".into(),
                        target_column: "account_id".into(),
                        skip: false,
                        target_native_type: None,
                    }],
                    ddl_override: None,
                    source_filter: None,
                    recordset: None,
                },
            ],
            options: Default::default(),
        };
        let mut inspected = vec![child, parent];
        inspected[0].column_mappings = vec![
            ColumnMapping {
                source_column: "id".into(),
                target_column: "invoice_id".into(),
                skip: false,
                target_native_type: None,
            },
            ColumnMapping {
                source_column: "account_id".into(),
                target_column: "account_ref".into(),
                skip: false,
                target_native_type: None,
            },
        ];
        inspected[1].column_mappings = vec![ColumnMapping {
            source_column: "id".into(),
            target_column: "account_id".into(),
            skip: false,
            target_native_type: None,
        }];
        let source_adapter = PgSyncAdapter;
        let target_adapter = MysqlSyncAdapter { is_mariadb: false };
        let target_driver = datazen_driver_mysql::MysqlDriver::new(false);
        let plan = build_structure_plan(
            Some(&source_adapter),
            Some(&target_adapter),
            &target_driver,
            &job,
            &inspected,
            &schemas,
        )
        .unwrap();

        assert_eq!(plan.len(), 4);
        assert!(matches!(plan[0].kind, DdlPreviewKind::Table));
        assert!(plan[0].ddl.contains("`accounts_copy`"));
        assert!(matches!(plan[1].kind, DdlPreviewKind::Table));
        assert!(plan[1].ddl.contains("`invoices_copy`"));
        assert!(matches!(plan[2].kind, DdlPreviewKind::Index));
        assert!(matches!(plan[3].kind, DdlPreviewKind::ForeignKey));
        assert!(plan[3].ddl.contains("REFERENCES `accounts_copy`"));
        assert!(plan[3].ddl.contains("`account_ref`"));
        assert!(plan.iter().all(|item| !item.ddl.contains("source_catalog")));
        assert!(plan.iter().all(|item| !item.ddl.contains("`public`")));
        assert_eq!(plan[3].depends_on, vec!["accounts"]);
    }

    #[test]
    fn structure_plan_fails_closed_for_unrepresentable_index_type() {
        let (table, mut schema) = structure_table("events", "events_copy", &[("id", "id")]);
        schema.indexes.push(datazen_driver_api::IndexInfo {
            name: "events_search_idx".into(),
            columns: vec!["id".into()],
            is_unique: false,
            is_primary: false,
            index_type: "gin".into(),
        });
        let mut schemas = HashMap::new();
        schemas.insert("events".into(), schema);
        let job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "source".into(),
                database: "source".into(),
                schema: None,
            },
            target: None,
            sql_file_target: Some(super::super::model::SqlFileTarget {
                file_token: "opaque".into(),
                database_type: Some("mysql".into()),
                database: None,
                schema: None,
                encoding: None,
                compression: None,
            }),
            mode: TransferMode::Structure,
            write_mode: WriteMode::Insert,
            tables: vec![TableMapping {
                source_table: "events".into(),
                target_table: "events_copy".into(),
                create_new: true,
                enabled: true,
                column_mappings: vec![ColumnMapping {
                    source_column: "id".into(),
                    target_column: "id".into(),
                    skip: false,
                    target_native_type: None,
                }],
                ddl_override: None,
                source_filter: None,
                recordset: None,
            }],
            options: Default::default(),
        };
        let source_adapter = PgSyncAdapter;
        let target_adapter = MysqlSyncAdapter { is_mariadb: false };
        let target_driver = datazen_driver_mysql::MysqlDriver::new(false);
        let error = build_structure_plan(
            Some(&source_adapter),
            Some(&target_adapter),
            &target_driver,
            &job,
            &[table],
            &schemas,
        )
        .expect_err("GIN must not be silently emitted as a MySQL index");
        assert!(error.to_string().contains("cannot represent index type"));
    }
}
