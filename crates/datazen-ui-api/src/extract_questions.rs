use serde_json::Value;

use crate::{AiQuestion, AiQuestionOption, AiToolCall};

const OPEN_TAG: &str = "<ask_questions>";
const CLOSE_TAG: &str = "</ask_questions>";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractQuestionsResult {
    pub clean_content: String,
    pub questions: Vec<AiQuestion>,
}

fn find_block(content: &str) -> Option<(usize, usize, usize, usize)> {
    let start = content.find(OPEN_TAG)?;
    let open_end = start + OPEN_TAG.len();
    let close_offset = content[open_end..].find(CLOSE_TAG)?;
    let close_start = open_end + close_offset;
    let close_end = close_start + CLOSE_TAG.len();
    Some((start, open_end, close_start, close_end))
}

fn js_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => "null".to_owned(),
        Value::Array(items) => items
            .iter()
            .map(|item| if item.is_null() { String::new() } else { js_string(item) })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

fn question_from_value(value: &Value) -> Option<AiQuestion> {
    let object = value.as_object()?;
    let id = object.get("id")?.as_str()?;
    let prompt = match object.get("prompt") {
        None | Some(Value::Null) => String::new(),
        Some(value) => js_string(value),
    };
    let options = match object.get("options") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                let option = item.as_object()?;
                let id = option.get("id")?.as_str()?;
                let label = option.get("label")?.as_str()?;
                Some(AiQuestionOption { id: id.to_owned(), label: label.to_owned() })
            })
            .collect(),
        _ => Vec::new(),
    };
    let allow_multiple = object.get("allowMultiple") == Some(&Value::Bool(true));
    Some(AiQuestion {
        id: id.to_owned(),
        prompt,
        options,
        allow_multiple: Some(allow_multiple),
    })
}

fn questions_from_array(items: &[Value]) -> Vec<AiQuestion> {
    items.iter().filter_map(question_from_value).collect()
}

pub fn extract_questions(content: &str) -> ExtractQuestionsResult {
    let Some((start, open_end, close_start, close_end)) = find_block(content) else {
        return ExtractQuestionsResult { clean_content: content.to_owned(), questions: Vec::new() };
    };
    let json_str = content[open_end..close_start].trim();
    let clean_content = format!("{}{}", &content[..start], &content[close_end..]);
    let clean_content = clean_content.trim().to_owned();
    let Ok(raw) = serde_json::from_str::<Value>(json_str) else {
        return ExtractQuestionsResult { clean_content: content.to_owned(), questions: Vec::new() };
    };
    let Some(items) = raw.as_array() else {
        return ExtractQuestionsResult { clean_content: content.to_owned(), questions: Vec::new() };
    };
    ExtractQuestionsResult { clean_content, questions: questions_from_array(items) }
}

pub fn parse_tool_call_questions(tool_calls: Option<&[AiToolCall]>) -> Vec<AiQuestion> {
    let Some(calls) = tool_calls else {
        return Vec::new();
    };
    if calls.is_empty() {
        return Vec::new();
    }
    let Some(ask_call) = calls.iter().find(|call| call.name == "ask_questions") else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<Value>(&ask_call.arguments) else {
        return Vec::new();
    };
    let Some(items) = parsed.get("questions").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    questions_from_array(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(id: &str, prompt: &str, options: Vec<(&str, &str)>, multiple: bool) -> AiQuestion {
        AiQuestion {
            id: id.to_owned(),
            prompt: prompt.to_owned(),
            options: options
                .into_iter()
                .map(|(id, label)| AiQuestionOption { id: id.to_owned(), label: label.to_owned() })
                .collect(),
            allow_multiple: Some(multiple),
        }
    }

    fn tool_call(name: &str, arguments: &str) -> AiToolCall {
        AiToolCall {
            id: "call".to_owned(),
            name: name.to_owned(),
            arguments: arguments.to_owned(),
        }
    }

    #[test]
    fn no_tag_keeps_the_original_content() {
        let result = extract_questions("  plain text  ");
        assert_eq!(result.clean_content, "  plain text  ");
        assert!(result.questions.is_empty());
    }

    #[test]
    fn valid_block_is_removed_and_questions_are_parsed() {
        let content = "before <ask_questions>\n[{\"id\":\"q1\",\"prompt\":\"Pick\",\"options\":[{\"id\":\"a\",\"label\":\"A\"}],\"allowMultiple\":true}]\n</ask_questions> after";
        let result = extract_questions(content);
        assert_eq!(result.clean_content, "before  after");
        assert_eq!(result.questions, vec![question("q1", "Pick", vec![("a", "A")], true)]);
    }

    #[test]
    fn non_array_and_invalid_json_restore_the_original_content() {
        for json in ["{\"a\":1}", "not json"] {
            let content = format!("head <ask_questions>{json}</ask_questions> tail");
            let result = extract_questions(&content);
            assert_eq!(result.clean_content, content);
            assert!(result.questions.is_empty());
        }
    }

    #[test]
    fn dirty_question_entries_are_filtered_and_coerced() {
        let json = r#"[
            {"id":"ok","prompt":null,"options":"bad","allowMultiple":"true"},
            {"id":42,"prompt":"numeric id"},
            null,
            "text",
            {"id":"p","prompt":7,"options":[{"id":"x","label":"X"},{"id":5,"label":"bad"},{"id":"y"}],"allowMultiple":true}
        ]"#;
        let content = format!("<ask_questions>{json}</ask_questions>");
        let result = extract_questions(&content);
        assert_eq!(result.clean_content, "");
        assert_eq!(
            result.questions,
            vec![
                question("ok", "", Vec::new(), false),
                question("p", "7", vec![("x", "X")], true),
            ]
        );
    }

    #[test]
    fn only_the_first_block_is_removed() {
        let content = "A<ask_questions>[]</ask_questions>B<ask_questions>[]</ask_questions>C";
        let result = extract_questions(content);
        assert_eq!(result.clean_content, "AB<ask_questions>[]</ask_questions>C");
        assert!(result.questions.is_empty());
    }

    #[test]
    fn tool_call_parsing_requires_an_ask_questions_call_with_a_questions_array() {
        assert!(parse_tool_call_questions(None).is_empty());
        assert!(parse_tool_call_questions(Some(&[])).is_empty());
        let other = tool_call("other", r#"{"questions":[]}"#);
        assert!(parse_tool_call_questions(Some(&[other])).is_empty());
        let invalid = tool_call("ask_questions", "not json");
        assert!(parse_tool_call_questions(Some(&[invalid])).is_empty());
        let not_array = tool_call("ask_questions", r#"{"questions":{}}"#);
        assert!(parse_tool_call_questions(Some(&[not_array])).is_empty());
    }

    #[test]
    fn tool_call_parsing_maps_questions_like_the_tag_path() {
        let first = tool_call("other", "{}");
        let ask = tool_call(
            "ask_questions",
            r#"{"questions":[{"id":"q","prompt":"Why","options":[{"id":"o","label":"Because"}],"allowMultiple":false}]}"#,
        );
        let questions = parse_tool_call_questions(Some(&[first, ask]));
        assert_eq!(questions, vec![question("q", "Why", vec![("o", "Because")], false)]);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::extract_questions;
        let _ = crate::parse_tool_call_questions;
        let _: Option<crate::ExtractQuestionsResult> = None;
    }
}
