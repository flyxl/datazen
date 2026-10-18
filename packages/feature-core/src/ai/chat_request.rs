//! Shared chat prompt and completion-request construction.

use crate::ai::prompt_resolver::PromptResolver;
use crate::ai::response::inject_language_hint;
use crate::ai::safety::redact_for_gate;
use crate::ai::schema_pipeline::{compose_schema_system_suffix, PromptSeed};
use datazen_ai_api::{
    AiSafetyGateConfig, ChatMessage, CompletionRequest, MessageRole, ToolDefinition,
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
    pub safety_gate: &'a AiSafetyGateConfig,
    pub attach_db_tools: bool,
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
        tools.extend(super::tools::db_tool_definitions());
    }
    tools.extend(input.mcp_tools);

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

    if let Some(schema) = &input.schema {
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
        format!(
            "{description}\n\n{}",
            compose_schema_system_suffix(schema.seed)
        )
    } else if is_workflow {
        let mut vars = HashMap::new();
        vars.insert("connections", input.connections_context);
        vars.insert("schema", "");
        vars.insert("db_type", "");
        let template = prompt_resolver
            .resolve(prompt_scenario, None, input.lang)
            .await;
        super::prompt_resolver::render_template(&template, &vars)
    } else {
        prompt_resolver
            .resolve(prompt_scenario, None, input.lang)
            .await
    }
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
                safety_gate: &gate,
                attach_db_tools: true,
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
    }
}
