//! One formatter edit on the real Native editor's document/history model.
//! The caller owns session/target, focus, and the injected formatter's budget.
//! No runtime is installed here and the mounted Format affordance is unchanged.
use super::SqlEditor;
use crate::components::sql_editor::format_editor_document::{
    format_editor_document, FormatSql, PartialSqlFormatOptions,
};

impl SqlEditor {
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
