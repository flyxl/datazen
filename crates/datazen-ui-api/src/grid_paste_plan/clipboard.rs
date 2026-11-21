use std::sync::Mutex;

use super::types::{ClipboardSource, RowCopyPayload};
use crate::TableChangeContext;

static LAST_COPY_SOURCE: Mutex<Option<ClipboardSource>> = Mutex::new(None);
static LAST_COPIED_TEXT: Mutex<Option<String>> = Mutex::new(None);
static LAST_ROW_COPY: Mutex<Option<RowCopyPayload>> = Mutex::new(None);
static LAST_ROW_COPY_TEXT: Mutex<Option<String>> = Mutex::new(None);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn normalize_clipboard_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

pub fn record_clipboard_source(source: ClipboardSource, text: Option<&str>) {
    *lock(&LAST_COPY_SOURCE) = Some(source);
    *lock(&LAST_COPIED_TEXT) = text.map(normalize_clipboard_text);
}

pub fn peek_clipboard_source() -> Option<ClipboardSource> {
    lock(&LAST_COPY_SOURCE).clone()
}

pub fn source_for_clipboard_text(text: &str) -> Option<ClipboardSource> {
    let source = lock(&LAST_COPY_SOURCE);
    let copied = lock(&LAST_COPIED_TEXT);
    let copied = copied.as_ref()?;
    if normalize_clipboard_text(text) == *copied {
        source.clone()
    } else {
        None
    }
}

pub fn clear_clipboard_source() {
    *lock(&LAST_COPY_SOURCE) = None;
    *lock(&LAST_COPIED_TEXT) = None;
}

pub fn record_row_copy(payload: RowCopyPayload, text: &str) {
    *lock(&LAST_ROW_COPY) = Some(payload);
    *lock(&LAST_ROW_COPY_TEXT) = Some(normalize_clipboard_text(text));
}

pub fn peek_row_copy() -> Option<RowCopyPayload> {
    lock(&LAST_ROW_COPY).clone()
}

pub fn row_copy_for_clipboard_text(text: &str) -> Option<RowCopyPayload> {
    let payload = lock(&LAST_ROW_COPY);
    let copied = lock(&LAST_ROW_COPY_TEXT);
    let copied = copied.as_ref()?;
    if normalize_clipboard_text(text) == *copied {
        payload.clone()
    } else {
        None
    }
}

pub fn clear_row_copy() {
    *lock(&LAST_ROW_COPY) = None;
    *lock(&LAST_ROW_COPY_TEXT) = None;
}

fn normalize_part(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_lowercase()
}

pub fn table_change_key(context: Option<&TableChangeContext>) -> String {
    let Some(context) = context else {
        return String::new();
    };
    serde_json::json!([
        normalize_part(Some(context.connection_id.as_str())),
        normalize_part(Some(context.database.as_str())),
        normalize_part(context.schema.as_deref()),
        normalize_part(Some(context.table.as_str())),
    ])
    .to_string()
}

pub fn pending_cell_key(row_index: i64, column_name: &str) -> String {
    format!("{row_index}:{column_name}")
}
