//! T200's pins for the workflow YAML editor chrome
//! (`views/workflow/yaml_yaml_editor.rs` — `WorkflowYamlEditor.tsx`'s port):
//! the host surface, the line-number gutter, the lint gutter's flag and the
//! editing surface the page mounts in the yaml mode. The React suite is a
//! shallow host-render check (CodeMirror mocked away, the `data-testid`
//! asserted); these pins drive the real surface through the page's own
//! navigation — the ruling the workspace-tabs document-model pin set.

use datazen_ui::test_support::*;

#[test]
fn the_yaml_editor_host_renders_in_the_yaml_mode() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-workflows").expect("workflows navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);

    // The row's edit affordance opens the edit pane (the visual mode
    // first); the mode tab swaps to the yaml surface.
    let (x, y) = runtime.find("workflow-audit-edit").expect("the edit affordance");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    assert!(
        runtime.find("workflow-yaml-editor").is_none(),
        "the visual mode mounts no yaml surface"
    );
    let (x, y) = runtime.find("workflow-mode-yaml").expect("the yaml mode tab");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);

    // The host: React's `data-testid="workflow-yaml-editor"` (the one
    // assertion the React suite makes, with CodeMirror mocked away).
    assert!(
        runtime.find("workflow-yaml-editor").is_some(),
        "the yaml editor host is the real surface"
    );
    // The document surface is the same editing model the workspace-tabs
    // pin already holds.
    assert!(
        runtime.find("yaml-editor-field").is_some(),
        "the document surface renders"
    );
}

#[test]
fn the_line_number_gutter_counts_the_document() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-workflows").expect("workflows navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    let (x, y) = runtime.find("workflow-audit-edit").expect("the edit affordance");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    let (x, y) = runtime.find("workflow-mode-yaml").expect("the yaml mode tab");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);

    // The gutter counts the document's rows — the shared splitter's law
    // (the trailing newline's empty row counts as a row, the sql editor
    // family's own row semantics), pinned against the live document
    // instead of a hardcoded literal so the pin follows the writer.
    let rows = datazen_ui::sql_editor::split_lines(runtime.state.workflow_yaml.doc()).len();
    assert!(rows >= 3, "the lite writer's document has content rows");
    assert!(
        runtime.find(&format!("yaml-line-no-{}", rows - 1)).is_some(),
        "the last row ({}) carries its number",
        rows - 1
    );
    assert!(
        runtime.find(&format!("yaml-line-no-{rows}")).is_none(),
        "no row past the document's end"
    );
    // The rows themselves still ride the shared line renderer (the
    // plain-panel lineage the workspace-tabs pin observes).
    assert!(runtime.find("sql-line-2").is_some(), "the shared line renderer");
}

#[test]
fn the_lint_gutter_flags_an_unclosed_param_line_and_clears_when_fixed() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime.find("rail-workflows").expect("workflows navigation");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    let (x, y) = runtime.find("workflow-audit-edit").expect("the edit affordance");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);
    let (x, y) = runtime.find("workflow-mode-yaml").expect("the yaml mode tab");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0);

    // A clean document flags no line: the lint gutter's cells render (the
    // column stays aligned) but no marker text rides any of them.
    assert!(
        runtime.find("yaml-lint-flag-0").is_none(),
        "the loaded draft's yaml has no unclosed parameter reference"
    );

    // Rewrite the document with an unclosed `${param}` on the sql line —
    // the editor's own document model (the surface the tabs pin).
    runtime.state.workflow_yaml =
        datazen_ui::sql_editor::SqlEditor::from_doc("sql: SELECT ${id FROM t\n");
    runtime.settle_all();
    runtime.frame(0.0);

    // The offending line carries the red badge: row 0's gutter cell.
    assert!(
        runtime.find("yaml-lint-flag-0").is_some(),
        "the unclosed parameter reference flags its line"
    );
    assert!(
        runtime.find("yaml-lint-flag-1").is_none(),
        "a line without an opener stays clean"
    );

    // Fixing the reference clears the flag — the gutter follows the pass.
    runtime.state.workflow_yaml =
        datazen_ui::sql_editor::SqlEditor::from_doc("sql: SELECT ${id} FROM t\n");
    runtime.settle_all();
    runtime.frame(0.0);
    assert!(
        runtime.find("yaml-lint-flag-0").is_none(),
        "the closed reference clears the flag"
    );
}
