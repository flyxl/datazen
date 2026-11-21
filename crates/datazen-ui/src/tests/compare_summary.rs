//! T286: `TransferPairingNote` + `CompareSummary` ports — the note's
//! reason/generic word split, the summary's five counters, the
//! incompatible-conditional, the button mounts, the copied word, and the
//! loading gate.

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

fn stats(inserts: usize, incompatible: usize) -> CompareSummaryStats {
    CompareSummaryStats {
        inserts,
        updates: 2,
        deletes: 3,
        unchanged_tables: 4,
        incompatible,
    }
}

#[derive(Default)]
struct Host {
    copies: usize,
    explains: usize,
}

fn summary_view(host: &Host, _cx: &mut Cx<Host>) -> El {
    let mut summary = CompareSummary::<Host>::new(stats(1, 0));
    if host.copies >= 0 {
        summary = summary.on_copy_report(|s: &mut Host| s.copies += 1);
    }
    if host.explains >= 0 {
        summary = summary.on_explain_diff(|s: &mut Host| s.explains += 1);
    }
    col().w_full().h_full().child(summary.build(true))
}

fn mount_summary(host: Host) -> Runtime<Host> {
    let mut rt = Runtime::new(host, summary_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    rt
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[derive(Default)]
struct NoteHost {
    reason: Option<String>,
}

fn note_view(host: &NoteHost, _cx: &mut Cx<NoteHost>) -> El {
    col().w_full()
        .h_full()
        .child(TransferPairingNote::new(host.reason.clone()).build(true))
}

#[test]
fn the_note_shows_the_reason_when_one_exists() {
    let host = NoteHost {
        reason: Some("驱动组合不支持".to_owned()),
    };
    let mut rt = Runtime::new(host, note_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    assert_eq!(text_of(&rt, "data-transfer-path-text"), "驱动组合不支持");
}

#[test]
fn the_note_falls_back_to_the_generic_word() {
    for reason in [None, Some(String::new())] {
        let host = NoteHost { reason };
        let mut rt = Runtime::new(host, note_view, (960.0, 640.0), env(true));
        rt.frame(0.0);
        assert_eq!(
            text_of(&rt, "data-transfer-path-text"),
            crate::i18n::t("common.unsupportedPair").to_owned()
        );
    }
}

#[test]
fn the_summary_counters_render_in_the_pack_spellings() {
    let rt = mount_summary(Host::default());
    assert_eq!(text_of(&rt, "data-sync-summary-title"), "比较摘要");
    assert_eq!(text_of(&rt, "data-sync-summary-inserts"), "+1 插入");
    assert_eq!(text_of(&rt, "data-sync-summary-updates"), "~2 更新");
    assert_eq!(text_of(&rt, "data-sync-summary-deletes"), "−3 删除");
    assert_eq!(text_of(&rt, "data-sync-summary-unchanged"), "4 张表无差异");
    // Zero incompatible: the span does not mount.
    assert!(!has_key(&rt, "data-sync-summary-incompatible"));
}

#[test]
fn the_incompatible_span_mounts_only_when_non_zero() {
    let mut rt = Runtime::new(
        Host::default(),
        |_, _| {
            col().w_full().h_full().child(
                CompareSummary::<Host>::new(stats(1, 2))
                    .on_copy_report(|_s| {})
                    .build(true),
            )
        },
        (960.0, 640.0),
        env(true),
    );
    rt.frame(0.0);
    assert!(has_key(&rt, "data-sync-summary-incompatible"));
    assert_eq!(text_of(&rt, "data-sync-summary-incompatible"), "2 不兼容");
}

#[test]
fn both_buttons_mount_with_wirings() {
    let rt = mount_summary(Host::default());
    assert!(has_key(&rt, "data-sync-copy-report"));
    assert!(has_key(&rt, "data-sync-explain-diff"));
    // The copy label is the un-copied word by default (a button's label,
    // not its text).
    assert_eq!(
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("data-sync-copy-report"))
            .and_then(|e| e.label)
            .unwrap_or_default(),
        "复制报告"
    );
}

#[test]
fn the_copied_word_flips_the_label() {
    let mut rt = Runtime::new(
        Host::default(),
        |_, _| {
            col().w_full().h_full().child(
                CompareSummary::<Host>::new(stats(1, 0))
                    .copied(true)
                    .on_copy_report(|_s| {})
                    .build(true),
            )
        },
        (960.0, 640.0),
        env(true),
    );
    rt.frame(0.0);
    assert_eq!(
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("data-sync-copy-report"))
            .and_then(|e| e.label)
            .unwrap_or_default(),
        "已复制"
    );
}

#[test]
fn presses_reach_both_actions() {
    let mut rt = mount_summary(Host::default());
    click(&mut rt, "data-sync-copy-report");
    assert_eq!(rt.state.copies, 1);
    click(&mut rt, "data-sync-explain-diff");
    assert_eq!(rt.state.explains, 1);
}

#[test]
fn without_wirings_no_buttons_mount() {
    let mut rt = Runtime::new(
        Host::default(),
        |_, _| {
            col().w_full().h_full().child(CompareSummary::<Host>::new(stats(1, 0)).build(true))
        },
        (960.0, 640.0),
        env(true),
    );
    rt.frame(0.0);
    assert!(!has_key(&rt, "data-sync-copy-report"));
    assert!(!has_key(&rt, "data-sync-explain-diff"));
    assert!(!has_key(&rt, "data-sync-summary-actions"));
}
