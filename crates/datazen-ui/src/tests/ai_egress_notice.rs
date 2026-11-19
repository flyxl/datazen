//! T103: the AI egress notice — the amber strip above the chat input.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::ai::{AiEgressNotice, ContextItem, ContextKind};
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::palette::{ai_amber_border, ai_amber_fill, ai_amber_text};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn item(kind: ContextKind, id: &str) -> ContextItem {
    ContextItem {
        kind,
        id: id.to_string(),
        name: id.to_string(),
    }
}

struct Probe {
    items: Vec<ContextItem>,
    summary: Option<String>,
    database: Option<String>,
    strict: bool,
}

fn view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    let notice = AiEgressNotice::new(
        &state.items,
        state.summary.as_deref(),
        state.database.as_deref(),
    )
    .strict_egress(state.strict)
    .build(dark);
    col().key("page").w_full().h_full().padding(24.0).child(
        notice.unwrap_or_else(|| col().key("empty").w_full().h(4.0)),
    )
}

fn runtime(probe: Probe, size: (f32, f32), dark: bool) -> Runtime<Probe> {
    let mut rt = Runtime::new(probe, view, size, Env { system_dark: dark, scale: 1.0 });
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
fn nothing_renders_when_strict_without_files_or_summary() {
    // Strict, no context items, no summary: React's null.
    let probe = Probe { items: vec![], summary: None, database: None, strict: true };
    let rt = runtime(probe, DESIGN, true);
    assert!(!has(&rt, "ai-egress-notice"), "the null return mounts nothing");
    assert!(has(&rt, "empty"), "the caller's null-arm placeholder shows");

    // A whitespace-only summary counts as empty (`egressSummary?.trim()`).
    let probe = Probe {
        items: vec![],
        summary: Some("   \n\t ".to_string()),
        database: None,
        strict: true,
    };
    let rt = runtime(probe, DESIGN, true);
    assert!(!has(&rt, "ai-egress-summary"), "a blank summary is not shown");

    // A database alone is NOT enough: React's guard checks relaxed /
    // context / summary only.
    let probe = Probe { items: vec![], summary: None, database: Some("olap".into()), strict: true };
    let rt = runtime(probe, DESIGN, true);
    assert!(!has(&rt, "ai-egress-notice"), "a database alone does not open the notice");
}

#[test]
fn the_summary_database_and_relaxed_message_render_in_react_order() {
    // Relaxed strict egress, a summary with surrounding whitespace, and a
    // database label: the summary first, the database second, the
    // relaxed message third.
    let probe = Probe {
        items: vec![],
        summary: Some("  3 tables, 1 file (12 KB)  ".to_string()),
        database: Some("  analytics  ".to_string()),
        strict: false,
    };
    for dark in [true, false] {
        let rt = runtime(Probe { ..probe.clone_items() }, DESIGN, dark);
        assert!(has(&rt, "ai-egress-notice"));
        let summary = element(&rt, "ai-egress-summary");
        assert_eq!(summary.text.as_deref(), Some("3 tables, 1 file (12 KB)"), "the summary is trimmed");
        let database = element(&rt, "ai-egress-database");
        assert_eq!(
            database.text.as_deref(),
            Some(t("chat.egressDatabase").replacen("{database}", "analytics", 1).as_str()),
            "the database label goes through the locale key"
        );
        assert_eq!(
            element(&rt, "ai-egress-message-0").text.as_deref(),
            Some(t("chat.egressNoticeRelaxed")),
            "the relaxed note is the only message"
        );
        assert!(!has(&rt, "ai-egress-message-1"), "no context note without files");
        // React's order: summary above database above messages.
        let root = element(&rt, "ai-egress-notice");
        let k = kit(dark);
        assert_eq!(root.fill, ai_amber_fill(dark), "bg-amber-500/10");
        assert_eq!(root.radius, k.radius.sm, "rounded");
        assert!((summary.rect[1] - (root.rect[1] + 6.0)).abs() <= 0.5, "py-1.5 above the summary");
        assert!(
            database.rect[1] > summary.rect[1] + summary.rect[3],
            "the database line sits under the summary"
        );
        assert!(
            element(&rt, "ai-egress-message-0").rect[1] > database.rect[1] + database.rect[3],
            "the message sits under the database line"
        );
        let _ = ai_amber_border(dark);
    }
}

impl Probe {
    fn clone_items(&self) -> Probe {
        Probe {
            items: self.items.clone(),
            summary: self.summary.clone(),
            database: self.database.clone(),
            strict: self.strict,
        }
    }
}

#[test]
fn file_and_dir_items_open_the_context_note_and_tables_do_not() {
    // A table item is not a context file: alone it renders nothing.
    let probe = Probe {
        items: vec![item(ContextKind::Table, "public.users")],
        summary: None,
        database: None,
        strict: true,
    };
    let rt = runtime(probe, DESIGN, true);
    assert!(!has(&rt, "ai-egress-notice"), "a table context item alone renders nothing");

    // Files and dirs do, with the context note.
    let probe = Probe {
        items: vec![
            item(ContextKind::Table, "public.users"),
            item(ContextKind::Dir, "src/sql/"),
            item(ContextKind::File, "notes.txt"),
        ],
        summary: None,
        database: None,
        strict: true,
    };
    let rt = runtime(probe, DESIGN, false);
    assert!(has(&rt, "ai-egress-notice"), "a file or dir opens the notice");
    assert_eq!(
        element(&rt, "ai-egress-message-0").text.as_deref(),
        Some(t("chat.egressNoticeContext")),
        "the context note is the only message"
    );
    assert!(!has(&rt, "ai-egress-summary"), "no summary line without a summary");
    assert!(!has(&rt, "ai-egress-database"), "no database line without a database");
}

#[test]
fn the_notice_fits_the_minimum_window_in_both_themes() {
    // Everything on: the notice stays inside the minimum window.
    for dark in [true, false] {
        let probe = Probe {
            items: vec![item(ContextKind::File, "big-plan.md")],
            summary: Some("rows from 3 tables will be sampled".to_string()),
            database: Some("trading_dev".to_string()),
            strict: false,
        };
        let rt = runtime(probe, MIN, dark);
        let root = element(&rt, "ai-egress-notice");
        assert!(root.rect[0] >= 24.0 - 0.5, "left inside the page padding");
        assert!(root.rect[1] >= 24.0 - 0.5);
        assert!(root.rect[0] + root.rect[2] <= MIN.0 - 24.0 + 0.5);
        assert!(root.rect[1] + root.rect[3] <= MIN.1 - 24.0 + 0.5);
        // All three lines are present in React's order.
        for key in ["ai-egress-summary", "ai-egress-database", "ai-egress-message-0", "ai-egress-message-1"] {
            assert!(has(&rt, key), "{key} mounts with everything on");
        }
        assert_eq!(element(&rt, "ai-egress-message-1").text.as_deref(), Some(t("chat.egressNoticeContext")));
    }
}
