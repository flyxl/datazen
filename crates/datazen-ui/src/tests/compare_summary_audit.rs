//! T286 audit (tester): independent checks of the two components. Fresh
//! observation channels: the explain button's disabled flag through the
//! element's disabled channel, geometry at both window sizes, the light
//! theme build path, and the note's border/spacing presence through the
//! rect.

use rune::{col, Cx, El, Env, Runtime};

use crate::views::data_sync::compare_summary::CompareSummary;
use crate::views::data_sync::mapping_view::CompareSummaryStats;
use crate::views::data_transfer::transfer_pairing_note::TransferPairingNote;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn audit_stats() -> CompareSummaryStats {
    CompareSummaryStats {
        inserts: 5,
        updates: 6,
        deletes: 7,
        unchanged_tables: 8,
        incompatible: 1,
    }
}

#[derive(Default)]
struct AuditHost {
    explains: usize,
}

fn audit_view(host: &AuditHost, _cx: &mut Cx<AuditHost>) -> El {
    col().w_full().h_full().child(
        CompareSummary::<AuditHost>::new(audit_stats())
            .on_explain_diff(|s: &mut AuditHost| s.explains += 1)
            .on_copy_report(|_s: &mut AuditHost| {})
            .explain_loading(host.explains > 0)
            .build(true),
    )
}

fn audit_mount(host: AuditHost, size: (f32, f32)) -> Runtime<AuditHost> {
    let mut rt = Runtime::new(host, audit_view, size, env(true));
    rt.frame(0.0);
    rt
}

fn audit_element<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn audit_the_explain_button_is_disabled_while_loading() {
    // Not loading: the press reaches the host.
    let mut rt = audit_mount(AuditHost::default(), (960.0, 640.0));
    let (x, y) = rt.find("data-sync-explain-diff").expect("mounted");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.explains, 1, "the idle explain is pressable");

    // Loading: the disabled button never fires (the kit's disabled gate).
    let mut rt = audit_mount(AuditHost { explains: 1 }, (960.0, 640.0));
    let (x, y) = rt.find("data-sync-explain-diff").expect("mounted");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.explains, 1, "the loading explain stays silent");
}

#[test]
fn audit_geometry_both_sizes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = audit_mount(AuditHost::default(), (w, h));
        let mut outside = Vec::new();
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else { continue };
            if !key.starts_with("data-sync-summary") {
                continue;
            }
            let [x, _y, ew, _eh] = e.rect;
            if x < -0.5 || x + ew > w + 0.5 {
                outside.push(format!("{key} {:?}", e.rect));
            }
        }
        assert!(outside.is_empty(), "{w}x{h}: {}", outside.join("; "));
    }
}

#[test]
fn audit_the_note_renders_in_light_theme_with_margin() {
    // The note builds through the light kit without panic, and its rect
    // sits 16pt below the container's top (mt-4) with a real size.
    let view = |_: &(), _cx: &mut Cx<()>| -> El {
        col()
            .w_full()
            .h_full()
            .child(TransferPairingNote::new(None).build(false))
    };
    let mut rt = Runtime::new((), view, (960.0, 640.0), env(false));
    rt.frame(0.0);
    let note = audit_element(&rt, "data-transfer-path").expect("the note lays out");
    let [x, y, w, h] = note.rect;
    assert!(h > 0.0 && w > 0.0, "the note has a size: {:?}", note.rect);
    assert!(y >= 15.5 && y <= 16.5, "mt-4 = 16pt: y={y}");
    assert!(x >= -0.5, "the note starts at the left edge");
}
