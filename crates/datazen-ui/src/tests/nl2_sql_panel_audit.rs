//! T280 independent audit: fresh vectors — the write action's
//! last-written bookkeeping across two generations, the guard's
//! whitespace-only input, the panel's key set stability across themes,
//! the not-configured bar's pack words verbatim, and the citation
//! journey at both themes.

use rune::{Cx, El, Env, Runtime};

use crate::components::ai::ai_input::AiInputState;
use crate::components::ai::chat_bubble::AiMemoryReference;
use crate::components::ai::nl2_sql_panel::{
    can_generate, preview_sql, sql_write_action, Nl2SqlPanel, Nl2SqlPanelVm,
};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn memory(reference: &str) -> AiMemoryReference {
    AiMemoryReference::new(reference, format!("id-{reference}"), "主题", "table", "database")
}

/// Two generations: the first SQL writes, the same SQL does not write
/// again, a NEW SQL writes again — the last-written cell the host keeps.
#[test]
fn the_write_action_bookkeeps_across_generations() {
    // First generation finishes: SELECT 1.
    assert_eq!(sql_write_action(false, "SELECT 1", ""), Some("SELECT 1".to_owned()));
    // Same SQL, already written: nothing.
    assert_eq!(sql_write_action(false, "SELECT 1", "SELECT 1"), None);
    // A new generation's SQL: writes again.
    assert_eq!(
        sql_write_action(false, "SELECT 2", "SELECT 1"),
        Some("SELECT 2".to_owned())
    );
    // The re-generation path: the streaming transition writes nothing.
    assert_eq!(sql_write_action(true, "SELECT 2", "SELECT 1"), None);
}

/// A whitespace-only input is a blank one (React's `trim()`).
#[test]
fn a_whitespace_input_is_blank() {
    assert!(can_generate("查询", false, "db"));
    assert!(!can_generate(" \t\n ", false, "db"), "whitespace-only refuses");
    // The preview derivation: a generated SQL with only whitespace is
    // still a preview (the check is on the empty string, not trim).
    let vm = Nl2SqlPanelVm { generated_sql: " ".to_owned(), ..Default::default() };
    assert_eq!(preview_sql(&vm), Some(" ".to_owned()));
}

/// Both themes mount the same key set for the same VM.
#[test]
fn the_key_set_is_theme_stable() {
    fn keys(dark: bool) -> Vec<String> {
        let state = AuditProbe {
            vm: Nl2SqlPanelVm {
                is_configured: true,
                generated_sql: "SELECT 1".to_owned(),
                memory_used: vec![memory("M1")],
                ..Default::default()
            },
            input: AiInputState::new(),
            dark,
            log: Vec::new(),
        };
        let mut rt = Runtime::new(state, audit_view, (960.0, 640.0), env(dark));
        rt.frame(0.0);
        rt.elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect()
    }
    assert_eq!(keys(true), keys(false));
}

/// The unconfigured bar's words are the pack's exact strings.
#[test]
fn the_unconfigured_words_are_exact() {
    let state = AuditProbe::unconfigured();
    let mut rt = Runtime::new(state, audit_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    let text = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("nl2sql-panel-not-configured-text"))
        .and_then(|e| e.text);
    assert_eq!(text, Some("请先在设置中配置 AI 服务".to_owned()));
    let settings = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("nl2sql-panel-open-settings"))
        .and_then(|e| e.label);
    assert_eq!(settings, Some("前往配置".to_owned()));
}

/// The citation journey at both themes: the chip toggles, the citation
/// focuses its reference, and the expanded row's detail carries the
/// kind and the scope.
#[test]
fn the_citation_journey_at_both_themes() {
    for dark in [true, false] {
        let state = AuditProbe {
            vm: Nl2SqlPanelVm {
                is_configured: true,
                memory_used: vec![memory("M1")],
                memory_open: true,
                ..Default::default()
            },
            input: AiInputState::new(),
            dark,
            log: Vec::new(),
        };
        let mut rt = Runtime::new(state, audit_view, (960.0, 640.0), env(dark));
        rt.frame(0.0);
        // The expanded row's detail: the kind and the scope.
        let detail = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("ai-memory-item-M1-detail"))
            .and_then(|e| e.text);
        assert_eq!(detail, Some("table · database".to_owned()), "the detail joins kind and scope (dark={dark})");
        // The citation press focuses the reference.
        let citation = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("ai-memory-citation-M1") && !e.exiting)
            .expect("the citation is laid out")
            .rect;
        let (x, y) = (citation[0] + citation[2] * 0.5, citation[1] + citation[3] * 0.5);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        rt.frame(1.0 / 60.0);
        assert_eq!(
            rt.state.log.last().map(String::as_str),
            Some("focus:M1"),
            "the citation focuses its reference (dark={dark})"
        );
    }
}

// -- the audit's own probe -----------------------------------------------------

#[derive(Clone)]
struct AuditProbe {
    vm: Nl2SqlPanelVm,
    input: AiInputState,
    dark: bool,
    log: Vec<String>,
}

impl AuditProbe {
    fn unconfigured() -> Self {
        Self {
            vm: Nl2SqlPanelVm::default(),
            input: AiInputState::new(),
            dark: true,
            log: Vec::new(),
        }
    }
}

fn audit_view(s: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    Nl2SqlPanel::<AuditProbe>::new(&s.vm, &s.input, "db")
        .on_generate(|st| st.log.push("generate".to_owned()))
        .on_focus_memory(|st, reference| st.log.push(format!("focus:{reference}")))
        .build(crate::ui::kit::kit(s.dark), "")
}
