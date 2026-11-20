//! Port of `packages/frontend/src/windows/workflow/WorkflowYamlEditor.tsx`
//! (T200; the page port's seam note calls it T202) — the editor chrome for
//! the workflow YAML editor, the yaml mode's real document surface.
//!
//! React mounts a CodeMirror 6 `EditorView` on a host div and drives it
//! with the app's `--cm-*` tokens; the port keeps the page's editing
//! document model — the lite editor's `AppState::workflow_yaml`
//! (`SqlEditor`), the very surface the workspace-tabs tests pin — and
//! gives it the chrome the React file adds over it. The `sql_editor`'s
//! rows render the lines (the plain panel's renderer, now fed the yaml
//! spans); this module adds the gutters React's extension list mounts.
//!
//! ## React → Rune mapping
//!
//! | React | Rune |
//! |---|---|
//! | host `div` (`data-testid="workflow-yaml-editor"`, `rounded-md border border-edge`, `h-full w-full flex-1 overflow-hidden`) | the keyed `workflow-yaml-editor` shell: `.border(1.0, border_base)`, `.radius(8.0)`, full width, growing, the scroll viewport inside |
//! | `lineNumbers()` | the line-number gutter cell per row (`yaml-line-no-{n}`, the row number, muted, the gutter's right rule) |
//! | `lintGutter()` + `yamlParamLint()` (T244) | the lint gutter cell per row (`yaml-lint-{n}`): a row whose line carries a diagnostic's from-offset gets the red marker (React's lint gutter marking the line) |
//! | `langYaml()` (T265) | [`highlight_yaml`] spans feeding the rows — the plain panel's empty span list replaced |
//! | `history()` + `defaultKeymap`/`historyKeymap` | the `SqlEditor` model's own undo (Command+Z, the plain panel's label) |
//! | `EditorView.lineWrapping` | the `sql_editor` platform ruling: Rune's multi-line surface cannot turn wrapping off, so the document keeps the unwrapped wide-surface viewport (the family seam `plain_panel` already carries) |
//! | `yamlEditorTheme` (`--cm-*` vars, 12px mono, instant re-color on theme change) | kit tokens + [`crate::ui::palette::yaml_palette`] per theme pass (the color follows `dark` with no reconfigure, the same instant re-color the CSS variables give) |
//! | `updateListener` → `onChange` | typing writes the model directly (`AppState::workflow_yaml`), the seam the page's tests pin |
//! | the `value` sync effect (dispatch a full-document change when the prop differs) | the mode switch's writer already rewrites the document (`views::workflow_yaml`, the T145 port) — each frame re-reads it, so the equivalence is the frame |
//! | `EditorState.readOnly.of(readOnly)` | nothing — the page never passes `readOnly` (React's default `false`), so every mount is the editable surface |
//! | fontSize 12px / `ui-monospace` | the family's `editor_font()` (13pt) — the editor family keeps one editor typeface; the seam rides with `plain_panel`'s |
//!
//! The React test suite is a shallow host render (`WorkflowYamlEditor.test.tsx`
//! mocks CodeMirror away and asserts the host `data-testid`); the port's
//! pins live in `tests/workflow_yaml_editor.rs`, driving the real surface.

use rune::{El, col, row, text};

use crate::sql_editor::{editor_font, line_row, split_lines};
use crate::ui::kit::kit;
use crate::AppState;

use super::yaml_lint::{param_ref_diagnostics, YamlLintDiagnostic};
use super::yaml_mode::highlight_yaml;

/// The line-number gutter width plus its separating rule — React's
/// `cm-gutters` block (the numbers column with the right border).
const YAML_LINE_NUMBER_WIDTH: f32 = 34.0;

/// The lint gutter width — React's `lintGutter()` column (the markers slot
/// before the numbers).
const YAML_LINT_WIDTH: f32 = 14.0;

/// The workflow YAML editor's chrome — the yaml mode's document surface.
///
/// Mounts the editing document (`AppState::workflow_yaml`) under the yaml
/// highlighting, the line-number gutter and the lint gutter, inside the
/// bordered rounded host React's `data-testid` marks. The press anywhere on
/// the surface focuses the editor model (the plain panel's focus affordance).
pub(crate) fn workflow_yaml_editor(state: &AppState, dark: bool) -> El {
    let k = kit(dark);
    let editor = &state.workflow_yaml;
    let doc = editor.doc();
    let diags = param_ref_diagnostics(doc);
    let spans = highlight_yaml(doc, k.c.text_base, dark);
    let carets: Vec<(usize, bool)> = editor
        .selection
        .ranges()
        .iter()
        .enumerate()
        .map(|(i, sel)| (sel.head, i == editor.selection().main_index()))
        .collect();

    let mut document = col().key("yaml-editor-field").grow(1.0).min_h(0.0);
    for (n, (start, line)) in split_lines(doc).into_iter().enumerate() {
        document = document.child(
            row()
                .key(format!("yaml-line-{n}"))
                .h(18.0)
                .child(lint_gutter_cell(n, start, line.len(), &diags, dark))
                .child(line_number_cell(n, dark))
                .child(line_row(n, start, line, &spans, &carets, editor, dark)),
        );
    }

    col()
        .key("workflow-yaml-editor")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .border(1.0, k.c.border_base)
        .radius(8.0)
        .child(
            col()
                .key("yaml-editor-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll_xy()
                .on_press(|state: &mut AppState| {
                    state.workflow_yaml.focus();
                })
                .child(document),
        )
}

/// React's `lintGutter()` cell: a line carrying a diagnostic's from-offset
/// gets the error marker (the red badge CodeMirror's lint gutter draws);
/// a clean line keeps the empty slot so the text column stays aligned.
fn lint_gutter_cell(
    index: usize,
    start: usize,
    line_len: usize,
    diags: &[YamlLintDiagnostic],
    dark: bool,
) -> El {
    let k = kit(dark);
    let line_end = start + line_len;
    let flagged = diags.iter().any(|d| d.from >= start && d.from < line_end);
    let marker = if flagged {
        text("!".to_owned())
            .font(editor_font())
            .color(k.c.danger.fg)
            .key(format!("yaml-lint-flag-{index}"))
    } else {
        text(" ".to_owned()).font(editor_font()).w(0.0).h(0.0)
    };
    row()
        .key(format!("yaml-lint-{index}"))
        .w(YAML_LINT_WIDTH)
        .h(18.0)
        .center_y()
        .child(marker)
}

/// React's `lineNumbers()` cell: the row number, muted, right-set against
/// the gutter's separating rule (`.cm-gutters` with its right border).
fn line_number_cell(index: usize, dark: bool) -> El {
    let k = kit(dark);
    row()
        .key(format!("yaml-line-no-{index}"))
        .w(YAML_LINE_NUMBER_WIDTH)
        .h(18.0)
        .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
        .center_y()
        .child(
            text(format!("{}", index + 1))
                .font(editor_font())
                .color(k.c.text_muted)
                .key(format!("yaml-line-no-text-{index}")),
        )
}
