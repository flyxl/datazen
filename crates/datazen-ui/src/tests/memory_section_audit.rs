//! T360 audit: the MemorySection port, verified against the React
//! source re-read line by line
//! (`windows/settings/MemorySection.tsx` — the expectations below are
//! re-derived from its JSX and handlers). Element evidence comes from
//! `Runtime::elements()`, presses go through real pointer events (the
//! `ai_settings_section_audit` harness pattern).

use std::rc::Rc;

use datazen_ui_api::{
    AiMemoryEntry, AiMemoryKind, AiMemorySettings, AiMemoryScopeKind, AiMemorySensitivity,
    AiMemorySource, AiMemoryStatus,
};
use rune::{col, El, Env, Runtime, TextEdit};

use crate::views::memory_section::MemorySection;
use crate::views::memory_section_state::{
    MemoryExportFormat, MemoryMergePlan, MemorySectionState, MEMORY_EXTRACTION_TOKEN_ESTIMATE,
    MEMORY_LOAD_DEBOUNCE, MEMORY_PAGE_LIMIT,
};

/// The edit lenses as fn items (the Option-borrow chain cannot be a
/// closure literal); the fallback arm covers a closed form (the
/// fields do not render then, so the lens never fires).
fn edit_subject_lens(s: &mut AuditState) -> &mut TextEdit {
    match s.section.editing.as_mut() {
        Some(edit) => &mut edit.subject,
        None => &mut s.edit_fallback,
    }
}

fn edit_content_lens(s: &mut AuditState) -> &mut TextEdit {
    match s.section.editing.as_mut() {
        Some(edit) => &mut edit.content,
        None => &mut s.edit_fallback,
    }
}

fn entry(id: &str, subject: &str, status: AiMemoryStatus, confidence: f64) -> AiMemoryEntry {
    AiMemoryEntry {
        id: id.to_owned(),
        scope_type: AiMemoryScopeKind::User,
        scope_id: "user-1".to_owned(),
        connection_id: Some("conn-a".to_owned()),
        database_name: None,
        schema_name: None,
        table_name: None,
        column_name: None,
        kind: AiMemoryKind::Preference,
        subject: subject.to_owned(),
        content: format!("{subject} content"),
        structured_json: None,
        source: AiMemorySource::Explicit,
        source_ref: None,
        confidence,
        sensitivity: AiMemorySensitivity::Normal,
        status,
        pinned: false,
        conflict: None,
        use_count: 0,
        last_used_at: None,
        expires_at: None,
        created_at: "2026-11-01T00:00:00Z".to_owned(),
        updated_at: "2026-11-01T00:00:00Z".to_owned(),
    }
}

fn memory() -> AiMemorySettings {
    AiMemorySettings {
        enabled: false,
        auto_learn: false,
        privacy_acknowledged: false,
        ..Default::default()
    }
}

#[derive(Clone)]
struct AuditState {
    section: MemorySectionState,
    dark: bool,
    now: f64,
    // the host's records
    toggles: Vec<bool>,
    enables_confirmed: usize,
    enables_cancelled: usize,
    auto_learns: Vec<bool>,
    policies: Vec<&'static str>,
    search_commits: usize,
    connection_filters: Vec<String>,
    kind_filters: Vec<String>,
    status_filters: Vec<String>,
    refreshes: usize,
    selection_toggles: Vec<String>,
    toggle_alls: usize,
    rows_updated: Vec<String>,
    confirm_alls: usize,
    deletes_one: Vec<String>,
    deletes_selected: usize,
    merges: usize,
    entry_confirms: Vec<String>,
    entry_rejects: Vec<String>,
    reviewed: Vec<String>,
    edits_begun: Vec<String>,
    edits_cancelled: usize,
    exports: Vec<MemoryExportFormat>,
    /// The edit lenses' closed-form arm (never read — the fields do
    /// not render without an open edit).
    edit_fallback: TextEdit,
}

impl AuditState {
    fn empty() -> Self {
        Self {
            section: MemorySectionState::new(memory()),
            dark: true,
            now: 100.0,
            toggles: Vec::new(),
            enables_confirmed: 0,
            enables_cancelled: 0,
            auto_learns: Vec::new(),
            policies: Vec::new(),
            search_commits: 0,
            connection_filters: Vec::new(),
            kind_filters: Vec::new(),
            status_filters: Vec::new(),
            refreshes: 0,
            selection_toggles: Vec::new(),
            toggle_alls: 0,
            rows_updated: Vec::new(),
            confirm_alls: 0,
            deletes_one: Vec::new(),
            deletes_selected: 0,
            merges: 0,
            entry_confirms: Vec::new(),
            entry_rejects: Vec::new(),
            reviewed: Vec::new(),
            edits_begun: Vec::new(),
            edits_cancelled: 0,
            exports: Vec::new(),
            edit_fallback: TextEdit::new(String::new()),
        }
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let connections: Vec<(String, String)> =
        vec![("conn-a".to_owned(), "Prod".to_owned())];
    let section = MemorySection::<AuditState>::new(
        &s.section,
        s.dark,
        s.now,
        &connections,
        |s: &mut AuditState| &mut s.section.search,
        edit_subject_lens,
        edit_content_lens,
    )
    .on_toggle_enabled(|s, next| {
        s.toggles.push(next);
        if s.section.toggle_enabled(next) {
            s.section.memory.enabled = next;
        }
    })
    .on_confirm_enable(|s| {
        s.enables_confirmed += 1;
        s.section.memory = s.section.confirm_enable();
    })
    .on_cancel_enable(|s| {
        s.enables_cancelled += 1;
        s.section.cancel_enable();
    })
    .on_set_auto_learn(|s, next| {
        s.auto_learns.push(next);
        s.section.memory.auto_learn = next;
    })
    .on_set_cloud_policy(|s, value| {
        s.policies.push(value);
        use datazen_ui_api::AiMemoryCloudPolicy as P;
        let policy = match value {
            "allow_internal" => P::AllowInternal,
            "disable_cloud" => P::DisableCloud,
            _ => P::NormalOnly,
        };
        s.section.memory = s.section.set_cloud_policy(policy);
    })
    .on_search_commit(|s| {
        s.search_commits += 1;
        s.section.note_filters_changed(s.now);
    })
    .on_set_connection_filter(|s, value| {
        s.connection_filters.push(value.clone());
        s.section.connection_filter = value;
        s.section.note_filters_changed(s.now);
    })
    .on_set_kind_filter(|s, value| {
        s.kind_filters.push(value.clone());
        s.section.kind_filter = value;
        s.section.note_filters_changed(s.now);
    })
    .on_set_status_filter(|s, value| {
        s.status_filters.push(value.clone());
        s.section.status_filter = value;
        s.section.note_filters_changed(s.now);
    })
    .on_refresh(|s| {
        s.refreshes += 1;
        s.section.refresh(s.now);
    })
    .on_toggle_selected(|s, id| {
        s.selection_toggles.push(id.clone());
        s.section.toggle_selected(&id);
    })
    .on_toggle_all(|s| {
        s.toggle_alls += 1;
        s.section.toggle_all();
    })
    .on_update_row(|s, id| {
        s.rows_updated.push(id.clone());
        s.section.note_row_updated(s.now);
    })
    .on_confirm_all(|s| {
        s.confirm_alls += 1;
        s.section.note_confirmed_all(s.now);
    })
    .on_delete_one(|s, id| {
        s.deletes_one.push(id.clone());
        s.section.note_deleted(&[id], s.now);
    })
    .on_delete_selected(|s| {
        s.deletes_selected += 1;
        let ids: Vec<String> = s.section.selected.iter().cloned().collect();
        s.section.note_deleted(&ids, s.now);
    })
    .on_merge_selected(|s| {
        s.merges += 1;
        if let MemoryMergePlan::Merge { merged_ids, .. } = s.section.merge_selected_plan() {
            s.section.note_merged(&merged_ids, s.now);
        }
    })
    .on_confirm_entry(|s, id| {
        s.entry_confirms.push(id.clone());
        s.section.refresh(s.now);
    })
    .on_reject_entry(|s, id| {
        s.entry_rejects.push(id.clone());
        s.section.refresh(s.now);
    })
    .on_mark_reviewed(|s, id| {
        s.reviewed.push(id.clone());
        s.section.refresh(s.now);
    })
    .on_begin_edit(|s, id| {
        s.edits_begun.push(id.clone());
        if let Some(found) = s.section.entries.iter().find(|e| e.id == id).cloned() {
            s.section.begin_edit(&found);
        }
    })
    .on_cancel_edit(|s| {
        s.edits_cancelled += 1;
        s.section.cancel_edit();
    })
    .on_export(|s, format| {
        s.exports.push(format);
        let _ = s.section.begin_export(format);
    })
    .on_select_toggle(|s, which| s.section.toggle_select(which))
    .on_select_close(|s| s.section.close_select())
    .build(&mut layer);
    col().max_w(720.0).h_full().child(section).children(layer)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    // The section is tall (settings + org + the entries list); a tall
    // viewport keeps the lower rows pressable.
    let mut rt = Runtime::new(scene, view, (960.0, 3200.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements().into_iter().any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    settle(rt);
}

// ---- the pure rules ----------------------------------------------------------

/// The debounced load plan: the search-vs-list face with the status
/// filter and the fixed limit.
#[test]
fn the_load_plan_picks_the_face_and_carries_the_filters() {
    let mut state = MemorySectionState::new(memory());
    state.refresh(100.0);
    assert!(!state.load_due(100.19), "not before the deadline");
    assert!(state.load_due(100.2), "due at 200ms");
    let plan = state.load_plan();
    // List face: no query; the limit rides; no filters set.
    assert!(matches!(plan[0], crate::views::memory_section_state::MemoryLoadPlan::List { .. }));
    assert!(matches!(plan[1], crate::views::memory_section_state::MemoryLoadPlan::Stats { .. }));
    let crate::views::memory_section_state::MemoryLoadPlan::List { request } = &plan[0] else {
        panic!("list face");
    };
    assert_eq!(request.value["limit"], MEMORY_PAGE_LIMIT);
    assert!(request.value.get("status").is_none());
    assert!(request.value.get("connectionId").is_none());

    // The search face: query + status; connection/kind do NOT ride
    // (the T12 ruling — they filter client-side).
    state.search = TextEdit::new("  pg  ".to_owned());
    state.status_filter = "candidate".to_owned();
    state.connection_filter = "conn-a".to_owned();
    state.kind_filter = "preference".to_owned();
    state.note_filters_changed(200.0);
    let plan = state.load_plan();
    let crate::views::memory_section_state::MemoryLoadPlan::Search { request, query } = &plan[0] else {
        panic!("search face");
    };
    assert_eq!(query, "pg", "the trimmed query");
    assert_eq!(request.value["query"], "pg");
    assert_eq!(request.value["status"], "candidate");
    assert!(request.value.get("connectionId").is_none(), "search does not carry the connection filter");
    assert!(request.value.get("kind").is_none());

    // The list face carries them.
    state.search = TextEdit::new(String::new());
    state.note_filters_changed(300.0);
    let plan = state.load_plan();
    let crate::views::memory_section_state::MemoryLoadPlan::List { request } = &plan[0] else {
        panic!("list face");
    };
    assert_eq!(request.value["connectionId"], "conn-a");
    assert_eq!(request.value["kind"], "preference");
    assert_eq!(request.value["status"], "candidate");
}

/// `apply_loaded`: the tombstone guard, the search-mode client
/// filters, the selection prune.
#[test]
fn apply_loaded_guards_tombstones_and_filters_in_search_mode() {
    let mut state = MemorySectionState::new(memory());
    // A deleted id never flashes back.
    state.deleted_ids.insert("gone".to_owned());
    state.selected.insert("kept".to_owned());
    state.selected.insert("dropped".to_owned());
    let page = vec![
        entry("gone", "Deleted", AiMemoryStatus::Active, 0.9),
        entry("kept", "Kept", AiMemoryStatus::Active, 0.9),
        entry("dropped", "Not on page", AiMemoryStatus::Active, 0.9),
    ];
    // Deliver WITHOUT dropped on the page: the selection prunes.
    state.apply_loaded(vec![page[0].clone(), page[1].clone()], false);
    assert_eq!(state.entries.len(), 1);
    assert_eq!(state.entries[0].id, "kept");
    assert!(state.selected.contains("kept"));
    assert!(!state.selected.contains("dropped"), "the selection prunes to the visible rows");

    // Search mode: the client-side connection/kind filters compose.
    state.connection_filter = "conn-b".to_owned();
    state.apply_loaded(
        vec![
            entry("a", "A", AiMemoryStatus::Active, 0.9),
            entry("b", "B", AiMemoryStatus::Active, 0.9),
        ],
        true,
    );
    assert!(state.entries.is_empty(), "conn-a entries drop under the conn-b filter");
    state.connection_filter = String::new();
    state.kind_filter = "glossary".to_owned();
    state.apply_loaded(
        vec![entry("a", "A", AiMemoryStatus::Active, 0.9)],
        true,
    );
    assert!(state.entries.is_empty(), "the kind filter applies in search mode too");
    // List mode does NOT client-filter (the IPC already did).
    state.kind_filter = String::new();
    state.apply_loaded(vec![entry("a", "A", AiMemoryStatus::Active, 0.9)], false);
    assert_eq!(state.entries.len(), 1);
}

/// The first-use gate, the candidate projections, the merge plan.
#[test]
fn the_first_use_gate_and_the_merge_plan() {
    let mut state = MemorySectionState::new(memory());
    // Enabling without the acknowledgement opens the card instead.
    assert!(!state.toggle_enabled(true));
    assert!(state.pending_enable);
    assert!(!state.memory.enabled, "no write yet");
    // The confirm writes enabled + acknowledged together.
    let next = state.confirm_enable();
    assert!(next.enabled && next.privacy_acknowledged);
    assert!(!state.pending_enable);
    // A later enable writes directly.
    state.memory = next;
    assert!(state.toggle_enabled(true));

    // The candidate projections.
    state.entries = vec![
        entry("c1", "C1", AiMemoryStatus::Candidate, 0.7),
        entry("c2", "C2", AiMemoryStatus::Candidate, 0.9),
        entry("a1", "A1", AiMemoryStatus::Active, 0.8),
        entry("c3", "C3", AiMemoryStatus::Candidate, 0.5),
    ];
    state.selected.insert("c1".to_owned());
    state.selected.insert("a1".to_owned());
    assert_eq!(state.candidate_entries().len(), 3);
    assert_eq!(state.selected_candidates().len(), 1, "a1 is not a candidate");
    assert_eq!(state.selected_candidates()[0].id, "c1");
    assert!(!state.all_selected());
    state.selected.insert("c2".to_owned());
    state.selected.insert("c3".to_owned());
    assert!(state.all_selected());

    // The merge plan: two chosen candidates merge; the merged content
    // dedups trimmed contents joined by a blank line, the first keeps
    // its subject, max confidence, active.
    let mut chosen_two = state.clone();
    chosen_two.entries = vec![
        entry("m1", "M1", AiMemoryStatus::Candidate, 0.7),
        entry("m2", "M2", AiMemoryStatus::Candidate, 0.9),
    ];
    chosen_two.entries[0].content = " one ".to_owned();
    chosen_two.entries[1].content = "two\n\n one ".to_owned();
    chosen_two.selected.insert("m1".to_owned());
    chosen_two.selected.insert("m2".to_owned());
    let MemoryMergePlan::Merge { update, delete_rest, merged_ids } =
        chosen_two.merge_selected_plan()
    else {
        panic!("two chosen candidates merge");
    };
    assert_eq!(update.value["id"], "m1");
    assert_eq!(update.value["patch"]["subject"], "M1", "the first keeps its subject");
    // The merged content: React trims the WHOLE content and dedups
    // exact matches — "two\n\n one" is its own trimmed value, not a
    // dup of "one".
    assert_eq!(update.value["patch"]["content"], "one\n\ntwo\n\n one", "whole-content trim + exact dedup");
    assert_eq!(update.value["patch"]["confidence"], 0.9, "max confidence");
    assert_eq!(update.value["patch"]["status"], "active");
    assert_eq!(delete_rest.len(), 1);
    assert_eq!(merged_ids, ["m1".to_owned(), "m2".to_owned()]);
    // Below two: nothing.
    let mut one = chosen_two.clone();
    one.selected.remove("m2");
    assert!(matches!(one.merge_selected_plan(), MemoryMergePlan::NeedTwo));
}

/// The delete tombstones and the connection options union.
#[test]
fn deletes_tombstone_and_connection_options_union() {
    let mut state = MemorySectionState::new(memory());
    state.entries = vec![
        entry("d1", "D1", AiMemoryStatus::Active, 0.9),
        entry("d2", "D2", AiMemoryStatus::Active, 0.9),
    ];
    state.selected.insert("d1".to_owned());
    state.note_deleted(&["d1".to_owned()], 100.0);
    assert_eq!(state.entries.len(), 1, "the row drops immediately");
    assert!(!state.selected.contains("d1"));
    assert!(state.deleted_ids.contains("d1"));
    // A racing refresh delivering the row again keeps it gone (the
    // page also carries the survivor).
    state.apply_loaded(
        vec![
            entry("d1", "D1", AiMemoryStatus::Active, 0.9),
            entry("d2", "D2", AiMemoryStatus::Active, 0.9),
        ],
        false,
    );
    assert_eq!(state.entries.len(), 1, "the tombstone guards the race");
    assert_eq!(state.entries[0].id, "d2");

    // connectionOptions: known ∪ the entries' unknown ids (id as label).
    state.entries.push({
        let mut e = entry("d3", "D3", AiMemoryStatus::Active, 0.9);
        e.connection_id = Some("unknown-conn".to_owned());
        e
    });
    let options = state.connection_options(&[("conn-a".to_owned(), "Prod".to_owned())]);
    assert!(options.contains(&("conn-a".to_owned(), "Prod".to_owned())));
    assert!(options.contains(&("unknown-conn".to_owned(), "unknown-conn".to_owned())));
}

/// The export notice and the constants.
#[test]
fn the_export_arms_the_notice() {
    assert_eq!(MEMORY_LOAD_DEBOUNCE, 0.2);
    assert_eq!(MEMORY_PAGE_LIMIT, 200);
    assert_eq!(MEMORY_EXTRACTION_TOKEN_ESTIMATE, 320);
    let mut state = MemorySectionState::new(memory());
    state.status_filter = "active".to_owned();
    let request = state.begin_export(MemoryExportFormat::Json);
    assert_eq!(state.notice.as_deref(), Some("settings.ai.memory.exportNotice"));
    assert_eq!(request.value["format"], "json");
    assert_eq!(request.value["status"], "active", "the CURRENT status filter rides");
    let _ = state.begin_export(MemoryExportFormat::Markdown);
    assert_eq!(MemoryExportFormat::Markdown.extension(), "md");
    assert_eq!(MemoryExportFormat::Json.extension(), "json");
    // The org load request rides the fixed limit.
    assert_eq!(state.org_load_request().value["limit"], 200);
}

// ---- the element evidence ------------------------------------------------------

/// The mount shape with an empty store.
#[test]
fn mount_renders_the_settings_card_and_empty_states() {
    let rt = rt(AuditState::empty());
    for key in [
        "ai-memory-settings",
        "ai-memory-enabled",
        "ai-memory-auto-learn",
        "ai-memory-cloud-policy-trigger",
        "ai-memory-disabled-note",
        "ai-org-memory-section",
        "ai-org-memory-note",
        "ai-org-memory-empty",
        "ai-memory-export-json",
        "ai-memory-export-markdown",
        "ai-memory-export-notice",
        "ai-memory-search",
        "ai-memory-filter-connection-trigger",
        "ai-memory-filter-kind-trigger",
        "ai-memory-filter-status-trigger",
        "ai-memory-refresh",
        "ai-memory-candidates-title",
        "ai-memory-candidate-count",
        "ai-memory-confirm-all",
        "ai-memory-empty",
        "ai-memory-privacy",
    ] {
        assert!(has(&rt, key), "{key} renders");
    }
    assert!(!has(&rt, "ai-memory-first-use"), "no card before the gate");
    assert!(!has(&rt, "ai-memory-auto-learn-cost"), "no cost line while auto-learn is off");
    assert!(!has(&rt, "ai-memory-list"), "no list without entries");
}

/// The first-use flow: the enable press opens the card, confirm
/// writes, the cost line follows auto-learn.
#[test]
fn the_first_use_card_flows() {
    let mut rt = rt(AuditState::empty());
    // The enable press opens the card (no write yet). The toggle
    // row's pressable is its switch element.
    press(&mut rt, "ai-memory-enabled-switch");
    assert!(has(&rt, "ai-memory-first-use"));
    assert!(!rt.state.section.memory.enabled, "no write yet");
    // Cancel closes without writing.
    press(&mut rt, "ai-memory-first-use-cancel");
    assert!(!has(&rt, "ai-memory-first-use"));
    // Again, this time confirm.
    press(&mut rt, "ai-memory-enabled-switch");
    press(&mut rt, "ai-memory-first-use-confirm");
    assert!(rt.state.section.memory.enabled);
    assert!(rt.state.section.memory.privacy_acknowledged);
    assert!(!has(&rt, "ai-memory-disabled-note"));
    // Auto-learn on shows the cost line.
    press(&mut rt, "ai-memory-auto-learn-switch");
    assert!(has(&rt, "ai-memory-auto-learn-cost"));
}

/// The rows: badges, the candidate actions, the stale badge, the
/// conflict badge, the edit flow.
#[test]
fn the_rows_render_their_badges_and_actions() {
    let mut scene = AuditState::empty();
    scene.section.memory.enabled = true;
    scene.section.entries = vec![
        entry("c1", "C1", AiMemoryStatus::Candidate, 0.7),
        entry("s1", "S1", AiMemoryStatus::Stale, 0.8),
    ];
    scene.section.entries[1].conflict = Some(true);
    let mut rt = rt(scene);
    assert!(has(&rt, "ai-memory-row-c1"));
    assert!(has(&rt, "ai-memory-select-c1"));
    assert!(has(&rt, "ai-memory-confirm-c1"));
    assert!(has(&rt, "ai-memory-reject-c1"));
    assert!(has(&rt, "ai-memory-edit-c1"));
    assert!(has(&rt, "ai-memory-delete-c1"));
    assert!(has(&rt, "ai-memory-stale-s1"));
    assert!(has(&rt, "ai-memory-conflict-s1"));
    assert!(has(&rt, "ai-memory-review-stale-s1"));
    assert!(!has(&rt, "ai-memory-confirm-s1"), "a stale row has no candidate actions");

    // The candidate confirm fires.
    press(&mut rt, "ai-memory-confirm-c1");
    assert_eq!(rt.state.entry_confirms, ["c1".to_owned()]);
    // The stale review fires.
    press(&mut rt, "ai-memory-review-stale-s1");
    assert_eq!(rt.state.reviewed, ["s1".to_owned()]);
    // The edit flow: pencil → form → save.
    press(&mut rt, "ai-memory-edit-c1");
    assert!(has(&rt, "ai-memory-edit-form-c1"));
    assert!(has(&rt, "ai-memory-edit-subject"));
    assert!(has(&rt, "ai-memory-edit-content"));
    press(&mut rt, "ai-memory-edit-save");
    assert_eq!(rt.state.rows_updated, ["c1".to_owned()]);
    assert!(!has(&rt, "ai-memory-edit-form-c1"), "the form closed");
    // The delete tombstones.
    press(&mut rt, "ai-memory-delete-s1");
    assert!(!has(&rt, "ai-memory-row-s1"));
    assert!(rt.state.section.deleted_ids.contains("s1"));
}

/// The select-all bar and delete-selected.
#[test]
fn the_select_all_bar_and_delete_selected() {
    let mut scene = AuditState::empty();
    scene.section.entries = vec![
        entry("a", "A", AiMemoryStatus::Active, 0.9),
        entry("b", "B", AiMemoryStatus::Active, 0.9),
    ];
    let mut rt = rt(scene);
    assert!(has(&rt, "ai-memory-select-all"));
    press(&mut rt, "ai-memory-select-a");
    press(&mut rt, "ai-memory-select-all");
    assert!(rt.state.section.all_selected(), "a direct + the all toggle select everything");
    press(&mut rt, "ai-memory-delete-selected");
    assert!(rt.state.section.entries.is_empty());
    assert!(has(&rt, "ai-memory-empty"), "the empty state shows");
}

/// The candidates bar: the count badge, confirm-all disabled without
/// candidates, show-candidates when the stats know more.
#[test]
fn the_candidates_bar_follows_its_rules() {
    let mut scene = AuditState::empty();
    scene.section.entries = vec![entry("a", "A", AiMemoryStatus::Active, 0.9)];
    scene.section.stats.by_status.insert("candidate".to_owned(), 5);
    let first = rt(scene.clone());
    // No candidates on the page, but the stats count them: the
    // show-candidates escape hatch appears.
    assert!(has(&first, "ai-memory-show-candidates"));
    // Confirm-all is disabled (no candidate rows).
    assert!(has(&first, "ai-memory-confirm-all"));

    // With two candidates both selected, merge-selected fires.
    scene.section.entries = vec![
        entry("c1", "C1", AiMemoryStatus::Candidate, 0.7),
        entry("c2", "C2", AiMemoryStatus::Candidate, 0.8),
    ];
    scene.section.selected.insert("c1".to_owned());
    scene.section.selected.insert("c2".to_owned());
    let mut rt = rt(scene);
    assert!(has(&rt, "ai-memory-merge-selected"));
    press(&mut rt, "ai-memory-merge-selected");
    assert_eq!(rt.state.merges, 1);
    // The merged-away row tombstones — the visible row drops only
    // when the debounced reload lands (React's refresh() ruling), so
    // the tombstone + the armed deadline are the evidence.
    assert!(rt.state.section.deleted_ids.contains("c2"));
    assert!(rt.state.section.load_deadline.is_some(), "the refresh armed");
}

/// The org list rows and the exports.
#[test]
fn the_org_list_and_exports() {
    let mut scene = AuditState::empty();
    scene.section.organization_entries = vec![entry("o1", "Org note", AiMemoryStatus::Active, 0.9)];
    let org = rt(scene.clone());
    assert!(has(&org, "ai-org-memory-row-o1"));
    assert!(!has(&org, "ai-org-memory-empty"));

    let mut rt = rt(scene);
    press(&mut rt, "ai-memory-export-json");
    press(&mut rt, "ai-memory-export-markdown");
    assert_eq!(rt.state.exports, [MemoryExportFormat::Json, MemoryExportFormat::Markdown]);
    assert!(has(&rt, "ai-memory-notice"), "the export notice shows");
}
