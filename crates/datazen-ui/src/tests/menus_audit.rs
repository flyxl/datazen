use rune::{col, row, El, Env, Runtime};
use rune_kit::Placement;

use crate::ui::kit::kit;
use crate::ui::menu::{Item, Menu};

#[derive(Default)]
struct S {
    open: bool,
    close_calls: u32,
    selected: Vec<String>,
}

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn tall_menu(state: &S, _cx: &mut rune::Cx<S>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let mut menu = Menu::new("tall")
        .trigger(row().key("tall-trigger").size(40.0, 24.0))
        .open(state.open)
        .at(940.0, 620.0)
        .on_close(|s: &mut S| {
            s.close_calls += 1;
            s.open = false;
        });
    for i in 0..30 {
        let label = format!("Item {i}");
        let tag = format!("i{i}");
        menu = menu.item(Item::new(label).on_select(move |s: &mut S| {
            s.selected.push(tag.clone());
            s.open = false;
        }));
    }
    let root = menu.build(k, &mut layer);
    let mut out = col().key("tall-root").w_full().h_full().child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

/// 30 items anchored near bottom-right exceed viewport height (no clamping/scroll into bounds).
#[test]
fn thirty_items_at_edge_overflow_viewport() {
    let state = S {
        open: true,
        ..S::default()
    };
    let mut rt = Runtime::new(state, tall_menu, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(rt.find("tall-menu").is_some(), "menu surface present");
    let mut max_bottom: f32 = 0.0;
    let mut oob = Vec::new();
    for e in rt.elements() {
        let [x, y, w, h] = e.rect;
        if y + h > max_bottom {
            max_bottom = y + h;
        }
        if x < -0.5 || y < -0.5 || x + w > 960.5 || y + h > 640.5 {
            oob.push((e.key.clone(), e.rect));
        }
    }
    eprintln!(
        "AUDIT 30-items: max_bottom={max_bottom} viewport_h=640 oob_count={}",
        oob.len()
    );
    assert!(
        max_bottom > 640.0 && !oob.is_empty(),
        "expected 30-item menu to overflow viewport; max_bottom={max_bottom} oob={oob:?}"
    );
}

fn forced_placement_menu(state: &S, _cx: &mut rune::Cx<S>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let mut menu = Menu::new("edgef")
        .trigger(row().key("edgef-trigger").size(40.0, 24.0))
        .open(state.open)
        .at(940.0, 620.0)
        .placement(Placement::BottomStart)
        .min_width(260.0)
        .on_close(|s: &mut S| {
            s.close_calls += 1;
            s.open = false;
        });
    for i in 0..12 {
        menu = menu.item(Item::new(format!("Row {i}")));
    }
    let root = menu.build(k, &mut layer);
    let mut out = col().key("edgef-root").w_full().h_full().child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

/// Reverse probe: even with explicit BottomStart + wide menu at edge, anchoring keeps it in bounds.
#[test]
fn reverse_forced_placement_still_clamped_by_anchoring() {
    let state = S {
        open: true,
        ..S::default()
    };
    let mut rt = Runtime::new(state, forced_placement_menu, (960.0, 640.0), env());
    rt.frame(0.0);
    let mut oob = Vec::new();
    for e in rt.elements() {
        let [x, y, w, h] = e.rect;
        if x < -0.5 || y < -0.5 || x + w > 960.5 || y + h > 640.5 {
            oob.push((e.key.clone(), e.rect));
        }
    }
    eprintln!("AUDIT reverse-placement: oob_count={}", oob.len());
    assert!(
        oob.is_empty(),
        "anchoring clamped forced-placement menu; reverse did not produce overflow: {oob:?}"
    );
}

fn select_close_menu(state: &S, _cx: &mut rune::Cx<S>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let menu = Menu::new("sc")
        .trigger(row().key("sc-trigger").size(40.0, 24.0))
        .open(state.open)
        .at(100.0, 100.0)
        .item(Item::new("Pick").on_select(|s: &mut S| {
            s.selected.push("pick".into());
            // intentionally do NOT set open=false in on_select
        }))
        .on_close(|s: &mut S| {
            s.close_calls += 1;
            s.open = false;
        });
    let root = menu.build(k, &mut layer);
    let mut out = col().key("sc-root").w_full().h_full().child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

/// Selecting an item runs on_select but does NOT auto-close or trigger on_close by itself.
#[test]
fn select_does_not_auto_close_or_call_on_close() {
    let state = S {
        open: true,
        ..S::default()
    };
    let mut rt = Runtime::new(state, select_close_menu, (960.0, 640.0), env());
    rt.frame(0.0);
    let (x, y) = rt.find("sc-item0").expect("item0");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    eprintln!(
        "AUDIT select-close: close_calls={} open={} selected={:?}",
        rt.state.close_calls, rt.state.open, rt.state.selected
    );
    assert_eq!(rt.state.selected, vec!["pick".to_string()]);
    assert_eq!(rt.state.close_calls, 0, "on_close not triggered by selection");
    assert!(rt.state.open, "open remains true unless on_select/on_close sets it");
}

fn select_and_close_menu(state: &S, _cx: &mut rune::Cx<S>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let menu = Menu::new("sc2")
        .trigger(row().key("sc2-trigger").size(40.0, 24.0))
        .open(state.open)
        .at(100.0, 100.0)
        .item(Item::new("Pick").on_select(|s: &mut S| {
            s.selected.push("pick".into());
            s.open = false; // also sets open in handler
        }))
        .on_close(|s: &mut S| {
            s.close_calls += 1;
            s.open = false;
        });
    let root = menu.build(k, &mut layer);
    let mut out = col().key("sc2-root").w_full().h_full().child(root);
    for el in layer {
        out = out.child(el);
    }
    out
}

/// on_select can close by mutating state; on_close is not invoked implicitly by selection.
#[test]
fn select_handler_can_close_without_triggering_on_close() {
    let state = S {
        open: true,
        ..S::default()
    };
    let mut rt = Runtime::new(state, select_and_close_menu, (960.0, 640.0), env());
    rt.frame(0.0);
    let (x, y) = rt.find("sc2-item0").expect("item0");
    rt.pointer_down(x + 2.0, y + 2.0);
    rt.pointer_up(x + 2.0, y + 2.0);
    rt.frame(0.0);
    eprintln!(
        "AUDIT select+handler-close: close_calls={} open={} selected={:?}",
        rt.state.close_calls, rt.state.open, rt.state.selected
    );
    assert_eq!(rt.state.selected, vec!["pick".to_string()]);
    assert_eq!(rt.state.close_calls, 0, "on_close not triggered by selection");
    assert!(!rt.state.open);
}

fn two_instances(state: &S, _cx: &mut rune::Cx<S>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let m1: Menu<S> = Menu::new("same")
        .trigger(row().key("t1").size(40.0, 24.0))
        .open(state.open)
        .at(50.0, 50.0)
        .item(Item::new("A"));
    let m2: Menu<S> = Menu::new("same")
        .trigger(row().key("t2").size(40.0, 24.0))
        .open(state.open)
        .at(300.0, 50.0)
        .item(Item::new("B"));
    let r1 = m1.build(k, &mut layer);
    let r2 = m2.build(k, &mut layer);
    let mut out = col().key("two-root").w_full().h_full().child(r1).child(r2);
    for el in layer {
        out = out.child(el);
    }
    out
}

/// Two open menus sharing the same key collide; runtime renames later duplicates (#dupN).
#[test]
fn two_menus_same_key_collide_and_runtime_renames() {
    let state = S {
        open: true,
        ..S::default()
    };
    let mut rt = Runtime::new(state, two_instances, (960.0, 640.0), env());
    rt.frame(0.0);
    let menu_count = rt
        .elements()
        .iter()
        .filter(|e| e.key.as_deref() == Some("same-menu"))
        .count();
    let dup_menu_count = rt
        .elements()
        .iter()
        .filter(|e| {
            e.key
                .as_deref()
                .is_some_and(|k| k.starts_with("same-menu#dup"))
        })
        .count();
    eprintln!(
        "AUDIT dup-keys: same-menu={} same-menu#dup*={}",
        menu_count, dup_menu_count
    );
    assert_eq!(menu_count, 1, "original key present once");
    assert!(
        dup_menu_count >= 1,
        "duplicate instance renamed with #dupN suffix"
    );
}
