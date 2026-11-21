use serde::Serialize;
use serde_json::Value;

use super::{text_area, TextAreaView};

#[derive(Clone, Debug, PartialEq)]
pub struct JsonEditorInput {
    pub auto_focus: bool,
    pub invalid: bool,
    pub format_label: String,
    pub minify_label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonAction {
    Format,
    Minify,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JsonEditorView {
    pub textarea: TextAreaView,
    pub actions: Vec<(JsonAction, String, &'static str)>,
}

pub fn build_json_view(input: &JsonEditorInput) -> JsonEditorView {
    JsonEditorView {
        textarea: text_area(3, input.auto_focus, input.invalid),
        actions: vec![
            (
                JsonAction::Format,
                input.format_label.clone(),
                "cell-editor-format-json",
            ),
            (
                JsonAction::Minify,
                input.minify_label.clone(),
                "cell-editor-minify-json",
            ),
        ],
    }
}

pub fn json_indent_of(action: JsonAction) -> usize {
    match action {
        JsonAction::Format => 2,
        JsonAction::Minify => 0,
    }
}

pub fn reformat_json(draft: &str, indent: usize) -> Option<String> {
    let value: Value = serde_json::from_str(draft).ok()?;
    let mut buffer = Vec::new();
    if indent == 0 {
        serde_json::to_writer(&mut buffer, &value).ok()?;
    } else {
        let spaces = vec![b' '; indent.min(10)];
        let formatter = serde_json::ser::PrettyFormatter::with_indent(&spaces);
        let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);
        value.serialize(&mut serializer).ok()?;
    }
    String::from_utf8(buffer).ok()
}
