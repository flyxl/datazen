//! T536: `TableClipboardStatus` — keys, paste summary/problem copy, visibility
//! transitions with the host-owned feedback deadline, and rect geometry at both
//! sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::table_clipboard_status::{
    build, describe_problem, has_result, message_segments, tick, PasteProblem, PasteSummary,
    TableClipboardAction, TableClipboardStatusVm,
};

fn summary(staged: usize, skipped: usize, overwritten: usize) -> PasteSummary {
    PasteSummary {
        cells: staged + skipped,
        staged,
        skipped,
        overwritten_pending: overwritten,
        ..Default::default()
    }
}

fn problem(code: &str, detail: &str) -> PasteProblem {
    PasteProblem {
        code: code.to_owned(),
        detail: Some(detail.to_owned()),
        ..Default::default()
    }
}

#[derive(Clone)]
struct Scene {
    vm: TableClipboardStatusVm,
    actions: Rc<RefCell<Vec<TableClipboardAction>>>,
    dark: bool,
    confirm: Option<El>,
}

fn scene(vm: TableClipboardStatusVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark, confirm: None }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        if action == TableClipboardAction::Dismiss {
            state.vm.last_paste = None;
            state.vm.problems.clear();
            state.vm.visible_until = None;
        }
        actions.borrow_mut().push(action);
    });
    col().w_full().h_full().child(build(&s.vm, s.confirm.clone(), Some(callback), s.dark))
}

/// Non-capturing view for the "no host port" branch (Runtime needs an fn pointer).
fn no_port_view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(build::<Scene>(&s.vm, None, None, s.dark))
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn problem_descriptions_use_exact_keys_and_keep_raw_detail() {
    assert_eq!(
        describe_problem(&problem("grid.paste.columnMismatch", "city, zip")),
        t("dataTable.pasteDroppedColumns")
            .replace("{count}", "2")
            .replace("{names}", "city, zip")
    );
    assert_eq!(
        describe_problem(&problem("grid.paste.unclosedQuote", "")),
        t("dataTable.pasteUnclosedQuote")
    );
    assert_eq!(
        describe_problem(&problem("grid.paste.unknownCode", "raw detail")),
        "raw detail",
        "an unknown problem must not be swallowed"
    );
    assert_eq!(
        describe_problem(&problem("grid.paste.unknownCode", "")),
        "",
        "React keeps Some(\"\") rather than falling back to the code"
    );
    assert_eq!(
        describe_problem(&PasteProblem {
            code: "grid.paste.unknownCode".to_owned(),
            detail: None,
            ..Default::default()
        }),
        "grid.paste.unknownCode",
        "only None falls back to the code"
    );
    assert_eq!(
        describe_problem(&PasteProblem {
            code: "grid.paste.outOfBounds".to_owned(),
            detail: None,
            ..Default::default()
        }),
        t("dataTable.pasteClippedToPage").replace("{count}", "0"),
        "a missing detail falls back to 0, not an empty string"
    );
}

#[test]
fn pasted_cells_and_skips_are_reported_with_exact_copy() {
    let vm = TableClipboardStatusVm {
        last_paste: Some(summary(3, 2, 0)),
        ..Default::default()
    };
    let expected = t("dataTable.pasteResult")
        .replace("{count}", "3")
        .replace("{skipped}", "2");
    assert_eq!(message_segments(&vm), vec![expected.clone()]);
    assert!(has_result(&vm));

    let rt = runtime(scene(vm, true), (1440.0, 816.0));
    for key in ["table-clipboard-slot", "table-clipboard-status", "table-clipboard-status-dismiss", "paste-data-staged-3", "paste-data-skipped-2"] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert!(texts(&rt).iter().any(|x| x == &expected), "the result sentence is shown");
}

#[test]
fn overwrite_only_revert_is_visible_without_a_zero_paste_line() {
    let vm = TableClipboardStatusVm {
        last_paste: Some(summary(0, 0, 2)),
        ..Default::default()
    };
    assert!(!has_result(&vm), "no cells moved");
    let segments = message_segments(&vm);
    assert_eq!(
        segments,
        vec![t("dataTable.pasteOverwrotePending").replace("{count}", "2")],
        "only the override note is rendered"
    );
    assert!(!segments.iter().any(|x| x.contains('0')), "no '0 cell(s) pasted' line");
    let rt = runtime(scene(vm, true), (1440.0, 816.0));
    assert!(has_key(&rt, "table-clipboard-status"));
}

#[test]
fn a_no_op_paste_stays_hidden() {
    let vm = TableClipboardStatusVm {
        last_paste: Some(summary(0, 0, 0)),
        ..Default::default()
    };
    let rt = runtime(scene(vm, true), (1440.0, 816.0));
    assert!(!has_key(&rt, "table-clipboard-status"), "nothing changed: no line");
    assert!(has_key(&rt, "table-clipboard-slot"), "confirm slot still mounts");
}

#[test]
fn first_problem_adds_a_remaining_count() {
    let vm = TableClipboardStatusVm {
        problems: vec![
            problem("grid.paste.columnMismatch", "city, zip"),
            problem("grid.paste.empty", ""),
        ],
        ..Default::default()
    };
    let segments = message_segments(&vm);
    assert_eq!(segments.len(), 1);
    assert!(segments[0].contains("(+1)"), "remaining problem count: {}", segments[0]);
}

#[test]
fn feedback_deadline_hides_the_line_when_the_host_poll_reaches_it() {
    let mut vm = TableClipboardStatusVm {
        last_paste: Some(summary(1, 0, 0)),
        visible_until: Some(2.5),
        now: 0.0,
        ..Default::default()
    };
    assert!(!tick(&mut vm, 2.49), "inside the window the line stays");
    assert!(has_result(&vm));
    assert!(tick(&mut vm, 2.5), "at the deadline the host clears the snapshot");
    assert!(vm.last_paste.is_none() && vm.visible_until.is_none());
    let rt = runtime(scene(vm, true), (1440.0, 816.0));
    assert!(!has_key(&rt, "table-clipboard-status"));
}

#[test]
fn dismiss_emits_the_host_intent_and_clears_the_line() {
    let mut rt = runtime(
        scene(
            TableClipboardStatusVm { last_paste: Some(summary(1, 0, 0)), ..Default::default() },
            true,
        ),
        (1440.0, 816.0),
    );
    click(&mut rt, "table-clipboard-status-dismiss");
    for _ in 0..3 {
        rt.invalidate();
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(rt.state.actions.borrow().as_slice(), &[TableClipboardAction::Dismiss]);
    assert!(!has_key(&rt, "table-clipboard-status"));
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            for vm in [
                TableClipboardStatusVm::default(),
                TableClipboardStatusVm {
                    last_paste: Some(summary(3, 2, 1)),
                    problems: vec![problem("grid.paste.columnMismatch", "city, zip")],
                    ..Default::default()
                },
            ] {
                let rt = runtime(scene(vm, dark), size);
                let outside: Vec<String> = rt.elements().iter()
                    .filter(|e| {
                        let r = e.rect;
                        r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                    })
                    .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                    .collect();
                assert!(outside.is_empty(), "{size:?} dark={dark}: outside {outside:?}");
            }
        }
    }
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Reporting zero-cell pastes as a result made `overwrite_only_revert_is_visible_without_a_zero_paste_line`
//    fail on `no '0 cell(s) pasted' line`.
// 2. Ignoring the recorded deadline made `feedback_deadline_hides_the_line_when_the_host_poll_reaches_it`
//    fail on `at the deadline the host clears the snapshot`.

#[test]
fn every_problem_code_maps_to_the_react_key_and_params() {
    // Expected strings are composed from the React `describePasteProblem` switch,
    // so a wrong/substituted key in the port produces a different string (`t`
    // falls back to the raw key) and fails here.
    let cases: Vec<(PasteProblem, String)> = vec![
        (problem("grid.paste.empty", ""), t("dataTable.pasteEmpty").to_owned()),
        (problem("grid.paste.tooLarge", "8"), t("dataTable.pasteTooLarge")
            .replace("{cells}", "8").replace("{maxCells}", "100000")),
        (problem("grid.paste.raggedRow", "row 2 has 3"), "row 2 has 3".to_owned()),
        (problem("grid.paste.unclosedQuote", ""), t("dataTable.pasteUnclosedQuote").to_owned()),
        (problem("grid.paste.outOfBounds", "4"), t("dataTable.pasteClippedToPage")
            .replace("{count}", "4")),
        (problem("grid.paste.noWritableCell", ""), t("dataTable.pasteNoWritableColumn").to_owned()),
        (PasteProblem {
            code: "grid.paste.notNullEmpty".to_owned(), row: Some(2),
            column: Some("age".to_owned()), ..Default::default()
        }, t("dataTable.pasteNotAllowedEmpty").replace("{row}", "3").replace("{column}", "age")),
        (PasteProblem {
            code: "grid.paste.identityChanged".to_owned(), row: Some(0),
            column: Some("id".to_owned()), ..Default::default()
        }, t("dataTable.pasteIdentityChanged").replace("{row}", "1").replace("{column}", "id")),
        (problem("grid.paste.columnMismatch", "a, b"), t("dataTable.pasteDroppedColumns")
            .replace("{count}", "2").replace("{names}", "a, b")),
        (problem("grid.paste.crossTable", "orders"), t("dataTable.pasteCrossTableNotice")
            .replace("{table}", "orders")),
        (PasteProblem {
            code: "grid.cell.readOnly".to_owned(), column: Some("id".to_owned()),
            ..Default::default()
        }, t("dataTable.pasteReadOnlyColumn").replace("{count}", "1").replace("{columns}", "id")),
        (problem("grid.cell.readOnly", "readOnly"), t("dataTable.readOnlyPasteDisabled").to_owned()),
        (problem("grid.cell.noRowIdentity", ""), t("dataTable.pasteNoRowIdentity").to_owned()),
        (problem("grid.clipboard.writeFailed", ""), t("dataTable.copyFailed").to_owned()),
        (problem("grid.clipboard.readFailed", ""), t("dataTable.pasteReadFailed")
            .replace("{shortcut}", "⌘V")),
        (problem("grid.pasteRows.notRowData", ""), t("dataTable.pasteRowsNotRowData").to_owned()),
        (problem("grid.pasteRows.noWritableColumn", ""),
            t("dataTable.pasteRowsNoWritableColumn").to_owned()),
    ];
    for (p, expected) in cases {
        assert_eq!(describe_problem(&p), expected, "code {} must use the React key", p.code);
    }
}

#[test]
fn host_confirm_element_mounts_even_when_the_line_is_hidden() {
    let mut scene = scene(TableClipboardStatusVm::default(), true);
    scene.confirm = Some(rune::text("host-confirm").key("host-confirm"));
    let mut rt = runtime(scene, (960.0, 640.0));
    rt.frame(0.0);
    assert!(has_key(&rt, "host-confirm"), "the injected confirm element is mounted");
    assert!(!has_key(&rt, "table-clipboard-status"), "a no-op paste still hides the line");
}

#[test]
fn dismiss_without_a_host_port_is_inert() {
    let scene = scene(
        TableClipboardStatusVm { last_paste: Some(summary(1, 0, 0)), ..Default::default() },
        true,
    );
    let mut rt = Runtime::new(
        scene,
        no_port_view,
        (1440.0, 816.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    click(&mut rt, "table-clipboard-status-dismiss");
    assert!(has_key(&rt, "table-clipboard-status"), "no host callback: nothing clears the snapshot");
}

#[test]
fn tick_clears_a_problem_only_snapshot_at_the_deadline() {
    let mut vm = TableClipboardStatusVm {
        problems: vec![problem("grid.paste.empty", "")],
        visible_until: Some(1.0),
        ..Default::default()
    };
    assert!(!tick(&mut vm, 0.9), "inside the window nothing changes");
    assert_eq!(vm.problems.len(), 1);
    assert!(tick(&mut vm, 1.0), "the deadline clears the problem snapshot");
    assert!(vm.problems.is_empty() && vm.visible_until.is_none());
}

// Independent lane-C repro probes (original D1/D2), retained as regression guards.
#[test]
fn probe_out_of_bounds_without_detail_says_zero_like_react() {
    let problem = PasteProblem {
        code: "grid.paste.outOfBounds".to_owned(),
        ..Default::default()
    };
    assert_eq!(
        describe_problem(&problem),
        t("dataTable.pasteClippedToPage").replace("{count}", "0"),
        "React defaults a missing outOfBounds detail to '0'"
    );
}

#[test]
fn probe_unknown_code_with_empty_detail_keeps_the_empty_detail() {
    let problem = PasteProblem {
        code: "grid.paste.unknownCode".to_owned(),
        detail: Some(String::new()),
        ..Default::default()
    };
    assert_eq!(
        describe_problem(&problem),
        "",
        "React returns detail ?? code, so an empty detail renders empty"
    );
}
