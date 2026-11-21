use super::*;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn language_map_direct_and_alias_values_are_pinned() {
    assert_eq!(sql_format_language(None, None), "sql");
    assert_eq!(sql_format_language(Some(""), None), "sql");
    assert_eq!(sql_format_language(Some("postgresql"), None), "postgresql");
    assert_eq!(sql_format_language(Some("mysql"), None), "mysql");
    assert_eq!(sql_format_language(Some("mariadb"), None), "mariadb");
    assert_eq!(sql_format_language(Some("sqlite"), None), "sqlite");
    assert_eq!(sql_format_language(Some("sqlserver"), None), "transactsql");
    assert_eq!(sql_format_language(Some("tsql"), None), "transactsql");
    assert_eq!(sql_format_language(Some("oracle"), None), "sql");
    assert_eq!(LANGUAGE_MAP.len(), 6);
}

#[test]
fn dialect_lookup_is_only_consulted_for_unmapped_database_types() {
    let lookup = |db: &str| match db {
        "cockroach" => Some("postgresql".to_owned()),
        "mystery" => Some("not-mapped".to_owned()),
        _ => None,
    };
    assert_eq!(
        sql_format_language(Some("cockroach"), Some(&lookup)),
        "postgresql"
    );
    assert_eq!(sql_format_language(Some("mystery"), Some(&lookup)), "sql");
    assert_eq!(
        sql_format_language(Some("postgresql"), Some(&lookup)),
        "postgresql"
    );
}

#[test]
fn indentation_maps_tab_four_spaces_and_default() {
    assert_eq!(
        sql_format_indentation("tab"),
        SqlIndent {
            tab_width: 1,
            use_tabs: true
        }
    );
    assert_eq!(
        sql_format_indentation("4spaces"),
        SqlIndent {
            tab_width: 4,
            use_tabs: false
        }
    );
    assert_eq!(
        sql_format_indentation("2spaces"),
        SqlIndent {
            tab_width: 2,
            use_tabs: false
        }
    );
    assert_eq!(
        sql_format_indentation("something-else"),
        SqlIndent {
            tab_width: 2,
            use_tabs: false
        }
    );
}

#[test]
fn the_default_options_are_frozen_equivalent() {
    let d = default_sql_format_options();
    assert_eq!(d.keyword_case, "upper");
    assert_eq!(d.indent_style, "2spaces");
    assert!(d.break_before_boolean_operators);
    assert_eq!(d.lines_between_queries, 1);
    assert_eq!(DEFAULT_KEYWORD_CASE, "upper");
    assert_eq!(DEFAULT_INDENT_STYLE, "2spaces");
    assert!(DEFAULT_BREAK_BEFORE_BOOLEAN_OPERATORS);
    assert_eq!(DEFAULT_LINES_BETWEEN_QUERIES, 1);
}

#[test]
fn wire_options_round_trip_into_the_format_config() {
    let wire: crate::SqlFormatOptions = match serde_json::from_str(
        r#"{"keywordCase":"lower","indentStyle":"tab","breakBeforeBooleanOperators":false,"linesBetweenQueries":3}"#,
    ) {
        Ok(value) => value,
        Err(_) => panic!("wire fixture must decode"),
    };
    let resolved = match SqlFormatConfig::from_wire(&wire) {
        Some(value) => value,
        None => panic!("wire options must convert"),
    };
    assert_eq!(resolved.keyword_case, "lower");
    assert_eq!(resolved.indent_style, "tab");
    assert!(!resolved.break_before_boolean_operators);
    assert_eq!(resolved.lines_between_queries, 3);
}

#[test]
fn blank_sql_is_returned_untouched_without_calling_the_formatter() {
    let called = Rc::new(RefCell::new(false));
    let flag = Rc::clone(&called);
    let formatter = move |sql: &str, _: &SqlFormatRequest| {
        *flag.borrow_mut() = true;
        sql.to_owned()
    };
    assert_eq!(format_sql("   ", None, None, None, &formatter), "   ");
    assert_eq!(format_sql("", None, None, None, &formatter), "");
    assert!(!*called.borrow());
}

#[test]
fn format_sql_trims_and_pins_every_formatter_argument() {
    let seen = Rc::new(RefCell::new(Vec::<SqlFormatRequest>::new()));
    let sink = Rc::clone(&seen);
    let formatter = move |sql: &str, request: &SqlFormatRequest| {
        assert_eq!(sql, "select 1");
        sink.borrow_mut().push(request.clone());
        "FORMATTED".to_owned()
    };
    let result = format_sql("  select 1 \n", Some("sqlserver"), None, None, &formatter);
    assert_eq!(result, "FORMATTED");
    let requests = seen.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].language, "transactsql");
    assert_eq!(requests[0].keyword_case, "upper");
    assert_eq!(requests[0].indent_style, "standard");
    assert_eq!(requests[0].tab_width, 2);
    assert!(!requests[0].use_tabs);
    assert_eq!(requests[0].logical_operator_newline, "before");
    assert_eq!(requests[0].lines_between_queries, 1);
}

#[test]
fn override_options_drive_tab_width_and_newline_placement() {
    let seen = Rc::new(RefCell::new(Vec::<SqlFormatRequest>::new()));
    let sink = Rc::clone(&seen);
    let formatter = move |sql: &str, request: &SqlFormatRequest| {
        sink.borrow_mut().push(request.clone());
        sql.to_owned()
    };
    let options = SqlFormatConfig {
        keyword_case: "preserve".to_owned(),
        indent_style: "tab".to_owned(),
        break_before_boolean_operators: false,
        lines_between_queries: 0,
    };
    assert_eq!(
        format_sql("select 1", Some("sqlite"), Some(&options), None, &formatter),
        "select 1"
    );
    let requests = seen.borrow();
    assert_eq!(requests[0].language, "sqlite");
    assert_eq!(requests[0].keyword_case, "preserve");
    assert_eq!(requests[0].tab_width, 1);
    assert!(requests[0].use_tabs);
    assert_eq!(requests[0].logical_operator_newline, "after");
    assert_eq!(requests[0].lines_between_queries, 0);
}

#[test]
fn crate_root_reexports_the_format_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::SqlFormatConfig>();
    resolves::<crate::SqlIndent>();
    resolves::<crate::SqlFormatRequest>();
    resolves::<crate::SqlFormatter>();
    resolves::<crate::SqlDialectLookup>();
    assert_eq!(crate::sql_format_language(Some("tsql"), None), "transactsql");
    assert_eq!(crate::sql_format_indentation("4spaces").tab_width, 4);
    assert_eq!(crate::default_sql_format_options().keyword_case, "upper");
}
