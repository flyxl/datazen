use crate::{AiChatMessage, AiMessageRole};

pub fn is_user(message: &AiChatMessage) -> bool {
    message.role == AiMessageRole::User
}

fn has_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let needle = word.as_bytes();
    let mut start = 0usize;
    while let Some(offset) = text[start..].find(word) {
        let from = start + offset;
        let before_ok = from == 0 || !is_word_byte(bytes[from - 1]);
        let to = from + needle.len();
        let after_ok = to >= bytes.len() || !is_word_byte(bytes[to]);
        if before_ok && after_ok {
            return true;
        }
        start = from + 1;
    }
    false
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn looks_like_workflow(yaml: &str) -> bool {
    let has_body = has_word(yaml, "steps") || has_word(yaml, "blocks");
    let has_identity = has_word(yaml, "id") || has_word(yaml, "name");
    has_body
        && has_identity
        && (yaml.contains("steps:") || yaml.contains("blocks:"))
        && (yaml.contains("id:") || yaml.contains("name:"))
}

pub fn extract_workflow_yaml(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut rest = text;
    loop {
        let yaml_open = rest.find("```yaml");
        let yml_open = rest.find("```yml");
        let open = match (yaml_open, yml_open) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => break,
        };
        let after_open = open + if rest[open..].starts_with("```yaml") { 7 } else { 6 };
        let after_newline = match rest[after_open..].find('\n') {
            Some(index) => after_open + index + 1,
            None => break,
        };
        let close = match rest[after_newline..].find("```") {
            Some(index) => after_newline + index,
            None => break,
        };
        let content = rest[after_newline..close].trim().to_owned();
        if looks_like_workflow(&content) {
            blocks.push(content);
        }
        rest = &rest[close + 3..];
    }
    blocks
}

pub fn copied_block_index(copied: bool, copied_idx: Option<usize>) -> Option<usize> {
    if copied {
        copied_idx
    } else {
        None
    }
}

pub fn yaml_actions_visible(blocks: &[String], is_streaming: bool) -> bool {
    !blocks.is_empty() && !is_streaming
}

pub fn questions_visible(
    message: &AiChatMessage,
    is_streaming: bool,
    is_last_assistant: bool,
    has_handler: bool,
) -> bool {
    !is_user(message)
        && !is_streaming
        && message.questions.as_ref().is_some_and(|questions| !questions.is_empty())
        && is_last_assistant
        && has_handler
}

pub fn is_last_assistant(messages: &[AiChatMessage], index: usize, is_streaming: bool) -> bool {
    messages.get(index).is_some_and(|message| message.role == AiMessageRole::Assistant)
        && index + 1 == messages.len()
        && !is_streaming
}

pub fn thinking_visible(is_streaming: bool, content: &str, reasoning: &str) -> bool {
    is_streaming && content.is_empty() && reasoning.is_empty()
}

pub fn streaming_bubble_visible(is_streaming: bool, content: &str, reasoning: &str) -> bool {
    is_streaming && (!content.is_empty() || !reasoning.is_empty())
}
