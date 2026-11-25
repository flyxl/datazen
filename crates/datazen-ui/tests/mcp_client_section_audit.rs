//! T336 audit: the McpClientSection port, verified against the React
//! source re-read line by line
//! (`windows/settings/McpClientSection.tsx` — its `__tests__/
//! McpClientSection.test.tsx` covers the React side; the expectations
//! below are re-derived from the JSX and handlers, plus the pure-rule
//! tests the React suite pins). Element evidence comes from
//! `Runtime::elements()`, presses go through real pointer events (the
//! `ai_settings_section_audit` harness pattern).

use std::rc::Rc;

use datazen_ui_api::WebMcpToolInfo;
use rune::{col, El, Env, Runtime, TextEdit};

use datazen_ui::views::mcp_client_section::McpClientSection;
use datazen_ui::views::mcp_client_section_state::{
    env_to_rows, is_valid_mcp_server_id, rows_to_env, EnvRowEdit, McpClientSectionState,
    McpServerConfigVm, SavePlan,
};

fn config(id: &str, name: &str, enabled: bool) -> McpServerConfigVm {
    McpServerConfigVm {
        id: id.to_owned(),
        name: name.to_owned(),
        enabled,
        ..Default::default()
    }
}

/// A connected server (`McpClientStatus`).
fn server(id: &str, name: &str, tools: usize) -> datazen_ui::views::mcp_client_section_state::McpClientStatus {
    datazen_ui::views::mcp_client_section_state::McpClientStatus {
        server_id: id.to_owned(),
        server_name: name.to_owned(),
        tool_count: tools,
    }
}

fn tool(server_id: &str, tool_name: &str) -> WebMcpToolInfo {
    WebMcpToolInfo {
        server_id: server_id.to_owned(),
        server_name: server_id.to_owned(),
        tool_name: tool_name.to_owned(),
        qualified_name: format!("mcp/{server_id}/{tool_name}"),
        description: Some(format!("{tool_name} description")),
        input_schema: serde_json::Value::Null,
    }
}

#[derive(Clone)]
struct AuditState {
    section: McpClientSectionState,
    dark: bool,
    // the host's records
    connects: Vec<String>,
    disconnects: Vec<String>,
    toggles: Vec<(String, bool)>,
    edits: Vec<String>,
    deletes: Vec<String>,
    adds: usize,
    saves: Vec<SavePlan>,
    cancels: usize,
    add_envs: usize,
    removes: Vec<usize>,
    browses: usize,
    clears: usize,
    server_error_clears: Vec<String>,
    tool_toggles: Vec<String>,
    id_edits: usize,
}

impl AuditState {
    fn empty() -> Self {
        Self {
            section: McpClientSectionState::new(),
            dark: true,
            connects: Vec::new(),
            disconnects: Vec::new(),
            toggles: Vec::new(),
            edits: Vec::new(),
            deletes: Vec::new(),
            adds: 0,
            saves: Vec::new(),
            cancels: 0,
            add_envs: 0,
            removes: Vec::new(),
            browses: 0,
            clears: 0,
            server_error_clears: Vec::new(),
            tool_toggles: Vec::new(),
            id_edits: 0,
        }
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let section = McpClientSection::<AuditState>::new(
        &s.section,
        s.dark,
        // the form lenses
        |s: &mut AuditState| &mut s.section.draft.id,
        |s: &mut AuditState| &mut s.section.draft.name,
        |s: &mut AuditState| &mut s.section.draft.command,
        |s: &mut AuditState| &mut s.section.draft.args,
        Rc::new(|s: &mut AuditState, i: usize| {
            s.section.env_rows.get_mut(i).map(|r| &mut r.key).unwrap()
        }),
        Rc::new(|s: &mut AuditState, i: usize| {
            s.section.env_rows.get_mut(i).map(|r| &mut r.value).unwrap()
        }),
    )
    .on_connect(|s, id| s.connects.push(id))
    .on_disconnect(|s, id| s.disconnects.push(id))
    .on_toggle_enabled(|s, id, next| {
        s.toggles.push((id.clone(), next));
        s.section.saved_configs = s.section.toggle_enabled_plan(&id, next);
    })
    .on_draft_enabled(|s, next| s.section.set_draft_enabled(next))
    .on_draft_enabled_for_ai(|s, next| s.section.set_draft_enabled_for_ai(next))
    .on_edit(|s, id| {
        s.edits.push(id.clone());
        if let Some(found) = s.section.saved_configs.iter().find(|c| c.id == id).cloned() {
            s.section.start_edit(&found);
        }
    })
    .on_delete(|s, id| {
        s.deletes.push(id.clone());
        let plan = s.section.delete_plan(&id);
        s.section.saved_configs = plan.next;
        s.section.mcp_server_errors.remove(&plan.clear_server_error_id);
    })
    .on_add_server(|s| {
        s.adds += 1;
        s.section.start_add();
    })
    .on_save(|s| {
        let plan = s.section.begin_save();
        s.saves.push(plan.clone());
        if let SavePlan::Save { next, .. } = plan {
            s.section.saved_configs = next;
            s.section.note_save_ok();
        }
    })
    .on_cancel(|s| {
        s.cancels += 1;
        s.section.close_form();
    })
    .on_add_env(|s| {
        s.add_envs += 1;
        s.section.add_env_row();
    })
    .on_remove_env(|s, i| {
        s.removes.push(i);
        s.section.remove_env_row(i);
    })
    .on_browse_command(|s| s.browses += 1)
    .on_clear_mcp_error(|s| {
        s.clears += 1;
        s.section.mcp_error = None;
    })
    .on_clear_server_error(|s, id| {
        s.server_error_clears.push(id.clone());
        s.section.mcp_server_errors.remove(&id);
    })
    .on_toggle_tools(|s, id| {
        s.tool_toggles.push(id.clone());
        s.section.toggle_server_tools(&id);
    })
    .build(&mut layer);
    col().max_w(720.0).h_full().child(section).children(layer)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(scene, view, (960.0, 900.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements().into_iter().any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<AuditState>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
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

// ---- the pure rules ---------------------------------------------------------

/// `isValidMcpServerId`: `^[a-zA-Z0-9_-]+$`.
#[test]
fn the_id_rule_is_ascii_word_dash_underscore_nonempty() {
    assert!(is_valid_mcp_server_id("a"));
    assert!(is_valid_mcp_server_id("My-Server_01"));
    assert!(!is_valid_mcp_server_id(""), "empty is invalid");
    assert!(!is_valid_mcp_server_id("has space"));
    assert!(!is_valid_mcp_server_id("dot.id"));
    assert!(!is_valid_mcp_server_id("中文"));
    assert!(!is_valid_mcp_server_id("a/b"));
}

/// `rowsToEnv`: trimmed keys, empty keys dropped, later duplicates win;
/// `envToRows` round-trips.
#[test]
fn rows_to_env_trims_drops_empty_and_lets_the_last_row_win() {
    let rows = vec![
        EnvRowEdit { key: TextEdit::new("  API_KEY ".to_owned()), value: TextEdit::new("secret".to_owned()) },
        EnvRowEdit { key: TextEdit::new("  ".to_owned()), value: TextEdit::new("dropped".to_owned()) },
        EnvRowEdit { key: TextEdit::new(String::new()), value: TextEdit::new("dropped".to_owned()) },
        EnvRowEdit { key: TextEdit::new("API_KEY".to_owned()), value: TextEdit::new("winner".to_owned()) },
    ];
    let env = rows_to_env(&rows);
    let mut expected = std::collections::BTreeMap::new();
    expected.insert("API_KEY".to_owned(), "winner".to_owned());
    assert_eq!(env, expected, "trimmed, empties dropped, the later row wins");

    // envToRows round-trips (sorted key order — the documented delta).
    let back = env_to_rows(&env);
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].key.text, "API_KEY");
    assert_eq!(back[0].value.text, "winner");
}

/// `handleSave`'s walk: the guard, the id rule, the duplicate rule,
/// then the build.
#[test]
fn the_save_walk_validates_in_order_and_builds_verbatim() {
    let mut state = McpClientSectionState::new();
    state.saved_configs = vec![config("existing", "Existing", true)];

    // The handler's early return: no id, or no command.
    state.draft.id = TextEdit::new("  ".to_owned());
    assert!(matches!(state.begin_save(), SavePlan::Refused));
    state.draft.id = TextEdit::new("good-id".to_owned());
    state.draft.command = TextEdit::new("   ".to_owned());
    assert!(matches!(state.begin_save(), SavePlan::Refused));

    // The id rule.
    state.draft.command = TextEdit::new("/usr/local/bin/my-mcp".to_owned());
    state.draft.id = TextEdit::new("bad id!".to_owned());
    assert!(matches!(state.begin_save(), SavePlan::InvalidId));
    assert_eq!(state.save_error, Some("mcpClient.invalidId"));

    // The duplicate rule — only for FRESH configs.
    state.draft.id = TextEdit::new("existing".to_owned());
    assert!(matches!(state.begin_save(), SavePlan::DuplicateId));
    assert_eq!(state.save_error, Some("mcpClient.duplicateId"));
    state.editing_id = Some("existing".to_owned());
    state.draft.name = TextEdit::new(String::new());
    state.draft.args = TextEdit::new("--one\n  --two  \n\n--three".to_owned());
    state.env_rows = vec![EnvRowEdit {
        key: TextEdit::new("A".to_owned()),
        value: TextEdit::new("1".to_owned()),
    }];
    let SavePlan::Save { config, next } = state.begin_save() else {
        panic!("editing the same id saves");
    };
    // The build: id trimmed, name falls back to the trimmed id, args
    // split/trimmed/filtered, env from rows, command UNTRIMMED.
    assert_eq!(config.id, "existing");
    assert_eq!(config.name, "existing", "the empty name falls back to the id");
    assert_eq!(config.args, ["--one", "--two", "--three"]);
    assert_eq!(config.env.get("A").map(String::as_str), Some("1"));
    assert!(state.saving, "saving is armed");
    assert_eq!(state.save_error, None);
    // Editing replaces in place; the count does not grow.
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].id, "existing");

    // A FRESH config appends.
    state.note_save_ok();
    state.start_add();
    state.draft.id = TextEdit::new("fresh".to_owned());
    state.draft.command = TextEdit::new("node".to_owned());
    state.draft.name = TextEdit::new("  Named  ".to_owned());
    let SavePlan::Save { config, next } = state.begin_save() else {
        panic!("a fresh unique id saves");
    };
    assert_eq!(config.name, "Named", "a non-empty trimmed name is used trimmed");
    assert_eq!(next.len(), 2, "the fresh config appends");
    assert_eq!(next[1].id, "fresh");
}

/// The failure path: React has no catch — the `finally` disarms saving
/// and the form stays open.
#[test]
fn a_failed_save_keeps_the_form_open_and_disarms_saving() {
    let mut state = McpClientSectionState::new();
    state.start_add();
    state.draft.id = TextEdit::new("will-fail".to_owned());
    state.draft.command = TextEdit::new("node".to_owned());
    assert!(matches!(state.begin_save(), SavePlan::Save { .. }));
    assert!(state.saving);
    state.note_save_failed();
    assert!(!state.saving);
    assert!(state.show_form, "the form stays open");
    assert_eq!(state.draft.id.text, "will-fail", "the draft stays");
}

/// `save_disabled` is the button's rule; `id_invalid`/`id_duplicate`
/// are the derived hints.
#[test]
fn the_save_button_rule_and_the_derived_hints() {
    let mut state = McpClientSectionState::new();
    state.saved_configs = vec![config("dup", "Dup", true)];
    assert!(state.save_disabled(), "empty id disables");
    state.draft.id = TextEdit::new("ok".to_owned());
    assert!(state.save_disabled(), "empty command disables");
    state.draft.command = TextEdit::new("node".to_owned());
    assert!(!state.save_disabled());
    // The invalid hint only for a non-empty invalid id.
    assert!(!state.id_invalid());
    state.draft.id = TextEdit::new("not ok".to_owned());
    assert!(state.id_invalid());
    assert!(state.save_disabled());
    // The duplicate hint: fresh config with a colliding id.
    state.draft.id = TextEdit::new("dup".to_owned());
    assert!(state.id_duplicate());
    assert!(state.save_disabled());
    // Editing escapes the duplicate check.
    state.editing_id = Some("dup".to_owned());
    assert!(!state.id_duplicate());
    assert!(!state.save_disabled());
}

/// `handleDelete` and `handleToggleEnabled` build the next list.
#[test]
fn delete_and_toggle_enabled_build_the_next_list() {
    let mut state = McpClientSectionState::new();
    state.saved_configs = vec![config("a", "A", true), config("b", "B", false)];
    let plan = state.delete_plan("a");
    assert_eq!(plan.next.len(), 1);
    assert_eq!(plan.next[0].id, "b");
    assert_eq!(plan.clear_server_error_id, "a");
    let next = state.toggle_enabled_plan("b", true);
    assert!(next[0].enabled, "a stays");
    assert!(next[1].enabled, "b flips on");
}

/// The runtime projections.
#[test]
fn the_runtime_projections_read_the_store_state() {
    let mut state = McpClientSectionState::new();
    state.mcp_servers = vec![server("a", "A", 2)];
    state.mcp_tools = vec![tool("a", "t1"), tool("a", "t2"), tool("b", "t3")];
    state.mcp_connecting = true;
    state.mcp_connecting_server_id = Some("b".to_owned());
    assert!(state.is_connected("a"));
    assert!(!state.is_connected("b"));
    assert!(!state.is_connecting("a"));
    assert!(state.is_connecting("b"));
    assert!(!state.is_connecting("c"), "an unknown id is not connecting");
    assert_eq!(state.tools_for("a").len(), 2, "filtered by serverId");
    assert_eq!(state.tools_for("b")[0].tool_name, "t3");
    state.toggle_server_tools("a");
    assert!(state.expanded_servers.contains("a"));
    state.toggle_server_tools("a");
    assert!(!state.expanded_servers.contains("a"), "toggling collapses");
}

// ---- the element evidence ----------------------------------------------------

/// The mount shape: three headings, the two empty lines, the add
/// button.
#[test]
fn mount_renders_the_headings_and_empty_states() {
    let rt = rt(AuditState::empty());
    assert!(has(&rt, "mcp-client-title"));
    assert!(has(&rt, "mcp-saved-configs"));
    assert!(has(&rt, "mcp-runtime-title"));
    assert!(!has(&rt, "mcp-server-form"), "no form before the add press");
    assert!(has(&rt, "mcp-add-server"));
    assert!(!has(&rt, "mcp-saved-config-list"), "no list without configs");
}

/// One saved config renders its card, switch, and actions; the connect
/// button follows the connected / enabled / error rules.
#[test]
fn saved_configs_render_their_cards_and_actions() {
    let mut scene = AuditState::empty();
    scene.section.saved_configs = vec![
        config("alpha", "Alpha", true),
        config("beta", "Beta", false),
        config("gamma", "Gamma", true),
    ];
    // alpha: enabled, not connected, no error → connect.
    // beta: disabled → nothing.
    // gamma: enabled with a server error → reconnect.
    scene.section.mcp_server_errors.insert("gamma".to_owned(), "boom".to_owned());
    let rt = rt(scene);
    for id in ["alpha", "beta", "gamma"] {
        assert!(has(&rt, &format!("mcp-saved-config-{id}")), "{id} card");
        assert!(has(&rt, &format!("mcp-config-switch-{id}")), "{id} switch");
        assert!(has(&rt, &format!("mcp-config-edit-{id}")), "{id} edit");
        assert!(has(&rt, &format!("mcp-config-delete-{id}")), "{id} delete");
    }
    assert!(has(&rt, "mcp-config-connect-alpha"));
    assert!(!has(&rt, "mcp-config-reconnect-alpha"));
    assert!(!has(&rt, "mcp-config-connect-beta"), "a disabled config has no connect");
    assert!(has(&rt, "mcp-config-reconnect-gamma"), "the errored config reconnects");
    // The failed badge and the per-server error strip.
    assert!(has(&rt, "mcp-config-failed-gamma"));
    assert!(has(&rt, "mcp-config-error-gamma"));
    assert!(!has(&rt, "mcp-config-error-alpha"));
}

/// The global error strip renders and closes.
#[test]
fn the_global_error_strip_renders_and_closes() {
    let mut scene = AuditState::empty();
    scene.section.mcp_error = Some("connection lost".to_owned());
    let mut rt = rt(scene);
    assert!(has(&rt, "mcp-client-error"));
    press(&mut rt, "mcp-client-error-close");
    assert!(!has(&rt, "mcp-client-error"), "the close clears the strip");
    assert_eq!(rt.state.clears, 1);
}

/// The add flow: open, fill, save — and the invalid/duplicate guards.
#[test]
fn the_add_form_walks_the_full_path() {
    let mut rt = rt(AuditState::empty());
    press(&mut rt, "mcp-add-server");
    assert!(has(&rt, "mcp-server-form"));
    assert!(has(&rt, "mcp-server-id"));
    assert!(has(&rt, "mcp-server-name"));
    assert!(has(&rt, "mcp-server-command-input"), "the PathInput's input");
    assert!(has(&rt, "mcp-server-args"));
    assert!(has(&rt, "mcp-add-env"), "the env block's + button");
    assert!(!has(&rt, "mcp-env-row-0"), "no env rows yet");

    // Fill the draft (the host's edit machinery writes the TextEdits).
    rt.update(|s, _| {
        s.section.draft.id = TextEdit::new("my-server".to_owned());
        s.section.draft.name = TextEdit::new(String::new());
        s.section.draft.command = TextEdit::new("node".to_owned());
        s.section.draft.args = TextEdit::new("--stdio".to_owned());
    });
    // An env row.
    press(&mut rt, "mcp-add-env");
    assert!(has(&mut rt, "mcp-env-row-0"));
    rt.update(|s, _| {
        let row = s.section.env_rows.get_mut(0).unwrap();
        row.key = TextEdit::new("DEBUG".to_owned());
        row.value = TextEdit::new("1".to_owned());
    });

    press(&mut rt, "mcp-save");
    assert!(!has(&rt, "mcp-server-form"), "the form closed on save");
    assert!(has(&rt, "mcp-saved-config-my-server"), "the config is listed");
    assert_eq!(rt.state.saves.len(), 1);
    let SavePlan::Save { config, .. } = rt.state.saves[0].clone() else {
        panic!("the save plan is a Save");
    };
    assert_eq!(config.id, "my-server");
    assert_eq!(config.name, "my-server", "the empty name falls back");
    assert_eq!(config.args, ["--stdio"]);
    assert_eq!(config.env.get("DEBUG").map(String::as_str), Some("1"));
}

/// The duplicate guard surfaces inline; editing keeps the id frozen.
#[test]
fn the_duplicate_guard_surfaces_and_the_edit_seeds_the_form() {
    let mut scene = AuditState::empty();
    scene.section.saved_configs = vec![config("taken", "Taken", true)];
    let mut rt = rt(scene);
    press(&mut rt, "mcp-add-server");
    rt.update(|s, _| {
        s.section.draft.id = TextEdit::new("taken".to_owned());
        s.section.draft.command = TextEdit::new("node".to_owned());
    });
    settle(&mut rt);
    assert!(has(&rt, "mcp-server-form-error"), "the derived duplicate hint shows");
    // The save button is disabled under the duplicate guard — no press.
    assert!(has(&rt, "mcp-save"));
    // Editing the existing one seeds the form (id frozen by disabled).
    press(&mut rt, "mcp-config-edit-taken");
    assert!(has(&rt, "mcp-server-form"));
    assert_eq!(rt.state.section.draft.id.text, "taken", "the seed");
    assert!(!has(&rt, "mcp-server-form-error"), "editing escapes the duplicate check");
}

/// The env rows add and remove.
#[test]
fn env_rows_add_and_remove() {
    let mut scene = AuditState::empty();
    scene.section.start_add();
    scene.section.add_env_row();
    scene.section.add_env_row();
    let mut rt = rt(scene);
    assert!(has(&rt, "mcp-env-row-0"));
    assert!(has(&rt, "mcp-env-row-1"));
    assert!(has(&rt, "mcp-env-remove-1"));
    press(&mut rt, "mcp-env-remove-1");
    assert_eq!(rt.state.removes, [1], "the remove fired");
    assert!(!has(&rt, "mcp-env-row-1"), "the row is gone");
    assert!(has(&rt, "mcp-env-row-0"));
    assert_eq!(rt.state.removes, [1]);
}

/// The runtime list: expansion shows the tool inventory, the second
/// press collapses it.
#[test]
fn the_runtime_list_expands_the_tool_inventory() {
    let mut scene = AuditState::empty();
    scene.section.mcp_servers = vec![server("alpha", "Alpha", 2)];
    scene.section.mcp_tools = vec![tool("alpha", "query"), tool("alpha", "fetch")];
    let mut rt = rt(scene);
    assert!(has(&rt, "mcp-runtime-server-alpha"));
    assert!(has(&rt, "mcp-server-tools-toggle-alpha"));
    assert!(has(&rt, "mcp-server-disconnect-alpha"));
    assert!(!has(&rt, "mcp-server-tools-alpha"), "collapsed by default");
    press(&mut rt, "mcp-server-tools-toggle-alpha");
    assert!(has(&rt, "mcp-server-tools-alpha"));
    assert!(has(&rt, "mcp-server-tool-mcp/alpha/query"));
    assert!(has(&rt, "mcp-server-tool-mcp/alpha/fetch"));
    press(&mut rt, "mcp-server-tools-toggle-alpha");
    assert!(!has(&rt, "mcp-server-tools-alpha"), "collapsed again");
    // Disconnect fires through the slot.
    press(&mut rt, "mcp-server-disconnect-alpha");
    assert_eq!(rt.state.disconnects, ["alpha".to_owned()]);
}

/// A server with no tools shows the no-tools line when expanded.
#[test]
fn an_expanded_empty_server_shows_the_no_tools_line() {
    let mut scene = AuditState::empty();
    scene.section.mcp_servers = vec![server("empty", "Empty", 0)];
    let mut rt = rt(scene);
    press(&mut rt, "mcp-server-tools-toggle-empty");
    assert!(!has(&rt, "mcp-server-tools-empty"));
    // The no-tools text line is present (its text is the mcp pack's).
    assert!(rt.elements().into_iter().any(|e| !e.exiting
        && e.text.as_deref() == Some(datazen_ui::i18n::t("mcpClient.noTools"))));
}

/// The connect actions fire and respect the busy guard.
#[test]
fn connect_fires_through_the_slot() {
    let mut scene = AuditState::empty();
    scene.section.saved_configs = vec![config("alpha", "Alpha", true)];
    let mut rt = rt(scene);
    press(&mut rt, "mcp-config-connect-alpha");
    assert_eq!(rt.state.connects, ["alpha".to_owned()]);
}
