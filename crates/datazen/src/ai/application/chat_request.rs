//! Shared chat prompt and completion-request construction.

use super::chat_injection::ChatInjection;
use super::prompt_resolver::PromptResolver;
use super::response::inject_language_hint;
use super::schema_pipeline::{compose_schema_system_suffix, PromptSeed};
use crate::ai::domain::safety::redact_for_gate;
use crate::ai::domain::types::{
    AiSafetyGateConfig, ChatMessage, CompletionRequest, MessageRole, ToolDefinition,
    MAX_CONNECTION_NOTES_BYTES, MAX_GLOBAL_INSTRUCTIONS_BYTES, MAX_PROMPT_TEMPLATE_BYTES,
};
use datazen_driver_api::{DatabaseDriver, PromptScenario};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatScenario {
    Chat,
    WorkflowGenerate,
}

pub struct ChatSchemaPrompt<'a> {
    pub driver: &'a dyn DatabaseDriver,
    pub seed: &'a PromptSeed,
}

pub struct ChatRequestInput<'a> {
    pub request_id: &'a str,
    pub model: &'a str,
    pub lang: &'a str,
    pub scenario: ChatScenario,
    pub schema: Option<ChatSchemaPrompt<'a>>,
    pub connections_context: &'a str,
    pub messages: Vec<ChatMessage>,
    /// Already formatted context-file content. File access and path policy
    /// remain in the host; this layer only inserts it into the last user turn.
    pub context_block: Option<&'a str>,
    pub injection: ChatInjection,
    pub safety_gate: &'a AiSafetyGateConfig,
    pub attach_db_tools: bool,
    /// Agent chat gets read-only query/sampling in addition to metadata tools.
    /// SQL generation keeps using [`super::tools::db_tool_definitions`].
    pub attach_agent_db_tools: bool,
    /// Query/sampling are only exposed when the session is bound to a concrete
    /// connection. This is an explicit caller decision, not inferred from tools.
    pub attach_agent_query_tools: bool,
    pub mcp_tools: Vec<ToolDefinition>,
}

/// Build chat messages, tools and completion settings without platform storage
/// or event dependencies.
pub async fn build_chat_request(
    prompt_resolver: &PromptResolver,
    input: ChatRequestInput<'_>,
) -> CompletionRequest {
    let mut messages = vec![ChatMessage {
        role: MessageRole::System,
        content: build_system_prompt(prompt_resolver, &input).await,
        reasoning: None,
        tool_calls: None,
        tool_call_id: None,
    }];

    messages.extend(input.messages.into_iter().map(|mut message| {
        message.content = redact_for_gate(&message.content, input.safety_gate);
        message
    }));

    if let Some(context_block) = input.context_block {
        if let Some(last_user) = messages
            .iter_mut()
            .rev()
            .find(|message| message.role == MessageRole::User)
        {
            last_user.content = format!("{context_block}\n\n{}", last_user.content);
        }
    }

    let mut tools = vec![ask_questions_tool_definition()];
    if input.attach_db_tools {
        let db_tools = if input.attach_agent_db_tools {
            super::tools::agent_db_tool_definitions(input.attach_agent_query_tools)
        } else {
            super::tools::db_tool_definitions()
        };
        tools.extend(super::tools::filter_db_tool_definitions(
            db_tools,
            input.safety_gate.db_tool_policy,
        ));
    }
    tools.extend(super::mcp_tools::filter_mcp_tool_definitions(
        input.mcp_tools,
        input.safety_gate.mcp_tool_policy,
    ));

    let mut request = CompletionRequest {
        request_id: input.request_id.to_string(),
        model: input.model.to_string(),
        messages,
        temperature: Some(0.7),
        stop: None,
        tools: Some(tools),
        previous_response_id: None,
        cancel_token: None,
    };
    inject_language_hint(&mut request.messages, input.lang);
    request
}

async fn build_system_prompt(
    prompt_resolver: &PromptResolver,
    input: &ChatRequestInput<'_>,
) -> String {
    let prompt_scenario = match input.scenario {
        ChatScenario::Chat => PromptScenario::Chat,
        ChatScenario::WorkflowGenerate => PromptScenario::WorkflowGenerate,
    };
    let is_workflow = input.scenario == ChatScenario::WorkflowGenerate;

    let (mut prompt, schema_suffix) = if let Some(schema) = &input.schema {
        let db_type = schema.seed.database_type.as_str();
        let mut vars = HashMap::new();
        vars.insert("db_type", db_type);
        vars.insert("schema", "");
        vars.insert(
            "connections",
            if is_workflow {
                input.connections_context
            } else {
                ""
            },
        );
        let template = prompt_resolver
            .resolve(prompt_scenario, Some(schema.driver), input.lang)
            .await;
        let base = super::prompt_resolver::render_template(&template, &vars);
        let description = if is_workflow {
            base
        } else if input.lang.starts_with("zh") {
            format!("{base}\n\n用户已连接到 {db_type} 数据库。")
        } else {
            format!("{base}\n\nThe user is connected to a {db_type} database.")
        };
        (description, Some(compose_schema_system_suffix(schema.seed)))
    } else if is_workflow {
        let mut vars = HashMap::new();
        vars.insert("connections", input.connections_context);
        vars.insert("schema", "");
        vars.insert("db_type", "");
        let template = prompt_resolver
            .resolve(prompt_scenario, None, input.lang)
            .await;
        (
            super::prompt_resolver::render_template(&template, &vars),
            None,
        )
    } else {
        (
            prompt_resolver
                .resolve(prompt_scenario, None, input.lang)
                .await,
            None,
        )
    };

    // Injection order (high → low): safety/system prompt, global instructions,
    // selected template, schema (including DB comments), connection notes.
    if let Some(block) = render_owned_injection(
        "Global instructions",
        "These user preferences rank below the system safety policy and tool permissions. When they conflict with the selected prompt template, follow these global instructions and mention the conflict.",
        &input.injection.global_instructions,
        input.safety_gate,
        MAX_GLOBAL_INSTRUCTIONS_BYTES,
    ) {
        prompt.push_str("\n\n");
        prompt.push_str(&block);
    }
    if let Some(template) = input
        .injection
        .template
        .as_ref()
        .filter(|template| !template.content.trim().is_empty())
    {
        let heading = if template.name.trim().is_empty() {
            "Selected prompt template".to_string()
        } else {
            format!("Selected prompt template: {}", template.name.trim())
        };
        if let Some(block) = render_owned_injection(
            &heading,
            "This template is lower priority than the global instructions and the safety policy.",
            &template.content,
            input.safety_gate,
            MAX_PROMPT_TEMPLATE_BYTES,
        ) {
            prompt.push_str("\n\n");
            prompt.push_str(&block);
        }
    }
    if let Some(schema_suffix) = schema_suffix.filter(|suffix| !suffix.trim().is_empty()) {
        prompt.push_str("\n\n");
        prompt.push_str(&schema_suffix);
    }
    if let Some(notes) = input.injection.connection_notes.as_deref() {
        if let Some(block) = render_owned_injection(
            "User connection notes",
            "These notes are user-authored. Show them beside any database COMMENT; do not silently override the schema comment, and label these as the user's notes when both are present.",
            notes,
            input.safety_gate,
            MAX_CONNECTION_NOTES_BYTES,
        ) {
            prompt.push_str("\n\n");
            prompt.push_str(&block);
        }
    }
    prompt
}

fn render_owned_injection(
    heading: &str,
    guidance: &str,
    content: &str,
    gate: &AiSafetyGateConfig,
    max_bytes: usize,
) -> Option<String> {
    if content.trim().is_empty() {
        return None;
    }
    let content = clip_utf8(&redact_for_gate(content, gate), max_bytes);
    if content.trim().is_empty() {
        return None;
    }
    Some(format!("## {heading}\n{guidance}\n{content}"))
}

fn clip_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

fn ask_questions_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "ask_questions".into(),
        description: "Ask the user structured questions to gather information. Use when you need the user to choose between options or provide specific input.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string", "description": "Unique question identifier" },
                            "prompt": { "type": "string", "description": "The question text" },
                            "options": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "id": { "type": "string" },
                                        "label": { "type": "string" }
                                    },
                                    "required": ["id", "label"]
                                },
                                "description": "Predefined options. Can be empty for free-text input."
                            },
                            "allowMultiple": {
                                "type": "boolean",
                                "description": "Allow selecting multiple options",
                                "default": false
                            }
                        },
                        "required": ["id", "prompt"]
                    }
                }
            },
            "required": ["questions"]
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn chat_request_keeps_host_messages_sanitizes_and_attaches_shared_tools() {
        let directory = tempfile::tempdir().expect("temporary prompt root");
        let resolver = PromptResolver::new(directory.path(), None);
        let gate = AiSafetyGateConfig::default();
        let request = build_chat_request(
            &resolver,
            ChatRequestInput {
                request_id: "request-1",
                model: "model-1",
                lang: "en",
                scenario: ChatScenario::Chat,
                schema: None,
                connections_context: "",
                messages: vec![ChatMessage {
                    role: MessageRole::User,
                    content: "api_key=secret".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                }],
                context_block: Some("Context file: notes.txt\nRelevant facts."),
                injection: ChatInjection::default(),
                safety_gate: &gate,
                attach_db_tools: true,
                attach_agent_db_tools: false,
                attach_agent_query_tools: false,
                mcp_tools: Vec::new(),
            },
        )
        .await;

        assert_eq!(request.request_id, "request-1");
        assert_eq!(request.temperature, Some(0.7));
        let user_message = request
            .messages
            .iter()
            .find(|message| message.role == MessageRole::User)
            .expect("user message is preserved");
        assert!(user_message.content.starts_with("Context file: notes.txt"));
        assert!(!user_message.content.contains("api_key=secret"));
        let tools = request.tools.expect("chat exposes tools");
        assert_eq!(
            tools.first().map(|tool| tool.name.as_str()),
            Some("ask_questions")
        );
        assert!(tools.iter().any(|tool| tool.name == "list_tables"));
        assert!(!tools.iter().any(|tool| tool.name == "query_readonly"));
        assert!(!tools.iter().any(|tool| tool.name == "sample_rows"));
    }

    #[tokio::test]
    async fn chat_request_agent_flag_adds_readonly_query_tools() {
        let directory = tempfile::tempdir().expect("temporary prompt root");
        let resolver = PromptResolver::new(directory.path(), None);
        let gate = AiSafetyGateConfig::default();
        let request = build_chat_request(
            &resolver,
            ChatRequestInput {
                request_id: "request-agent",
                model: "model-1",
                lang: "en",
                scenario: ChatScenario::Chat,
                schema: None,
                connections_context: "",
                messages: vec![ChatMessage {
                    role: MessageRole::User,
                    content: "inspect".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                }],
                context_block: None,
                injection: ChatInjection::default(),
                safety_gate: &gate,
                attach_db_tools: true,
                attach_agent_db_tools: true,
                attach_agent_query_tools: true,
                mcp_tools: Vec::new(),
            },
        )
        .await;

        let names = request
            .tools
            .expect("tools")
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        assert!(names.contains(&"list_tables".into()));
        assert!(names.contains(&"query_readonly".into()));
        assert!(names.contains(&"sample_rows".into()));
    }

    async fn agent_request(
        gate: &AiSafetyGateConfig,
        mcp_tools: Vec<ToolDefinition>,
    ) -> CompletionRequest {
        let directory = tempfile::tempdir().expect("temporary prompt root");
        let resolver = PromptResolver::new(directory.path(), None);
        build_chat_request(
            &resolver,
            ChatRequestInput {
                request_id: "request-policy",
                model: "model-1",
                lang: "en",
                scenario: ChatScenario::Chat,
                schema: None,
                connections_context: "",
                messages: vec![ChatMessage {
                    role: MessageRole::User,
                    content: "inspect".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                }],
                context_block: None,
                injection: ChatInjection::default(),
                safety_gate: gate,
                attach_db_tools: true,
                attach_agent_db_tools: true,
                attach_agent_query_tools: true,
                mcp_tools,
            },
        )
        .await
    }

    fn mcp_tool(name: &str, parameters: serde_json::Value) -> ToolDefinition {
        ToolDefinition {
            name: name.into(),
            description: name.into(),
            parameters,
        }
    }

    #[tokio::test]
    async fn redact_credentials_false_keeps_user_message_secrets() {
        let gate = AiSafetyGateConfig {
            redact_credentials: false,
            ..Default::default()
        };
        let directory = tempfile::tempdir().expect("temporary prompt root");
        let resolver = PromptResolver::new(directory.path(), None);
        let request = build_chat_request(
            &resolver,
            ChatRequestInput {
                request_id: "request-redact",
                model: "model-1",
                lang: "en",
                scenario: ChatScenario::Chat,
                schema: None,
                connections_context: "",
                messages: vec![ChatMessage {
                    role: MessageRole::User,
                    content: "password=db-secret".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                }],
                context_block: None,
                injection: ChatInjection::default(),
                safety_gate: &gate,
                attach_db_tools: false,
                attach_agent_db_tools: false,
                attach_agent_query_tools: false,
                mcp_tools: Vec::new(),
            },
        )
        .await;

        let user_message = request
            .messages
            .iter()
            .find(|message| message.role == MessageRole::User)
            .expect("user message");
        assert_eq!(user_message.content, "password=db-secret");
    }

    #[tokio::test]
    async fn db_policy_disabled_attaches_only_ask_questions() {
        let gate = AiSafetyGateConfig {
            db_tool_policy: crate::ai::domain::types::AiToolPermissionPolicy::Disabled,
            ..Default::default()
        };
        let request = agent_request(&gate, Vec::new()).await;

        let names = request
            .tools
            .expect("tools")
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["ask_questions"]);
    }

    #[tokio::test]
    async fn db_policy_read_only_keeps_read_only_query_and_sampling_tools() {
        let gate = AiSafetyGateConfig {
            db_tool_policy: crate::ai::domain::types::AiToolPermissionPolicy::ReadOnly,
            ..Default::default()
        };
        let request = agent_request(&gate, Vec::new()).await;
        let names = request
            .tools
            .expect("tools")
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        assert!(names.contains(&"list_tables".into()));
        assert!(names.contains(&"query_readonly".into()));
        assert!(names.contains(&"sample_rows".into()));
    }

    #[tokio::test]
    async fn db_policy_require_confirm_still_attaches_database_tools() {
        let gate = AiSafetyGateConfig {
            db_tool_policy: crate::ai::domain::types::AiToolPermissionPolicy::RequireConfirm,
            ..Default::default()
        };
        let request = agent_request(&gate, Vec::new()).await;
        let names = request
            .tools
            .expect("tools")
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        assert!(names.contains(&"list_tables".into()));
        assert!(names.contains(&"query_readonly".into()));
    }

    #[tokio::test]
    async fn mcp_policy_disabled_removes_every_mcp_tool() {
        let gate = AiSafetyGateConfig {
            mcp_tool_policy: crate::ai::domain::types::AiToolPermissionPolicy::Disabled,
            ..Default::default()
        };
        let request = agent_request(
            &gate,
            vec![
                mcp_tool("mcp/demo/list_files", serde_json::json!({})),
                mcp_tool(
                    "mcp/demo/write_file",
                    serde_json::json!({"type": "object", "x-write": true}),
                ),
            ],
        )
        .await;
        let names = request
            .tools
            .expect("tools")
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        assert!(!names.iter().any(|name| name.starts_with("mcp/")));
    }

    #[tokio::test]
    async fn mcp_policy_read_only_drops_write_capable_tools() {
        let gate = AiSafetyGateConfig {
            mcp_tool_policy: crate::ai::domain::types::AiToolPermissionPolicy::ReadOnly,
            ..Default::default()
        };
        let request = agent_request(
            &gate,
            vec![
                mcp_tool("mcp/demo/list_files", serde_json::json!({})),
                mcp_tool(
                    "mcp/demo/write_file",
                    serde_json::json!({"type": "object", "x-write": true}),
                ),
                mcp_tool("mcp/demo/create_issue", serde_json::json!({})),
            ],
        )
        .await;
        let names = request
            .tools
            .expect("tools")
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        assert!(names.contains(&"mcp/demo/list_files".into()));
        assert!(!names.contains(&"mcp/demo/write_file".into()));
        assert!(!names.contains(&"mcp/demo/create_issue".into()));
    }

    #[tokio::test]
    async fn mcp_policy_require_confirm_keeps_write_tools_for_the_gate() {
        let gate = AiSafetyGateConfig {
            mcp_tool_policy: crate::ai::domain::types::AiToolPermissionPolicy::RequireConfirm,
            ..Default::default()
        };
        let request = agent_request(
            &gate,
            vec![mcp_tool(
                "mcp/demo/write_file",
                serde_json::json!({"type": "object", "x-write": true}),
            )],
        )
        .await;
        let names = request
            .tools
            .expect("tools")
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        assert!(names.contains(&"mcp/demo/write_file".into()));
    }

    #[tokio::test]
    async fn mcp_policy_unrestricted_keeps_the_full_face() {
        let gate = AiSafetyGateConfig {
            mcp_tool_policy: crate::ai::domain::types::AiToolPermissionPolicy::Unrestricted,
            ..Default::default()
        };
        let request = agent_request(
            &gate,
            vec![
                mcp_tool("mcp/demo/list_files", serde_json::json!({})),
                mcp_tool(
                    "mcp/demo/write_file",
                    serde_json::json!({"type": "object", "x-write": true}),
                ),
            ],
        )
        .await;
        let names = request
            .tools
            .expect("tools")
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        assert!(names.contains(&"mcp/demo/list_files".into()));
        assert!(names.contains(&"mcp/demo/write_file".into()));
    }

    #[tokio::test]
    async fn user_prompt_material_is_injected_in_priority_order_and_redacted() {
        let directory = tempfile::tempdir().expect("temporary prompt root");
        let resolver = PromptResolver::new(directory.path(), None);
        let gate = AiSafetyGateConfig::default();
        let injection = ChatInjection {
            global_instructions: "GLOBAL_MARKER use UTC".into(),
            template: Some(crate::ai::domain::types::PromptTemplate {
                id: "pg".into(),
                name: "Postgres style".into(),
                content: "TEMPLATE_MARKER prefer explicit joins".into(),
                database_types: vec!["postgresql".into()],
                is_default: true,
            }),
            connection_notes: Some("NOTES_MARKER password=note-secret".into()),
        };
        let request = build_chat_request(
            &resolver,
            ChatRequestInput {
                request_id: "request-injection",
                model: "model-1",
                lang: "en",
                scenario: ChatScenario::Chat,
                schema: None,
                connections_context: "",
                messages: vec![ChatMessage {
                    role: MessageRole::User,
                    content: "USER_MARKER question".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                }],
                context_block: None,
                injection,
                safety_gate: &gate,
                attach_db_tools: false,
                attach_agent_db_tools: false,
                attach_agent_query_tools: false,
                mcp_tools: Vec::new(),
            },
        )
        .await;

        let system = &request.messages[0].content;
        let global = system.find("GLOBAL_MARKER").expect("global marker");
        let template = system.find("TEMPLATE_MARKER").expect("template marker");
        let notes = system.find("NOTES_MARKER").expect("notes marker");
        assert!(
            global < template,
            "global instructions precede the template"
        );
        assert!(template < notes, "template precedes connection notes");
        assert!(!system.contains("note-secret"), "notes are redacted");
        assert!(system.contains("User connection notes"));
        assert!(system.contains("do not silently override the schema comment"));
        assert!(request
            .messages
            .iter()
            .any(|message| message.role == MessageRole::User
                && message.content.contains("USER_MARKER")));
    }
}
