//! Independent audit for T0-11 tree row math + reveal wiring (commit 9abc9cb8f).
//!
//! Focus:
//! - pure math contract parity with React `VirtualTree.tsx` / `navigation.ts`
//!   (`firstVisibleTreeIndex`, `revealOffset`, `ancestorIndexes` semantics)
//! - reveal behavior wiring via Runtime (keyboard + external select)
//! - `ancestor_indices` robustness on non-compliant/depth-jump inputs (no panic)

use crate::ui::tree::{ancestor_indices, first_visible_index, reveal_offset, DataTree, TreeRowData};
use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};
use rune_kit::TreeModel;


fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Clone, Default)]
struct Item {
    label: String,
}

struct Probe {
    key: &'static str,
    model: TreeModel<Item>,
    selects: Vec<String>,
    opens: Vec<String>,
    viewport: f32,
    scroll_y: f32,
    container_h: f32,
}

impl Probe {
    fn range(key: &'static str, n: usize) -> Self {
        let mut model = TreeModel::new().key(key);
        for i in 0..n {
            let label = format!("node-{i}");
            model.insert(None, label.clone(), Item { label });
        }
        Self {
            key,
            model,
            selects: Vec::new(),
            opens: Vec::new(),
            viewport: 0.0,
            scroll_y: 0.0,
            container_h: 200.0,
        }
    }
}

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let key = state.key;
    let tree = DataTree::new(
        key,
        |s: &Probe| &s.model,
        |s: &mut Probe| &mut s.model,
        |n: &Item| n.label.as_str(),
    )
    .grow()
    .viewport(|s: &Probe| s.viewport, |s: &mut Probe, h| s.viewport = h)
    .scroll_y(|s: &Probe| s.scroll_y, |s: &mut Probe, y| s.scroll_y = y)
    .on_select(|s: &mut Probe, id| s.selects.push(id.to_owned()))
    .on_open(|s: &mut Probe, id| s.opens.push(id.to_owned()))
    .build(state, true);
    col().w_full().h_full().padding(24.0).child(col().w_full().h(state.container_h).child(tree))
}

fn probe(key: &'static str, n: usize, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(Probe::range(key, n), view, size, env());
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is mounted"))
}

fn settle<S>(rt: &mut Runtime<S>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn assert_selected_row_in_viewport(rt: &Runtime<Probe>, label: &str) {
    let selected = rt
        .state
        .model
        .cursor()
        .map(str::to_owned)
        .expect("cursor set");
    let row_idx = rt
        .state
        .model
        .visible_row(&selected)
        .expect("cursor visible");
    let row_key = format!("{}-row-{row_idx}", rt.state.key);
    let row = element(rt, &row_key);
    let vp = element(rt, &format!("{}-viewport", rt.state.key));
    let [ry, rh] = [row.rect[1], row.rect[3]];
    let [vy, vh] = [vp.rect[1], vp.rect[3]];
    assert!(
        ry + 0.5 >= vy && ry + rh <= vy + vh + 0.5,
        "{label}: selected row {selected} (idx {row_idx}) rect {:?} not inside viewport {:?}, scroll_y={}",
        row.rect,
        vp.rect,
        rt.state.scroll_y
    );
}

#[test]
fn first_visible_index_contract_edges() {
    assert_eq!(first_visible_index(0.0, 10, 24.0), 0);
    assert_eq!(first_visible_index(-10.0, 10, 24.0), 0, "negative scroll_top clamps to 0");
    assert_eq!(first_visible_index(23.999, 10, 24.0), 0);
    assert_eq!(first_visible_index(24.0, 10, 24.0), 1);
    assert_eq!(first_visible_index(47.9, 10, 24.0), 1);
    assert_eq!(first_visible_index(48.0, 10, 24.0), 2);
    assert_eq!(first_visible_index(10_000.0, 10, 24.0), 9, "scroll beyond content clamps to last row");
    assert_eq!(first_visible_index(240.0, 10, 24.0), 9, "exact content end clamps to last row");
    assert_eq!(first_visible_index(50.0, 0, 24.0), 0, "row_count 0 -> 0");
    assert_eq!(first_visible_index(50.0, 10, 0.0), 0, "row_height 0 -> 0");
    assert_eq!(first_visible_index(50.0, 10, -1.0), 0, "negative row_height -> 0");
    assert_eq!(first_visible_index(f32::NAN, 10, 24.0), 0, "NaN scroll_top -> 0");
}

#[test]
fn reveal_offset_contract_edges() {
    let row_h = 24.0;
    let vh = 100.0;

    assert_eq!(reveal_offset(0, row_h, 0.0, vh), None, "row at top, fully visible");
    assert_eq!(reveal_offset(1, row_h, 0.0, vh), None, "middle row fully visible");
    assert_eq!(reveal_offset(3, row_h, 0.0, vh), None, "row bottom=96 inside viewport 100");
    assert_eq!(reveal_offset(2, row_h, 48.0, vh), None, "exact top edge: no scroll needed");
    assert_eq!(reveal_offset(4, row_h, 20.0, vh), None, "exact bottom edge (120==120): no scroll needed");

    assert_eq!(reveal_offset(1, row_h, 50.0, vh), Some(24.0), "above viewport aligns to row top");
    assert_eq!(reveal_offset(0, row_h, 10.0, vh), Some(0.0), "above viewport clamps target to 0");
    assert_eq!(reveal_offset(4, row_h, 0.0, vh), Some(20.0), "below viewport aligns bottom");
    assert_eq!(reveal_offset(5, row_h, 0.0, vh), Some(44.0), "further below aligns bottom minimally");

    assert_eq!(reveal_offset(0, row_h, -10.0, vh), None, "negative scroll_top but row 0 still visible");
    assert_eq!(reveal_offset(10, row_h, -10.0, vh), Some(164.0), "negative scroll_top, target below bottom");
    assert_eq!(reveal_offset(0, row_h, 500.0, vh), Some(0.0), "scroll far beyond content, target above -> top");

    let big_vh = 500.0;
    assert_eq!(reveal_offset(0, row_h, 100.0, big_vh), Some(0.0), "large viewport, scrolled down -> clamp to 0");
    assert_eq!(reveal_offset(5, row_h, 0.0, big_vh), None, "large viewport, row already inside");

    assert_eq!(reveal_offset(0, 0.0, 0.0, vh), None, "row_height 0 -> None");
    assert_eq!(reveal_offset(0, -1.0, 0.0, vh), None, "negative row_height -> None");
    assert_eq!(reveal_offset(0, row_h, 0.0, 0.0), None, "viewport_h 0 -> None");
    assert_eq!(reveal_offset(0, row_h, 0.0, -10.0), None, "negative viewport_h -> None");
}

#[test]
fn ancestor_indices_contract_and_depth_jumps_no_panic() {
    let rows = vec![
        TreeRowData { depth: 0, is_branch: true },
        TreeRowData { depth: 1, is_branch: true },
        TreeRowData { depth: 2, is_branch: false },
        TreeRowData { depth: 1, is_branch: true },
        TreeRowData { depth: 1, is_branch: false },
        TreeRowData { depth: 0, is_branch: true },
        TreeRowData { depth: 0, is_branch: false },
    ];
    assert_eq!(ancestor_indices(&rows, 0), Vec::<usize>::new());
    assert_eq!(ancestor_indices(&rows, 1), vec![0]);
    assert_eq!(ancestor_indices(&rows, 2), vec![0, 1]);
    assert_eq!(ancestor_indices(&rows, 3), vec![0, 1]);
    assert_eq!(ancestor_indices(&rows, 4), vec![0, 3]);
    assert_eq!(ancestor_indices(&rows, 5), vec![0]);
    assert_eq!(ancestor_indices(&rows, 6), vec![5]);
    assert_eq!(ancestor_indices(&rows, rows.len()), Vec::<usize>::new());
    assert_eq!(ancestor_indices(&rows, 99), Vec::<usize>::new());
    assert!(ancestor_indices(&[], 0).is_empty());
    assert!(ancestor_indices(&[], 10).is_empty());

    let jumpy = vec![
        TreeRowData { depth: 0, is_branch: true },
        TreeRowData { depth: 3, is_branch: false },
        TreeRowData { depth: 1, is_branch: true },
        TreeRowData { depth: 0, is_branch: false },
    ];
    for top in 0..=jumpy.len() + 2 {
        let _ = ancestor_indices(&jumpy, top);
    }
    let irregular = vec![
        TreeRowData { depth: 2, is_branch: true },
        TreeRowData { depth: 0, is_branch: false },
        TreeRowData { depth: 5, is_branch: true },
        TreeRowData { depth: 1, is_branch: false },
    ];
    for top in 0..=irregular.len() + 1 {
        let _ = ancestor_indices(&irregular, top);
    }
}

#[test]
fn keyboard_end_pagedown_pageup_down30_keep_selected_row_in_200pt_viewport() {
    let mut rt = probe("audit-kbd", 250, (960.0, 640.0));
    assert!(rt.state.viewport > 0.0, "viewport measured");
    let root = rt.state.key;
    click(&mut rt, &format!("{root}-row-0"));
    settle(&mut rt);

    press(&mut rt, Key::End);
    settle(&mut rt);
    assert_eq!(rt.state.model.cursor(), Some("node-249"), "End selects last row");
    assert_selected_row_in_viewport(&rt, "after End");

    press(&mut rt, Key::PageDown);
    settle(&mut rt);
    press(&mut rt, Key::PageDown);
    settle(&mut rt);
    assert_eq!(rt.state.model.cursor(), Some("node-249"));
    assert_selected_row_in_viewport(&rt, "after PageDown x2 at end");

    press(&mut rt, Key::Home);
    settle(&mut rt);
    press(&mut rt, Key::PageDown);
    settle(&mut rt);
    assert_selected_row_in_viewport(&rt, "after PageDown #1 from home");
    let after_pd1 = rt.state.model.cursor().map(str::to_owned);
    press(&mut rt, Key::PageDown);
    settle(&mut rt);
    assert_selected_row_in_viewport(&rt, "after PageDown #2 from home");
    let after_pd2 = rt.state.model.cursor().map(str::to_owned);
    assert_ne!(after_pd1, after_pd2, "second PageDown should move further when possible");

    press(&mut rt, Key::PageUp);
    settle(&mut rt);
    assert_selected_row_in_viewport(&rt, "after PageUp");

    press(&mut rt, Key::Home);
    settle(&mut rt);
    for _ in 0..30 {
        press(&mut rt, Key::Down);
    }
    settle(&mut rt);
    assert_eq!(rt.state.model.cursor(), Some("node-30"));
    assert_selected_row_in_viewport(&rt, "after Down x30");
}

#[test]
fn external_model_select_reveals_selected_row_after_frames() {
    let mut rt = probe("audit-ext", 250, (960.0, 640.0));
    let root = rt.state.key;
    click(&mut rt, &format!("{root}-row-0"));
    settle(&mut rt);

    press(&mut rt, Key::End);
    settle(&mut rt);
    assert_selected_row_in_viewport(&rt, "after End before external select");

    rt.state.model.select("node-2");
    for _ in 0..16 {
        rt.frame(1.0 / 60.0);
    }
    settle(&mut rt);
    assert_eq!(rt.state.model.cursor(), Some("node-2"));
    assert_selected_row_in_viewport(&rt, "after external select to node-2");

    rt.state.model.select("node-220");
    for _ in 0..16 {
        rt.frame(1.0 / 60.0);
    }
    settle(&mut rt);
    assert_eq!(rt.state.model.cursor(), Some("node-220"));
    assert_selected_row_in_viewport(&rt, "after external select to node-220");
}
#[test]
fn selected_row_in_viewport_advancing_100_frames_does_not_change_scroll() {
    let mut rt = probe("audit-nojitter-visible", 250, (960.0, 640.0));
    let root = rt.state.key;
    click(&mut rt, &format!("{root}-row-0"));
    settle(&mut rt);

    // Navigate to a middle row so it is revealed and in viewport.
    press(&mut rt, Key::Home);
    settle(&mut rt);
    for _ in 0..30 {
        press(&mut rt, Key::Down);
    }
    settle(&mut rt);
    assert_eq!(rt.state.model.cursor(), Some("node-30"));
    assert_selected_row_in_viewport(&rt, "baseline node-30 in viewport");

    let scroll_before = rt.state.scroll_y;
    for _ in 0..100 {
        rt.frame(1.0 / 60.0);
    }
    settle(&mut rt);
    assert_eq!(
        rt.state.scroll_y, scroll_before,
        "scroll_y must not change while selected row remains in viewport and cursor unchanged"
    );
}

#[test]
fn manual_scroll_away_with_unchanged_cursor_does_not_pull_back_after_30_frames() {
    let mut rt = probe("audit-nojitter-manual", 250, (960.0, 640.0));
    let root = rt.state.key;
    click(&mut rt, &format!("{root}-row-0"));
    settle(&mut rt);

    press(&mut rt, Key::Home);
    settle(&mut rt);
    for _ in 0..30 {
        press(&mut rt, Key::Down);
    }
    settle(&mut rt);
    assert_eq!(rt.state.model.cursor(), Some("node-30"));
    assert_selected_row_in_viewport(&rt, "baseline node-30 in viewport");

    // Manual/user scroll away while selection/cursor stays the same.
    let vp_key = format!("{root}-viewport");
    rt.update(|s, tx| {
        s.scroll_y = 0.0;
        tx.scroll_to_instant(vp_key, 0.0, 0.0);
    });
    settle(&mut rt);
    let scroll_after_manual = rt.state.scroll_y;
    assert_eq!(rt.state.model.cursor(), Some("node-30"), "cursor unchanged by manual scroll");

    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    settle(&mut rt);
    assert_eq!(
        rt.state.scroll_y, scroll_after_manual,
        "manual scroll-away must not be pulled back while cursor is unchanged"
    );
    assert_eq!(rt.state.model.cursor(), Some("node-30"));
}

#[test]
fn external_select_to_already_visible_row_does_not_write_scroll() {
    let mut rt = probe("audit-no-write-visible", 250, (960.0, 640.0));
    let root = rt.state.key;
    click(&mut rt, &format!("{root}-row-0"));
    settle(&mut rt);

    // node-30 via keyboard is revealed/in viewport; record stable scroll.
    press(&mut rt, Key::Home);
    settle(&mut rt);
    for _ in 0..30 {
        press(&mut rt, Key::Down);
    }
    settle(&mut rt);
    assert_eq!(rt.state.model.cursor(), Some("node-30"));
    assert_selected_row_in_viewport(&rt, "baseline node-30 in viewport");
    let scroll_before = rt.state.scroll_y;

    // Re-select the same already-visible row externally/programmatically.
    // If implementation treats same-id select as cursor unchanged, scroll must remain stable.
    // (No-op or redundant select must not cause jitter.)
    rt.state.model.select("node-30");
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    settle(&mut rt);
    assert_eq!(rt.state.model.cursor(), Some("node-30"));
    assert_eq!(
        rt.state.scroll_y, scroll_before,
        "selecting already-visible/current row must not write/change scroll"
    );
    assert_selected_row_in_viewport(&rt, "after redundant select to visible row");
}


#[test]
fn cursor_unchanged_across_viewport_resize_does_not_jump_scroll() {
    let mut rt = probe("audit-resize", 250, (960.0, 640.0));
    let root = rt.state.key;
    click(&mut rt, &format!("{root}-row-0"));
    settle(&mut rt);

    press(&mut rt, Key::Home);
    settle(&mut rt);
    for _ in 0..30 {
        press(&mut rt, Key::Down);
    }
    settle(&mut rt);
    assert_eq!(rt.state.model.cursor(), Some("node-30"));
    assert_selected_row_in_viewport(&rt, "baseline node-30 in 200pt viewport");
    let scroll_before = rt.state.scroll_y;

    // Resize viewport/container while cursor and selection remain unchanged.
    rt.state.container_h = 300.0;
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    settle(&mut rt);
    assert_eq!(rt.state.model.cursor(), Some("node-30"), "cursor unchanged by viewport resize");
    assert_eq!(
        rt.state.scroll_y, scroll_before,
        "cursor unchanged across viewport/layout resize must not jump scroll when row remains in view"
    );
    assert_selected_row_in_viewport(&rt, "after viewport resize, cursor unchanged");
}
