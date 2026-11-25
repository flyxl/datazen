//! Port of `packages/frontend/src/commands/file.ts` (T298).
//!
//! File IO that never accepts a path from JS (XSS-safe): dialog +
//! read/write happen atomically on the Rust side
//! (`crates/datazen-desktop/src/commands/file.rs`), so the UI layer's
//! job is only to shape the payloads and decode the receipts — the
//! `schema_diff` runner / `ai::wire` shaper pattern this crate's
//! command family uses.
//!
//! ## What maps where
//!
//! - The `fileCommands` object's nine legacy calls become the
//!   [`FileCommandRequest`] shapers: the three driver-sdk dialog
//!   re-exports (`saveTextWithDialog` / `saveBase64WithDialog` /
//!   `openBase64WithDialog` — §7.2's thin re-export transition rides
//!   the bound transport with the same command names the desktop crate
//!   registers) and the six host-only streaming/session commands
//!   (`openTextWithDialog`, the `begin/append/finish/abort` save
//!   session, and `export_tables_stream`). Every command name and
//!   payload key is the React source's verbatim.
//! - [`ExportTablesWire`] is the `export_tables_stream` request, wire
//!   shape camelCase exactly as the React test pins it
//!   (`payload.request.dbSessionId` — "wrapper forwards verbatim;
//!   camelCase→snake_case mapping is Tauri's job"). Optional keys are
//!   **absent** when `None`, matching JS's `undefined`-dropping (the
//!   `driver.rs` Serialize precedent).
//! - `ExportTableSource::Provided` exists in the type because the React
//!   union has it — rows the caller already holds never reach the Rust
//!   export command (the backend's `TableDataSource` has no provided
//!   variant by design); the web-shaped exporter consumes it
//!   (`platform/blob_tables_exporter.rs`).
//! - [`ExportTablesResult`] decodes the three arms exactly as the
//!   backend serializes them: `{"Saved": n}`, `{"Cancelled": null}`,
//!   `{"Aborted": {"partialDeleted": b}}` — the variant names are
//!   wire-visible (the backend's own comment pins why: a `rename_all`
//!   would break the frontend's `'Saved' in result` checks). A plain
//!   `"Cancelled"` string — serde's default external tag for a unit
//!   variant — is **not** accepted; the receipt this port decodes is
//!   the object form the desktop host emits.
//! - `onExportProgress` is the EventChannel seam: the host subscribes
//!   `listen_cross_window([`EXPORT_PROGRESS_EVENT`], …)` and feeds the
//!   raw payloads into [`ExportProgressEvent`] (no Tauri envelope, the
//!   React test's "RAW payload" pin).
//!
//! Known deltas: none behavioural — every command name and payload key
//! is the React source's verbatim.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The wire `ColumnInfo` (`../types` re-exports backend-client's; the
/// Rust face is the driver-api crate's) — one shape, not a second
/// mirror.
pub use datazen_driver_api::ColumnInfo;

/// One shaped legacy command: the `legacyInvoke` name plus its payload
/// (the `backup.rs` / `driver.rs` shaper shape).
#[derive(Clone, Debug, PartialEq)]
pub struct FileCommandRequest {
    /// The legacy command name.
    pub command: &'static str,
    /// The exact args object the React wrapper passes.
    pub value: Value,
}

impl FileCommandRequest {
    fn native(command: &'static str, value: Value) -> Self {
        Self { command, value }
    }
}

/// React `OpenedTextFile` — basename + content, no path (the dialog +
/// read happen atomically on the host).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedTextFile {
    pub file_name: String,
    pub content: String,
}

/// React `OpenedBinaryFile` (re-exported from `@datazen/driver-sdk`) —
/// basename + base64, no path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedBinaryFile {
    pub file_name: String,
    pub data_base64: String,
}

// ── The dialog re-exports (driver-sdk `fileCommands`) ────────────────────────

/// `fileCommands.saveTextWithDialog(contents, defaultFileName,
/// filterName, extensions)`: save UTF-8 text via the native save
/// dialog. The receipt is `false` when the dialog was dismissed.
pub fn save_text_with_dialog_request(
    contents: &str,
    default_file_name: &str,
    filter_name: &str,
    extensions: &[&str],
) -> FileCommandRequest {
    FileCommandRequest::native(
        "save_text_with_dialog",
        serde_json::json!({
            "contents": contents,
            "defaultFileName": default_file_name,
            "filterName": filter_name,
            "extensions": extensions,
        }),
    )
}

/// `fileCommands.saveBase64WithDialog(dataBase64, defaultFileName,
/// filterName, extensions)`: save base64-decoded bytes the same way.
pub fn save_base64_with_dialog_request(
    data_base64: &str,
    default_file_name: &str,
    filter_name: &str,
    extensions: &[&str],
) -> FileCommandRequest {
    FileCommandRequest::native(
        "save_base64_with_dialog",
        serde_json::json!({
            "dataBase64": data_base64,
            "defaultFileName": default_file_name,
            "filterName": filter_name,
            "extensions": extensions,
        }),
    )
}

/// `fileCommands.openBase64WithDialog(filterName, extensions)`: open a
/// binary file via the native open dialog; the receipt is
/// [`OpenedBinaryFile`] or `null` when cancelled. §7.1 declares no
/// binary-open capability, so this stays a transport call.
pub fn open_base64_with_dialog_request(
    filter_name: &str,
    extensions: &[&str],
) -> FileCommandRequest {
    FileCommandRequest::native(
        "open_base64_with_dialog",
        serde_json::json!({ "filterName": filter_name, "extensions": extensions }),
    )
}

// ── The host-only streaming/session commands ─────────────────────────────────

/// `fileCommands.openTextWithDialog(filterName, extensions)`: open a
/// text file; the receipt is [`OpenedTextFile`] or `null`.
pub fn open_text_with_dialog_request(
    filter_name: &str,
    extensions: &[&str],
) -> FileCommandRequest {
    FileCommandRequest::native(
        "open_text_with_dialog",
        serde_json::json!({ "filterName": filter_name, "extensions": extensions }),
    )
}

/// `fileCommands.beginSaveWithDialog(defaultFileName, filterName,
/// extensions)`: open a save dialog and keep an opaque write session
/// (the path never returns to the UI). The receipt is the session
/// token, or `null` if the user cancelled.
pub fn begin_save_with_dialog_request(
    default_file_name: &str,
    filter_name: &str,
    extensions: &[&str],
) -> FileCommandRequest {
    FileCommandRequest::native(
        "begin_save_with_dialog",
        serde_json::json!({
            "defaultFileName": default_file_name,
            "filterName": filter_name,
            "extensions": extensions,
        }),
    )
}

/// `fileCommands.appendSaveText(token, chunk)`: append one text chunk
/// to the write session.
pub fn append_save_text_request(token: &str, chunk: &str) -> FileCommandRequest {
    FileCommandRequest::native(
        "append_save_text",
        serde_json::json!({ "token": token, "chunk": chunk }),
    )
}

/// `fileCommands.finishSave(token)`: close and flush the write session.
pub fn finish_save_request(token: &str) -> FileCommandRequest {
    FileCommandRequest::native("finish_save", serde_json::json!({ "token": token }))
}

/// `fileCommands.abortSave(token)`: abort a save session — the
/// in-progress `.datazen-part` sibling is removed and the picked path
/// left untouched (DBR-06). The receipt says whether the partial file
/// was really deleted, so the UI can report it honestly.
pub fn abort_save_request(token: &str) -> FileCommandRequest {
    FileCommandRequest::native("abort_save", serde_json::json!({ "token": token }))
}

/// `fileCommands.exportTablesStream(request)`: stream the selected
/// tables to a single file or ZIP entirely on the Rust side (it opens
/// its own native save dialog); whole tables are never buffered in the
/// UI. The receipt is [`ExportTablesResult`] or `null`.
pub fn export_tables_stream_request(request: &ExportTablesWire) -> FileCommandRequest {
    FileCommandRequest::native(
        "export_tables_stream",
        serde_json::json!({ "request": request }),
    )
}

// ── The export request's wire types ──────────────────────────────────────────

/// `ExportMode`: what goes into the output files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportMode {
    StructureOnly,
    DataOnly,
    DataAndStructure,
}

/// `ExportDataFormat`: the data files' format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportDataFormat {
    Csv,
    Tsv,
    Json,
    Markdown,
    SqlInsert,
    SqlUpdate,
    Xlsx,
    Parquet,
}

/// `ExportOutputMode`: one file or a ZIP per table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportOutputMode {
    Single,
    Zip,
}

/// `ExportTableSource` — where one export item's rows come from (the
/// serde `{"type": …}` tag). `None` on [`ExportTableWire::source`]
/// means `table`: the named table as a SELECT.
///
/// A cell of a `provided` source is the wire `Value` shape:
/// null/boolean/number/string, a JSON object for `Json`, and a number
/// array for `Bytes` (a JSON array value is indistinguishable from
/// bytes on this wire shape, the same ambiguity query results already
/// carry).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ExportTableSource {
    /// The named table as a SELECT.
    Table,
    /// One statement the backend streams (filtered export).
    Sql { sql: String },
    /// Rows the caller already holds, with their schema — never
    /// touches the backend (consumed by the web-shaped exporter).
    Provided { columns: Vec<ColumnInfo>, rows: Vec<Vec<Option<Value>>> },
}

/// `ExportTableInput`: one table in an export request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTableWire {
    pub table_name: String,
    /// Schema the table lives in (PostgreSQL family); resolved per
    /// table. Absent when `None` (JS drops `undefined`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ddl: Option<String>,
    /// Row source; absent means the named table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ExportTableSource>,
    /// Key column for `sql_update` output; first column when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_key: Option<String>,
}

/// `ExportTablesRequest` — the `export_tables_stream` request, camelCase
/// on the wire (the React test pins `payload.request.dbSessionId`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTablesWire {
    /// Runtime db session id.
    pub db_session_id: String,
    /// Opaque per-run cancellation token, carried only by SQL-path
    /// requests. The desktop host registers it before the save dialog;
    /// `cancel_export_tables` flips it. Provided exports never reach
    /// Rust, so they never carry one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_type: Option<String>,
    /// Database to read from; the session's own database is only a
    /// fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// Fallback schema for tables that do not carry one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub mode: ExportMode,
    pub data_format: ExportDataFormat,
    pub output_mode: ExportOutputMode,
    pub tables: Vec<ExportTableWire>,
}

/// `ExportTablesResult`: the host's answer to `export_tables_stream`.
///
/// Wire-visible variant names (`{"Saved": 3}` / `{"Cancelled": null}` /
/// `{"Aborted": {"partialDeleted": true}}`) — the backend's comment
/// pins why they must stay: the frontend's `'Saved' in result` checks
/// would never match a lowercased tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportTablesResult {
    /// `n` files written.
    Saved(u64),
    /// The native save dialog was dismissed — a normal exit, not an
    /// error.
    Cancelled,
    /// The run was cancelled mid-flight; `partial_deleted` says whether
    /// the in-progress output was removed (an abort that could not
    /// delete it is reported instead of assumed).
    Aborted { partial_deleted: bool },
}

impl<'de> Deserialize<'de> for ExportTablesResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let object = value.as_object().ok_or_else(|| {
            serde::de::Error::custom("ExportTablesResult: expected a tagged object")
        })?;
        if let Some(saved) = object.get("Saved") {
            let count = saved.as_u64().ok_or_else(|| {
                serde::de::Error::custom("ExportTablesResult::Saved: expected a number")
            })?;
            return Ok(Self::Saved(count));
        }
        if object.contains_key("Cancelled") {
            return Ok(Self::Cancelled);
        }
        if let Some(serde_json::Value::Object(aborted)) = object.get("Aborted") {
            let partial_deleted = aborted.get("partialDeleted").and_then(Value::as_bool).ok_or_else(|| {
                serde::de::Error::custom(
                    "ExportTablesResult::Aborted: expected { partialDeleted: boolean }",
                )
            })?;
            return Ok(Self::Aborted { partial_deleted });
        }
        Err(serde::de::Error::custom(
            "ExportTablesResult: expected Saved / Cancelled / Aborted",
        ))
    }
}

/// The `batch-export-progress` subscription name (`onExportProgress`
/// rides the EventChannel port — form-split F6 prep, R6 burn).
pub const EXPORT_PROGRESS_EVENT: &str = "batch-export-progress";

/// Progress emitted by the Rust exporter while streaming a table to
/// disk. The EventChannel rail delivers the **raw** payload (no Tauri
/// envelope — the React test's pin), so this decodes it as-is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgressEvent {
    /// Table currently being streamed.
    pub table: String,
    /// Cumulative row count written for *this* table so far.
    pub rows_written: u64,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    // ---- the dialog re-exports (the React suite's exact wire pins) ----

    #[test]
    fn save_text_forwards_text_and_dialog_filter_args() {
        assert_eq!(
            save_text_with_dialog_request("body", "out.csv", "CSV", &["csv", ".txt"]),
            FileCommandRequest {
                command: "save_text_with_dialog",
                value: json!({
                    "contents": "body",
                    "defaultFileName": "out.csv",
                    "filterName": "CSV",
                    "extensions": ["csv", ".txt"],
                })
            }
        );
    }

    #[test]
    fn save_base64_forwards_payload_and_filter_args() {
        assert_eq!(
            save_base64_with_dialog_request("AAAA", "icon.png", "PNG", &["png"]),
            FileCommandRequest {
                command: "save_base64_with_dialog",
                value: json!({
                    "dataBase64": "AAAA",
                    "defaultFileName": "icon.png",
                    "filterName": "PNG",
                    "extensions": ["png"],
                })
            }
        );
    }

    #[test]
    fn open_base64_takes_only_the_filter() {
        assert_eq!(
            open_base64_with_dialog_request("ZIP", &[".zip"]),
            FileCommandRequest {
                command: "open_base64_with_dialog",
                value: json!({ "filterName": "ZIP", "extensions": [".zip"] })
            }
        );
    }

    #[test]
    fn the_opened_receipts_decode_without_any_path() {
        let text: OpenedTextFile =
            serde_json::from_value(json!({ "fileName": "a.sql", "content": "SELECT 1;" }))
                .expect("text decodes");
        assert_eq!(text.file_name, "a.sql");
        assert_eq!(text.content, "SELECT 1;");
        let binary: OpenedBinaryFile =
            serde_json::from_value(json!({ "fileName": "b.zip", "dataBase64": "UEs=" }))
                .expect("binary decodes");
        assert_eq!(binary.data_base64, "UEs=");
        // The shapes carry basename + payload only — never a path.
        assert!(
            serde_json::to_value(&binary).unwrap_or_default().to_string().find("path").is_none()
        );
    }

    // ---- the streaming save session ----

    #[test]
    fn begin_append_finish_abort_share_the_opaque_token() {
        assert_eq!(
            begin_save_with_dialog_request("log.txt", "Text", &["txt"]),
            FileCommandRequest {
                command: "begin_save_with_dialog",
                value: json!({
                    "defaultFileName": "log.txt",
                    "filterName": "Text",
                    "extensions": ["txt"],
                })
            }
        );
        assert_eq!(
            append_save_text_request("token-1", "chunk"),
            FileCommandRequest {
                command: "append_save_text",
                value: json!({ "token": "token-1", "chunk": "chunk" })
            }
        );
        assert_eq!(
            finish_save_request("token-1"),
            FileCommandRequest { command: "finish_save", value: json!({ "token": "token-1" }) }
        );
        assert_eq!(
            abort_save_request("token-2"),
            FileCommandRequest { command: "abort_save", value: json!({ "token": "token-2" }) }
        );
    }

    // ---- exportTablesStream + progress ----

    #[test]
    fn the_export_request_wraps_verbatim_and_keeps_camel_case() {
        let request = ExportTablesWire {
            db_session_id: "session-7".to_owned(),
            cancel_token: None,
            database_type: Some("sqlite".to_owned()),
            database: None,
            schema: None,
            mode: ExportMode::DataAndStructure,
            data_format: ExportDataFormat::SqlInsert,
            output_mode: ExportOutputMode::Zip,
            tables: vec![ExportTableWire {
                table_name: "users".to_owned(),
                schema: None,
                columns: vec!["id".to_owned()],
                // React's `ddl: null` in the test fixture is an
                // explicit null; on the wire this port mirrors the
                // production callers, which omit the key (JS drops
                // `undefined`), and the host's `#[serde(default)]`
                // reads both.
                ddl: None,
                source: None,
                primary_key: None,
            }],
        };
        let shaped = export_tables_stream_request(&request);
        assert_eq!(shaped.command, "export_tables_stream");
        // The React pin: `payload.request.dbSessionId` — camelCase
        // forwards verbatim; the snake_case mapping is Tauri's job.
        assert_eq!(shaped.value["request"]["dbSessionId"], json!("session-7"));
        assert_eq!(shaped.value["request"]["mode"], json!("data_and_structure"));
        assert_eq!(shaped.value["request"]["dataFormat"], json!("sql_insert"));
        assert_eq!(shaped.value["request"]["outputMode"], json!("zip"));
        assert_eq!(shaped.value["request"]["tables"][0]["tableName"], json!("users"));
        // Optional halves are absent, not null.
        let request_object = shaped.value["request"].as_object().expect("an object");
        assert!(!request_object.contains_key("cancelToken"));
        assert!(!request_object.contains_key("database"));
        assert!(!request_object.contains_key("schema"));
        assert!(!request_object.contains_key("source"));
        let table = shaped.value["request"]["tables"][0].as_object().expect("an object");
        assert!(!table.contains_key("schema"));
        assert!(!table.contains_key("ddl"));
        assert!(!table.contains_key("primaryKey"));
    }

    #[test]
    fn a_sql_source_and_a_cancel_token_ride_the_request() {
        let request = ExportTablesWire {
            db_session_id: "s".to_owned(),
            cancel_token: Some("run-42".to_owned()),
            database_type: None,
            database: Some("shop".to_owned()),
            schema: Some("public".to_owned()),
            mode: ExportMode::DataOnly,
            data_format: ExportDataFormat::Csv,
            output_mode: ExportOutputMode::Single,
            tables: vec![ExportTableWire {
                table_name: "orders".to_owned(),
                schema: Some("sales".to_owned()),
                columns: Vec::new(),
                ddl: Some("CREATE TABLE orders (id int);".to_owned()),
                source: Some(ExportTableSource::Sql { sql: "SELECT * FROM orders".to_owned() }),
                primary_key: Some("id".to_owned()),
            }],
        };
        let shaped = export_tables_stream_request(&request);
        assert_eq!(shaped.value["request"]["cancelToken"], json!("run-42"));
        assert_eq!(shaped.value["request"]["tables"][0]["source"], json!({
            "type": "sql",
            "sql": "SELECT * FROM orders",
        }));
        assert_eq!(shaped.value["request"]["tables"][0]["primaryKey"], json!("id"));
        // Round-trip: the wire decodes back into the same request.
        let decoded: ExportTablesWire =
            serde_json::from_value(shaped.value["request"].clone()).expect("round-trips");
        assert_eq!(decoded, request);
    }

    #[test]
    fn a_provided_source_carries_its_schema_and_rows() {
        let request = ExportTablesWire {
            db_session_id: "s".to_owned(),
            cancel_token: None,
            database_type: None,
            database: None,
            schema: None,
            mode: ExportMode::DataOnly,
            data_format: ExportDataFormat::Json,
            output_mode: ExportOutputMode::Single,
            tables: vec![ExportTableWire {
                table_name: "pinned".to_owned(),
                schema: None,
                columns: vec!["id".to_owned()],
                ddl: None,
                source: Some(ExportTableSource::Provided {
                    columns: vec![ColumnInfo {
                        name: "id".to_owned(),
                        data_type: "int8".to_owned(),
                        nullable: false,
                    }],
                    rows: vec![vec![Some(json!(1)), None]],
                }),
                primary_key: None,
            }],
        };
        let shaped = export_tables_stream_request(&request);
        assert_eq!(
            shaped.value["request"]["tables"][0]["source"],
            json!({
                "type": "provided",
                "columns": [{ "name": "id", "dataType": "int8", "nullable": false }],
                "rows": [[1, null]],
            })
        );
    }

    #[test]
    fn the_result_decodes_all_three_arms_and_refuses_everything_else() {
        let saved: ExportTablesResult =
            serde_json::from_value(json!({ "Saved": 3 })).expect("saved decodes");
        assert_eq!(saved, ExportTablesResult::Saved(3));
        let cancelled: ExportTablesResult =
            serde_json::from_value(json!({ "Cancelled": null })).expect("cancelled decodes");
        assert_eq!(cancelled, ExportTablesResult::Cancelled);
        let aborted: ExportTablesResult = serde_json::from_value(json!({
            "Aborted": { "partialDeleted": true }
        }))
        .expect("aborted decodes");
        assert_eq!(aborted, ExportTablesResult::Aborted { partial_deleted: true });
        let kept: ExportTablesResult = serde_json::from_value(json!({
            "Aborted": { "partialDeleted": false }
        }))
        .expect("an honest abort decodes");
        assert_eq!(kept, ExportTablesResult::Aborted { partial_deleted: false });

        // serde's default external tag for a unit variant is a bare
        // string — the receipt this port decodes is the object form.
        assert!(serde_json::from_value::<ExportTablesResult>(json!("Cancelled")).is_err());
        assert!(serde_json::from_value::<ExportTablesResult>(json!({ "saved": 3 })).is_err());
        assert!(serde_json::from_value::<ExportTablesResult>(json!(null)).is_err());
    }

    #[test]
    fn progress_subscribes_to_batch_export_progress_and_decodes_raw_payloads() {
        assert_eq!(EXPORT_PROGRESS_EVENT, "batch-export-progress");
        // The EventChannel rail delivers the RAW payload (no envelope).
        let event: ExportProgressEvent = serde_json::from_value(json!({
            "table": "users",
            "rowsWritten": 10,
        }))
        .expect("raw payload decodes");
        assert_eq!(event.table, "users");
        assert_eq!(event.rows_written, 10);
        // The wire spelling round-trips (the host emits camelCase).
        assert_eq!(
            serde_json::to_value(&event).unwrap_or_default(),
            json!({ "table": "users", "rowsWritten": 10 })
        );
    }
}
