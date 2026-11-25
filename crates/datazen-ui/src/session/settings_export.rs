//! Port of `packages/frontend/src/lib/settingsExport.ts`.
//!
//! A safe whitelist serialiser for editor/snippet preferences: the export never
//! contains credentials, connection URLs, passwords, AI keys or any other
//! non-editor setting. Import is deliberately defensive — unknown/mistyped
//! fields are ignored so a stale or hostile file cannot overwrite a current
//! preference, and the two parse failures carry React's exact messages.
//!
//! React reads `AppSettings`, a store-wide type not present in the Rune port,
//! and calls `new Date().toISOString()`. Both are injected here:
//!
//! * [`EditorSettings`] / [`EditorSettingsPatch`] are local VMs carrying only
//!   the whitelisted fields. Removal stage: use `AppSettings` from
//!   `datazen_ui_api` when that DTO lands.
//! * `export_editor_settings` takes the timestamp as a string. Removal stage:
//!   format the host clock at the call site; no wall clock in pure logic.
//!
//! No view consumer yet: every item carries a per-item `#[allow(dead_code)]`
//! with the removal stage "drop when the settings view wires this".

use serde_json::{Map, Number, Value};

/// One snippet row from the whitelist. React's import guard requires only
/// `id`, `prefix` and `template`; `descriptionKey` may be absent.
#[allow(dead_code)] // local VM; remove with the shared settings DTO.
#[derive(Clone, Debug, PartialEq)]
pub struct SqlSnippet {
    pub id: String,
    pub prefix: String,
    pub description_key: Option<String>,
    pub template: String,
}

/// The whitelisted editor preferences, as the host holds them.
#[allow(dead_code)] // local VM; remove with the shared settings DTO.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorSettings {
    pub font_size: Option<Number>,
    pub font_family: Option<String>,
    pub completion_quote_policy: Option<String>,
    pub completion_include_table_prefix: Option<bool>,
    pub keymap_preset: Option<String>,
    pub custom_keymap: Option<Map<String, Value>>,
    pub sql_format_options: Option<Map<String, Value>>,
    pub sql_execution_strategy: Option<String>,
    pub sql_snippets: Vec<SqlSnippet>,
}

/// What an import may change: absent fields leave the current value untouched.
#[allow(dead_code)] // local VM; remove with the shared settings DTO.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorSettingsPatch {
    pub font_size: Option<Number>,
    pub font_family: Option<String>,
    pub completion_quote_policy: Option<String>,
    pub completion_include_table_prefix: Option<bool>,
    pub keymap_preset: Option<String>,
    pub custom_keymap: Option<Map<String, Value>>,
    pub sql_format_options: Option<Map<String, Value>>,
    pub sql_execution_strategy: Option<String>,
    pub sql_snippets: Option<Vec<SqlSnippet>>,
}

#[allow(dead_code)] // error mirror; remove with the settings view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsImportError {
    InvalidJson,
    RootMustBeObject,
}

#[allow(dead_code)] // error mirror; remove with the settings view.
impl SettingsImportError {
    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidJson => "Invalid JSON format",
            Self::RootMustBeObject => "Root must be an object",
        }
    }
}

/// React `exportEditorSettings`: pretty JSON (2-space indent) of only the
/// whitelisted editor/snippet fields. `exported_at` is the host's ISO string.
#[allow(dead_code)] // helper; remove with the settings view.
pub fn export_editor_settings(settings: &EditorSettings, exported_at: &str) -> String {
    let mut editor = Map::new();
    if let Some(value) = &settings.font_size {
        editor.insert("fontSize".to_owned(), Value::Number(value.clone()));
    }
    if let Some(value) = &settings.font_family {
        editor.insert("fontFamily".to_owned(), Value::String(value.clone()));
    }
    if let Some(value) = &settings.completion_quote_policy {
        editor.insert("completionQuotePolicy".to_owned(), Value::String(value.clone()));
    }
    if let Some(value) = settings.completion_include_table_prefix {
        editor.insert("completionIncludeTablePrefix".to_owned(), Value::Bool(value));
    }
    if let Some(value) = &settings.keymap_preset {
        editor.insert("keymapPreset".to_owned(), Value::String(value.clone()));
    }
    if let Some(value) = &settings.custom_keymap {
        editor.insert("customKeymap".to_owned(), Value::Object(value.clone()));
    }
    if let Some(value) = &settings.sql_format_options {
        editor.insert("sqlFormatOptions".to_owned(), Value::Object(value.clone()));
    }
    if let Some(value) = &settings.sql_execution_strategy {
        editor.insert("sqlExecutionStrategy".to_owned(), Value::String(value.clone()));
    }

    let snippets: Vec<Value> = settings
        .sql_snippets
        .iter()
        .map(|snippet| {
            let mut row = Map::new();
            row.insert("id".to_owned(), Value::String(snippet.id.clone()));
            row.insert("prefix".to_owned(), Value::String(snippet.prefix.clone()));
            if let Some(description) = &snippet.description_key {
                row.insert("descriptionKey".to_owned(), Value::String(description.clone()));
            }
            row.insert("template".to_owned(), Value::String(snippet.template.clone()));
            Value::Object(row)
        })
        .collect();

    let payload = serde_json::json!({
        "version": 1,
        "exportedAt": exported_at,
        "editor": editor,
        "sqlSnippets": snippets,
    });
    serde_json::to_string_pretty(&payload).unwrap_or_default()
}

/// React `importEditorSettings`: parse, validate the root, then map only the
/// safe editor/snippet fields. Everything else is silently dropped.
#[allow(dead_code)] // helper; remove with the settings view.
pub fn import_editor_settings(json_str: &str) -> Result<EditorSettingsPatch, SettingsImportError> {
    let parsed: Value = serde_json::from_str(json_str)
        .map_err(|_| SettingsImportError::InvalidJson)?;
    // React's guard is `!parsed || typeof parsed !== 'object'`: arrays pass
    // (`typeof [] === 'object'`, `![] === false`) and simply have no named
    // fields. null/false/0/string still fail, as an empty object cannot stand
    // in for those.
    // React's guard is `!parsed || typeof parsed !== 'object'`: arrays pass
    // (`typeof [] === 'object'`, `![] === false`) and simply have no named
    // fields. null/false/0/string still fail, as an empty object cannot stand
    // in for those.
    let empty_root = Map::new();
    let root = match &parsed {
        Value::Object(map) => map,
        Value::Array(_) => &empty_root,
        _ => return Err(SettingsImportError::RootMustBeObject),
    };

    let mut patch = EditorSettingsPatch::default();
    if let Some(editor) = root.get("editor").and_then(Value::as_object) {
        if let Some(value) = editor.get("completionIncludeTablePrefix").and_then(Value::as_bool) {
            patch.completion_include_table_prefix = Some(value);
        }
        if let Some(value) = editor.get("fontSize").and_then(Value::as_number) {
            patch.font_size = Some(value.clone());
        }
        if let Some(value) = editor.get("fontFamily").and_then(Value::as_str) {
            patch.font_family = Some(value.to_owned());
        }
        if let Some(value) = editor
            .get("completionQuotePolicy")
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "unquoted" | "always" | "both"))
        {
            patch.completion_quote_policy = Some(value.to_owned());
        }
        if let Some(value) = editor
            .get("keymapPreset")
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "default" | "dbeaver" | "navicat"))
        {
            patch.keymap_preset = Some(value.to_owned());
        }
        if let Some(value) = editor.get("customKeymap").and_then(Value::as_object) {
            patch.custom_keymap = Some(value.clone());
        }
        if let Some(value) = editor.get("sqlFormatOptions").and_then(Value::as_object) {
            patch.sql_format_options = Some(value.clone());
        }
        if let Some(value) = editor
            .get("sqlExecutionStrategy")
            .and_then(Value::as_str)
            .filter(|value| {
                matches!(
                    *value,
                    "current_statement" | "entire_script" | "largest_statement" | "ask"
                )
            })
        {
            patch.sql_execution_strategy = Some(value.to_owned());
        }
    }

    if let Some(snippets) = root.get("sqlSnippets").and_then(Value::as_array) {
        patch.sql_snippets = Some(
            snippets
                .iter()
                .filter_map(|snippet| {
                    let row = snippet.as_object()?;
                    Some(SqlSnippet {
                        id: row.get("id")?.as_str()?.to_owned(),
                        prefix: row.get("prefix")?.as_str()?.to_owned(),
                        description_key: row
                            .get("descriptionKey")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        template: row.get("template")?.as_str()?.to_owned(),
                    })
                })
                .collect(),
        );
    }

    Ok(patch)
}
