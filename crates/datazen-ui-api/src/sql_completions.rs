#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlFunctionCompletion {
    pub label: String,
    pub completion_type: String,
    pub apply: String,
}

pub const SQL_COMPLETION_KEYWORDS: [(&str, &str, &str); 2] = [
    ("CASE", "keyword", "CASE "),
    ("EXISTS", "keyword", "EXISTS ("),
];

pub const DEFAULT_SQL_COMPLETION_DIALECT: &str = "standard";

pub fn keyword_completions() -> Vec<SqlFunctionCompletion> {
    SQL_COMPLETION_KEYWORDS
        .iter()
        .map(|(label, completion_type, apply)| SqlFunctionCompletion {
            label: (*label).to_owned(),
            completion_type: (*completion_type).to_owned(),
            apply: (*apply).to_owned(),
        })
        .collect()
}

pub fn sql_function_completions<F>(
    database_type: Option<&str>,
    build_function_completions: F,
) -> Vec<SqlFunctionCompletion>
where
    F: Fn(Option<&str>) -> Vec<SqlFunctionCompletion>,
{
    let mut completions = build_function_completions(database_type);
    completions.extend(keyword_completions());
    completions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(label: &str) -> SqlFunctionCompletion {
        SqlFunctionCompletion {
            label: label.to_owned(),
            completion_type: "function".to_owned(),
            apply: format!("{label}("),
        }
    }

    #[test]
    fn keyword_completions_are_pinned() {
        let keywords = keyword_completions();
        assert_eq!(keywords.len(), 2);
        assert_eq!(keywords[0].label, "CASE");
        assert_eq!(keywords[0].completion_type, "keyword");
        assert_eq!(keywords[0].apply, "CASE ");
        assert_eq!(keywords[1].label, "EXISTS");
        assert_eq!(keywords[1].completion_type, "keyword");
        assert_eq!(keywords[1].apply, "EXISTS (");
        assert_eq!(DEFAULT_SQL_COMPLETION_DIALECT, "standard");
    }

    #[test]
    fn function_completions_precede_the_two_keywords() {
        let completions = sql_function_completions(Some("postgresql"), |dialect| {
            assert_eq!(dialect, Some("postgresql"));
            vec![function("COUNT"), function("SUM")]
        });
        assert_eq!(
            completions
                .iter()
                .map(|value| value.label.as_str())
                .collect::<Vec<_>>(),
            ["COUNT", "SUM", "CASE", "EXISTS"]
        );
        assert_eq!(completions[0].completion_type, "function");
        assert_eq!(completions[2].completion_type, "keyword");
    }

    #[test]
    fn absent_dialect_is_forwarded_and_keywords_still_appear() {
        let completions = sql_function_completions(None, |dialect| {
            assert_eq!(dialect, None);
            Vec::new()
        });
        assert_eq!(
            completions
                .iter()
                .map(|value| value.label.as_str())
                .collect::<Vec<_>>(),
            ["CASE", "EXISTS"]
        );
    }

    #[test]
    fn duplicate_labels_are_not_deduplicated() {
        let completions = sql_function_completions(Some("sqlite"), |_| vec![function("CASE")]);
        assert_eq!(
            completions
                .iter()
                .map(|value| value.label.as_str())
                .collect::<Vec<_>>(),
            ["CASE", "CASE", "EXISTS"]
        );
    }
}
