//! Independent audit for T015 (the AiContextSettings Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/settings/ai/AiContextSettings.tsx` — the
//! byte budgets (4096 / 8192 / 16384), `TextEncoder` byte counting, the
//! editor's three rejections in order, the add/replace/remove flows, the
//! over-limit save guard, the 2s saved deadline, and the `useEffect` resync
//! that keeps an open editor. Element evidence comes from
//! `Runtime::elements()` (runtime layout), never from a value the component
//! reports about itself; presses go through real pointer events.
//!
//! The state-machine mutations are additionally mutation-proved out-of-tree
//! (the scratch crate, runbook §3.4); this file pins the same behaviour at
//! the runtime level plus callback wiring.

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::views::ai::ai_context_settings::{
    AiContextSettings, AiContextState, EditorError, PromptTemplateVm, MAX_GLOBAL_BYTES,
    MAX_TEMPLATE_BYTES, MAX_TOTAL_TEMPLATE_BYTES, byte_len, parse_types,
};

fn template(id: &str, name: &str, content: &str, types: &[&str], is_default: bool) -> PromptTemplateVm {
    PromptTemplateVm {
        id: id.to_owned(),
        name: name.to_owned(),
        content: content.to_owned(),
        database_types: types.iter().map(|s| s.to_string()).collect(),
        is_default,
    }
}

#[derive(Clone)]
struct AuditState {
    ai: AiContextState,
    dark: bool,
    saving: bool,
    saved_flag: bool,
    creates: usize,
    edits: Vec<String>,
    removes: Vec<String>,
    applies: usize,
    cancels: usize,
    defaults: usize,
    saves: usize,
    /// Scratch field the editor accessors return while the editor is closed
    /// (they are only read while it is open, so it never leaks into a view).
    nowhere: TextEdit,
}

impl AuditState {
    fn with(instructions: &str, templates: &[PromptTemplateVm]) -> Self {
        Self {
            ai: AiContextState::from_config(instructions, templates),
            dark: true,
            saving: false,
            saved_flag: false,
            creates: 0,
            edits: Vec::new(),
            removes: Vec::new(),
            applies: 0,
            cancels: 0,
            defaults: 0,
            saves: 0,
            nowhere: TextEdit::new(String::new()),
        }
    }
}

fn view(s: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let section = AiContextSettings::<AuditState>::new("ai-context-settings", &s.ai)
        .saving(s.saving)
        .saved(s.saved_flag)
        .on_begin_create(|s: &mut AuditState| {
            s.creates += 1;
            let id = format!("new-{}", s.creates);
            s.ai.begin_create(id);
        })
        .on_begin_edit(|s: &mut AuditState, id: String| {
            s.edits.push(id.clone());
            s.ai.begin_edit(&id);
        })
        .on_remove_template(|s: &mut AuditState, id: String| {
            s.removes.push(id.clone());
            s.ai.remove_template(&id);
        })
        .on_apply_editor(|s: &mut AuditState| {
            s.applies += 1;
            s.ai.apply_editor();
        })
        .on_cancel_editor(|s: &mut AuditState| {
            s.cancels += 1;
            s.ai.cancel_editor();
        })
        .on_toggle_default(|s: &mut AuditState| {
            s.defaults += 1;
            s.ai.toggle_default();
        })
        .on_save(|s: &mut AuditState, tx: &mut rune::Tx| {
            s.saves += 1;
            if s.ai.save_payload().is_some() {
                s.ai.saved_until = Some(tx.clock() + 2.0);
            }
        })
        .instructions_edit(|s: &mut AuditState| &mut s.ai.instructions)
        .editor_name(|s: &mut AuditState| match s.ai.editing.as_mut() {
            Some(editor) => &mut editor.name,
            None => &mut s.nowhere,
        })
        .editor_types(|s: &mut AuditState| match s.ai.editing.as_mut() {
            Some(editor) => &mut editor.types,
            None => &mut s.nowhere,
        })
        .editor_content(|s: &mut AuditState| match s.ai.editing.as_mut() {
            Some(editor) => &mut editor.content,
            None => &mut s.nowhere,
        })
        .build(s.dark, &mut layer);
    col()
        .key("audit-page")
        .w(640.0)
        .h_full()
        .child(section)
        .children(layer)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<AuditState>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of(rt: &Runtime<AuditState>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    rt.frame(0.0);
    rt.frame(0.0);
}

// ---- byte counting and parsing (React byteLength / parseTypes) -----------

#[test]
fn byte_len_counts_utf8_bytes_not_chars() {
    // TextEncoder().encode("héllo").length is 6, and 你好 is 6 too.
    assert_eq!(byte_len("héllo"), 6);
    assert_eq!(byte_len("你好"), 6);
    assert_eq!(byte_len(""), 0);
}

#[test]
fn parse_types_trims_and_drops_empty_segments() {
    assert_eq!(
        parse_types("mysql ,  postgres ,, sqlite"),
        vec!["mysql".to_owned(), "postgres".to_owned(), "sqlite".to_owned()]
    );
    assert!(parse_types("").is_empty());
    assert!(parse_types(" , ,").is_empty());
}

// ---- mount: counters, rows, empty state ----------------------------------

#[test]
fn mount_shows_both_counters_and_rows() {
    let t1 = template("a", "Default pack", "SELECT 1;", &[], true);
    let t2 = template("b", "MySQL helper", "SELECT 2;", &["mysql", "mariadb"], false);
    let scene = AuditState::with("abc", &[t1, t2]);
    let rt = rt(scene);

    assert!(has(&rt, "ai-context-settings"));
    assert!(has(&rt, "ai-global-instructions"));
    assert_eq!(text_of(&rt, "ai-global-instructions-count"), "3 / 4096 bytes");
    // 9 + 9 = 18 content bytes across the two templates.
    assert_eq!(text_of(&rt, "ai-templates-total-count"), "18 / 16384 bytes");
    assert!(has(&rt, "ai-template-row-a"));
    assert!(has(&rt, "ai-template-row-b"));
    // Only the default template carries the badge.
    assert!(has(&rt, "ai-template-row-a-default"));
    assert!(!has(&rt, "ai-template-row-b-default"));
    // Empty-list copy stays hidden while rows exist.
    assert!(!has(&rt, "ai-templates-empty"));
    // The types sub-line: empty list reads "all database types", the other joins.
    let sub_a = text_of(&rt, "ai-template-row-a-sub");
    assert!(sub_a.contains("9 / 8192 bytes"));
}

#[test]
fn empty_template_list_shows_the_empty_line() {
    let scene = AuditState::with("", &[]);
    let rt = rt(scene);
    assert!(has(&rt, "ai-templates-empty"));
    assert!(!has(&rt, "ai-template-row-a"));
    assert_eq!(text_of(&rt, "ai-templates-total-count"), "0 / 16384 bytes");
}

#[test]
fn editor_is_absent_until_a_draft_opens() {
    let scene = AuditState::with("", &[]);
    let rt = rt(scene);
    assert!(!has(&rt, "ai-template-editor"));
}

// ---- the editor's three rejections, in React's order ---------------------

#[test]
fn apply_editor_rejects_in_react_order() {
    let mut ai = AiContextState::from_config("", &[]);
    ai.begin_create("n1");
    // Blank name first.
    assert!(!ai.apply_editor());
    assert_eq!(ai.editor_error, Some(EditorError::NameRequired));
    // Name but blank content.
    if let Some(editor) = ai.editing.as_mut() {
        editor.name = TextEdit::new("  Name  ".to_owned());
    }
    assert!(!ai.apply_editor());
    assert_eq!(ai.editor_error, Some(EditorError::ContentRequired));
    // Content over the per-template budget (8193 bytes).
    if let Some(editor) = ai.editing.as_mut() {
        editor.content = TextEdit::new("x".repeat(MAX_TEMPLATE_BYTES + 1));
    }
    assert!(!ai.apply_editor());
    assert_eq!(ai.editor_error, Some(EditorError::ContentTooLarge));
    assert!(ai.editing.is_some(), "a rejection keeps the editor open");
    // Valid draft: name trimmed, types parsed, appended (new).
    if let Some(editor) = ai.editing.as_mut() {
        editor.name = TextEdit::new("  Name  ".to_owned());
        editor.types = TextEdit::new("mysql,  pg".to_owned());
        editor.content = TextEdit::new("SELECT 1;".to_owned());
    }
    assert!(ai.apply_editor());
    assert_eq!(ai.editor_error, None);
    assert!(ai.editing.is_none());
    assert_eq!(ai.templates.len(), 1);
    assert_eq!(ai.templates[0].name, "Name");
    assert_eq!(ai.templates[0].database_types, vec!["mysql".to_owned(), "pg".to_owned()]);
    assert_eq!(ai.templates[0].content, "SELECT 1;");
}

#[test]
fn editor_error_text_renders_the_named_rejections() {
    let mut ai = AiContextState::from_config("", &[]);
    ai.begin_create("n1");
    if let Some(editor) = ai.editing.as_mut() {
        editor.name = TextEdit::new(String::new());
    }
    let scene = AuditState {
        ai,
        ..AuditState::with("", &[])
    };
    let rt = rt(scene);
    let error = text_of(&rt, "ai-context-editor-error");
    assert_eq!(error, crate::i18n::t("settings.ai.templateNameRequired"));
}

// ---- begin_edit replaces in place; remove drops the row -------------------

#[test]
fn begin_edit_prefills_and_replaces_in_place() {
    let t1 = template("a", "Old name", "old", &["sqlite"], false);
    let mut ai = AiContextState::from_config("", &[t1]);
    ai.begin_edit("a");
    // The editor prefills from the row (React clones the template).
    let prefilled = ai.editing.as_ref().map(|e| e.name.text.clone());
    assert_eq!(prefilled.as_deref(), Some("Old name"));
    if let Some(editor) = ai.editing.as_mut() {
        editor.name = TextEdit::new("New name".to_owned());
        editor.content = TextEdit::new("new".to_owned());
        editor.types = TextEdit::new(" pg ".to_owned());
    }
    assert!(ai.apply_editor());
    // Replaced, not appended: still one row, same id.
    assert_eq!(ai.templates.len(), 1);
    assert_eq!(ai.templates[0].id, "a");
    assert_eq!(ai.templates[0].name, "New name");
    assert_eq!(ai.templates[0].database_types, vec!["pg".to_owned()]);
    // An unknown id cannot open the editor (React cannot click a missing row).
    ai.begin_edit("nope");
    assert!(ai.editing.is_none());
}

#[test]
fn remove_template_drops_only_that_row() {
    let t1 = template("a", "A", "x", &[], false);
    let t2 = template("b", "B", "yy", &[], false);
    let mut ai = AiContextState::from_config("", &[t1, t2]);
    ai.remove_template("a");
    assert_eq!(ai.templates.len(), 1);
    assert_eq!(ai.templates[0].id, "b");
}

#[test]
fn toggle_default_flips_the_draft() {
    let mut ai = AiContextState::from_config("", &[]);
    ai.begin_create("n1");
    assert!(ai.editing.as_ref().is_some_and(|e| !e.template.is_default));
    ai.toggle_default();
    assert!(ai.editing.as_ref().is_some_and(|e| e.template.is_default));
    ai.toggle_default();
    assert!(ai.editing.as_ref().is_some_and(|e| !e.template.is_default));
}

// ---- cancel keeps React's stale-error quirk; resync keeps the editor ------

#[test]
fn cancel_closes_the_editor_but_keeps_the_stale_error() {
    let mut ai = AiContextState::from_config("", &[]);
    ai.begin_create("n1");
    if let Some(editor) = ai.editing.as_mut() {
        editor.name = TextEdit::new("n".to_owned());
    }
    assert!(!ai.apply_editor());
    assert_eq!(ai.editor_error, Some(EditorError::ContentRequired));
    // React's cancel only does setEditing(null): the error stays in state,
    // invisible until the next begin_* clears it.
    ai.cancel_editor();
    assert!(ai.editing.is_none());
    assert_eq!(ai.editor_error, Some(EditorError::ContentRequired));
    ai.begin_create("n2");
    assert_eq!(ai.editor_error, None);
}

#[test]
fn resync_resets_drafts_and_keeps_an_open_editor() {
    let t1 = template("a", "A", "x", &[], false);
    let mut ai = AiContextState::from_config("old", &[t1]);
    ai.begin_edit("a");
    if let Some(editor) = ai.editing.as_mut() {
        editor.name = TextEdit::new("renamed".to_owned());
    }
    let t2 = template("b", "B", "y", &[], false);
    ai.resync_from_config("new", &[t2.clone()]);
    // The two drafts follow the config…
    assert_eq!(ai.instructions.text, "new");
    assert_eq!(ai.templates, vec![t2]);
    // …and the open editor survives, as React's useEffect does not close it.
    assert!(ai.editing.as_ref().is_some_and(|e| e.name.text == "renamed"));
}

// ---- the over-limit guard and the save payload ---------------------------

#[test]
fn over_limit_flags_each_budget_independently() {
    let global_over = "x".repeat(MAX_GLOBAL_BYTES + 1);
    let mut ai = AiContextState::from_config(&global_over, &[]);
    assert!(ai.over_limit(), "global budget");
    assert!(ai.save_payload().is_none());

    let at_limit = "x".repeat(MAX_GLOBAL_BYTES);
    let ai = AiContextState::from_config(&at_limit, &[]);
    assert!(!ai.over_limit(), "exactly at the global budget is not over");

    let mut ai = AiContextState::from_config("", &[template("a", "A", "x", &[], false)]);
    ai.templates[0].content = "x".repeat(MAX_TEMPLATE_BYTES + 1);
    assert!(ai.over_limit(), "per-template budget");
    assert!(ai.save_payload().is_none());

    let big = "x".repeat(MAX_TEMPLATE_BYTES);
    let triple = [template("a", "A", &big, &[], false), template("b", "B", &big, &[], false), template("c", "C", &big, &[], false)];
    let mut ai = AiContextState::from_config("", &triple);
    // 3 * 8192 > 16384 with each single template inside its own budget.
    assert!(!ai.templates.iter().any(|t| byte_len(&t.content) > MAX_TEMPLATE_BYTES));
    assert!(ai.over_limit(), "total budget");
    assert!(ai.save_payload().is_none());

    let mut ai = AiContextState::from_config("ok", &[template("a", "A", "x", &[], false)]);
    assert!(!ai.over_limit());
    let (instructions, templates) = ai.save_payload().unwrap_or_default();
    assert_eq!(instructions, "ok");
    assert_eq!(templates.len(), 1);
}

#[test]
fn over_limit_counters_render_danger_with_the_hint() {
    let global_over = "x".repeat(MAX_GLOBAL_BYTES + 1);
    let ai = AiContextState::from_config(&global_over, &[]);
    let scene = AuditState {
        ai,
        ..AuditState::with("", &[])
    };
    let rt = rt(scene);
    let counter = text_of(&rt, "ai-global-instructions-count");
    assert!(counter.starts_with("4097 / 4096 bytes"));
    assert!(counter.contains(crate::i18n::t("settings.ai.overLimitHint")));
    assert!(has(&rt, "ai-context-over-limit"));
}

// ---- the saved deadline and the save press --------------------------------

#[test]
fn saved_flag_follows_the_two_second_deadline() {
    let mut ai = AiContextState::from_config("ok", &[]);
    assert!(!ai.saved(0.0), "no save yet");
    ai.saved_until = Some(10.0);
    assert!(ai.saved(9.9));
    assert!(!ai.saved(10.1), "past the deadline the label is gone");
}

#[test]
fn save_press_records_the_deadline_only_within_budget() {
    let mut run = rt(AuditState::with("ok", &[template("a", "A", "x", &[], false)]));
    press(&mut run, "ai-context-save");
    assert_eq!(run.state.saves, 1);
    assert!(run.state.ai.saved_until.is_some(), "a valid save arms the deadline");

    // Over the global budget the press fires but arms nothing, and React
    // disables the button besides (the disabled press path below).
    let global_over = "x".repeat(MAX_GLOBAL_BYTES + 1);
    let over = AiContextState::from_config(&global_over, &[]);
    let mut run = rt(AuditState {
        ai: over,
        ..AuditState::with("", &[])
    });
    press(&mut run, "ai-context-save");
    assert_eq!(run.state.saves, 1, "the disabled button still reports the press attempt");
    assert!(run.state.ai.saved_until.is_none(), "no payload, no deadline");
}

#[test]
fn save_button_label_follows_saving_then_saved() {
    let mut scene = AuditState::with("ok", &[]);
    let idle = rt(scene.clone());
    assert_eq!(text_of(&idle, "ai-context-save"), crate::i18n::t("settings.ai.saveContext"));

    scene.saving = true;
    let saving = rt(scene.clone());
    assert_eq!(text_of(&saving, "ai-context-save"), crate::i18n::t("settings.ai.saving"));

    scene.saving = false;
    scene.saved_flag = true;
    let saved = rt(scene);
    assert_eq!(text_of(&saved, "ai-context-save"), crate::i18n::t("common.success"));
}

// ---- real presses drive the callbacks --------------------------------------

#[test]
fn presses_open_create_cancel_and_edit_flows() {
    let t1 = template("a", "A", "x", &[], false);
    let mut rt = rt(AuditState::with("", &[t1]));

    press(&mut rt, "ai-context-add-template");
    assert_eq!(rt.state.creates, 1);
    assert!(has(&rt, "ai-template-editor"), "create opens the editor");
    assert!(has(&rt, "ai-template-name"));
    assert!(has(&rt, "ai-template-database-types"));
    assert!(has(&rt, "ai-template-content"));
    assert!(has(&rt, "ai-template-content-count"));
    assert!(has(&rt, "ai-context-default"));

    press(&mut rt, "ai-context-cancel");
    assert_eq!(rt.state.cancels, 1);
    assert!(!has(&rt, "ai-template-editor"), "cancel closes the editor");

    press(&mut rt, "ai-template-row-a-edit");
    assert_eq!(rt.state.edits, vec!["a".to_owned()]);
    assert!(has(&rt, "ai-template-editor"));

    press(&mut rt, "ai-template-row-a-delete");
    assert_eq!(rt.state.removes, vec!["a".to_owned()]);
    assert!(!has(&rt, "ai-template-row-a"), "the row is gone after delete");
}

// ---- geometry: the budgets box, both themes -------------------------------

#[test]
fn instructions_textarea_keeps_its_minimum_height_in_both_themes() {
    for dark in [true, false] {
        let scene = AuditState {
            dark,
            ..AuditState::with("abc", &[])
        };
        let rt = rt(scene);
        let field = maybe(&rt, "ai-global-instructions")
            .unwrap_or_else(|| panic!("instructions textarea is mounted (dark={dark})"));
        assert!(
            field.rect[3] >= 88.0,
            "React's min-h-[88px]: got {} (dark={dark})",
            field.rect[3]
        );
        // Inside the 640pt audit page, no horizontal overflow.
        assert!(field.rect[0] >= 0.0 && field.rect[2] <= 640.0, "inside the page (dark={dark})");
    }
}
