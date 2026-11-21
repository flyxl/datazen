use crate::{AiChatMessage, AiMessageRole, ContextItem, ContextKind};

use super::*;

fn connection(id: &str, name: &str, db_type: &str, database: Option<&str>) -> WorkflowPanelConnection {
    WorkflowPanelConnection {
        id: id.to_owned(),
        name: name.to_owned(),
        database_type: db_type.to_owned(),
        database: database.map(str::to_owned),
    }
}

fn sessions(pairs: &[(&str, Option<&str>)]) -> ActiveSessionSnapshot {
    pairs
        .iter()
        .map(|(connection, session)| ((*connection).to_owned(), session.map(str::to_owned)))
        .collect()
}

fn ports(
    blocks: Vec<String>,
    parse_result: Result<ParsedWorkflowDefinition, WorkflowSaveError>,
    save_result: Result<(), String>,
) -> WorkflowChatPanelPorts {
    WorkflowChatPanelPorts {
        yaml_blocks: Box::new(move |_| blocks.clone()),
        parse_and_validate: Box::new(move |_, _| parse_result.clone()),
        save_workflow: Box::new(move |_| save_result.clone()),
    }
}

#[test]
fn constants_and_dom_gap_are_pinned() {
    assert_eq!(COPIED_FEEDBACK_MS, 1500);
    assert_eq!(SAVE_OK_RESET_MS, 3000);
    assert_eq!(WORKFLOW_CHAT_PANEL_GAP.len(), 5);
    assert!(WORKFLOW_CHAT_PANEL_GAP.contains(&"openSettingsWindow"));
}

#[test]
fn db_session_requires_a_connected_entry_with_a_session() {
    let active = sessions(&[("c1", Some("s1")), ("c2", None), ("c3", Some(""))]);
    assert_eq!(selected_db_session_id(Some("c1"), &active).as_deref(), Some("s1"));
    assert_eq!(selected_db_session_id(Some("c2"), &active), None);
    assert_eq!(selected_db_session_id(Some("c3"), &active), None);
    assert_eq!(selected_db_session_id(Some("missing"), &active), None);
    assert_eq!(selected_db_session_id(None, &active), None);
}

#[test]
fn connection_options_and_validation_connections() {
    let connections = vec![connection("c1", "Local", "postgres", Some("db"))];
    let options = connection_options(&connections, "No connection");
    assert_eq!(options, vec![
        (String::new(), "No connection".to_owned()),
        ("c1".to_owned(), "Local (postgres)".to_owned()),
    ]);
    let validated = validation_connections(&connections, &|_| true);
    assert_eq!(validated[0].id, "c1");
    assert!(validated[0].requires_explicit_database);
    assert!(validation_connections(&connections, &|_| false)[0].requires_explicit_database == false);
    assert_eq!(selected_database(&connections, Some("c1")).as_deref(), Some("db"));
    assert_eq!(selected_database(&connections, Some("none")), None);
    assert_eq!(selected_database(&connections, None), None);
    assert!(is_configured_gate(true));
    assert!(!is_configured_gate(false));
}

#[test]
fn send_request_splits_context_and_guards_empty_or_streaming() {
    let items = vec![
        ContextItem { kind: ContextKind::Table, id: "t1".to_owned(), name: "t1".to_owned(), path: None, database: None },
        ContextItem { kind: ContextKind::File, id: "f".to_owned(), name: "f".to_owned(), path: Some("/a.sql".to_owned()), database: None },
    ];
    let request = build_send_request("  hello  ", false, Some("s1".to_owned()), &items);
    assert!(request.should_send);
    assert_eq!(request.params.content, "hello");
    assert_eq!(request.params.db_session_id.as_deref(), Some("s1"));
    assert_eq!(request.params.include_schema, Some(true));
    assert_eq!(request.params.context_tables, Some(vec!["t1".to_owned()]));
    assert_eq!(request.params.context_files, Some(vec!["/a.sql".to_owned()]));

    let no_session = build_send_request("hi", false, None, &[]);
    assert!(no_session.should_send);
    assert_eq!(no_session.params.include_schema, Some(false));
    assert_eq!(no_session.params.context_files, None);
    assert_eq!(no_session.params.context_tables, None);
    assert!(!build_send_request("   ", false, None, &[]).should_send);
    assert!(!build_send_request("hi", true, None, &[]).should_send);
}

#[test]
fn stop_collects_the_stream_into_an_assistant_message() {
    let mut state = WorkflowChatState {
        is_streaming: true,
        stream_content: "answer".to_owned(),
        stream_reasoning: "thought".to_owned(),
        request_id: Some("r1".to_owned()),
        ..Default::default()
    };
    handle_stop(&mut state);
    assert!(!state.is_streaming);
    assert_eq!(state.stream_content, "");
    assert_eq!(state.stream_reasoning, "");
    assert_eq!(state.request_id, None);
    assert_eq!(state.messages.len(), 1);
    assert_eq!(state.messages[0].role, AiMessageRole::Assistant);
    assert_eq!(state.messages[0].content, "answer");
    assert_eq!(state.messages[0].reasoning.as_deref(), Some("thought"));

    let mut empty = WorkflowChatState { is_streaming: true, ..Default::default() };
    handle_stop(&mut empty);
    assert!(empty.messages.is_empty(), "no stream means no synthetic message");
}

#[test]
fn new_chat_reset_and_answer_params() {
    let reset = handle_new_chat_reset();
    assert_eq!(reset, NewChatReset { clear_input: true, clear_save_ok: true, clear_save_error: true });
    let params = answer_questions_params("a,b", Some("s1".to_owned()));
    assert_eq!(params.content, "a,b");
    assert_eq!(params.include_schema, Some(true));
    assert_eq!(params.context_files, None);
    assert_eq!(params.context_tables, None);
}

#[test]
fn save_yaml_maps_success_and_each_failure() {
    let validation = vec![WorkflowValidationConnection {
        id: "c1".to_owned(),
        requires_explicit_database: true,
    }];
    let ok_ports = ports(Vec::new(), Ok(serde_json::json!({"id": "w"})), Ok(()));
    let (definition, success) = save_yaml("id: w", &validation, &ok_ports).expect("saved");
    assert_eq!(definition, serde_json::json!({"id": "w"}));
    assert_eq!(success.reset_after_ms, 3000);

    let missing = ports(
        Vec::new(),
        Err(WorkflowSaveError::MissingField("steps".to_owned())),
        Ok(()),
    );
    assert_eq!(
        save_yaml("id: w", &validation, &missing).unwrap_err(),
        WorkflowSaveError::MissingField("steps".to_owned())
    );
    let invalid = ports(
        Vec::new(),
        Err(WorkflowSaveError::Invalid("bad shape".to_owned())),
        Ok(()),
    );
    assert_eq!(save_yaml("x", &validation, &invalid).unwrap_err().message(), "bad shape");
    let failed = ports(
        Vec::new(),
        Ok(serde_json::json!({"id": "w"})),
        Err("offline".to_owned()),
    );
    assert_eq!(
        save_yaml("x", &validation, &failed).unwrap_err(),
        WorkflowSaveError::Command("offline".to_owned())
    );
}

#[test]
fn yaml_extraction_accepts_both_fence_spellings_and_checks_body() {
    let text = "before\n```yaml\nid: w\nsteps:\n  - a\n```\n```yml\nname: n\nblocks:\n  - b\n```\n```yaml\nid: x\n```\nafter";
    let blocks = extract_workflow_yaml(text);
    assert_eq!(blocks.len(), 2);
    assert!(blocks[0].contains("id: w"));
    assert!(blocks[1].contains("name: n"));
    assert!(extract_workflow_yaml("no fences").is_empty());
    assert!(extract_workflow_yaml("```yaml\nsteps: []\n```").is_empty(), "no id/name is not a workflow");
}

#[test]
fn bubble_visibility_and_copy_gating() {
    assert_eq!(copied_block_index(true, Some(2)), Some(2));
    assert_eq!(copied_block_index(true, None), None);
    assert_eq!(copied_block_index(false, Some(2)), None);
    assert!(yaml_actions_visible(&["a".to_owned()], false));
    assert!(!yaml_actions_visible(&["a".to_owned()], true));
    assert!(!yaml_actions_visible(&[], false));

    let assistant = AiChatMessage::new(AiMessageRole::Assistant, "hi");
    let user = AiChatMessage::new(AiMessageRole::User, "hi");
    assert!(is_user(&user));
    assert!(!is_user(&assistant));
    assert!(is_last_assistant(&[assistant.clone()], 0, false));
    assert!(!is_last_assistant(&[assistant.clone()], 0, true));
    assert!(!is_last_assistant(&[user.clone(), assistant.clone()], 0, false));
    assert!(!questions_visible(&assistant, false, true, true), "no questions");
    let mut with_questions = assistant.clone();
    with_questions.questions = Some(vec![crate::AiQuestion {
        id: "q".to_owned(),
        prompt: "p".to_owned(),
        options: Vec::new(),
        allow_multiple: None,
    }]);
    assert!(questions_visible(&with_questions, false, true, true));
    assert!(!questions_visible(&with_questions, true, true, true));
    assert!(!questions_visible(&with_questions, false, false, true));
    assert!(!questions_visible(&with_questions, false, true, false));
    assert!(thinking_visible(true, "", ""));
    assert!(!thinking_visible(true, "x", ""));
    assert!(streaming_bubble_visible(true, "", "thought"));
    assert!(!streaming_bubble_visible(false, "x", ""));
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::selected_db_session_id;
    let _ = crate::connection_options;
    let _ = crate::validation_connections;
    let _ = crate::selected_database;
    let _ = crate::build_send_request;
    let _ = crate::handle_stop;
    let _ = crate::handle_new_chat_reset;
    let _ = crate::save_yaml;
    let _ = crate::answer_questions_params;
    let _ = crate::extract_workflow_yaml;
    let _ = crate::copied_block_index;
    let _ = crate::yaml_actions_visible;
    let _ = crate::questions_visible;
    let _ = crate::is_last_assistant;
    let _ = crate::thinking_visible;
    let _ = crate::streaming_bubble_visible;
    assert_eq!(crate::COPIED_FEEDBACK_MS, 1500);
    let _: Option<crate::WorkflowChatPanelPorts> = None;
    let _: Option<crate::WorkflowChatState> = None;
    let _: Option<crate::WorkflowPanelConnection> = None;
    let _: Option<crate::WorkflowValidationConnection> = None;
    let _: Option<crate::WorkflowSaveError> = None;
    let _: Option<crate::SendRequest> = None;
    let _: Option<crate::SaveSuccess> = None;
    let _: Option<crate::NewChatReset> = None;
}
