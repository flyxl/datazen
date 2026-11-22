use super::*;
use crate::CellRange;
use std::cell::RefCell;

fn columns() -> Vec<String> {
    vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]
}

fn cell_selection() -> GridSelection {
    GridSelection {
        anchor: Some(CellCoord::new(3, "b")),
        focus: Some(CellCoord::new(1, "a")),
        extra_ranges: Vec::new(),
        mode: GridSelectionMode::Cell,
    }
}

fn plain_event() -> ClipboardEvent {
    ClipboardEvent {
        target_chain: vec![DomTarget::Plain],
    }
}

#[test]
fn editable_targets_cover_inputs_editors_and_ancestors() {
    for target in [
        DomTarget::Input,
        DomTarget::Textarea,
        DomTarget::Select,
        DomTarget::ContentEditableTrue,
        DomTarget::EditingCell,
    ] {
        assert!(is_editable_target(&[DomTarget::Plain, target]));
    }
    assert!(!is_editable_target(&[]));
    assert!(!is_editable_target(&[DomTarget::None]));
    assert!(!is_editable_target(&[DomTarget::Plain]));
}

#[test]
fn no_bridge_binds_nothing() {
    let selection = cell_selection();
    let cols = columns();
    let rows = BTreeSet::new();
    let input = ClipboardDomInput {
        clipboard: None,
        editing_cell: None,
        enabled: true,
        column_names: &cols,
        selection: &selection,
        selected_rows: &rows,
    };
    assert_eq!(
        bind_clipboard_dom(&input),
        ClipboardBinding {
            copy_bound: false,
            paste_bound: false
        }
    );
    assert_eq!(
        handle_copy(&input, &plain_event()),
        CopyOutcome {
            prevented: false,
            set_text: None,
            copied_reported: None
        }
    );
    assert_eq!(
        handle_paste(
            &input,
            &PasteEvent {
                target_chain: vec![DomTarget::Plain],
                text: "x".to_owned()
            }
        ),
        PasteOutcome {
            prevented: false,
            pasted: None
        }
    );
}

#[test]
fn cell_copy_uses_the_normalized_main_rectangle_only() {
    let mut selection = cell_selection();
    selection.extra_ranges.push(CellRange {
        start: CellCoord::new(5, "a"),
        end: CellCoord::new(5, "c"),
    });
    let cols = columns();
    let rows = BTreeSet::new();
    let input = ClipboardDomInput {
        clipboard: None,
        editing_cell: None,
        enabled: true,
        column_names: &cols,
        selection: &selection,
        selected_rows: &rows,
    };
    let target = match resolve_copy_target(&input) {
        Some(GridCopyTarget::Cell { range }) => range,
        other => panic!("expected a cell target, got {other:?}"),
    };
    assert_eq!(target.start, CellCoord::new(1, "a"));
    assert_eq!(target.end, CellCoord::new(3, "b"));
    assert_eq!(
        resolve_paste_anchor(&input),
        Some(PasteAnchor {
            row_index: 1,
            column_name: "a".to_owned()
        })
    );
}

#[test]
fn row_copy_is_sorted_and_paste_is_cell_only() {
    let selection = GridSelection::row_mode();
    let cols = columns();
    let rows: BTreeSet<usize> = [5, 1, 3].into_iter().collect();
    let input = ClipboardDomInput {
        clipboard: None,
        editing_cell: None,
        enabled: true,
        column_names: &cols,
        selection: &selection,
        selected_rows: &rows,
    };
    assert_eq!(
        resolve_copy_target(&input),
        Some(GridCopyTarget::Row {
            row_indices: vec![1, 3, 5]
        })
    );
    assert_eq!(resolve_paste_anchor(&input), None);

    let empty_rows = BTreeSet::new();
    let empty = ClipboardDomInput {
        selected_rows: &empty_rows,
        ..input
    };
    assert_eq!(resolve_copy_target(&empty), None);
}

#[test]
fn disabled_or_incomplete_selections_have_no_targets() {
    let cols = columns();
    let rows = BTreeSet::new();
    let selection = cell_selection();
    let disabled = ClipboardDomInput {
        clipboard: None,
        editing_cell: None,
        enabled: false,
        column_names: &cols,
        selection: &selection,
        selected_rows: &rows,
    };
    assert_eq!(resolve_copy_target(&disabled), None);
    assert_eq!(resolve_paste_anchor(&disabled), None);

    let mut missing = GridSelection {
        anchor: Some(CellCoord::new(1, "unknown")),
        focus: Some(CellCoord::new(2, "a")),
        ..cell_selection()
    };
    missing.mode = GridSelectionMode::Cell;
    let bad = ClipboardDomInput {
        selection: &missing,
        ..disabled
    };
    assert_eq!(resolve_copy_target(&bad), None);
    assert_eq!(resolve_paste_anchor(&bad), None);
}

#[test]
fn copy_sets_text_and_reports_success() {
    let calls = RefCell::new(Vec::new());
    let build = |target: &GridCopyTarget| -> Option<String> {
        calls.borrow_mut().push(format!("build:{target:?}"));
        Some("a\tb\nc\td".to_owned())
    };
    let copied = |target: &GridCopyTarget, ok: bool| {
        calls.borrow_mut().push(format!("copied:{ok}:{target:?}"));
    };
    let pasted = |text: &str, anchor: &PasteAnchor| {
        calls.borrow_mut().push(format!("pasted:{text}:{}", anchor.row_index));
    };
    let bridge = ClipboardBridge {
        build_copy_text: &build,
        on_copied: &copied,
        on_paste_text: &pasted,
    };
    let cols = columns();
    let rows = BTreeSet::new();
    let selection = cell_selection();
    let input = ClipboardDomInput {
        clipboard: Some(&bridge),
        editing_cell: None,
        enabled: true,
        column_names: &cols,
        selection: &selection,
        selected_rows: &rows,
    };
    assert_eq!(
        bind_clipboard_dom(&input),
        ClipboardBinding {
            copy_bound: true,
            paste_bound: true
        }
    );
    let outcome = handle_copy(&input, &plain_event());
    assert!(outcome.prevented);
    assert_eq!(outcome.set_text.as_deref(), Some("a\tb\nc\td"));
    assert_eq!(outcome.copied_reported, Some(true));
    assert_eq!(calls.borrow().len(), 2);
}

#[test]
fn an_empty_copy_reports_failure_without_setting_data() {
    let reported = RefCell::new(Vec::new());
    let build = |_target: &GridCopyTarget| -> Option<String> { Some(String::new()) };
    let copied = |_target: &GridCopyTarget, ok: bool| reported.borrow_mut().push(ok);
    let pasted = |_text: &str, _anchor: &PasteAnchor| {};
    let bridge = ClipboardBridge {
        build_copy_text: &build,
        on_copied: &copied,
        on_paste_text: &pasted,
    };
    let cols = columns();
    let rows = BTreeSet::new();
    let selection = cell_selection();
    let input = ClipboardDomInput {
        clipboard: Some(&bridge),
        editing_cell: None,
        enabled: true,
        column_names: &cols,
        selection: &selection,
        selected_rows: &rows,
    };
    let outcome = handle_copy(&input, &plain_event());
    assert!(outcome.prevented);
    assert_eq!(outcome.set_text, None);
    assert_eq!(outcome.copied_reported, Some(false));
    assert_eq!(reported.into_inner(), vec![false]);
}

#[test]
fn a_null_copy_text_is_left_to_the_native_behaviour() {
    let reported = RefCell::new(0);
    let build = |_target: &GridCopyTarget| -> Option<String> { None };
    let copied = |_target: &GridCopyTarget, _ok: bool| *reported.borrow_mut() += 1;
    let pasted = |_text: &str, _anchor: &PasteAnchor| {};
    let bridge = ClipboardBridge {
        build_copy_text: &build,
        on_copied: &copied,
        on_paste_text: &pasted,
    };
    let cols = columns();
    let rows = BTreeSet::new();
    let selection = cell_selection();
    let input = ClipboardDomInput {
        clipboard: Some(&bridge),
        editing_cell: None,
        enabled: true,
        column_names: &cols,
        selection: &selection,
        selected_rows: &rows,
    };
    assert_eq!(
        handle_copy(&input, &plain_event()),
        CopyOutcome {
            prevented: false,
            set_text: None,
            copied_reported: None
        }
    );
    assert_eq!(*reported.borrow(), 0);
}

#[test]
fn an_editor_or_editable_target_blocks_both_gestures() {
    let called = RefCell::new(0);
    let build = |_target: &GridCopyTarget| -> Option<String> { Some("x".to_owned()) };
    let copied = |_target: &GridCopyTarget, _ok: bool| *called.borrow_mut() += 1;
    let pasted = |_text: &str, _anchor: &PasteAnchor| *called.borrow_mut() += 1;
    let bridge = ClipboardBridge {
        build_copy_text: &build,
        on_copied: &copied,
        on_paste_text: &pasted,
    };
    let cols = columns();
    let rows = BTreeSet::new();
    let selection = cell_selection();
    let clipboard = Some(&bridge);
    let editing = ClipboardDomInput {
        clipboard,
        editing_cell: Some(CellCoord::new(1, "a")),
        enabled: true,
        column_names: &cols,
        selection: &selection,
        selected_rows: &rows,
    };
    let editable_event = ClipboardEvent {
        target_chain: vec![DomTarget::Plain, DomTarget::Input],
    };
    assert!(!handle_copy(&editing, &plain_event()).prevented);
    assert!(!handle_copy(&editing, &editable_event).prevented);
    let idle_copy = CopyOutcome {
        prevented: false,
        set_text: None,
        copied_reported: None,
    };
    assert_eq!(handle_copy(&editing, &plain_event()), idle_copy);

    let paste_event = PasteEvent {
        target_chain: vec![DomTarget::EditingCell],
        text: "x".to_owned(),
    };
    assert!(!handle_paste(&editing, &paste_event).prevented);
    let clean = ClipboardDomInput {
        editing_cell: None,
        ..editing
    };
    assert!(!handle_paste(&clean, &paste_event).prevented);
    assert_eq!(*called.borrow(), 0);
}

#[test]
fn paste_lands_at_the_anchor_and_reports_no_writes() {
    let seen = RefCell::new(Vec::new());
    let build = |_target: &GridCopyTarget| -> Option<String> { Some("x".to_owned()) };
    let copied = |_target: &GridCopyTarget, _ok: bool| {};
    let pasted = |text: &str, anchor: &PasteAnchor| {
        seen.borrow_mut().push((text.to_owned(), anchor.clone()));
    };
    let bridge = ClipboardBridge {
        build_copy_text: &build,
        on_copied: &copied,
        on_paste_text: &pasted,
    };
    let cols = columns();
    let rows = BTreeSet::new();
    let selection = cell_selection();
    let input = ClipboardDomInput {
        clipboard: Some(&bridge),
        editing_cell: None,
        enabled: true,
        column_names: &cols,
        selection: &selection,
        selected_rows: &rows,
    };
    let outcome = handle_paste(
        &input,
        &PasteEvent {
            target_chain: vec![DomTarget::Plain],
            text: "pasted".to_owned(),
        },
    );
    assert!(outcome.prevented);
    assert_eq!(
        outcome.pasted,
        Some((
            "pasted".to_owned(),
            PasteAnchor {
                row_index: 1,
                column_name: "a".to_owned()
            }
        ))
    );
    assert_eq!(seen.borrow().len(), 1);
}

#[test]
fn crate_root_reexports_the_clipboard_dom_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::GridCopyTarget>();
    resolves::<crate::PasteAnchor>();
    resolves::<crate::ClipboardBridge>();
    resolves::<crate::ClipboardDomInput>();
    resolves::<crate::CopyOutcome>();
    resolves::<crate::PasteOutcome>();
    assert!(crate::is_editable_target(&[crate::DomTarget::Input]));
    assert_eq!(crate::CLIPBOARD_DOM_GAP.len(), 3);
}
