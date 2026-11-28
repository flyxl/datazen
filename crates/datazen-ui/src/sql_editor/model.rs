use datazen_sql::sql_split::{is_comment_only_or_empty, split_sql_statements};
use rune::{col, row, text, text_input, El, Ime, Key, KeyEvent, Span, TextEdit, TextStyle, Tx};

use super::round_pairs::{RoundCloser, RoundClosers};

pub(super) const KEYWORDS: &[&str] = &[
    "ALL",
    "ALTER",
    "AND",
    "AS",
    "AVG",
    "BY",
    "CASE",
    "COUNT",
    "CREATE",
    "DELETE",
    "DISTINCT",
    "DROP",
    "ELSE",
    "END",
    "FOREIGN",
    "FROM",
    "GROUP",
    "IN",
    "INDEX",
    "INNER",
    "INSERT",
    "INTO",
    "IS",
    "JOIN",
    "KEY",
    "LEFT",
    "LIMIT",
    "MAX",
    "MIN",
    "NOT",
    "NULL",
    "OFFSET",
    "ON",
    "OR",
    "ORDER",
    "PRIMARY",
    "REFERENCES",
    "RIGHT",
    "SELECT",
    "SET",
    "SUM",
    "TABLE",
    "THEN",
    "UPDATE",
    "VALUES",
    "WHEN",
    "WHERE",
    "WITH",
];

/// One selection range. `head` is the caret. `anchor` is the other end. Equal means a caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sel {
    pub anchor: usize,
    pub head: usize,
}

impl Sel {
    pub fn caret(at: usize) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }

    pub fn from(&self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn to(&self) -> usize {
        self.anchor.max(self.head)
    }
}

/// CodeMirror's `EditorSelection`: ranges plus which one is main.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub ranges: Vec<Sel>,
    pub main: usize,
}

impl Selection {
    pub fn single(at: usize) -> Self {
        Self {
            ranges: vec![Sel::caret(at)],
            main: 0,
        }
    }

    pub fn main(&self) -> Sel {
        self.ranges[self.main]
    }

    pub fn main_index(&self) -> usize {
        self.main
    }

    pub fn ranges(&self) -> &[Sel] {
        &self.ranges
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }
}

/// One undo step: the document and selection before a change.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub doc: String,
    pub selection: Selection,
    round_closers: Vec<RoundCloser>,
}

/// How many insert and delete steps the editor remembers.
pub(super) const HISTORY_LIMIT: usize = 100;

/// How many keyword completions the canvas list shows at most.
pub(super) const COMPLETION_LIMIT: usize = 8;

/// Catalog candidates are tied to one query tab and its exact live database.
pub(crate) type CompletionScope = (u32, datazen_ui_api::NativeQueryDatabase);

/// The in-canvas completion: the exact original range and its candidates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    start: usize,
    prefix: String,
    items: Vec<String>,
    selected: usize,
    scope: Option<CompletionScope>,
}

#[derive(Debug)]
pub struct SqlEditor {
    pub doc: String,
    pub selection: Selection,
    /// Marked text at the main caret. Not part of `doc` until commit.
    pub composition: Option<String>,
    pub focused: bool,
    pub(super) preferred_columns: Vec<usize>,
    pub(super) round_closers: RoundClosers,
    /// Undo and redo cover character insertion and deletion only.
    pub undo_stack: Vec<Snapshot>,
    pub redo_stack: Vec<Snapshot>,
    /// The Query-only find dock opens on demand; each draft owns its needle.
    pub(super) find_open: bool,
    pub find_input: TextEdit,
    pub replace_input: TextEdit,
    /// The open completion list, if any.
    pub completion: Option<Completion>,
}

impl Default for SqlEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl SqlEditor {
    pub fn new() -> Self {
        let long = "x".repeat(180);
        let sample = format!(
            "-- customer lookup\nSELECT id, name FROM customers WHERE status = 'active' AND note = '{long}';\n"
        );
        Self::from_doc(sample)
    }

    pub fn from_doc(doc: impl Into<String>) -> Self {
        Self {
            doc: doc.into(),
            selection: Selection::single(0),
            composition: None,
            focused: false,
            preferred_columns: Vec::new(),
            round_closers: RoundClosers::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            find_open: false,
            find_input: TextEdit::new(String::new()),
            replace_input: TextEdit::new(String::new()),
            completion: None,
        }
    }

    pub fn doc(&self) -> &str {
        &self.doc
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// T1: the text of every non-empty range, joined by newlines the way a
    /// multi-caret copy works; empty when every range is a caret.
    pub fn selected_text(&self) -> String {
        self.selection
            .ranges()
            .iter()
            .filter(|range| range.from() != range.to())
            .map(|range| self.doc[range.from()..range.to()].to_owned())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// T1: delete every non-empty range in one history step. All-caret
    /// selections are a no-op, so a plain Command-X never bites.
    pub fn cut_selection(&mut self) {
        if self.composition.is_some()
            || self
                .selection
                .ranges
                .iter()
                .all(|range| range.from() == range.to())
        {
            return;
        }
        self.push_history();
        self.change_by_range(|sel| (sel.from(), sel.to(), String::new()));
    }

    pub fn composition(&self) -> Option<&str> {
        self.composition.as_deref()
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn focus(&mut self) {
        self.focused = true;
    }

    pub fn cursor_count(&self) -> usize {
        self.selection.len()
    }

    pub fn main_head(&self) -> usize {
        self.selection.main().head
    }

    /// Extra carets, not the main one. Byte offsets into `doc`.
    pub fn extra_carets(&self) -> Vec<usize> {
        self.selection
            .ranges
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != self.selection.main)
            .map(|(_, sel)| sel.head)
            .collect()
    }

    /// The completion candidates, newest refresh; empty when closed.
    pub fn completion_items(&self) -> &[String] {
        self.completion
            .as_ref()
            .map(|c| c.items.as_slice())
            .unwrap_or(&[])
    }

    /// Which completion item is highlighted.
    pub fn completion_selected(&self) -> usize {
        self.completion.as_ref().map(|c| c.selected).unwrap_or(0)
    }

    pub(crate) fn completion_scope(&self) -> Option<&CompletionScope> {
        self.completion.as_ref()?.scope.as_ref()
    }

    pub(crate) fn clear_catalog_completion(&mut self) {
        if self.completion_scope().is_some() {
            self.completion_close();
        }
    }

    pub fn has_completion(&self) -> bool {
        self.completion.is_some()
    }
    pub fn completion_down(&mut self) {
        if let Some(completion) = &mut self.completion {
            completion.selected = (completion.selected + 1) % completion.items.len();
        }
    }

    pub fn completion_up(&mut self) {
        if let Some(completion) = &mut self.completion {
            completion.selected =
                (completion.selected + completion.items.len() - 1) % completion.items.len();
        }
    }

    pub fn completion_close(&mut self) {
        self.completion = None;
    }

    /// Insert the highlighted completion over its prefix.
    pub fn completion_accept(&mut self) {
        self.try_accept_completion();
    }

    /// Whether a still-applicable completion handled the key.
    pub(super) fn try_accept_completion(&mut self) -> bool {
        self.try_accept_completion_index(self.completion_selected())
    }

    /// Insert completion item `index` over the prefix (the canvas list's click).
    pub fn completion_accept_index(&mut self, index: usize) {
        self.try_accept_completion_index(index);
    }

    fn try_accept_completion_index(&mut self, index: usize) -> bool {
        let Some(completion) = self.completion.take() else {
            return false;
        };
        let Some(word) = completion.items.get(index) else {
            return false;
        };
        let caret = self.selection.main();
        if caret.anchor != caret.head
            || (completion.scope.is_some() && self.selection.len() != 1)
            || self.composition.is_some()
            || self.doc.get(completion.start..caret.head) != Some(completion.prefix.as_str())
        {
            return false;
        }
        self.push_history();
        self.apply_change(completion.start, caret.head, word);
        true
    }

    /// Rebuild the completion from the word at the caret; close it when nothing matches.
    pub fn refresh_completion(&mut self) {
        let head = self.selection.main().head;
        let start = word_start(&self.doc, head);
        let prefix = &self.doc[start..head];
        if prefix.is_empty() || !prefix.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            self.completion = None;
            return;
        }
        let items: Vec<String> = KEYWORDS
            .iter()
            .filter(|keyword| starts_with_ignore_ascii_case(keyword, prefix))
            .take(COMPLETION_LIMIT)
            .map(|keyword| (*keyword).to_owned())
            .collect();
        self.completion = if items.is_empty() {
            None
        } else {
            Some(Completion {
                start,
                prefix: prefix.to_owned(),
                items,
                selected: 0,
                scope: None,
            })
        };
    }

    /// Replace the ordinary keyword popup only if this exact native catalog can
    /// actually offer a relation in a supported FROM position.
    pub(crate) fn refresh_from_catalog(
        &mut self,
        scope: CompletionScope,
        catalog: &datazen_ui_api::Catalog,
    ) {
        let caret = self.selection.main();
        if self.selection.len() != 1 || caret.anchor != caret.head || self.composition.is_some() {
            return;
        }
        let Some((start, prefix)) = super::from_completion::from_prefix(&self.doc, caret.head) else {
            return;
        };
        let items = super::from_completion::relations(catalog, &scope.1.database, prefix);
        if !items.is_empty() {
            self.completion = Some(Completion {
                start,
                prefix: prefix.to_owned(),
                items,
                selected: 0,
                scope: Some(scope),
            });
        }
    }

    /// Put one caret at `byte`, replacing every range (a single selection).
    pub fn set_main_caret(&mut self, byte: usize) {
        self.preferred_columns.clear();
        self.completion_close();
        let at = boundary(&self.doc, byte);
        self.selection = Selection::single(at);
        self.keep_round_closers_on_main_line();
    }

    /// Put one range selection (anchor, head) as the only range. Used by the
    /// SqlEditor component port (`toggleLineComment`/`insertAt` land a
    /// selection, CodeMirror's `selection: {anchor, head}` dispatch).
    pub fn set_main_selection(&mut self, anchor: usize, head: usize) {
        self.preferred_columns.clear();
        self.completion_close();
        let anchor = boundary(&self.doc, anchor);
        let head = boundary(&self.doc, head);
        self.selection = Selection {
            ranges: vec![Sel { anchor, head }],
            main: 0,
        };
        self.keep_round_closers_on_main_line();
    }

    /// Replace `from..to` with `text` (clamped to char boundaries, remapping
    /// the live ranges). The component port's `applyChange` entry; history
    /// bookkeeping stays with the caller, as with CodeMirror transactions.
    pub fn replace_range(&mut self, from: usize, to: usize, text: &str) {
        self.apply_change(from, to, text);
    }

    /// CodeMirror `addCursor`: a new caret, and it becomes the main range.
    pub fn add_cursor(&mut self, byte: usize) {
        self.preferred_columns.clear();
        self.completion_close();
        let at = boundary(&self.doc, byte);
        if let Some(i) = self
            .selection
            .ranges
            .iter()
            .position(|r| r.anchor == at && r.head == at)
        {
            self.selection.main = i;
            self.keep_round_closers_on_main_line();
            return;
        }
        self.selection.ranges.push(Sel::caret(at));
        self.selection.main = self.selection.ranges.len() - 1;
        self.keep_round_closers_on_main_line();
    }

    pub fn add_cursor_at_end(&mut self) {
        self.add_cursor(self.doc.len());
    }

    /// Same insertion at every range (`changeByRange`). A composition owns the caret, so
    /// typing does not also go into the document.
    pub fn insert(&mut self, text: &str) {
        if self.composition.is_some() || text.is_empty() {
            return;
        }
        self.push_history();
        self.change_by_range(|sel| (sel.from(), sel.to(), text.to_owned()));
    }

    pub fn backspace(&mut self) {
        self.delete_grapheme(false);
    }

    pub fn delete_forward(&mut self) {
        self.delete_grapheme(true);
    }

    /// Undo the last insert or delete; false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        self.preferred_columns.clear();
        self.completion_close();
        match self.undo_stack.pop() {
            Some(snapshot) => {
                self.redo_stack.push(Snapshot {
                    doc: std::mem::take(&mut self.doc),
                    selection: self.selection.clone(),
                    round_closers: self.round_closers.snapshot(),
                });
                self.restore_snapshot(snapshot);
                true
            }
            None => false,
        }
    }

    /// Redo the last undone insert or delete; false when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        self.preferred_columns.clear();
        self.completion_close();
        match self.redo_stack.pop() {
            Some(snapshot) => {
                self.undo_stack.push(Snapshot {
                    doc: std::mem::take(&mut self.doc),
                    selection: self.selection.clone(),
                    round_closers: self.round_closers.snapshot(),
                });
                self.restore_snapshot(snapshot);
                true
            }
            None => false,
        }
    }

    fn restore_snapshot(&mut self, snapshot: Snapshot) {
        self.doc = snapshot.doc;
        self.selection = snapshot.selection;
        self.round_closers.restore_survivors(snapshot.round_closers);
        self.keep_round_closers_on_main_line();
    }

    /// Split the document into statements at semicolons: each non-empty segment's
    /// byte range, without its terminating semicolon. Empty segments are skipped.
    pub fn statements(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = 0;
        for (i, byte) in self.doc.bytes().enumerate() {
            if byte == b';' {
                if !self.doc[start..i].trim().is_empty() {
                    out.push((start, i));
                }
                start = i + 1;
            }
        }
        if !self.doc[start..].trim().is_empty() {
            out.push((start, self.doc.len()));
        }
        out
    }

    /// The statement the main caret is in: the first statement ending at or after
    /// the caret. A caret right after a semicolon picks the next statement; a
    /// caret past the last statement picks the last one. None when the document
    /// holds no statement.
    pub fn statement_at_cursor(&self) -> Option<String> {
        let head = self.selection.main().head;
        let statements = self.statements();
        statements
            .iter()
            .find(|(_, end)| head <= *end)
            .or_else(|| statements.last())
            .map(|(start, end)| self.doc[*start..*end].trim().to_owned())
    }

    /// Explicit text selection wins over the caret. The live host may supply
    /// dialect-exact byte ranges; the generic fallback is only used when it
    /// has not implemented an editor parser for the active connection.
    pub(crate) fn query_execution_text(
        &self,
        resolve: impl FnOnce(&str) -> Option<Result<Vec<std::ops::Range<usize>>, String>>,
    ) -> Result<Option<String>, String> {
        let mut selected = self
            .selection
            .ranges()
            .iter()
            .filter(|range| range.from() < range.to());
        if let Some(range) = selected.next() {
            if selected.next().is_some() {
                return Err("Select one SQL statement before running.".into());
            }
            let sql = self.doc[range.from()..range.to()].trim();
            if let Some(ranges) = resolve(sql) {
                let ranges = ranges?;
                return match ranges.as_slice() {
                    [statement] => Ok(Some(sql[statement.clone()].trim().to_owned())),
                    _ => Err("Select one SQL statement before running.".into()),
                };
            }
            let count = split_sql_statements(sql)
                .iter()
                .filter(|statement| !is_comment_only_or_empty(statement))
                .count();
            return if count == 1 {
                Ok(Some(sql.to_owned()))
            } else {
                Err("Select one SQL statement before running.".into())
            };
        }

        let Some(ranges) = resolve(&self.doc) else {
            return Ok(self.statement_at_cursor());
        };
        let ranges = ranges?;
        let head = self.selection.main().head;
        let statement = ranges.iter().find(|range| head <= range.end).or_else(|| {
            // The editor caret normally sits immediately after a trailing
            // semicolon typed by the user. Do not jump back across a later
            // comment or other non-whitespace text.
            ranges.last().filter(|range| {
                self.doc[range.end..]
                    .strip_prefix(';')
                    .is_some_and(|rest| rest.trim().is_empty())
            })
        });
        Ok(statement.map(|range| self.doc[range.clone()].trim().to_owned()))
    }

    /// Find the next occurrence of `needle` from the main caret, wrapping around,
    /// and select it. False when the needle is missing or empty.
    pub fn find_next(&mut self, needle: &str) -> bool {
        if needle.is_empty() {
            return false;
        }
        let main = self.selection.main();
        let start =
            if main.from() != main.to() && self.doc.get(main.from()..main.to()) == Some(needle) {
                main.to()
            } else {
                main.head
            };
        let found = self.doc[start..]
            .find(needle)
            .map(|i| start + i)
            .or_else(|| self.doc[..start].find(needle));
        match found {
            Some(at) => {
                self.preferred_columns.clear();
                self.selection = Selection {
                    ranges: vec![Sel {
                        anchor: at,
                        head: at + needle.len(),
                    }],
                    main: 0,
                };
                self.keep_round_closers_on_main_line();
                true
            }
            None => false,
        }
    }

    fn find_previous_from(&mut self, needle: &str) -> bool {
        if needle.is_empty() {
            return false;
        }
        let main = self.selection.main();
        let from = if self.doc.get(main.from()..main.to()) == Some(needle) {
            main.from()
        } else {
            main.head
        };
        let found = self.doc[..from].rfind(needle)
            .or_else(|| self.doc[from..].rfind(needle).map(|at| at + from));
        let Some(at) = found else { return false; };
        self.preferred_columns.clear();
        self.selection = Selection {
            ranges: vec![Sel { anchor: at, head: at + needle.len() }],
            main: 0,
        };
        self.keep_round_closers_on_main_line();
        true
    }

    pub(super) fn find_previous_from_bar(&mut self) -> bool {
        let needle = self.find_input.text.clone();
        self.find_previous_from(&needle)
    }

    pub(crate) fn dismiss_find(&mut self) {
        self.find_open = false;
        self.find_input.focused = false;
        self.replace_input.focused = false;
    }

    /// Replace the main selection when it matches `needle`, then select the next
    /// occurrence. False when the selection is not a match.
    pub fn replace_current(&mut self, needle: &str, replacement: &str) -> bool {
        let main = self.selection.main();
        if needle.is_empty() || main.from() == main.to() {
            return false;
        }
        if self.doc.get(main.from()..main.to()) != Some(needle) {
            return false;
        }
        self.push_history();
        self.apply_change(main.from(), main.to(), replacement);
        self.find_next(needle);
        true
    }

    /// Replace every occurrence of `needle`; returns how many landed.
    pub fn replace_all(&mut self, needle: &str, replacement: &str) -> usize {
        if needle.is_empty() {
            return 0;
        }
        let mut positions = Vec::new();
        let mut at = 0;
        while let Some(i) = self.doc[at..].find(needle) {
            positions.push(at + i);
            at = at + i + needle.len();
        }
        if positions.is_empty() {
            return 0;
        }
        self.push_history();
        for from in positions.iter().rev() {
            self.apply_change(*from, from + needle.len(), replacement);
        }
        positions.len()
    }

    /// The find bar's Find: read the bar's needle.
    pub fn find_from_bar(&mut self) -> bool {
        let needle = self.find_input.text.clone();
        self.find_next(&needle)
    }

    /// The find bar's Replace: select the next occurrence or edit the one
    /// already selected. True means the dock should reveal the selection.
    pub fn replace_current_from_bar(&mut self) -> bool {
        let needle = self.find_input.text.clone();
        let replacement = self.replace_input.text.clone();
        self.replace_next_literal(&needle, &replacement)
    }

    /// The find bar's Replace all.
    pub fn replace_all_from_bar(&mut self) -> usize {
        let needle = self.find_input.text.clone();
        let replacement = self.replace_input.text.clone();
        self.replace_all(&needle, &replacement)
    }

    pub fn push_history(&mut self) {
        if self.undo_stack.len() >= HISTORY_LIMIT {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(Snapshot {
            doc: self.doc.clone(),
            selection: self.selection.clone(),
            round_closers: self.round_closers.snapshot(),
        });
        self.redo_stack.clear();
    }

    /// `Ime::Preedit`. Empty ends the composition and leaves the document alone.
    pub fn preedit(&mut self, text: &str) {
        self.completion_close();
        self.composition = if text.is_empty() {
            None
        } else {
            Some(text.to_owned())
        };
    }

    /// `Ime::Commit`: the composed string replaces the main range only. Other ranges are mapped
    /// through that change, which is what CodeMirror does when the input method updates the DOM.
    pub fn commit(&mut self, text: &str) {
        self.composition = None;
        if text.is_empty() {
            return;
        }
        let main = self.selection.main();
        let (from, to) = super::navigation::edit_range(&self.doc, main);
        if self.doc.get(from..to) != Some(text) {
            self.push_history();
            self.apply_change(from, to, text);
        }
    }

    pub fn on_ime(&mut self, ev: &Ime) -> bool {
        if !self.focused {
            return false;
        }
        match ev {
            Ime::Preedit(text, _) => self.preedit(text),
            Ime::Commit(text) => self.commit(text),
            Ime::Disabled => self.preedit(""),
            Ime::Enabled => {}
        }
        true
    }

    pub fn change_by_range(&mut self, mut change: impl FnMut(Sel) -> (usize, usize, String)) {
        self.preferred_columns.clear();
        self.completion_close();
        let snapshot = self.selection.ranges.clone();
        let main_i = self.selection.main;
        let mut specs: Vec<(usize, usize, String, bool)> = snapshot
            .iter()
            .enumerate()
            .map(|(i, sel)| {
                let (from, to, text) = change(*sel);
                (from, to, text, i == main_i)
            })
            .collect();
        specs.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        let mut built: Vec<(bool, Sel)> = Vec::new();
        for (from, to, text, is_main) in specs {
            let from = boundary(&self.doc, from);
            let to = boundary(&self.doc, to.max(from));
            let end = from + text.len();
            self.splice_doc(from, to, &text);
            for (placed_main, sel) in &mut built {
                let _ = placed_main;
                let shift = text.len() as isize - (to - from) as isize;
                if sel.head >= to {
                    sel.head = (sel.head as isize + shift) as usize;
                    sel.anchor = (sel.anchor as isize + shift) as usize;
                }
            }
            built.push((is_main, Sel::caret(end)));
        }
        let main = built.iter().position(|(is_main, _)| *is_main).unwrap_or(0);
        self.selection.ranges = built.into_iter().map(|(_, sel)| sel).collect();
        self.selection.main = main;
        self.keep_round_closers_on_main_line();
    }

    pub(super) fn splice_doc(&mut self, from: usize, to: usize, text: &str) {
        self.round_closers.map_edit(from, to, text.len());
        self.doc.replace_range(from..to, text);
    }

    pub fn apply_change(&mut self, from: usize, to: usize, text: &str) {
        self.preferred_columns.clear();
        self.completion_close();
        let from = boundary(&self.doc, from);
        let to = boundary(&self.doc, to.max(from));
        let new_len = text.len();
        self.splice_doc(from, to, text);
        let map = |pos: usize| -> usize {
            if pos < from {
                pos
            } else if pos > to {
                pos - (to - from) + new_len
            } else {
                from + new_len
            }
        };
        for sel in &mut self.selection.ranges {
            sel.anchor = map(sel.anchor);
            sel.head = map(sel.head);
        }
    }
}

pub(super) fn is_keyword(word: &str) -> bool {
    KEYWORDS
        .binary_search_by(|k| cmp_ignore_ascii_case(k, word))
        .is_ok()
}

pub(super) fn cmp_ignore_ascii_case(a: &str, b: &str) -> std::cmp::Ordering {
    a.as_bytes()
        .iter()
        .map(|c| c.to_ascii_uppercase())
        .cmp(b.as_bytes().iter().map(|c| c.to_ascii_uppercase()))
}

/// The byte offset where the word (identifier characters) before `head` starts.
pub(super) fn word_start(doc: &str, head: usize) -> usize {
    let bytes = doc.as_bytes();
    let mut i = head.min(doc.len());
    while i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_') {
        i -= 1;
    }
    i
}

pub(super) fn starts_with_ignore_ascii_case(haystack: &str, prefix: &str) -> bool {
    haystack.len() >= prefix.len()
        && haystack.as_bytes()[..prefix.len()]
            .iter()
            .zip(prefix.as_bytes())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

pub(super) fn boundary(text: &str, byte: usize) -> usize {
    text.floor_char_boundary(byte.min(text.len()))
}
