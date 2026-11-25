//! T219: the SQL code block — port of
//! `packages/frontend/src/components/SqlCodeBlock.tsx` (186 lines).
//!
//! The read-only / editable CodeMirror SQL surface the app embeds in
//! dialogs and panels (the AI NL2SQL preview, the connection DDL view,
//! the Data Transfer DDL override). React mounts one CodeMirror 6
//! `EditorView` with line numbers, the SQL language extension under the
//! resolved dialect, a dark-aware theme, and — when `onChange` is set —
//! the editable mode whose document changes stream through the callback
//! (external `code` updates come back as `remote`-annotated dispatches
//! that never echo).
//!
//! The port renders on the crate's SQL editor engine
//! (`crate::sql_editor`): the doc model ([`SqlEditor`]), the
//! dark-aware highlighter, and the line rows with their carets. The
//! host owns a [`SqlCodeBlockState`], renders through
//! [`SqlCodeBlock`], and mutates through the wire: a block without
//! [`SqlCodeBlock::on_change`] is read-only (React's `readOnly =
//! !onChange`), a block with it is editable and streams.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | root | `sql-code-block` |
//! | gutter | `sql-code-block-gutters` |
//! | one gutter number | `sql-code-block-gutter-<n>` |
//! | the code column | `sql-code-block-content` |
//! | one code line | `sql-line-<n>` (the engine's line row key) |
//!
//! ## Deliberate deviations from the React source
//!
//! * **The engine highlighter is the current highlighting.** React's
//!   token colours come from `lib/themeEditorColors.ts` (the CSS-var
//!   read + the `sqlSyntaxTheme` presets); that lib is its own ledger
//!   task, so the block renders the engine's dark-aware
//!   [`highlight_sql`] and carries the preset seam —
//!   [`SqlCodeBlockState::syntax_preset`] / [`set_syntax_preset`] — for
//!   the same reconfigure effect React's second `useEffect` owns (test
//!   asserted there: the setting change re-highlights). The seam
//!   records the setting; the preset tables land with the lib task.
//! * **The dialect map is React's, not the editor contract's.**
//!   `resolveDialect` maps exactly `postgresql` / `mysql` / `mariadb`
//!   / `sqlite` and falls back to StandardSQL — `sqlserver` is NOT a
//!   key here (contrast [`resolve_cm_dialect`], whose map carries it);
//!   an unknown or `sqlserver` dialect renders with the standard
//!   grammar in React too.
//! * **The dark/class observer is host wiring.** React's
//!   MutationObserver rebuilds the state when the `dark` class flips;
//!   the port's `build(dark)` takes the theme as its input (the host
//!   re-renders), the same rule every other port documents.
//! * **The container clips instead of CodeMirror's own scroll.**
//!   React's root is `overflow-hidden` with CM's internal scroller; the
//!   port's root is a `scroll_xy()` viewport over the lines — the same
//!   surface the engine's editor panel uses — so the content scrolls
//!   inside the block.
//! * **The gutter is a sibling column**, right-aligned numbers on the
//!   React-verbatim washes (`#111827`/`#f1f5f9` bg, `#9ca3af`/`#64748b`
//!   ink, `#374151`/`#cbd5e1` right rule), 44pt wide. CodeMirror sizes
//!   it to the digit count; a fixed width is the honest rune surface.
//! * **`cm-content`'s `padding: 12px 0`** is the code column's `py(12)`;
//!   the mono 13px step is the engine's [`editor_font`] (13pt).
//! * **Selection keeps the engine's look.** `cm-selectionBackground`
//!   rides the engine's selection spans (the `editor_selection_bg`
//!   wash); `select-text` is the rune default on text.
//! * **IME rides the app router.** The engine's `on_ime` is the
//!   document's IME entry; the app-level IME router (the
//!   `AppSpec::on_ime` seam, as `app.rs` routes the workflow editor)
//!   forwards to the focused block through the host wiring — the
//!   component exposes [`SqlCodeBlockState::handle_ime`] for it.

// The production consumers are the NL2SQL panel / DDL view / Data
// Transfer wiring tasks; until then the component tests are the only
// callers.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, Align, Color, El, KeyEvent, Tx};

use crate::components::sql_editor::contracts::{resolve_cm_dialect, CmSqlDialect};
use crate::sql_editor::{
    editor_font, highlight_sql, line_row, on_key as editor_on_key, on_ime as editor_on_ime,
    split_lines, SqlEditor,
};

/// React's `resolveDialect`: the four keys the component's own map
/// carries, else StandardSQL (`dialectMap[dialect] ?? StandardSQL`).
/// Notably `sqlserver` is not a key here — the DDL view passes the
/// registry's `sqlDialect`, and anything outside the four renders with
/// the standard grammar in React too (contrast [`resolve_cm_dialect`],
/// whose map carries `sqlserver`).
pub fn resolve_dialect(dialect: &str) -> CmSqlDialect {
    match dialect {
        "postgresql" => CmSqlDialect::PostgreSql,
        "mysql" => CmSqlDialect::MySql,
        "mariadb" => CmSqlDialect::MariaSql,
        "sqlite" => CmSqlDialect::Sqlite,
        _ => CmSqlDialect::StandardSql,
    }
}

/// The gutter wash, React's `.cm-gutters` background.
fn gutter_bg(dark: bool) -> Color {
    let hex = if dark { "#111827" } else { "#f1f5f9" };
    rune::color::hex(hex).unwrap_or(Color::WHITE)
}

/// The gutter ink, React's `.cm-gutters` color.
fn gutter_fg(dark: bool) -> Color {
    let hex = if dark { "#9ca3af" } else { "#64748b" };
    rune::color::hex(hex).unwrap_or(Color::WHITE)
}

/// The gutter's right rule, React's `borderRight`.
fn gutter_rule(dark: bool) -> Color {
    let hex = if dark { "#374151" } else { "#cbd5e1" };
    rune::color::hex(hex).unwrap_or(Color::WHITE)
}

/// The state the host owns for one block: the document model, the
/// resolved dialect, and the syntax-preset seam.
#[derive(Debug)]
pub struct SqlCodeBlockState {
    editor: SqlEditor,
    dialect: CmSqlDialect,
    /// React's `sqlSyntaxTheme` setting (the reconfigure seam; the
    /// preset tables are the themeEditorColors lib task).
    syntax_preset: Option<String>,
}

impl SqlCodeBlockState {
    /// One block: the initial `code` and the dialect key.
    pub fn new(code: impl Into<String>, dialect: &str) -> Self {
        Self {
            editor: SqlEditor::from_doc(code),
            dialect: resolve_dialect(dialect),
            syntax_preset: None,
        }
    }

    /// The document (React's `view.state.doc`).
    pub fn doc(&self) -> &str {
        self.editor.doc()
    }

    /// The resolved dialect.
    pub fn dialect(&self) -> CmSqlDialect {
        self.dialect
    }

    /// The engine model, for the host's own routing.
    pub fn editor(&self) -> &SqlEditor {
        &self.editor
    }

    /// React's code-prop effect: replace the document when it differs —
    /// the `remote`-annotated dispatch, which never echoes through
    /// `onDocChange` (the caller never fires the change callback for
    /// this write). The focus survives the replace, as React's dispatch
    /// keeps the view's focus.
    pub fn set_code(&mut self, code: &str) {
        if self.editor.doc() != code {
            let focused = self.editor.focused();
            self.editor = SqlEditor::from_doc(code);
            if focused {
                self.editor.focus();
            }
        }
    }

    /// The `sqlSyntaxTheme` seam: React's second effect reconfigures the
    /// theme compartment when the setting changes; the port records the
    /// preset here (the highlighter swap itself lands with the
    /// themeEditorColors lib task).
    pub fn set_syntax_preset(&mut self, preset: Option<&str>) {
        self.syntax_preset = preset.map(str::to_owned);
    }

    /// The recorded preset (the seam's read end).
    pub fn syntax_preset(&self) -> Option<&str> {
        self.syntax_preset.as_deref()
    }

    /// Whether the surface shows the caret (the engine's focus flag).
    pub fn focused(&self) -> bool {
        self.editor.focused()
    }

    /// Focus the surface (React's editable view takes focus on press;
    /// the read-only view never does).
    pub fn focus(&mut self) {
        self.editor.focus();
    }

    /// The editable mode's key path (React's `updateListener`: the engine
    /// consumes the key; the changed document — a local edit, never a
    /// remote one — comes back so the host fires its `onChange`). A
    /// consumed caret move answers `(true, None)`.
    pub fn handle_key(&mut self, event: &KeyEvent, tx: &mut Tx) -> (bool, Option<String>) {
        let before = self.editor.doc().to_owned();
        let consumed = editor_on_key(&mut self.editor, event, tx);
        let after = self.editor.doc();
        let changed = (after != before).then(|| after.to_owned());
        (consumed, changed)
    }

    /// The IME path (the app-level router forwards; see the deviations).
    pub fn handle_ime(&mut self, event: &rune::Ime) -> (bool, Option<String>) {
        let before = self.editor.doc().to_owned();
        let consumed = editor_on_ime(&mut self.editor, event);
        let after = self.editor.doc();
        let changed = (after != before).then(|| after.to_owned());
        (consumed, changed)
    }
}

/// The SQL code block. Without [`SqlCodeBlock::on_change`] the block is
/// read-only (selectable text, no caret, no key handling); with it the
/// block is editable and streams document changes through the callback.
pub struct SqlCodeBlock<'a, S: 'static> {
    state: &'a SqlCodeBlockState,
    state_access: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut SqlCodeBlockState>>,
    on_change: Option<Rc<dyn Fn(&mut S, String) + 'static>>,
}

impl<'a, S: 'static> SqlCodeBlock<'a, S> {
    pub fn new(state: &'a SqlCodeBlockState) -> Self {
        Self { state, state_access: None, on_change: None }
    }

    /// React's `onChange` prop: presence makes the block editable.
    pub fn on_change(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// The wire that binds the editable surface to the host's state
    /// (the focus press and the key routing mutate through it).
    pub fn state_access(
        mut self,
        access: impl for<'b> Fn(&'b mut S) -> &'b mut SqlCodeBlockState + Clone + 'static,
    ) -> Self {
        self.state_access = Some(Rc::new(access));
        self
    }

    /// The block, mounted: the gutter beside the code column. Only an
    /// editable block (an `on_change` + the state wire) takes focus and
    /// keys.
    pub fn build(self, dark: bool) -> El {
        let editable = self.on_change.is_some() && self.state_access.is_some();
        let editor = &self.state.editor;
        let spans = highlight_sql(editor.doc(), dark);
        let carets: Vec<(usize, bool)> = editor
            .selection()
            .ranges()
            .iter()
            .enumerate()
            .map(|(i, sel)| (sel.head, i == editor.selection().main_index()))
            .collect();

        // The gutter column: one right-aligned number per line, on the
        // React washes, ruled on its right edge.
        let lines = split_lines(editor.doc());
        let mut gutters = col()
            .key("sql-code-block-gutters")
            .w(44.0)
            .fill(gutter_bg(dark))
            .border_sides([0.0, 1.0, 0.0, 0.0], gutter_rule(dark))
            .py(12.0);
        for (index, _) in lines.iter().enumerate() {
            gutters = gutters.child(
                text((index + 1).to_string())
                    .key(format!("sql-code-block-gutter-{index}"))
                    .font(editor_font())
                    .color(gutter_fg(dark))
                    .align(Align::End),
            );
        }

        // The code column: React's `cm-content` (`padding: 12px 0`), the
        // engine's line rows (the highlighted spans, the carets).
        let mut content = col()
            .key("sql-code-block-content")
            .grow(1.0)
            .min_w(0.0)
            .py(12.0);
        for (index, (start, line)) in lines.iter().enumerate() {
            content = content.child(line_row(index, *start, line, &spans, &carets, editor, dark));
        }

        let mut root = row()
            .key("sql-code-block")
            .w_full()
            .h_full()
            .scroll_xy()
            .child(gutters)
            .child(content);

        if editable {
            if let Some(access) = self.state_access.clone() {
                let on_change = self.on_change.clone();
                let focus_access = Rc::clone(&access);
                let key_access = Rc::clone(&access);
                root = root
                    .focusable(true)
                    .on_press(move |s: &mut S| focus_access(s).focus())
                    .on_key(move |s: &mut S, event: &KeyEvent, tx: &mut Tx| {
                        let block = key_access(s);
                        let (consumed, changed) = block.handle_key(event, tx);
                        if let Some(doc) = changed {
                            if let Some(on_change) = &on_change {
                                on_change(s, doc);
                            }
                        }
                        consumed
                    });
            }
        }
        root
    }
}
