use super::*;

fn ctx(text: &str) -> CompletionContextInput {
    CompletionContextInput { text_before_cursor: text.to_owned() }
}

fn direct(options: Vec<CompletionOption>) -> CompletionOutcome {
    CompletionOutcome::Direct(Some(CompletionResult::new(options)))
}

fn resolve(outcome: CompletionOutcome) -> Option<CompletionResult> {
    match outcome {
        CompletionOutcome::Direct(result) => result,
        CompletionOutcome::Deferred(task) => task(),
    }
}

#[test]
fn kind_and_keyword_sets_are_pinned() {
    assert_eq!(SqlCompletionKind::Column.as_str(), "column");
    assert_eq!(SqlCompletionKind::Table.as_str(), "table");
    assert_eq!(SqlCompletionKind::Any.as_str(), "any");
    assert_eq!(TABLE_CONTEXT_ALLOWED_KEYWORDS.len(), 8);
    assert_eq!(COLUMN_CONTEXT_ALLOWED_KEYWORDS.len(), 50);
    assert!(TABLE_CONTEXT_ALLOWED_KEYWORDS.contains(&"json_table"));
    assert!(COLUMN_CONTEXT_ALLOWED_KEYWORDS.contains(&"collate"));
}

#[test]
fn last_keyword_ignores_strings_and_comments() {
    let sql = "SELECT 'FROM where' -- JOIN x\n/* UPDATE y */ FROM t WHERE id = 1";
    let found = last_bare_sql_keyword_with_offset(sql).expect("a keyword");
    assert_eq!(found.0, "where");
    assert!(sql[..found.1].to_lowercase().ends_with("where"));
    assert_eq!(last_bare_sql_keyword(sql).as_deref(), Some("where"));
    assert_eq!(last_bare_sql_keyword("  123 ; "), None);
    assert_eq!(last_bare_sql_keyword("SELECT 'unterminated FROM").as_deref(), Some("select"));
}

#[test]
fn table_context_detection() {
    assert!(is_table_context(" users"));
    assert!(is_table_context(" "));
    assert!(!is_table_context(" users "), "a space after the name means the table is done");
    assert!(!is_table_context("users u"), "a second identifier is an alias");
    assert!(is_table_context(" a, b"), "the last comma segment wins");
    assert!(is_table_context(" public . users"));
}

#[test]
fn qualifier_parts_are_strict_about_the_trailing_dot() {
    assert!(extract_qualifier_parts("").is_empty());
    assert!(extract_qualifier_parts("SELECT a.b FROM t WHERE x").is_empty());
    assert_eq!(extract_qualifier_parts("eo."), vec!["eo"]);
    assert_eq!(extract_qualifier_parts("schema.table."), vec!["schema", "table"]);
    assert_eq!(extract_qualifier_parts("\"My Schema\".t."), vec!["My Schema", "t"]);
    assert_eq!(extract_qualifier_parts("`db`.`tbl`."), vec!["db", "tbl"]);
    assert_eq!(extract_qualifier_parts("[db].[tbl]."), vec!["db", "tbl"]);
}

#[test]
fn completion_kind_infers_from_the_statement() {
    assert_eq!(infer_sql_completion_kind("eo."), SqlCompletionKind::Any);
    assert_eq!(infer_sql_completion_kind("INSERT INTO t ("), SqlCompletionKind::Column);
    assert_eq!(infer_sql_completion_kind("SELECT * FROM "), SqlCompletionKind::Table);
    assert_eq!(infer_sql_completion_kind("SELECT * FROM users "), SqlCompletionKind::Any);
    assert_eq!(infer_sql_completion_kind("SELECT a WHERE "), SqlCompletionKind::Column);
    assert_eq!(infer_sql_completion_kind("SELECT 1; UPDATE "), SqlCompletionKind::Table);
    assert_eq!(infer_sql_completion_kind("garbage"), SqlCompletionKind::Any);
}

#[test]
fn alias_dot_context_distinguishes_unique_and_qualified() {
    assert_eq!(detect_alias_dot_context("SELECT * FROM t WHERE x"), ("none", String::new()));
    assert_eq!(detect_alias_dot_context("eo."), ("unique", "eo".to_owned()));
    assert_eq!(detect_alias_dot_context("schema.table."), ("none", "schema.table".to_owned()));
}

#[test]
fn completion_options_are_filtered_with_boosts() {
    let options = vec![
        CompletionOption::new("id", Some("property"), None),
        CompletionOption::new("users", Some("type"), None),
        CompletionOption::new("kept", Some("property"), Some(2.0)),
    ];
    let columns = filter_completions_by_kind(&options, SqlCompletionKind::Column);
    assert_eq!(columns.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), vec!["id", "kept"]);
    assert_eq!(columns[0].boost, Some(5.0));
    assert_eq!(columns[1].boost, Some(2.0));
    let tables = filter_completions_by_kind(&options, SqlCompletionKind::Table);
    assert_eq!(tables.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), vec!["users"]);
    assert_eq!(tables[0].boost, Some(10.0));
    assert_eq!(filter_completions_by_kind(&options, SqlCompletionKind::Any), options);
}

#[test]
fn keyword_options_are_filtered_and_deboosted() {
    let options = vec![
        CompletionOption::new("SELECT", None, None),
        CompletionOption::new("FROM", None, Some(3.0)),
        CompletionOption::new("CREATE", None, Some(9.0)),
        CompletionOption::new("where", None, None),
    ];
    let table = filter_keywords_by_kind(&options, SqlCompletionKind::Table);
    assert_eq!(table.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), vec!["SELECT"]);
    assert_eq!(table[0].boost, Some(-2.0));
    let columns = filter_keywords_by_kind(&options, SqlCompletionKind::Column);
    assert_eq!(columns.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), vec!["SELECT", "FROM", "where"]);
    assert_eq!(filter_keywords_by_kind(&options, SqlCompletionKind::Any), options);
}

#[test]
fn contextual_keyword_source_suppresses_alias_dot_and_filters() {
    let source: CompletionSourceFn = Box::new(|_| {
        direct(vec![
            CompletionOption::new("SELECT", None, None),
            CompletionOption::new("CREATE", None, None),
        ])
    });
    let wrapped = contextual_keyword_completion(source);
    assert_eq!(resolve(wrapped(&ctx("eo."))), None, "alias dot never suggests keywords");
    let result = resolve(wrapped(&ctx("SELECT * FROM "))).expect("result");
    assert_eq!(result.options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), vec!["SELECT"]);
    assert_eq!(result.options[0].boost, Some(-2.0));
}

#[test]
fn contextual_keyword_source_handles_null_and_deferred() {
    let null_source: CompletionSourceFn = Box::new(|_| CompletionOutcome::Direct(None));
    let wrapped = contextual_keyword_completion(null_source);
    assert_eq!(resolve(wrapped(&ctx("SELECT * FROM "))), None);

    let deferred: CompletionSourceFn = Box::new(|_| {
        CompletionOutcome::Deferred(Box::new(|| {
            Some(CompletionResult::new(vec![
                CompletionOption::new("where", None, None),
                CompletionOption::new("DROP", None, None),
            ]))
        }))
    });
    let wrapped = contextual_keyword_completion(deferred);
    let result = resolve(wrapped(&ctx("SELECT a WHERE "))).expect("result");
    assert_eq!(result.options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), vec!["where"]);
}

#[test]
fn contextual_schema_source_uses_the_injected_inner_and_alias_rule() {
    let inner: CompletionSourceFn = Box::new(|_| {
        direct(vec![
            CompletionOption::new("id", Some("property"), None),
            CompletionOption::new("users", Some("type"), None),
        ])
    });
    let wrapped = contextual_schema_completion(inner);
    assert_eq!(resolve(wrapped(&ctx("eo."))), None, "alias dot is left to the semantic source");
    let result = resolve(wrapped(&ctx("SELECT * FROM "))).expect("result");
    assert_eq!(result.options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), vec!["users"]);
    assert_eq!(result.options[0].boost, Some(10.0));
    let result = resolve(wrapped(&ctx("SELECT id FROM t WHERE "))).expect("result");
    assert_eq!(result.options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), vec!["id"]);
    assert_eq!(result.options[0].boost, Some(5.0));
}

#[test]
fn result_metadata_survives_filtering() {
    let inner: CompletionSourceFn = Box::new(|_| {
        let mut result = CompletionResult::new(vec![
            CompletionOption::new("users", Some("type"), None),
            CompletionOption::new("id", Some("property"), None),
        ]);
        result.span = Some((1, 5));
        CompletionOutcome::Direct(Some(result))
    });
    let wrapped = contextual_schema_completion(inner);
    let result = resolve(wrapped(&ctx("SELECT * FROM "))).expect("result");
    assert_eq!(result.span, Some((1, 5)));
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::last_bare_sql_keyword_with_offset;
    let _ = crate::last_bare_sql_keyword;
    let _ = crate::infer_sql_completion_kind;
    let _ = crate::detect_alias_dot_context;
    let _ = crate::extract_qualifier_parts;
    let _ = crate::is_table_context;
    let _ = crate::filter_completions_by_kind;
    let _ = crate::filter_keywords_by_kind;
    let _ = crate::contextual_keyword_completion;
    let _ = crate::contextual_schema_completion;
    assert_eq!(crate::TABLE_CONTEXT_ALLOWED_KEYWORDS.len(), 8);
    assert_eq!(crate::COLUMN_CONTEXT_ALLOWED_KEYWORDS.len(), 50);
    let _: Option<crate::CompletionOption> = None;
    let _: Option<crate::CompletionResult> = None;
    let _: Option<crate::CompletionContextInput> = None;
    let _: Option<crate::CompletionOutcome> = None;
    let _: Option<crate::CompletionSourceFn> = None;
    let _: Option<crate::SqlCompletionKind> = None;
}
