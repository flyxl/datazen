mod enum_editor;
mod json_editor;
mod long_text_editor;

#[cfg(test)]
mod tests;

pub use enum_editor::{build_enum_view, enum_key_decision, EnumEditorInput, EnumEditorMode};
pub use json_editor::{
    build_json_view, json_indent_of, reformat_json, JsonAction, JsonEditorInput, JsonEditorView,
};
pub use long_text_editor::{build_long_text_view, LongTextEditorInput};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextAreaView {
    pub rows: usize,
    pub auto_focus: bool,
    pub aria_invalid: Option<bool>,
    pub spell_check: bool,
    pub test_id: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyDecision {
    Swallow,
    Forward,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadButtonView {
    pub label: String,
    pub disabled: bool,
    pub test_id: &'static str,
}

pub fn text_area(rows: usize, auto_focus: bool, invalid: bool) -> TextAreaView {
    TextAreaView {
        rows,
        auto_focus,
        aria_invalid: if invalid { Some(true) } else { None },
        spell_check: false,
        test_id: "table-edit-textarea",
    }
}

pub const CELL_EDITORS_SURFACE: [&str; 8] = [
    "EnumCellEditor",
    "JsonCellEditor",
    "LongTextCellEditor",
    "buildEnumView",
    "reformatJson",
    "buildJsonView",
    "buildLongTextView",
    "CELL_EDITORS_GAP",
];

pub const CELL_EDITORS_GAP: [&str; 6] = [
    "Select and SelectOption from @datazen/ui",
    "input / textarea / button DOM elements and their data-testid markers",
    "React KeyboardEvent, FocusEvent and composition events",
    "useI18n().t for the editor labels",
    "cn class merging and the Tailwind class literals",
    "onDraftChange / onPick / onLoadCandidates host callbacks",
];
