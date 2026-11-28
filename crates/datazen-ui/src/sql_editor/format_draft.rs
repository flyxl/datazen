//! One formatter edit on the real Native editor's document/history model.
//! The caller owns session/target, focus, and the injected formatter's budget.
use std::{ops::Range, rc::Rc};

use datazen_ui_api::native_sql_format::{
    trim_like_react, NATIVE_SQL_FORMAT_MAX_INPUT_BYTES, NATIVE_SQL_FORMAT_MAX_OUTPUT_BYTES,
};

use super::{Selection, SqlEditor};
use crate::components::sql_editor::format_editor_document::{
    format_editor_document, format_editor_output, project_lf, FormatEdit, FormatSql, PartialSqlFormatOptions,
};

// A selected fragment is a different workload than its parent document.
// The model takes a full document lease and an undo snapshot, so keep an
// independent, finite editor-side bound instead of unbounded cloning.
const NATIVE_FORMAT_DRAFT_LEASE_MAX_BYTES: usize = 1024 * 1024;

pub(crate) struct FormatDraftSnapshot {
    identity: Rc<()>,
    revision: u64,
    document: String,
    selection: Selection,
}

pub(crate) enum FormatSourceError {
    BlankOrInvalid,
    Limit,
}

fn acceptable_edge(document: &str, edge: usize) -> bool {
    document.is_char_boundary(edge) && !(edge > 0
        && document.as_bytes()[edge - 1] == b'\r'
        && document.as_bytes().get(edge) == Some(&b'\n'))
}

impl SqlEditor {
    pub(crate) fn capture_native_format(&self) -> Result<(FormatDraftSnapshot, String), FormatSourceError> {
        if self.composition().is_some() {
            return Err(FormatSourceError::BlankOrInvalid);
        }
        let document = self.doc();
        if document.len() > NATIVE_FORMAT_DRAFT_LEASE_MAX_BYTES {
            return Err(FormatSourceError::Limit);
        }
        let range = self.selection.ranges().get(self.selection.main_index())
            .ok_or(FormatSourceError::BlankOrInvalid)?;
        if ![range.anchor, range.head].into_iter().all(|edge| acceptable_edge(document, edge)) {
            return Err(FormatSourceError::BlankOrInvalid);
        }
        let region: Range<usize> = if range.anchor != range.head {
            range.from()..range.to()
        } else {
            0..document.len()
        };
        if region.len() > NATIVE_SQL_FORMAT_MAX_INPUT_BYTES {
            return Err(FormatSourceError::Limit);
        }
        let projected = project_lf(&document[region]);
        let text = trim_like_react(&projected);
        if text.is_empty() {
            return Err(FormatSourceError::BlankOrInvalid);
        }
        Ok((FormatDraftSnapshot {
            identity: Rc::clone(&self.identity),
            revision: self.document_revision,
            document: document.to_owned(),
            selection: self.selection.clone(),
        }, text.to_owned()))
    }

    pub(crate) fn owns_native_format(&self, pending: &FormatDraftSnapshot) -> bool {
        Rc::ptr_eq(&self.identity, &pending.identity)
            && self.document_revision == pending.revision
            && self.composition().is_none()
            && self.selection == pending.selection
            && self.doc() == pending.document.as_str()
    }

    pub(crate) fn apply_native_format(&mut self, pending: &FormatDraftSnapshot, formatted: String) -> bool {
        if !self.owns_native_format(pending)
            || formatted.len() > NATIVE_SQL_FORMAT_MAX_OUTPUT_BYTES
            || trim_like_react(&formatted).is_empty()
        {
            return false;
        }
        let range = self.selection.main();
        let Some(edit) = format_editor_output(
            self.doc(), range.from(), range.to(), range.head,
            project_lf(&formatted).into_owned(), NATIVE_SQL_FORMAT_MAX_OUTPUT_BYTES,
        ) else {
            return false;
        };
        self.apply_format_edit(edit)
    }

    /// Plan against the exact current main range, then commit once. A bounded,
    /// synchronous provider cannot switch tabs or mutate this borrowed draft.
    pub(crate) fn format_draft_with(
        &mut self,
        database_type: Option<&str>,
        options: PartialSqlFormatOptions,
        formatter: FormatSql<'_>,
    ) -> bool {
        if self.composition().is_some() {
            return false;
        }
        let selection = self.selection();
        let Some(range) = selection.ranges().get(selection.main_index()).copied() else {
            return false;
        };
        let Some(edit) = format_editor_document(
            self.doc(), range.from(), range.to(), range.head, database_type, options, formatter,
        ) else {
            return false;
        };
        self.apply_format_edit(edit)
    }

    fn apply_format_edit(&mut self, edit: FormatEdit) -> bool {
        // Indentation restoration can make different formatter output equal
        // to the original again. Avoid changing the selection or history.
        if self.doc().get(edit.from..edit.to) == Some(edit.insert.as_str()) {
            return false;
        }

        self.push_history();
        self.replace_range(edit.from, edit.to, &edit.insert);
        self.set_main_selection(edit.anchor, edit.head);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::sql_editor::format_editor_document::{FormatRejection, IndentStyle, KeywordCase};
    use crate::sql_editor::Sel;

    #[test]
    fn native_format_capture_preserves_original_raw_line_endings_and_enforces_input_bound() {
        let mut editor = SqlEditor::from_doc("\u{feff}select 1;\r\nselect 2;\r\n");
        let source = editor.doc().to_owned();
        let second = source.rfind("2").unwrap();
        editor.set_main_caret(second);
        let (captured, request) = editor.capture_native_format().ok().unwrap();
        assert_eq!(request, "select 1;\nselect 2;");
        let formatted = "SELECT 1;\n\nSELECT 2;";
        assert!(editor.apply_native_format(&captured, formatted.to_owned()));
        assert_eq!(editor.main_head(), formatted.rfind("SELECT").unwrap());
        assert!(editor.undo());
        assert_eq!(editor.doc(), source);
        assert_eq!(editor.main_head(), second);

        let large = SqlEditor::from_doc("X".repeat(NATIVE_SQL_FORMAT_MAX_INPUT_BYTES + 1));
        assert!(matches!(large.capture_native_format(), Err(FormatSourceError::Limit)));
        let blank = SqlEditor::from_doc("\u{feff}\t");
        assert!(matches!(blank.capture_native_format(), Err(FormatSourceError::BlankOrInvalid)));
    }

    #[test]
    fn a_small_selection_gets_its_own_format_budget_and_one_undo() {
        let start = "select 1;";
        let tail = "x".repeat(NATIVE_SQL_FORMAT_MAX_INPUT_BYTES);
        let original = format!("{start}\n/* {tail} */");
        let mut editor = SqlEditor::from_doc(original.clone());

        assert!(matches!(editor.capture_native_format(), Err(FormatSourceError::Limit)));
        editor.set_main_selection(start.len(), 0);
        let (lease, input) = editor.capture_native_format().ok().expect("small, exact region");
        assert_eq!(input, start);
        assert!(editor.apply_native_format(&lease, "SELECT\n  1;".into()));
        assert_eq!(editor.doc(), format!("SELECT\n  1;{}", &original[start.len()..]));
        assert!(editor.undo());
        assert_eq!(editor.doc(), original);
        assert!(!editor.undo());
        assert!(!editor.owns_native_format(&lease), "undo never revives an earlier request");

        let mut too_large = SqlEditor::from_doc("a".repeat(NATIVE_FORMAT_DRAFT_LEASE_MAX_BYTES + 1));
        too_large.set_main_selection(0, 3);
        assert!(matches!(too_large.capture_native_format(), Err(FormatSourceError::Limit)));
    }

    #[test]
    fn selection_format_uses_draft_options_and_is_one_editor_history_transaction() {
        let original = "select 1;\n    select 'q;z', lower('M') where true and false;\nselect 3;";
        let mut editor = SqlEditor::from_doc(original);
        let from = original.find("select 'q").unwrap();
        let end = original.find("\nselect 3").unwrap();
        editor.set_main_selection(end, from);
        let original_selection = editor.selection().clone();
        let options = PartialSqlFormatOptions {
            keyword_case: Some(KeywordCase::Lower),
            indent_style: Some(IndentStyle::FourSpaces),
            break_before_boolean_operators: Some(false),
            lines_between_queries: Some(2),
        };
        assert!(editor.format_draft_with(Some("sqlite"), options, &|sql, dialect, options| {
            assert_eq!(sql, &original[from..end]);
            assert_eq!(dialect, Some("sqlite"));
            assert_eq!(options.keyword_case, KeywordCase::Lower);
            assert_eq!(options.indent_style, IndentStyle::FourSpaces);
            assert!(!options.break_before_boolean_operators);
            assert_eq!(options.lines_between_queries, 2);
            // Pinned React formatter fixture; this test supplies the oracle
            // result to the transaction, it does not install a provider.
            Ok("select\n    'q;z',\n    lower('M')\nwhere\n    true and\n    false;".into())
        }));
        let expected = "select 1;\n    select\n        'q;z',\n        lower('M')\n    where\n        true and\n        false;\nselect 3;";
        assert_eq!(editor.doc(), expected);
        assert_eq!(editor.selection().main(), Sel { anchor: from, head: expected.find("\nselect 3").unwrap() });
        let formatted_selection = editor.selection().clone();
        assert!(editor.undo());
        assert_eq!(editor.doc(), original);
        assert_eq!(editor.selection(), &original_selection);
        assert!(!editor.undo(), "only one formatting transaction");
        assert!(editor.redo());
        assert_eq!(editor.doc(), expected);
        assert_eq!(editor.selection(), &formatted_selection);
    }

    #[test]
    fn rejection_blank_noop_and_ime_do_not_edit_or_clear_existing_redo() {
        let doc = "  SELECT 1;\n  SELECT 2;";
        let mut editor = SqlEditor::from_doc(doc);
        editor.set_main_caret(doc.len());
        editor.insert("end");
        assert!(editor.undo());
        editor.set_main_selection(2, doc.len());
        let selected = editor.selection().clone();
        let options = PartialSqlFormatOptions::default();
        assert!(!editor.format_draft_with(None, options, &|_, _, _| Err(FormatRejection)));
        assert!(!editor.format_draft_with(None, options, &|sql, _, _| Ok(sql.to_owned())));
        assert!(!editor.format_draft_with(None, options, &|_, _, _| Ok("SELECT 1;\nSELECT 2;".into())));
        assert_eq!(editor.doc(), doc);
        assert_eq!(editor.selection(), &selected);
        assert!(editor.redo());

        editor.preedit("输入");
        assert!(!editor.format_draft_with(None, options, &|_, _, _| panic!("IME owns the selection")));
        let mut blank = SqlEditor::from_doc(" \n\t");
        assert!(!blank.format_draft_with(None, options, &|_, _, _| panic!("blank SQL is never sent")));
        assert!(!blank.undo());
    }
}
