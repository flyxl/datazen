pub const CANDIDATE_VALUE_LIMIT: u32 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowLimitPlacement {
    SelectList,
    Trailing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowLimitShape {
    pub placement: RowLimitPlacement,
    pub prefix: &'static str,
    pub suffix: &'static str,
}

impl RowLimitShape {
    pub fn clause(self, limit_text: &str) -> String {
        format!("{}{}{}", self.prefix, limit_text, self.suffix)
    }
}

pub const LIMIT_ROW_LIMIT: RowLimitShape = RowLimitShape {
    placement: RowLimitPlacement::Trailing,
    prefix: "LIMIT ",
    suffix: "",
};
pub const TOP_ROW_LIMIT: RowLimitShape = RowLimitShape {
    placement: RowLimitPlacement::SelectList,
    prefix: "TOP ",
    suffix: "",
};
pub const FETCH_FIRST_ROW_LIMIT: RowLimitShape = RowLimitShape {
    placement: RowLimitPlacement::Trailing,
    prefix: "FETCH FIRST ",
    suffix: " ROWS ONLY",
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentifierQuote {
    None,
    Backtick,
    DoubleQuote,
    Bracket,
}

impl IdentifierQuote {
    pub fn escape(self, name: &str) -> String {
        match self {
            Self::None => name.to_owned(),
            Self::Backtick => format!("`{}`", name.replace('`', "``")),
            Self::DoubleQuote => format!("\"{}\"", name.replace('"', "\"\"")),
            Self::Bracket => format!("[{}]", name.replace(']', "]]")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateDialect {
    pub quote: IdentifierQuote,
    pub row_limit: RowLimitShape,
}

pub struct DistinctCandidatesArgs<'a> {
    pub table: &'a str,
    pub column: &'a str,
    pub schema: Option<&'a str>,
    pub database: Option<&'a str>,
    pub dialect: Option<CandidateDialect>,
    pub limit: Option<f64>,
}

pub fn normalized_limit(limit: Option<f64>) -> String {
    let value = limit.unwrap_or(CANDIDATE_VALUE_LIMIT as f64);
    let truncated = value.trunc();
    if truncated.is_nan() {
        return "NaN".to_owned();
    }
    let capped = truncated.max(1.0).min(CANDIDATE_VALUE_LIMIT as f64);
    format!("{}", capped as i64)
}

pub fn build_row_limited_select(
    shape: Option<RowLimitShape>,
    distinct: bool,
    columns: &[String],
    table: &str,
    limit_text: &str,
) -> String {
    let shape = shape.unwrap_or(LIMIT_ROW_LIMIT);
    let head = if distinct { "SELECT DISTINCT" } else { "SELECT" };
    let joined = columns.join(", ");
    let clause = shape.clause(limit_text);
    if shape.placement == RowLimitPlacement::SelectList {
        format!("{head} {clause} {joined} FROM {table}")
    } else {
        format!("{head} {joined} FROM {table} {clause}")
    }
}

pub fn build_distinct_candidates_sql(args: &DistinctCandidatesArgs<'_>) -> Option<String> {
    let column = args.column.trim();
    if column.is_empty() {
        return None;
    }
    let table = args.table.trim();
    if table.is_empty() {
        return None;
    }
    let limit_text = normalized_limit(args.limit);
    let (quote, row_limit) = match args.dialect {
        Some(dialect) => (dialect.quote, Some(dialect.row_limit)),
        None => (IdentifierQuote::DoubleQuote, None),
    };
    let quoted_column = quote.escape(column);
    let schema = args.schema.map(str::trim).filter(|value| !value.is_empty());
    let quoted_table = match schema {
        Some(schema) => format!("{}.{}", quote.escape(schema), quote.escape(table)),
        None => quote.escape(table),
    };
    Some(build_row_limited_select(
        row_limit,
        true,
        &[quoted_column],
        &quoted_table,
        &limit_text,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args<'a>(table: &'a str, column: &'a str) -> DistinctCandidatesArgs<'a> {
        DistinctCandidatesArgs {
            table,
            column,
            schema: None,
            database: None,
            dialect: None,
            limit: None,
        }
    }

    #[test]
    fn the_default_statement_uses_double_quotes_and_trailing_limit() {
        assert_eq!(
            build_distinct_candidates_sql(&args("users", "status")).as_deref(),
            Some("SELECT DISTINCT \"status\" FROM \"users\" LIMIT 200")
        );
        assert_eq!(CANDIDATE_VALUE_LIMIT, 200);
    }

    #[test]
    fn a_blank_column_or_table_produces_no_statement() {
        assert_eq!(build_distinct_candidates_sql(&args("users", "  ")), None);
        assert_eq!(build_distinct_candidates_sql(&args(" ", "status")), None);
        assert_eq!(build_distinct_candidates_sql(&args("", "")), None);
    }

    #[test]
    fn identifiers_and_schema_are_trimmed() {
        let mut request = args("  users  ", "  status  ");
        request.schema = Some("  public  ");
        assert_eq!(
            build_distinct_candidates_sql(&request).as_deref(),
            Some("SELECT DISTINCT \"status\" FROM \"public\".\"users\" LIMIT 200")
        );
    }

    #[test]
    fn an_empty_schema_is_omitted() {
        let mut request = args("users", "status");
        request.schema = Some("   ");
        assert_eq!(
            build_distinct_candidates_sql(&request).as_deref(),
            Some("SELECT DISTINCT \"status\" FROM \"users\" LIMIT 200")
        );
    }

    #[test]
    fn limits_are_truncated_and_clamped_to_one_and_two_hundred() {
        for (input, expected) in [
            (None, "200"),
            (Some(0.0), "1"),
            (Some(-8.0), "1"),
            (Some(0.9), "1"),
            (Some(2.9), "2"),
            (Some(200.0), "200"),
            (Some(500.0), "200"),
            (Some(f64::INFINITY), "200"),
            (Some(f64::NEG_INFINITY), "1"),
        ] {
            let mut request = args("users", "status");
            request.limit = input;
            assert_eq!(normalized_limit(input), expected, "{input:?}");
            let expected_sql = format!("SELECT DISTINCT \"status\" FROM \"users\" LIMIT {expected}");
            assert_eq!(
                build_distinct_candidates_sql(&request).as_deref(),
                Some(expected_sql.as_str()),
                "{input:?}"
            );
        }
    }

    #[test]
    fn a_nan_limit_stays_visible_in_the_statement() {
        assert_eq!(normalized_limit(Some(f64::NAN)), "NaN");
        let mut request = args("users", "status");
        request.limit = Some(f64::NAN);
        assert_eq!(
            build_distinct_candidates_sql(&request).as_deref(),
            Some("SELECT DISTINCT \"status\" FROM \"users\" LIMIT NaN")
        );
    }

    #[test]
    fn a_backtick_dialect_doubles_embedded_backticks() {
        let mut request = args("we`ird", "co`l");
        request.dialect = Some(CandidateDialect {
            quote: IdentifierQuote::Backtick,
            row_limit: LIMIT_ROW_LIMIT,
        });
        assert_eq!(
            build_distinct_candidates_sql(&request).as_deref(),
            Some("SELECT DISTINCT `co``l` FROM `we``ird` LIMIT 200")
        );
    }

    #[test]
    fn a_bracket_dialect_doubles_embedded_closing_brackets() {
        let mut request = args("we]ird", "co]l");
        request.dialect = Some(CandidateDialect {
            quote: IdentifierQuote::Bracket,
            row_limit: LIMIT_ROW_LIMIT,
        });
        assert_eq!(
            build_distinct_candidates_sql(&request).as_deref(),
            Some("SELECT DISTINCT [co]]l] FROM [we]]ird] LIMIT 200")
        );
    }

    #[test]
    fn a_dialect_without_quoting_leaves_identifiers_bare() {
        let mut request = args("users", "status");
        request.dialect = Some(CandidateDialect {
            quote: IdentifierQuote::None,
            row_limit: LIMIT_ROW_LIMIT,
        });
        assert_eq!(
            build_distinct_candidates_sql(&request).as_deref(),
            Some("SELECT DISTINCT status FROM users LIMIT 200")
        );
    }

    #[test]
    fn a_select_list_cap_places_the_clause_before_the_columns() {
        let mut request = args("users", "status");
        request.limit = Some(5.0);
        request.dialect = Some(CandidateDialect {
            quote: IdentifierQuote::DoubleQuote,
            row_limit: TOP_ROW_LIMIT,
        });
        assert_eq!(
            build_distinct_candidates_sql(&request).as_deref(),
            Some("SELECT DISTINCT TOP 5 \"status\" FROM \"users\"")
        );
    }

    #[test]
    fn a_fetch_first_cap_places_the_clause_at_the_end() {
        let mut request = args("users", "status");
        request.limit = Some(7.0);
        request.dialect = Some(CandidateDialect {
            quote: IdentifierQuote::DoubleQuote,
            row_limit: FETCH_FIRST_ROW_LIMIT,
        });
        assert_eq!(
            build_distinct_candidates_sql(&request).as_deref(),
            Some("SELECT DISTINCT \"status\" FROM \"users\" FETCH FIRST 7 ROWS ONLY")
        );
    }

    #[test]
    fn the_database_argument_does_not_change_the_statement() {
        let mut request = args("users", "status");
        request.database = Some("sales");
        assert_eq!(
            build_distinct_candidates_sql(&request),
            build_distinct_candidates_sql(&args("users", "status"))
        );
    }

    #[test]
    fn the_statement_is_always_one_statement_without_a_semicolon() {
        let sql = build_distinct_candidates_sql(&args("users", "status"));
        assert!(sql.is_some());
        let sql = sql.unwrap_or_default();
        assert!(!sql.contains(';'));
        assert!(sql.starts_with("SELECT DISTINCT "));
    }

    #[test]
    fn clause_rendering_pins_the_three_dialect_shapes() {
        assert_eq!(LIMIT_ROW_LIMIT.placement, RowLimitPlacement::Trailing);
        assert_eq!(LIMIT_ROW_LIMIT.clause("12"), "LIMIT 12");
        assert_eq!(TOP_ROW_LIMIT.placement, RowLimitPlacement::SelectList);
        assert_eq!(TOP_ROW_LIMIT.clause("12"), "TOP 12");
        assert_eq!(FETCH_FIRST_ROW_LIMIT.clause("12"), "FETCH FIRST 12 ROWS ONLY");
    }
}
