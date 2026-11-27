//! Port of `packages/frontend/src/components/sql-editor/SqlEditor.tsx`.
//!
//! The React file is the S6-D central assembly: it mounts a CodeMirror 6 view
//! with eight compartment groups of extension factories and exposes an
//! imperative handle. Rune has no CodeMirror; the engine under
//! [`crate::sql_editor`] is its replacement ("SQL editor shaped like CodeMirror
//! 6, drawn by Rune"). This port therefore maps the assembly, not the engine:
//! the props surface, the handle, and the reconfiguration effects collapse
//! into [`SqlEditorState`] (state + handle) plus [`SqlEditorComponent`] (view)
//! plus [`on_key`]/[`on_ime`] (update listener + base keymap).
//!
//! ## React → Rune mapping
//!
//! | React | Rune |
//! | --- | --- |
//! | `EditorView` mount + compartments | [`crate::sql_editor::SqlEditor`] inside `SqlEditorState` |
//! | `useImperativeHandle` (`SqlEditorHandle`) | methods on `SqlEditorState` (1:1 with `contracts.ts`) |
//! | `[value]` effect + `BumpDocumentVersion` | [`SqlEditorState::set_value`] (bumps `document_version`) |
//! | `StartExecutionEffect` / `FinishExecutionEffect` | [`SqlEditorState::set_execution`] |
//! | `createUpdateListener` (onChange/onSelectionChange/onQualifiedPath) | update events returned by [`on_key`]/[`on_ime`] |
//! | `createBaseEditorExtensions` keymap (Mod-Enter / Mod-Shift-Enter / Mod-S) | shortcut arm of [`on_key`] |
//! | `cmPlaceholder` | `sql-editor-placeholder` line when the doc is empty |
//! | execution gutter spinner | `sql-editor-executing` badge row |
//! | callback props | [`EditorEvent`] values the host matches on |
//!
//! ## Seams for sibling tasks (not re-implemented here)
//!
//! - `contracts.ts` (T107): the props/handle VMs are inlined here, as
//!   `components/schema` did for its types.
//! - `editorExtensions.ts` / `proCompartments.ts` / fold / snippets /
//!   `formatEditorDocument` (T141/T335/T196/T347/T359/T220): CodeMirror
//!   extension factories have no engine counterpart yet. The handle keeps
//!   [`SqlEditorState::format_document`] as an injected-formatter seam and
//!   [`SqlEditorState::insert_snippet`] inserts the template without a
//!   tabstop session; those tasks will deepen both.
//! - `parseQualifiedPathParents` lives in `lib/sqlPathPrefix.ts` (another
//!   segment); the pure function is ported 1:1 in `lib_helpers` and will
//!   be absorbed when that file lands. `toggleSqlLineComments` already
//!   was: `lib/sqlEditorContextMenu.ts` ported as
//!   `sql_editor_context_menu` (T155) and took the function home.
//! - `metadataSnapshot`, `schema`, `onNavigateTo*`, `onDropTable`,
//!   `onContextMenu` props feed completion/hover/paste extensions that do not
//!   exist in the engine yet; they are deliberately absent from
//!   [`SqlEditorProps`] and documented here so the omission is a decision,
//!   not an oversight.
//!
//! ## Keys
//!
//! - `sql-editor`: root column (React `data-testid="sql-editor"` on `view.dom`)
//! - `sql-editor-executing`: running badge row (visible only while executing)
//! - `sql-editor-content`: scroll surface (React `data-testid="sql-editor-content"`)
//! - `sql-editor-doc`: unwrapped line column, `EDITOR_SURFACE_WIDTH` wide
//! - `sql-editor-line-{n}`: one row per document line
//! - `sql-editor-caret`: caret bars (the main one is the IME cursor)
//! - `sql-editor-composing`: pre-commit IME text at the main caret
//! - `sql-editor-placeholder`: placeholder line when the document is empty
//! - `sql-completion` / `sql-completion-item-{i}`: keyword completion list

use std::rc::Rc;

use rune::{col, row, text, tok, El, Ime, KeyEvent, Span, Tx};

#[allow(unused_imports)] // re-export keeps the pre-T105 paths (mod.rs, completion) stable
pub use super::contracts::{CompletionQuotePolicy, ExecutionStatus};
use super::lib_helpers::parse_qualified_path_parents;
use super::sql_editor_context_menu::toggle_sql_line_comments;
use crate::sql_editor::{self, SqlEditor};
use crate::ui::palette;
use crate::ui::spinner::{Spinner, SpinnerVariant};


// ---------------------------------------------------------------------------
// Props (mirror of `contracts.ts` `SqlEditorProps`, minus seams listed above;
// the vocabulary types moved to `contracts.rs` — their single source, T105)
// ---------------------------------------------------------------------------

/// Per-frame configuration, the port of `SqlEditorProps`. Callback props
/// become [`EditorEvent`]s; fields that only feed unwritten extension
/// factories are kept as documented wiring points.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // reusable port; wired by the workspace view wiring
pub struct SqlEditorProps {
    /// `placeholder`: shown on the first line while the document is empty.
    pub placeholder: Option<String>,
    /// `databaseType`: dialect wiring point for the completion task.
    pub database_type: Option<String>,
    /// `database`: metadata isolation wiring point for the hover task.
    pub database: Option<String>,
    /// `namespaceLoading`: true while a lazy namespace path is fetching.
    pub namespace_loading: bool,
    /// `defaultSchema`: completion preference (unqualified table completion).
    pub default_schema: Option<String>,
    /// `defaultTable`: completion preference (top-level column completion).
    pub default_table: Option<String>,
    /// `executionStatus` + `executingRange`: running badge and pinned mark.
    pub execution_status: ExecutionStatus,
    pub executing_range: Option<(usize, usize)>,
    /// `connectionId`: cross-connection drop validation wiring point.
    pub connection_id: Option<String>,
    /// `completionIncludeTablePrefix` (React default `true`).
    pub completion_include_table_prefix: bool,
    /// `completionQuotePolicy` (React default `'unquoted'`).
    pub completion_quote_policy: CompletionQuotePolicy,
}

// ---------------------------------------------------------------------------
// Update events (the React callback props)
// ---------------------------------------------------------------------------

/// What [`on_key`]/[`on_ime`] report back, mirroring the React callback props:
/// `onChange`, `onSelectionChange`, `onQualifiedPath`, `onExecute`,
/// `onExecuteAll`, `onSaveQuery`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the workspace view wiring
pub enum EditorEvent {
    /// `onChange(value)` — the document changed by an edit or IME commit.
    Changed(String),
    /// `onSelectionChange(selection)` — empty string means no selected text.
    SelectionChanged(String),
    /// `onQualifiedPath(parents)` — the qualified path parents changed.
    QualifiedPath(Vec<String>),
    /// `onExecute` (Mod-Enter).
    Execute,
    /// `onExecuteAll` (Mod-Shift-Enter).
    ExecuteAll,
    /// `onSaveQuery` (Mod-S).
    SaveQuery,
}

/// `StartExecutionEffect` payload, pinned to the document version at start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable port; wired by the workspace view wiring
pub struct ExecutionMark {
    pub target_range: Option<(usize, usize)>,
    pub document_version: u64,
}

// ---------------------------------------------------------------------------
// State + handle (the `SqlEditorHandle` surface, 1:1)
// ---------------------------------------------------------------------------

/// Stateful assembly: the engine model plus the fields the React component
/// kept in refs (`documentVersionField`, execution effects, `lastParentsRef`).
#[derive(Debug)]
#[allow(dead_code)] // reusable port; wired by the workspace view wiring
pub struct SqlEditorState {
    /// The CodeMirror-equivalent engine model.
    pub editor: SqlEditor,
    /// `documentVersionField`: bumped by external value replacement only,
    /// exactly where React dispatches `BumpDocumentVersion`.
    document_version: u64,
    /// `executionStateField`: `Some` while a statement is executing.
    execution: Option<ExecutionMark>,
    /// `lastParentsRef`: the last qualified-path parents reported.
    last_parents: Vec<String>,
}

impl Default for SqlEditorState {
    fn default() -> Self {
        Self::new(SqlEditor::from_doc(String::new()))
    }
}

#[allow(dead_code)]
impl SqlEditorState {
    pub fn new(editor: SqlEditor) -> Self {
        let mut state = Self {
            editor,
            document_version: 0,
            execution: None,
            last_parents: Vec::new(),
        };
        // React's mount effect seeds `lastParentsRef` and fires the initial
        // `onQualifiedPath`; hosts read the same value via `mount_parents`.
        state.last_parents =
            parse_qualified_path_parents(state.editor.doc(), state.editor.main_head());
        state
    }

    /// The initial `onQualifiedPath` payload the mount effect fires.
    pub fn mount_parents(&self) -> &[String] {
        &self.last_parents
    }

    // ---- handle methods (contracts.ts `SqlEditorHandle`) ----

    /// `getSelection`: the text of the selection, `""` for a plain caret.
    pub fn get_selection(&self) -> String {
        self.editor.selected_text()
    }

    /// `toggleLineComment`: toggle `-- ` on the selected lines (or the line
    /// holding the caret), then select the replaced region.
    pub fn toggle_line_comment(&mut self) {
        let sel = self.editor.selection().main();
        let (from, to) = line_bounds(self.editor.doc(), sel.from(), sel.to());
        let original = self.editor.doc()[from..to].to_string();
        let next = toggle_sql_line_comments(&original);
        if next == original {
            return;
        }
        self.editor.replace_range(from, to, &next);
        self.editor.set_main_selection(from, from + next.len());
    }

    /// `insertAt(text, pos)`: replace an empty document, or insert with the
    /// React newline padding rules and land the caret after the insertion.
    pub fn insert_at(&mut self, text: &str, pos: Option<usize>) {
        let doc = self.editor.doc();
        if doc.trim().is_empty() {
            let doc_len = doc.len();
            let caret = text.len();
            self.editor.replace_range(0, doc_len, text);
            self.editor.set_main_selection(caret, caret);
            return;
        }
        let doc_len = doc.len();
        let target = pos.map_or(doc_len, |p| p.min(doc_len));
        let before = &doc[..target];
        let after = &doc[target..];
        let prefix = if !before.is_empty() && !before.ends_with("\n\n") {
            if before.ends_with('\n') { "\n" } else { "\n\n" }
        } else {
            ""
        };
        let suffix = if !after.is_empty() && !after.starts_with("\n\n") {
            if after.starts_with('\n') { "\n" } else { "\n\n" }
        } else {
            ""
        };
        let insert = format!("{prefix}{text}{suffix}");
        let caret = target + insert.len();
        self.editor.replace_range(target, target, &insert);
        self.editor.set_main_selection(caret, caret);
    }

    /// `getDocumentVersion`.
    pub fn get_document_version(&self) -> u64 {
        self.document_version
    }

    /// `getDocument`.
    pub fn get_document(&self) -> &str {
        self.editor.doc()
    }

    /// `rawInsert(text, pos)`: insert without newline padding.
    pub fn raw_insert(&mut self, text: &str, pos: usize) {
        let doc_len = self.editor.doc().len();
        let target = pos.min(doc_len);
        let caret = target + text.len();
        self.editor.replace_range(target, target, text);
        self.editor.set_main_selection(caret, caret);
    }

    /// `focus`.
    pub fn focus(&mut self) {
        self.editor.focus();
    }

    /// `formatDocument`: format the selection, or the whole document when
    /// nothing is selected. The formatter is injected; the
    /// selection-reindent and statement-anchored cursor mapping are
    /// T220's `format_editor_document` port (this method is the
    /// infallible-formatter shorthand — a formatter that cannot reject).
    pub fn format_document(&mut self, formatter: &dyn Fn(&str) -> String) -> bool {
        self.format_document_with(None, super::format_editor_document::PartialSqlFormatOptions::default(), &|sql, _database_type, _options| {
            Ok(formatter(sql))
        })
    }

    /// The full `formatEditorDocument` seam: the dialect, the partial
    /// options overlay and a formatter that may reject (sql-formatter
    /// throws on syntax it cannot parse; the buffer stays untouched).
    pub fn format_document_with(
        &mut self,
        database_type: Option<&str>,
        options: super::format_editor_document::PartialSqlFormatOptions,
        format_sql: super::format_editor_document::FormatSql<'_>,
    ) -> bool {
        let sel = self.editor.selection().main();
        let cursor = self.editor.main_head();
        let doc = self.editor.doc().to_string();
        let Some(edit) = super::format_editor_document::format_editor_document(
            &doc,
            sel.from(),
            sel.to(),
            cursor,
            database_type,
            options,
            format_sql,
        ) else {
            return false;
        };
        self.editor.replace_range(edit.from, edit.to, &edit.insert);
        self.editor.set_main_selection(edit.anchor, edit.head);
        true
    }

    /// `insertSnippet(template)`: replace the selection with the template.
    /// Tabstop navigation (React `snippet()` sessions) is the snippets task's
    /// seam; the caret lands after the template here.
    pub fn insert_snippet(&mut self, template: &str) {
        let sel = self.editor.selection().main();
        let caret = sel.from() + template.len();
        self.editor.replace_range(sel.from(), sel.to(), template);
        self.editor.set_main_selection(caret, caret);
        self.editor.focus();
    }

    /// `getCursorOffset`.
    pub fn get_cursor_offset(&self) -> usize {
        self.editor.main_head()
    }

    // ---- effects (the React `useEffect` reconfiguration layer) ----

    /// The `[value]` effect: replace the document when it differs from the
    /// external value, bumping `document_version` (`BumpDocumentVersion`).
    pub fn set_value(&mut self, value: &str) {
        if self.editor.doc() == value {
            return;
        }
        let len = self.editor.doc().len();
        self.editor.replace_range(0, len, value);
        self.document_version += 1;
        self.editor.set_main_selection(value.len(), value.len());
        self.refresh_parents();
    }

    /// The execution-state effect: `StartExecutionEffect` pins the document
    /// version; `FinishExecutionEffect` (idle or cancelling) clears the mark.
    pub fn set_execution(&mut self, status: ExecutionStatus, range: Option<(usize, usize)>) {
        match status {
            ExecutionStatus::Running => {
                self.execution = Some(ExecutionMark {
                    target_range: range,
                    document_version: self.document_version,
                });
            }
            ExecutionStatus::Idle | ExecutionStatus::Cancelling => {
                self.execution = None;
            }
        }
    }

    /// The running mark, if any (gutter-spinner state in React).
    pub fn execution_mark(&self) -> Option<ExecutionMark> {
        self.execution
    }

    /// `lastParentsRef`.
    pub fn last_parents(&self) -> &[String] {
        &self.last_parents
    }

    /// Recompute the qualified-path parents; `Some(parents)` when they
    /// changed, mirroring the update listener's guard.
    fn refresh_parents(&mut self) -> Option<Vec<String>> {
        let parents = parse_qualified_path_parents(self.editor.doc(), self.editor.main_head());
        if parents == self.last_parents {
            return None;
        }
        self.last_parents = parents.clone();
        Some(parents)
    }
}

/// Byte bounds of the lines spanned by `from..to`, the `lineAt` pair from the
/// React `toggleLineComment` handle method.
#[allow(dead_code)]
fn line_bounds(doc: &str, from: usize, to: usize) -> (usize, usize) {
    let line_start = |pos: usize| -> usize {
        doc[..pos.min(doc.len())].rfind('\n').map_or(0, |i| i + 1)
    };
    let line_end = |pos: usize| -> usize {
        let pos = pos.min(doc.len());
        doc[pos..].find('\n').map_or(doc.len(), |i| pos + i)
    };
    let start = line_start(from);
    // React: `to > from ? to - 1 : to` so a selection ending exactly at a
    // line start does not pull the following line in.
    let end_pos = if to > from { to.saturating_sub(1) } else { to };
    (start, line_end(end_pos))
}

// ---------------------------------------------------------------------------
// Key routing: base keymap + engine + update listener
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn update_events(
    state: &mut SqlEditorState,
    doc_before: &str,
    selection_before: &str,
) -> Vec<EditorEvent> {
    let mut events = Vec::new();
    let doc_after = state.editor.doc();
    if doc_after != doc_before {
        events.push(EditorEvent::Changed(doc_after.to_string()));
    }
    let selection_after = state.editor.selected_text();
    if selection_after != selection_before {
        events.push(EditorEvent::SelectionChanged(selection_after));
    }
    if let Some(parents) = state.refresh_parents() {
        events.push(EditorEvent::QualifiedPath(parents));
    }
    events
}

/// The port of the React event plumbing: `createBaseEditorExtensions`' keymap
/// (Mod-Enter / Mod-Shift-Enter / Mod-S), the engine's editing keys, and the
/// update listener (`onChange` / `onSelectionChange` / `onQualifiedPath`).
/// Returns `(consumed, events)`.
#[allow(dead_code)] // reusable port; wired by the workspace view wiring
pub fn on_key(state: &mut SqlEditorState, ev: &KeyEvent, tx: &mut Tx) -> (bool, Vec<EditorEvent>) {
    if !state.editor.focused() || state.editor.composition().is_some() {
        return (false, Vec::new());
    }
    // Base keymap. Order matters: Mod-Shift-Enter before Mod-Enter.
    if matches!(ev.key, rune::Key::Enter) && ev.mods.cmd && ev.mods.shift {
        return (true, vec![EditorEvent::ExecuteAll]);
    }
    if matches!(ev.key, rune::Key::Enter) && ev.mods.cmd {
        return (true, vec![EditorEvent::Execute]);
    }
    if ev.is_command('s') {
        return (true, vec![EditorEvent::SaveQuery]);
    }
    let doc_before = state.editor.doc().to_string();
    let selection_before = state.editor.selected_text();
    let consumed = sql_editor::on_key(&mut state.editor, ev, tx);
    if !consumed {
        return (false, Vec::new());
    }
    (true, update_events(state, &doc_before, &selection_before))
}

/// IME routing with the same update listener: a commit changes the document.
#[allow(dead_code)] // reusable port; wired by the workspace view wiring
pub fn on_ime(state: &mut SqlEditorState, ev: &Ime) -> (bool, Vec<EditorEvent>) {
    let doc_before = state.editor.doc().to_string();
    let selection_before = state.editor.selected_text();
    let consumed = sql_editor::on_ime(&mut state.editor, ev);
    if !consumed {
        return (false, Vec::new());
    }
    (true, update_events(state, &doc_before, &selection_before))
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

#[allow(dead_code)] // reusable port; wired by the workspace view wiring
#[allow(dead_code)]
type FocusFn<S> = Rc<dyn Fn(&mut S) + 'static>;
#[allow(dead_code)] // reusable port; wired by the workspace view wiring
#[allow(dead_code)]
type CompletionFn<S> = Rc<dyn Fn(&mut S, usize) + 'static>;

/// The component view, builder style like `components/history`.
#[allow(dead_code)] // reusable port; wired by the workspace view wiring
pub struct SqlEditorComponent<'a, S: 'static> {
    state: &'a SqlEditorState,
    props: &'a SqlEditorProps,
    on_focus: Option<FocusFn<S>>,
    on_completion_accept: Option<CompletionFn<S>>,
}

#[allow(dead_code)] // reusable port; wired by the workspace view wiring
impl<'a, S: 'static> SqlEditorComponent<'a, S> {
    pub fn new(state: &'a SqlEditorState, props: &'a SqlEditorProps) -> Self {
        Self { state, props, on_focus: None, on_completion_accept: None }
    }

    /// Pressing the editing surface focuses the editor.
    pub fn on_focus(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_focus = Some(Rc::new(f));
        self
    }

    /// Clicking completion item `i` accepts it (React: the canvas list click).
    pub fn on_completion_accept(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_completion_accept = Some(Rc::new(f));
        self
    }

    pub fn build(self, dark: bool) -> El {
        let theme = tok::theme(dark);
        let ink = theme.text;
        let muted = theme.text_secondary;
        let editor = &self.state.editor;
        let spans = sql_editor::highlight_sql(editor.doc(), dark);
        let carets: Vec<(usize, bool)> = editor
            .selection()
            .ranges()
            .iter()
            .enumerate()
            .map(|(i, sel)| (sel.head, i == editor.selection().main_index()))
            .collect();

        let mut doc = col()
            .key("sql-editor-doc")
            .w(sql_editor::EDITOR_SURFACE_WIDTH)
            .min_h(120.0);
        let lines = split_lines(editor.doc());
        let has_placeholder = editor.doc().is_empty() && self.props.placeholder.is_some();
        for (n, (start, line)) in lines.iter().enumerate() {
            if has_placeholder {
                doc = doc.child(
                    text(self.props.placeholder.clone().unwrap_or_default())
                        .font(sql_editor::editor_font())
                        .color(muted)
                        .key("sql-editor-placeholder"),
                );
                continue;
            }
            doc = doc.child(line_row(n, *start, line, &spans, &carets, editor, dark));
            if !editor.completion_items().is_empty()
                && editor.main_head() >= *start
                && editor.main_head() <= start + line.len()
            {
                doc = doc.child(self.completion_list(ink, dark));
            }
        }

        let mut root = col().key("sql-editor").w_full().h_full().min_h(60.0);
        if self.props.execution_status == ExecutionStatus::Running {
            root = root.child(
                row()
                    .key("sql-editor-executing")
                    .h(20.0)
                    .gap(6.0)
                    .center_y()
                    .child(
                        Spinner::new("sql-editor-executing-spinner")
                            .variant(SpinnerVariant::Icon)
                            .build(dark),
                    ),
            );
        }
        let mut surface = col()
            .key("sql-editor-content")
            .w_full()
            .grow(1.0)
            .min_h(60.0)
            .scroll_xy()
            .radius(8.0)
            .fill(palette::editor_bg(dark));
        if let Some(focus) = &self.on_focus {
            let focus = focus.clone();
            surface = surface.on_press(move |s: &mut S| focus(s));
        }
        root.child(surface.child(doc))
    }

    fn completion_list(&self, ink: rune::Color, dark: bool) -> El {
        let bg = palette::editor_bg(dark);
        let sel_bg = palette::editor_selection_bg(dark);
        let editor = &self.state.editor;
        let mut list = col().key("sql-completion").w(170.0).radius(6.0).fill(bg);
        for (index, item) in editor.completion_items().iter().enumerate() {
            let selected = index == editor.completion_selected();
            let mut row_el = row()
                .key(format!("sql-completion-item-{index}"))
                .w_full()
                .h(18.0)
                .px(8.0)
                .center_y()
                .fill(if selected { sel_bg } else { bg })
                .child(text(item.clone()).font(sql_editor::editor_font()).color(ink));
            if let Some(accept) = &self.on_completion_accept {
                let accept = accept.clone();
                row_el = row_el.on_press(move |s: &mut S| accept(s, index));
            }
            list = list.child(row_el);
        }
        list
    }
}

#[allow(dead_code)]
fn line_row(
    index: usize,
    start: usize,
    line: &str,
    spans: &[Span],
    carets: &[(usize, bool)],
    editor: &SqlEditor,
    dark: bool,
) -> El {
    let end = start + line.len();
    let mut here: Vec<(usize, bool)> = carets
        .iter()
        .copied()
        .filter(|(at, _)| *at >= start && *at <= end)
        .collect();
    here.sort_by_key(|(at, _)| *at);
    here.dedup_by_key(|(at, _)| *at);
    let mut row_el = row().key(format!("sql-editor-line-{index}")).h(18.0);
    let mut cursor = 0usize;
    for (at, is_main) in here {
        let rel = (at - start).min(line.len());
        if rel > cursor {
            row_el = row_el.child(slice_text(line, start, cursor, rel, spans));
        }
        if is_main {
            if let Some(composed) = editor.composition() {
                row_el = row_el.child(
                    text(composed)
                        .font(sql_editor::editor_font())
                        .spans(vec![Span::new(0, composed.len())
                            .underline(palette::editor_composition_underline(dark))])
                        .key("sql-editor-composing"),
                );
            }
            if editor.focused() {
                row_el = row_el.child(caret(true, dark));
            }
        } else {
            row_el = row_el.child(caret(false, dark));
        }
        cursor = rel;
    }
    if cursor < line.len() {
        row_el = row_el.child(slice_text(line, start, cursor, line.len(), spans));
    }
    row_el
}

#[allow(dead_code)]
fn slice_text(line: &str, abs: usize, from: usize, to: usize, spans: &[Span]) -> El {
    let local: Vec<Span> = spans
        .iter()
        .filter_map(|span| clip_span(span, abs + from, abs + to))
        .collect();
    text(&line[from..to]).font(sql_editor::editor_font()).spans(local)
}

#[allow(dead_code)]
fn clip_span(span: &Span, abs_from: usize, abs_to: usize) -> Option<Span> {
    let lo = (span.start as usize).max(abs_from);
    let hi = (span.end as usize).min(abs_to);
    if lo >= hi {
        return None;
    }
    let mut out = Span::new(lo - abs_from, hi - abs_from);
    out.color = span.color;
    out.italic = span.italic;
    out.weight = span.weight;
    Some(out)
}

#[allow(dead_code)]
fn caret(main: bool, dark: bool) -> El {
    let bar = col()
        .w(2.0)
        .h(16.0)
        .key("sql-editor-caret")
        .fill(palette::editor_caret_fill(main, dark));
    if main {
        bar.ime_cursor()
    } else {
        bar
    }
}

#[allow(dead_code)]
fn split_lines(doc: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, byte) in doc.bytes().enumerate() {
        if byte == b'\n' {
            out.push((start, &doc[start..i]));
            start = i + 1;
        }
    }
    out.push((start, &doc[start..]));
    out
}
