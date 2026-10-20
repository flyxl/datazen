//! Portable natural-language filter generation.

use crate::ai::prompt_resolver::PromptResolver;
use crate::ai::response::{inject_language_hint, parse_ai_json};
use crate::error::FeatureError;
use datazen_ai_api::{AiProvider, ChatMessage, CompletionRequest, MessageRole};
use datazen_driver_api::filters::{FilterCondition, FilterOperator};
use datazen_driver_api::{ColumnSchema, DatabaseDriver, PromptScenario, Value};
use std::collections::{HashMap, HashSet};

/// Ask an AI provider to turn natural language into validated table filters.
/// Schema retrieval and egress redaction are supplied by the host adapter.
pub async fn parse_natural_language_filter(
    provider: &dyn AiProvider,
    model: &str,
    prompt_resolver: &PromptResolver,
    driver: &dyn DatabaseDriver,
    natural_language: String,
    columns: &[ColumnSchema],
    lang: &str,
) -> Result<Vec<FilterCondition>, FeatureError> {
    let columns_ddl = format_filter_columns(columns);
    let db_type = driver.driver_type();
    let mut vars = HashMap::new();
    vars.insert("db_type", db_type.as_str());
    vars.insert("columns", columns_ddl.as_str());
    let template = prompt_resolver
        .resolve(PromptScenario::NlFilter, Some(driver), lang)
        .await;
    let system_content = super::prompt_resolver::render_template(&template, &vars);

    let mut request = CompletionRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        model: model.to_string(),
        messages: vec![
            ChatMessage {
                role: MessageRole::System,
                content: system_content,
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: MessageRole::User,
                content: natural_language,
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        temperature: Some(0.0),
        stop: None,
        tools: None,
        previous_response_id: None,
        cancel_token: None,
    };
    inject_language_hint(&mut request.messages, lang);

    let response = provider.complete(&request).await?;
    let filters = parse_ai_json::<Vec<FilterCondition>>(
        &response.content,
        response.finish_reason.as_deref(),
        "ai_parse_filter",
    )?;
    Ok(normalize_filter_conditions(filters, columns))
}

/// Format trusted table metadata for the natural-language filter prompt.
pub fn format_filter_columns(columns: &[ColumnSchema]) -> String {
    columns
        .iter()
        .map(|column| {
            let nullable = if column.nullable {
                " NULL"
            } else {
                " NOT NULL"
            };
            let primary_key = if column.is_primary_key { " PK" } else { "" };
            format!(
                "  {} {}{}{}",
                column.name, column.data_type, nullable, primary_key
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Drop filters for columns that do not exist and normalize SQL NULL tests.
pub fn normalize_filter_conditions(
    mut filters: Vec<FilterCondition>,
    columns: &[ColumnSchema],
) -> Vec<FilterCondition> {
    let valid_columns: HashSet<&str> = columns.iter().map(|column| column.name.as_str()).collect();
    filters.retain(|filter| valid_columns.contains(filter.column.as_str()));

    for filter in &mut filters {
        match (&filter.operator, &filter.value) {
            (FilterOperator::Eq, Value::Null) => filter.operator = FilterOperator::IsNull,
            (FilterOperator::Ne, Value::Null) => filter.operator = FilterOperator::IsNotNull,
            _ => {}
        }
    }
    filters
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            data_type: "TEXT".into(),
            nullable: true,
            is_primary_key: false,
            is_auto_increment: false,
            default_value: None,
            comment: None,
        }
    }

    fn filter(column: &str, operator: FilterOperator, value: Value) -> FilterCondition {
        FilterCondition {
            column: column.into(),
            operator,
            value,
        }
    }

    #[test]
    fn filter_normalization_drops_unknown_columns_and_converts_null_comparisons() {
        let columns = vec![column("name"), column("deleted_at")];
        let filters = vec![
            filter("name", FilterOperator::Eq, Value::String("Ada".into())),
            filter("deleted_at", FilterOperator::Eq, Value::Null),
            filter("deleted_at", FilterOperator::Ne, Value::Null),
            filter("missing", FilterOperator::Eq, Value::Null),
        ];

        let normalized = normalize_filter_conditions(filters, &columns);

        assert_eq!(normalized.len(), 3);
        assert!(matches!(&normalized[0].operator, &FilterOperator::Eq));
        assert!(matches!(&normalized[1].operator, &FilterOperator::IsNull));
        assert!(matches!(
            &normalized[2].operator,
            &FilterOperator::IsNotNull
        ));
    }

    #[test]
    fn filter_prompt_columns_include_nullability_and_primary_key() {
        let mut id = column("id");
        id.nullable = false;
        id.is_primary_key = true;

        assert_eq!(
            format_filter_columns(&[id, column("name")]),
            "  id TEXT NOT NULL PK\n  name TEXT NULL"
        );
    }
}
