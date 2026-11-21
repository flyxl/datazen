//! T546: `TablePendingChangesBar` + `TableTransactionControls` — the React test
//! cases as view/state tests: staged counts, E2E keys, save/discard actions,
//! busy/disabled gating, manual-tx lifecycle, lost-session recovery and rect
//! geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::table_pending_changes_bar::{
    build_pending, build_tx, DirtyCountsVm, PendingAction, PendingChangesVm, TransactionControlsVm,
    PENDING_CELLS_KEY, PENDING_COMMIT_KEY, PENDING_DELETES_KEY, PENDING_INSERTS_KEY,
    PENDING_OFFPAGE_KEY, PENDING_PREVIEW_KEY, PENDING_ROOT, PENDING_ROLLBACK_KEY, PENDING_TITLE_KEY,
    PENDING_UPDATES_KEY, TX_BEGIN_KEY, TX_COMMIT_KEY, TX_CONTROLS_KEY, TX_ERROR_KEY,
    TX_OPEN_NEW_KEY, TX_ROLLBACK_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    pending: PendingChangesVm,
    tx: TransactionControlsVm,
    actions: Rc<RefCell<Vec<PendingAction>>>,
    dark: bool,
    show_pending: bool,
    show_tx: bool,
}

fn pending() -> PendingChangesVm {
    PendingChangesVm {
        pending_count: 2,
        update_count: 1,
        delete_count: 1,
        insert_count: 0,
        counts: None,
        busy: false,
    }
}

fn tx() -> TransactionControlsVm {
    TransactionControlsVm {
        is_editable: true,
        loading: false,
        ..TransactionControlsVm::default()
    }
}

fn scene(dark: bool) -> Scene {
    Scene {
        pending: pending(),
        tx: tx(),
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
        show_pending: true,
        show_tx: false,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Some(Rc::new(move |_state: &mut Scene, action| {
        actions.borrow_mut().push(action);
    }) as Rc<dyn Fn(&mut Scene, PendingAction) + 'static>);
    let mut root = col().w_full().h_full().gap(0.0);
    if s.show_pending {
        root = root.child(build_pending(&s.pending, callback.clone(), s.dark));
    }
    if s.show_tx {
        root = root.child(build_tx(&s.tx, callback, s.dark));
    }
    root
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn texts(rt: &Runtime<Scene>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside(rt: &Runtime<Scene>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: outside {outside:?}");
}

// ── pure rules ────────────────────────────────────────────────────────────────

#[test]
fn dirty_counts_off_page_is_clamped() {
    let counts = DirtyCountsVm { total_cells: 5, cells_on_page: 2 };
    assert_eq!(counts.cells_off_page(), 3);
    let less = DirtyCountsVm { total_cells: 1, cells_on_page: 4 };
    assert_eq!(less.cells_off_page(), 0);
}

#[test]
fn summary_counts_use_exact_i18n_keys() {
    let mut vm = pending();
    vm.insert_count = 2;
    vm.counts = Some(DirtyCountsVm { total_cells: 7, cells_on_page: 3 });
    assert_eq!(vm.summary_text(), t("tableData.pendingChanges").replace("{count}", "2"));
    assert_eq!(vm.inserts_text(), t("tableData.pendingInserts").replace("{count}", "2"));
    assert_eq!(vm.updates_text(), t("tableData.pendingUpdates").replace("{count}", "1"));
    assert_eq!(vm.deletes_text(), t("tableData.pendingDeletes").replace("{count}", "1"));
    assert_eq!(vm.cells_text(), Some(t("tableData.pendingCells").replace("{count}", "7")));
    assert_eq!(vm.offpage_text(), Some(t("tableData.pendingOffPage").replace("{count}", "4")));

    let bare = pending();
    assert_eq!(bare.cells_text(), None, "no dirty index means no cell segment");
    assert_eq!(bare.offpage_text(), None);
}

// ── pending bar ───────────────────────────────────────────────────────────────

#[test]
fn pending_bar_keeps_e2e_keys_and_counts() {
    let rt = runtime(scene(true), DESIGN);
    for key in [
        PENDING_ROOT, PENDING_TITLE_KEY, PENDING_UPDATES_KEY, PENDING_DELETES_KEY,
        PENDING_PREVIEW_KEY, PENDING_COMMIT_KEY, PENDING_ROLLBACK_KEY,
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert!(!has_key(&rt, PENDING_INSERTS_KEY), "zero insert count is omitted");
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|text| text == t("tableData.preview")));
    assert!(rendered.iter().any(|text| text == t("tableData.commit")));
    assert!(rendered.iter().any(|text| text == t("tableData.rollback")));
    assert!(rendered.iter().any(|text| text.as_str() == t("tableData.pendingChanges").replace("{count}", "2")), "summary shown");
}

#[test]
fn pending_bar_renders_nothing_when_empty() {
    let mut s = scene(true);
    s.pending.pending_count = 0;
    let rt = runtime(s, MIN);
    assert!(!has_key(&rt, PENDING_ROOT));
    assert!(!has_key(&rt, PENDING_COMMIT_KEY));
}

#[test]
fn pending_bar_segments_appear_only_when_non_zero() {
    let mut s = scene(true);
    s.pending.update_count = 0;
    s.pending.delete_count = 0;
    s.pending.insert_count = 3;
    s.pending.counts = Some(DirtyCountsVm { total_cells: 6, cells_on_page: 6 });
    let rt = runtime(s, DESIGN);
    assert!(has_key(&rt, PENDING_INSERTS_KEY));
    assert!(!has_key(&rt, PENDING_UPDATES_KEY));
    assert!(!has_key(&rt, PENDING_DELETES_KEY));
    assert!(has_key(&rt, PENDING_CELLS_KEY));
    assert!(!has_key(&rt, PENDING_OFFPAGE_KEY), "no off-page cells");
}

#[test]
fn pending_bar_routes_preview_commit_rollback() {
    let mut rt = runtime(scene(true), DESIGN);
    click(&mut rt, PENDING_PREVIEW_KEY);
    click(&mut rt, PENDING_COMMIT_KEY);
    click(&mut rt, PENDING_ROLLBACK_KEY);
    assert_eq!(
        *rt.state.actions.borrow(),
        [PendingAction::Preview, PendingAction::Commit, PendingAction::Rollback]
    );
}

#[test]
fn busy_pending_bar_actions_are_inert() {
    let mut s = scene(true);
    s.pending.busy = true;
    let mut rt = runtime(s, DESIGN);
    click(&mut rt, PENDING_PREVIEW_KEY);
    click(&mut rt, PENDING_COMMIT_KEY);
    click(&mut rt, PENDING_ROLLBACK_KEY);
    assert!(rt.state.actions.borrow().is_empty(), "a busy bar disables every action");
}

// ── transaction controls ─────────────────────────────────────────────────────

#[test]
fn read_only_panel_has_no_tx_controls() {
    let mut s = scene(true);
    s.show_pending = false;
    s.show_tx = true;
    s.tx.is_editable = false;
    let rt = runtime(s, DESIGN);
    assert!(!has_key(&rt, TX_CONTROLS_KEY));
    assert!(!has_key(&rt, TX_BEGIN_KEY));
}

#[test]
fn begin_then_commit_and_rollback_are_offered_in_tx() {
    let mut s = scene(true);
    s.show_pending = false;
    s.show_tx = true;
    let mut rt = runtime(s, DESIGN);
    assert!(has_key(&rt, TX_BEGIN_KEY));
    assert!(!has_key(&rt, TX_COMMIT_KEY));
    click(&mut rt, TX_BEGIN_KEY);
    assert_eq!(*rt.state.actions.borrow(), [PendingAction::BeginTx]);

    let mut s = scene(true);
    s.show_pending = false;
    s.show_tx = true;
    s.tx.in_tx = true;
    let mut rt = runtime(s, DESIGN);
    assert!(has_key(&rt, TX_COMMIT_KEY));
    assert!(has_key(&rt, TX_ROLLBACK_KEY));
    assert!(!has_key(&rt, TX_BEGIN_KEY));
    click(&mut rt, TX_COMMIT_KEY);
    assert_eq!(*rt.state.actions.borrow(), [PendingAction::CommitTx]);

    let mut s = scene(true);
    s.show_pending = false;
    s.show_tx = true;
    s.tx.in_tx = true;
    let mut rt = runtime(s, DESIGN);
    click(&mut rt, TX_ROLLBACK_KEY);
    assert_eq!(*rt.state.actions.borrow(), [PendingAction::RollbackTx]);
}

#[test]
fn tx_busy_or_loading_disables_actions() {
    let mut s = scene(true);
    s.show_pending = false;
    s.show_tx = true;
    s.tx.tx_busy = true;
    let mut rt = runtime(s, DESIGN);
    click(&mut rt, TX_BEGIN_KEY);
    assert!(rt.state.actions.borrow().is_empty());

    let mut s = scene(true);
    s.show_pending = false;
    s.show_tx = true;
    s.tx.loading = true;
    let mut rt = runtime(s, DESIGN);
    click(&mut rt, TX_BEGIN_KEY);
    assert!(rt.state.actions.borrow().is_empty());
}

#[test]
fn tx_error_banner_and_lost_session_recovery() {
    let mut s = scene(true);
    s.show_pending = false;
    s.show_tx = true;
    s.tx.tx_error = "tx_deadlock; effect: rolled_back".to_owned();
    let rt = runtime(s, DESIGN);
    assert!(has_key(&rt, TX_ERROR_KEY));
    assert!(!has_key(&rt, TX_OPEN_NEW_KEY), "no open-new when session not lost");

    let mut s = scene(true);
    s.show_pending = false;
    s.show_tx = true;
    s.tx.tx_error = "session_gone".to_owned();
    s.tx.tx_lost = true;
    let mut rt = runtime(s, DESIGN);
    assert!(has_key(&rt, TX_OPEN_NEW_KEY));
    click(&mut rt, TX_OPEN_NEW_KEY);
    assert_eq!(*rt.state.actions.borrow(), [PendingAction::OpenNewSession]);
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut s = scene(dark);
            s.pending.insert_count = 2;
            s.pending.counts = Some(DirtyCountsVm { total_cells: 9, cells_on_page: 3 });
            s.pending.busy = true;
            let rt = runtime(s.clone(), size);
            assert_inside(&rt, size, &format!("pending {size:?} dark={dark}"));

            s.show_pending = false;
            s.show_tx = true;
            s.tx.in_tx = true;
            s.tx.tx_error = "tx failed".to_owned();
            s.tx.tx_lost = true;
            let rt = runtime(s, size);
            assert_inside(&rt, size, &format!("tx {size:?} dark={dark}"));
        }
    }
}
