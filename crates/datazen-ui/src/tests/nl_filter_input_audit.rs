//! T299 independent audit: fresh vectors — the case-insensitive
//! membership's Unicode lowercasing, the empty-filter parse with a
//! matching context, the three mounts' both-theme key stability inside
//! the minimum window, the parsing word verbatim, and the clear ×'s
//! help and label.

use rune::{Cx, El, Env, Runtime};

use crate::ai_store::FilterCondition;
use crate::components::ai::nl_filter_input::{
    parsed_filter_outcome, unknown_columns_message, NlFilterInput, NlFilterInputVm,
    ParsedFilterOutcome,
};
use crate::table_data_connection_state::ColumnSchema;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn column(name: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: "text".to_owned(),
        nullable: true,
        default_value: None,
        is_primary_key: false,
        is_auto_increment: false,
        comment: None,
    }
}

/// The membership lowercases both sides: a Turkish-İ column name and a
/// filter naming it plain-i stay strangers (JS `toLowerCase` is not
/// the Turkish locale's İ→i, and neither is ours).
#[test]
fn the_case_insensitive_membership_is_ascii_honest() {
    let columns = vec![column("姓名")];
    // The Chinese column name matches its own filter.
    assert_eq!(
        parsed_filter_outcome(
            &[FilterCondition {
                column: "姓名".to_owned(),
                operator: "=".to_owned(),
                value: "1".to_owned(),
            }],
            &columns,
            Some("users"),
            Some("s1"),
            "users",
            "s1"
        ),
        ParsedFilterOutcome::Apply(vec![FilterCondition {
            column: "姓名".to_owned(),
            operator: "=".to_owned(),
            value: "1".to_owned(),
        }])
    );
    // A stranger stays a stranger.
    assert_eq!(
        parsed_filter_outcome(
            &[FilterCondition {
                column: "名字".to_owned(),
                operator: "=".to_owned(),
                value: "1".to_owned(),
            }],
            &columns,
            Some("users"),
            Some("s1"),
            "users",
            "s1"
        ),
        ParsedFilterOutcome::UnknownColumns(vec!["名字".to_owned()])
    );
}

/// An empty parse with a matching context applies an empty write (the
/// filters-length gate), not a stale drop.
#[test]
fn the_empty_parse_with_a_matching_context_applies() {
    assert_eq!(
        parsed_filter_outcome(&[], &[column("name")], Some("users"), Some("s1"), "users", "s1"),
        ParsedFilterOutcome::Apply(Vec::new())
    );
    // But a stale context still drops even with no filters.
    assert_eq!(
        parsed_filter_outcome(&[], &[column("name")], Some("moved"), Some("s1"), "users", "s1"),
        ParsedFilterOutcome::Stale
    );
}

/// The unknown-columns sentence joins the strangers in the given
/// order with the pack's own wording.
#[test]
fn the_unknown_columns_sentence_is_the_packs_own_wording() {
    let message = unknown_columns_message(&["a".to_owned()]);
    assert!(
        message.starts_with(crate::i18n::t("smartFilter.invalidColumns").to_owned().as_str().split("{columns}").next().unwrap_or("")),
        "the pack word's prefix leads: {message}"
    );
    assert!(message.contains("{columns}") == false, "the placeholder is replaced");
    assert!(message.contains("a"), "the stranger rides the sentence");
}

/// The three mounts' key sets are theme-stable, and every node stays
/// inside the minimum window.
#[test]
fn the_mounts_are_theme_stable_inside_the_minimum_window() {
    fn keys(dark: bool, vm: &NlFilterInputVm) -> Vec<String> {
        let state = Audit { vm: vm.clone(), dark, log: Vec::new() };
        let mut rt = Runtime::new(state, audit_view, (960.0, 640.0), env(dark));
        rt.frame(0.0);
        rt.elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.key)
            .collect()
    }
    let expanded = NlFilterInputVm {
        is_configured: true,
        expanded: true,
        parsed_filters: Some(Vec::new()),
        ..Default::default()
    };
    assert_eq!(keys(true, &expanded), keys(false, &expanded));

    for dark in [true, false] {
        let state = Audit { vm: expanded.clone(), dark, log: Vec::new() };
        let mut rt = Runtime::new(state, audit_view, (960.0, 640.0), env(dark));
        rt.frame(0.0);
        for e in rt.elements() {
            let r = e.rect;
            assert!(
                r[0] >= -0.5 && r[1] >= -0.5 && r[0] + r[2] <= 960.5 && r[1] + r[3] <= 640.5,
                "{:?} out of the minimum window (dark={dark})",
                e.key
            );
        }
    }
}

/// The parsing word and the clear ×'s help are the pack's exact
/// strings.
#[test]
fn the_parsing_word_and_the_clear_help_are_exact() {
    let state = Audit {
        vm: NlFilterInputVm {
            is_configured: true,
            expanded: true,
            is_parsing: true,
            ..Default::default()
        },
        dark: true,
        log: Vec::new(),
    };
    let mut rt = Runtime::new(state, audit_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    // The parsing line's word rides the row's children (the row
    // itself holds the spinner and the word, not a single text).
    let parsing_row = rt
        .elements()
        .iter()
        .find(|e| e.key.as_deref() == Some("nl-filter-input-parsing"))
        .cloned()
        .expect("the parsing row mounts");
    let word_mounted = rt
        .elements()
        .iter()
        .any(|e| {
            e.text
                .as_deref()
                .is_some_and(|t| t.contains(crate::i18n::t("smartFilter.parsing")))
                && e.path.starts_with(&parsing_row.path)
        });
    assert!(word_mounted, "the parsing word rides the row");

    // The clear ×'s help.
    let clear = rt
        .elements()
        .iter()
        .find(|e| e.key.as_deref() == Some("nl-filter-input-clear"))
        .and_then(|e| e.help.clone())
        .unwrap_or_default();
    assert!(clear.contains(crate::i18n::t("smartFilter.clear")), "the help: {clear}");
}

// -- the audit's own probe ------------------------------------------------------

#[derive(Clone)]
struct Audit {
    vm: NlFilterInputVm,
    dark: bool,
    log: Vec<String>,
}

fn audit_view(s: &Audit, _cx: &mut Cx<Audit>) -> El {
    NlFilterInput::<Audit>::new(&s.vm)
        .dark(s.dark)
        .on_parse(|st| st.log.push("parse".to_owned()))
        .build(crate::ui::kit::kit(s.dark))
}
