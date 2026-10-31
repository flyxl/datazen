//! AI 响应解析的行为钉子（2026-10-31 自桌面 `commands/ai/tests.rs` 归位：
//! 被测函数一直在 modules，测试却住在宿主命令层——单元测试跟着逻辑走）。
//!
//! 覆盖：围栏剥离、JSON 边界提取、`parse_ai_json` 的截断/空响应/schema
//! 失败分类、语言提示注入与多字节截断。

use super::inject_language_hint;
use super::language_hint;
use super::parse_ai_json;
use super::strip_markdown_fences;
use super::truncate_str;
use super::extract_json_boundary;
use crate::ai::domain::types::{ChatMessage, MessageRole};

#[test]
fn test_strip_markdown_fences_plain_json() {
    let input = r#"{"explanation":"test","suggestedSql":null,"changes":[]}"#;
    assert_eq!(strip_markdown_fences(input), input);
}

#[test]
fn test_strip_markdown_fences_with_json_fence() {
    let input = "```json\n{\"explanation\":\"test\"}\n```";
    assert_eq!(strip_markdown_fences(input), "{\"explanation\":\"test\"}");
}

#[test]
fn test_strip_markdown_fences_with_bare_fence() {
    let input = "```\n{\"explanation\":\"test\"}\n```";
    assert_eq!(strip_markdown_fences(input), "{\"explanation\":\"test\"}");
}

#[test]
fn test_strip_markdown_fences_whitespace() {
    let input = "  ```json\n  {\"key\":\"val\"}  \n```  ";
    assert_eq!(strip_markdown_fences(input), "{\"key\":\"val\"}");
}

#[test]
fn test_extract_json_boundary_object_with_trailing() {
    let input = r#"{"key":"val"}The user wants me to..."#;
    assert_eq!(extract_json_boundary(input), Some(r#"{"key":"val"}"#));
}

#[test]
fn test_extract_json_boundary_array_with_trailing() {
    let input = r#"[{"a":1}]Some reasoning text"#;
    assert_eq!(extract_json_boundary(input), Some(r#"[{"a":1}]"#));
}

#[test]
fn test_extract_json_boundary_nested() {
    let input = r#"{"a":{"b":"c"},"d":[1,2]}trailing"#;
    assert_eq!(
        extract_json_boundary(input),
        Some(r#"{"a":{"b":"c"},"d":[1,2]}"#)
    );
}

#[test]
fn test_extract_json_boundary_with_escaped_quotes() {
    let input = r#"{"msg":"say \"hello\""}extra"#;
    assert_eq!(
        extract_json_boundary(input),
        Some(r#"{"msg":"say \"hello\""}"#)
    );
}

#[test]
fn test_extract_json_boundary_no_json() {
    assert_eq!(extract_json_boundary("not json at all"), None);
}

#[test]
fn test_extract_json_boundary_clean() {
    let input = r#"{"key":"val"}"#;
    assert_eq!(extract_json_boundary(input), Some(input));
}

#[test]
fn test_parse_ai_json_with_trailing_reasoning() {
    #[derive(serde::Deserialize)]
    struct Simple {
        key: String,
    }
    let raw = r#"{"key":"val"}The user wants me to analyze..."#;
    let result: Result<Simple, _> = parse_ai_json(raw, None, "test");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().key, "val");
}

#[test]
fn test_language_hint_known_locales() {
    assert!(language_hint("zh-CN").contains("Chinese (Simplified)"));
    assert!(language_hint("zh-TW").contains("Chinese (Traditional)"));
    assert!(language_hint("en").contains("English"));
    assert!(language_hint("ja").contains("Japanese"));
    assert!(language_hint("ko").contains("Korean"));
    assert!(language_hint("es").contains("Spanish"));
    assert!(language_hint("fr").contains("French"));
    assert!(language_hint("custom-locale").contains("custom-locale"));
}

#[test]
fn test_inject_language_hint_appends_to_system() {
    let mut messages = vec![
        ChatMessage {
            role: MessageRole::System,
            content: "Base prompt".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        },
        ChatMessage {
            role: MessageRole::User,
            content: "Hi".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        },
    ];
    inject_language_hint(&mut messages, "zh-CN");
    assert!(messages[0].content.contains("Base prompt"));
    assert!(messages[0].content.contains("Chinese (Simplified)"));
    assert!(!messages[1].content.contains("Chinese"));
}

#[test]
fn test_truncate_str_multibyte() {
    assert_eq!(truncate_str("héllo", 3), "hé");
    assert_eq!(truncate_str("你好世界", 7), "你好");
}

#[test]
fn test_parse_ai_json_empty_response() {
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct Simple {
        key: String,
    }
    let err = parse_ai_json::<Simple>("   ", None, "test").unwrap_err();
    assert!(err.to_string().contains("empty"));
}

#[test]
fn test_parse_ai_json_truncated_finish_reason() {
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct Simple {
        items: Vec<String>,
    }
    let raw = r#"{"items": ["unclosed"#;
    let err = parse_ai_json::<Simple>(raw, Some("length"), "test").unwrap_err();
    assert!(err.to_string().contains("truncated"));
}

#[test]
fn test_parse_ai_json_invalid_with_reason() {
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct NeedsField {
        required_field: String,
    }
    let raw = r#"{"wrong": "field"}"#;
    let err = parse_ai_json::<NeedsField>(raw, Some("stop"), "test").unwrap_err();
    assert!(err.to_string().contains("schema"));
}

#[test]
fn test_language_hint_unknown_locale_uses_code() {
    assert!(language_hint("de").contains("de"));
}
