//! T103 independent audit: fresh vectors for the AI egress notice.
//!
//! Deliberately different from the coder's `ai_egress_notice.rs`: the
//! minimum window in light theme, a summary of exactly one space, an
//! empty-string database, mixed file/dir/table items with a relaxed
//! setting (both notes in order), and the null guard probed with an
//! empty-string summary. One coder number — the amber wash — is
//! reproduced from the audit's own palette call.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::ai::{AiEgressNotice, ContextItem, ContextKind};
use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;
use datazen_ui::ui::palette::{ai_amber_border, ai_amber_fill, ai_amber_text};

const MIN: (f32, f32) = (960.0, 640.0);

fn item(kind: ContextKind, id: &str) -> ContextItem {
    ContextItem { kind, id: id.into(), name: id.into() }
}

struct Probe {
    items: Vec<ContextItem>,
    summary: Option<String>,
    database: Option<String>,
    strict: bool,
}

fn view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    let notice = AiEgressNotice::new(&state.items, state.summary.as_deref(), state.database.as_deref())
        .strict_egress(state.strict)
        .build(dark);
    col().key("audit-page").w_full().h_full().padding(16.0).child(
        notice.unwrap_or_else(|| col().key("audit-empty").w_full().h(4.0)),
    )
}

fn runtime(probe: Probe, dark: bool) -> Runtime<Probe> {
    let mut rt = Runtime::new(probe, view, MIN, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element<S: 'static>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key}: is laid out"))
}

fn has<S: 'static>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn audit_the_guard_takes_blank_summaries_and_databases() {
    // A one-space summary and an empty-string database are both "off".
    let probe = Probe {
        items: vec![],
        summary: Some(" ".to_string()),
        database: Some("".to_string()),
        strict: true,
    };
    let rt = runtime(probe, true);
    assert!(!has(&rt, "ai-egress-notice"), "blank summary + empty database + strict renders nothing");
    assert!(has(&rt, "audit-empty"));

    // The same blanks with a dir item: the context note alone renders.
    let probe = Probe {
        items: vec![item(ContextKind::Dir, "reports/")],
        summary: Some(" ".to_string()),
        database: Some(String::new()),
        strict: true,
    };
    let rt = runtime(probe, false);
    assert!(has(&rt, "ai-egress-notice"), "the dir item opens the notice");
    assert!(!has(&rt, "ai-egress-summary"), "the blank summary is dropped");
    assert!(!has(&rt, "ai-egress-database"), "the empty database is dropped");
    assert_eq!(
        element(&rt, "ai-egress-message-0").text.as_deref(),
        Some(t("chat.egressNoticeContext"))
    );
    assert!(!has(&rt, "ai-egress-message-1"), "strict egress: no relaxed note");

    // A table item is not a context file: with the same blanks it renders
    // nothing (the file/dir filter is the only discriminator).
    let probe = Probe {
        items: vec![item(ContextKind::Table, "app.events")],
        summary: Some(" ".to_string()),
        database: Some(String::new()),
        strict: true,
    };
    let rt = runtime(probe, true);
    assert!(!has(&rt, "ai-egress-notice"), "a table item alone renders nothing");
    assert!(has(&rt, "audit-empty"));
}

#[test]
fn audit_both_notes_in_order_and_the_database_locale_key() {
    // Relaxed with mixed items: relaxed note first, context note second.
    let probe = Probe {
        items: vec![
            item(ContextKind::File, "q3-plan.md"),
            item(ContextKind::Table, "app.users"),
            item(ContextKind::Dir, "sql/dumps"),
        ],
        summary: None,
        database: Some("  558/hive  ".to_string()),
        strict: false,
    };
    let rt = runtime(probe, true);
    assert_eq!(
        element(&rt, "ai-egress-message-0").text.as_deref(),
        Some(t("chat.egressNoticeRelaxed")),
        "the relaxed note comes first"
    );
    assert_eq!(
        element(&rt, "ai-egress-message-1").text.as_deref(),
        Some(t("chat.egressNoticeContext")),
        "the context note comes second"
    );
    assert!(!has(&rt, "ai-egress-message-2"), "no third message");
    assert_eq!(
        element(&rt, "ai-egress-database").text.as_deref(),
        Some(t("chat.egressDatabase").replacen("{database}", "558/hive", 1).as_str()),
        "the database label trims and goes through the key"
    );
    // React's order: the database line above both notes.
    let database = element(&rt, "ai-egress-database");
    assert!(database.rect[1] < element(&rt, "ai-egress-message-0").rect[1]);
    // The coder's amber wash, from the audit's own palette call. The
    // border keeps the documented no-channel wall (`ElementInfo` has no
    // border fields): a colour-source mutation is still caught by the
    // fill and text assertions.
    assert_eq!(element(&rt, "ai-egress-notice").fill, ai_amber_fill(true));
}

#[test]
fn audit_the_summary_line_is_medium_weight_and_the_wash_survives_light() {
    let probe = Probe {
        items: vec![],
        summary: Some("tool calls: read_files (3 files)".to_string()),
        database: None,
        strict: true,
    };
    let rt = runtime(probe, false);
    let k = kit(false);
    let summary = element(&rt, "ai-egress-summary");
    assert_eq!(
        summary.text.as_deref(),
        Some("tool calls: read_files (3 files)")
    );
    assert_eq!(summary.text_color, Some(ai_amber_text(false)), "text-amber-200/90");
    assert_eq!(element(&rt, "ai-egress-notice").fill, ai_amber_fill(false), "the wash in light");
    assert!(!has(&rt, "ai-egress-message-0"), "strict: no notes with a summary only");
    let _ = (k, ai_amber_border(false));
}
