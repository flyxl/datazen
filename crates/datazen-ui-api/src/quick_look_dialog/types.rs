use super::value::bytes_to_base64;
use serde_json::Value;

pub const BINARY_TEST_ID: &str = "detail-quick-look-binary";
pub const JSON_TEST_ID: &str = "detail-quick-look-json";
pub const DIALOG_CLASS: &str = "max-w-2xl";
pub const TEXTAREA_CLASS: &str = "max-h-[50vh] w-full resize-y rounded-md border border-edge bg-surface p-3 font-mono text-xs text-fg focus:border-accent focus:outline-none focus:ring-1 focus:ring-accent/30";
pub const PRE_CLASS: &str = "max-h-[50vh] overflow-auto whitespace-pre-wrap break-all rounded-md border border-edge bg-surface p-3 font-mono text-xs text-fg";
pub const TEXTAREA_ROWS: u32 = 12;
pub const TEXTAREA_WRAP: &str = "off";

#[derive(Clone, Debug, PartialEq)]
pub struct QuickLookInput {
    pub open: bool,
    pub column_name: String,
    pub column_type: String,
    pub value: Value,
    pub editable: bool,
    pub can_commit: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickLookBody {
    Binary {
        bytes: Vec<u8>,
        file_name: String,
        test_id: &'static str,
    },
    Editor {
        text: String,
    },
    Json {
        text: String,
        test_id: &'static str,
    },
    Pre {
        text: String,
        class: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickLookView {
    pub open: bool,
    pub title: String,
    pub description: String,
    pub class: String,
    pub body: QuickLookBody,
    pub show_cancel: bool,
    pub show_save: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuickLookModel {
    pub view: QuickLookView,
    pub initial_text: String,
    pub pretty: Option<String>,
    pub type_lower: String,
    pub can_commit: bool,
    pub blob_bytes: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommitValue {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Json(Value),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobSaveRequest {
    pub base64: String,
    pub suggested_name: String,
    pub extension: String,
    pub upper_extension: String,
    pub filters: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SaveOutcome {
    pub commit: Option<CommitValue>,
    pub close: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickLookEvent {
    Close,
    Save,
    BlobSaveAs { suggested_name: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum QuickLookEffect {
    Close,
    Commit(CommitValue),
    BlobSave(BlobSaveRequest),
}

fn js_number(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

pub fn resolve_save(model: &QuickLookModel, local: &str) -> SaveOutcome {
    if !model.can_commit {
        return SaveOutcome {
            commit: None,
            close: false,
        };
    }
    if local.is_empty() {
        return SaveOutcome {
            commit: Some(CommitValue::Null),
            close: true,
        };
    }
    if model.pretty.is_some() {
        let commit = match serde_json::from_str::<Value>(local) {
            Ok(value) => CommitValue::Json(value),
            Err(_) => CommitValue::Text(local.to_owned()),
        };
        return SaveOutcome {
            commit: Some(commit),
            close: true,
        };
    }
    let type_name = model.type_lower.as_str();
    let commit = if type_name.contains("int") || type_name.contains("serial") || type_name.contains("bigint") {
        CommitValue::Number(js_number(local))
    } else if type_name.contains("bool") {
        CommitValue::Bool(local == "true")
    } else if type_name.contains("float")
        || type_name.contains("double")
        || type_name.contains("numeric")
        || type_name.contains("decimal")
        || type_name.contains("real")
    {
        CommitValue::Number(js_number(local))
    } else {
        CommitValue::Text(local.to_owned())
    };
    SaveOutcome {
        commit: Some(commit),
        close: true,
    }
}

pub fn save_blob_request(bytes: &[u8], suggested_name: &str) -> BlobSaveRequest {
    let extension = if suggested_name.contains('.') {
        suggested_name.rsplit('.').next().unwrap_or("bin").to_owned()
    } else {
        "bin".to_owned()
    };
    BlobSaveRequest {
        base64: bytes_to_base64(bytes),
        suggested_name: suggested_name.to_owned(),
        upper_extension: extension.to_uppercase(),
        filters: vec![extension.clone()],
        extension,
    }
}

pub fn resolve_event(
    event: QuickLookEvent,
    model: &QuickLookModel,
    local: &str,
) -> Vec<QuickLookEffect> {
    match event {
        QuickLookEvent::Close => vec![QuickLookEffect::Close],
        QuickLookEvent::Save => {
            let outcome = resolve_save(model, local);
            let mut effects = Vec::new();
            if let Some(commit) = outcome.commit {
                effects.push(QuickLookEffect::Commit(commit));
            }
            if outcome.close {
                effects.push(QuickLookEffect::Close);
            }
            effects
        }
        QuickLookEvent::BlobSaveAs { suggested_name } => {
            let bytes = model.blob_bytes.clone().unwrap_or_default();
            vec![QuickLookEffect::BlobSave(save_blob_request(
                &bytes,
                &suggested_name,
            ))]
        }
    }
}
