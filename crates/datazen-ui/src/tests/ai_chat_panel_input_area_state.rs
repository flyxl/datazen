//! T009-S5: `AiChatInputAreaState` pure logic (`AiChatPanel.tsx:777–949`).
//!
//! The state machine behind the input area, tested without a runtime:
//! the send gate, submit, conflict resolve / dismiss, snapshot-field
//! removal, and every show gate the builder reads. Rendering tests live
//! in `ai_chat_panel_input_area.rs`.

use crate::components::ai::{
    AiChatContextSnapshot, AiChatInputAreaState, AiChatResultPreview, AiInjection, AiLoadedSkill,
    AiPendingDraft, ConflictResolution,
};
use crate::i18n::t;

fn full_snapshot() -> AiChatContextSnapshot {
    AiChatContextSnapshot {
        editor_sql: Some("select 1".to_string()),
        selection: Some("x".to_string()),
        last_error: Some("boom".to_string()),
        result_preview: Some(AiChatResultPreview {
            columns: vec!["a".to_string()],
            rows: Vec::new(),
        }),
    }
}

fn full_injection() -> AiInjection {
    AiInjection {
        template_name: Some("Tmpl".to_string()),
        has_connection_notes: true,
        has_global_instructions: true,
    }
}

// ---- AiChatInputAreaState: pure logic ---------------------------------------

#[test]
fn the_fresh_vm_shows_nothing_and_cannot_send() {
    let state = AiChatInputAreaState::default();
    assert!(!state.show_egress_notice());
    assert!(!state.show_loaded_skills());
    assert!(!state.show_auto_context());
    assert!(!state.show_injection());
    assert!(!state.show_kv_chip());
    assert!(!state.show_conflict_bar());
    assert!(!state.can_send());
    assert_eq!(state.input_placeholder(), Some(t("chat.placeholder")));
}

#[test]
fn can_send_needs_nonblank_input_without_streaming() {
    let mut state = AiChatInputAreaState::default();
    state.set_input("   ");
    assert!(!state.can_send(), "whitespace-only is blank");
    state.set_input("select 1");
    assert!(state.can_send());
    state.set_streaming(true);
    assert!(
        !state.can_send(),
        "React disables the input while streaming"
    );
}

#[test]
fn submit_takes_trimmed_text_and_clears_the_field() {
    let mut state = AiChatInputAreaState::default();
    assert_eq!(state.submit(), None, "blank draft submits nothing");
    state.set_input("  hello  ");
    assert_eq!(state.submit().as_deref(), Some("hello"));
    assert_eq!(state.input_text(), "", "handleSend clears the input");
    state.set_input("hello");
    state.set_streaming(true);
    assert_eq!(state.submit(), None, "streaming submits nothing");
    assert_eq!(
        state.input_text(),
        "hello",
        "a refused submit keeps the draft"
    );
}

#[test]
fn resolve_conflict_replace_prefills_the_draft() {
    let mut state = AiChatInputAreaState::default();
    assert_eq!(state.resolve_conflict(ConflictResolution::Replace), None);
    state.set_input("keep me");
    state.set_pending_conflict(Some(AiPendingDraft::new("r1", "draft text")));
    assert_eq!(
        state
            .resolve_conflict(ConflictResolution::Replace)
            .as_deref(),
        Some("r1"),
        "the request id is returned so the caller can ack it"
    );
    assert_eq!(state.input_text(), "draft text");
    assert!(
        !state.show_conflict_bar(),
        "the bar goes away once resolved"
    );
}

#[test]
fn resolve_conflict_append_joins_with_the_separator() {
    let mut state = AiChatInputAreaState::default();
    state.set_input("keep me");
    state.set_pending_conflict(Some(AiPendingDraft::new("r1", "draft text")));
    assert_eq!(
        state
            .resolve_conflict(ConflictResolution::Append)
            .as_deref(),
        Some("r1")
    );
    let separator = t("chat.draft.appendSeparator");
    assert_eq!(
        state.input_text(),
        format!("keep me{separator}draft text"),
        "append is prev + separator + draft, exactly as React builds it"
    );
}

#[test]
fn dismiss_conflict_keeps_the_input() {
    let mut state = AiChatInputAreaState::default();
    state.set_input("keep me");
    state.set_pending_conflict(Some(AiPendingDraft::new("r1", "draft text")));
    state.dismiss_conflict();
    assert!(!state.show_conflict_bar());
    assert_eq!(state.input_text(), "keep me");
}

#[test]
fn remove_snapshot_field_drops_one_field_then_the_snapshot() {
    let mut state = AiChatInputAreaState::default();
    state.set_snapshot(Some(full_snapshot()));
    state.remove_snapshot_field("editorSql");
    assert!(state.show_auto_context(), "other fields keep the row");
    assert_eq!(state.snapshot().and_then(|s| s.editor_sql.as_deref()), None);
    assert!(state
        .snapshot()
        .and_then(|s| s.selection.as_deref())
        .is_some());
    state.remove_snapshot_field("selection");
    state.remove_snapshot_field("lastError");
    state.remove_snapshot_field("resultPreview");
    assert!(
        state.snapshot().is_none(),
        "removeSnapshotField nulls the snapshot once nothing with content is left"
    );
    assert!(!state.show_auto_context());
}

#[test]
fn remove_snapshot_field_ignores_unknown_fields() {
    let mut state = AiChatInputAreaState::default();
    state.set_snapshot(Some(full_snapshot()));
    state.remove_snapshot_field("nope");
    assert!(state.snapshot().is_some(), "unknown field is a no-op");
    state.remove_snapshot_field("editorSql");
    state.set_snapshot(None);
    state.remove_snapshot_field("editorSql");
    assert!(state.snapshot().is_none(), "no snapshot is a no-op too");
}

#[test]
fn egress_notice_needs_more_than_a_database_label() {
    let mut state = AiChatInputAreaState::default();
    state.set_egress(None, Some("mydb".to_string()), false, false);
    assert!(
        !state.show_egress_notice(),
        "React's gate never looks at the database on its own"
    );
    state.set_egress(Some("  ".to_string()), None, false, false);
    assert!(!state.show_egress_notice(), "a blank summary is no summary");
    state.set_egress(Some("summary".to_string()), None, false, false);
    assert!(state.show_egress_notice());
    state.set_egress(None, None, true, false);
    assert!(state.show_egress_notice(), "relaxed egress shows it");
    state.set_egress(None, None, false, true);
    assert!(state.show_egress_notice(), "attached context files show it");
}

#[test]
fn egress_messages_come_relaxed_then_context() {
    let mut state = AiChatInputAreaState::default();
    state.set_egress(None, None, true, true);
    assert_eq!(
        state.egress_messages(),
        vec![
            t("chat.egressNoticeRelaxed").to_string(),
            t("chat.egressNoticeContext").to_string(),
        ],
        "React pushes the relaxed note before the context note"
    );
    state.set_egress(None, None, false, true);
    assert_eq!(
        state.egress_messages(),
        vec![t("chat.egressNoticeContext").to_string()]
    );
}

#[test]
fn egress_database_label_formats_and_trims() {
    let mut state = AiChatInputAreaState::default();
    state.set_egress(None, Some("  mydb  ".to_string()), false, false);
    assert_eq!(
        state.egress_database_label().as_deref(),
        Some("数据库：mydb"),
        "the label is chat.egressDatabase with the trimmed name"
    );
    state.set_egress(None, Some("   ".to_string()), false, false);
    assert_eq!(state.egress_database_label(), None);
    state.set_egress(Some("  summary  ".to_string()), None, false, false);
    assert_eq!(state.egress_summary_text(), Some("summary"));
}

#[test]
fn injection_row_and_priority_gates() {
    let mut state = AiChatInputAreaState::default();
    assert!(!state.show_injection());
    state.set_injection(Some(AiInjection {
        template_name: Some("Tmpl".to_string()),
        has_connection_notes: false,
        has_global_instructions: false,
    }));
    assert!(state.show_injection());
    assert!(
        !state.injection().is_some_and(AiInjection::shows_priority),
        "no global instructions, no priority chip"
    );
    state.set_injection(Some(AiInjection {
        template_name: None,
        has_connection_notes: false,
        has_global_instructions: true,
    }));
    assert!(
        state.show_injection(),
        "global instructions alone still show the row"
    );
    assert!(
        !state.injection().is_some_and(AiInjection::shows_priority),
        "the priority chip needs a template beside the global instructions"
    );
    state.set_injection(Some(AiInjection {
        template_name: Some("  ".to_string()),
        has_connection_notes: false,
        has_global_instructions: false,
    }));
    assert!(
        !state.show_injection(),
        "a blank template name counts as no template"
    );
    state.set_injection(Some(AiInjection {
        template_name: Some("  ".to_string()),
        has_connection_notes: false,
        has_global_instructions: true,
    }));
    assert!(
        state.show_injection(),
        "global instructions alone still show the row with a blank template"
    );
    assert!(
        !state
            .injection()
            .is_some_and(AiInjection::shows_priority),
        "a blank template name counts as no template for the priority chip"
    );
    state.set_injection(Some(full_injection()));
    assert!(state.injection().is_some_and(AiInjection::shows_priority));
}

#[test]
fn kv_chip_needs_a_nonblank_key() {
    let mut state = AiChatInputAreaState::default();
    state.set_kv_key_name(Some("   ".to_string()));
    assert!(
        !state.show_kv_chip(),
        "buildKvAiContext returns null for an empty key"
    );
    state.set_kv_key_name(Some("user:1001".to_string()));
    assert!(state.show_kv_chip());
}

#[test]
fn loaded_skill_key_prefers_the_id() {
    assert_eq!(AiLoadedSkill::new("sql-review").id("s1").key(), "s1");
    assert_eq!(AiLoadedSkill::new("explain").key(), "explain");
    let mut state = AiChatInputAreaState::default();
    assert!(!state.show_loaded_skills());
    state.set_loaded_skills(vec![AiLoadedSkill::new("explain")]);
    assert!(state.show_loaded_skills());
}

#[test]
fn egress_getters_report_the_seeded_values() {
    let mut state = AiChatInputAreaState::default();
    state.set_egress(
        Some("summary".to_string()),
        Some("mydb".to_string()),
        true,
        true,
    );
    assert_eq!(state.egress_summary(), Some("summary"));
    assert_eq!(state.egress_database(), Some("mydb"));
    assert!(state.egress_relaxed());
    assert!(state.egress_has_context_files());
    state.set_context_items_count(3);
    assert_eq!(state.context_items_count(), 3);
}

#[test]
fn stored_only_ai_input_props_round_trip() {
    let mut state = AiChatInputAreaState::default();
    assert_eq!(state.db_session_id(), None);
    assert_eq!(state.database(), None);
    state.set_db_session_id(Some("sess-1".to_string()));
    state.set_database(Some("mydb".to_string()));
    assert_eq!(state.db_session_id(), Some("sess-1"));
    assert_eq!(state.database(), Some("mydb"));
    state.set_db_session_id(None);
    state.set_database(None);
    assert_eq!(state.db_session_id(), None);
    assert_eq!(state.database(), None);
}

#[test]
fn input_placeholder_hides_once_context_items_attach() {
    let mut state = AiChatInputAreaState::default();
    assert_eq!(state.input_placeholder(), Some(t("chat.placeholder")));
    state.set_context_items_count(2);
    assert_eq!(
        state.input_placeholder(),
        None,
        "AiInput suppresses the placeholder while context tokens are attached"
    );
}
