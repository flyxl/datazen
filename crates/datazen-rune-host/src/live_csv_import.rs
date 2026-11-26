//! Assemble shared CSV parsing and the reviewed SQLite append port.
use std::collections::HashSet;
use std::io::Read as _;
use std::path::Path;

use datazen::connection::application::ConnectionService;
use datazen_driver_api::{ConnectionConfig, Value};
use datazen_driver_sqlite::{CsvImportTarget, SqliteDriver};
use datazen_ui_api::native_csv_import::{CsvImportChoice, CsvImportReview};
use datazen_ui_api::{parse_csv_import, CsvParseOptions, ImportParseErrorKind};
use sha2::{Digest, Sha256};

use super::live_transfer::profile;

const MAX_BYTES: u64 = 256 * 1024;
const MAX_ROWS: usize = 100;
const MAX_COLUMNS: usize = 20;

struct Prepared {
    review: CsvImportReview,
    target: CsvImportTarget,
    config: ConnectionConfig,
    rows: Vec<Vec<Value>>,
}

pub(super) async fn review(
    service: &ConnectionService,
    choice: CsvImportChoice,
) -> Result<CsvImportReview, String> {
    Ok(prepare(service, choice).await?.review)
}

pub(super) async fn append(
    service: &ConnectionService,
    approved: CsvImportReview,
) -> Result<usize, String> {
    let current = prepare(service, approved.choice.clone()).await?;
    if current.review != approved {
        return Err("The CSV or target table changed. Preview the import again.".into());
    }
    SqliteDriver::append_reviewed_csv(
        &current.config,
        &approved.choice.table,
        &current.target,
        &approved.columns,
        &current.rows,
    )
    .await
}

async fn prepare(service: &ConnectionService, choice: CsvImportChoice) -> Result<Prepared, String> {
    let config = profile(service, &choice.connection_id).await?;
    if config.read_only {
        return Err("The CSV import target is read-only.".into());
    }
    let source = Path::new(&choice.source_path);
    if !source
        .extension()
        .and_then(|part| part.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
        || !source.is_file()
    {
        return Err("Choose an existing UTF-8 CSV file.".into());
    }
    let file = std::fs::File::open(source).map_err(|_| "Could not open the CSV file.")?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the CSV file.")?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("The CSV must be at most 256 KiB.".into());
    }
    let input = std::str::from_utf8(&bytes).map_err(|_| "The CSV must be UTF-8.")?;
    let csv = parse_csv_import(
        input.trim_start_matches('\u{feff}'),
        &CsvParseOptions {
            delimiter: ",".into(),
            // Keep the literal first record: the shared convenience header
            // mode fills blank headers, which a reviewed exact mapping rejects.
            first_row_is_header: false,
            ..Default::default()
        },
    )
    .map_err(|error| match error.kind {
        ImportParseErrorKind::UnterminatedQuote => "The CSV contains an unterminated quoted field.",
        _ => "Could not parse the CSV file.",
    })?;
    if !csv.diagnostics.is_empty() {
        return Err("The CSV has rows with the wrong number of fields.".into());
    }
    let data_count = csv.rows.len().saturating_sub(1);
    if data_count == 0
        || data_count > MAX_ROWS
        || csv.columns.is_empty()
        || csv.columns.len() > MAX_COLUMNS
    {
        return Err("The CSV needs headers and 1–100 data rows with at most 20 columns.".into());
    }
    let header = &csv.rows[0];
    let columns = csv
        .columns
        .iter()
        .map(|key| {
            header
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .map(str::to_owned)
        })
        .collect::<Option<Vec<_>>>()
        .ok_or("CSV headers must not be blank.")?;
    let names: HashSet<_> = columns.iter().collect();
    if names.len() != columns.len() {
        return Err("The CSV has duplicate headers.".into());
    }
    let target = SqliteDriver::inspect_csv_import_target(&config, &choice.table).await?;
    if columns.iter().any(|name| !target.columns.contains(name)) {
        return Err("Every CSV header must match an existing writable column exactly.".into());
    }
    let rows = csv
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            csv.columns
                .iter()
                .map(|key| match row.get(key) {
                    Some(serde_json::Value::String(value)) => Value::String(value.clone()),
                    _ => Value::Null,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let facts = serde_json::to_vec(&(&target.identity, &target.ddl, &target.columns))
        .map_err(|_| "Could not prepare the SQLite target review.")?;
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Could not identify the CSV file name.")?
        .to_owned();
    Ok(Prepared {
        review: CsvImportReview {
            choice,
            file_name,
            target_name: config.name.clone(),
            columns,
            row_count: rows.len(),
            file_digest: Sha256::digest(&bytes).into(),
            table_digest: Sha256::digest(facts).into(),
        },
        target,
        config,
        rows,
    })
}
