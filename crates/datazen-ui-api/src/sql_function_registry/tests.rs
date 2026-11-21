use super::*;

fn f(name: &str, params: usize) -> FunctionEntry {
    FunctionEntry {
        name: name.to_owned(),
        description: format!("desc {name}"),
        params: (0..params)
            .map(|index| FunctionParam {
                name: format!("p{index}"),
                param_type: "any".to_owned(),
                optional: None,
            })
            .collect(),
        dialects: None,
        returns_value: None,
    }
}

fn meta(dialect: Option<&str>, functions: Vec<FunctionEntry>) -> DriverFunctionMeta {
    DriverFunctionMeta {
        sql_dialect: dialect.map(str::to_owned),
        sql_functions: functions,
    }
}

fn names(entries: &[FunctionEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

#[test]
fn common_ansi_functions_are_exact() {
    let functions = common_functions();
    assert_eq!(functions.len(), 9);
    assert_eq!(
        names(functions),
        [
            "COUNT", "SUM", "AVG", "MIN", "MAX", "COALESCE", "NULLIF", "CAST", "CONCAT"
        ]
    );
    assert_eq!(COMMON_FUNCTION_NAMES.len(), 9);
    assert_eq!(functions[0].description, "Count rows or non-null values");
    assert_eq!(functions[0].params[0].name, "expr");
    assert_eq!(functions[0].params[0].param_type, "any");
    assert_eq!(functions[1].description, "Sum of numeric values");
    assert_eq!(functions[1].params[0].param_type, "number");
    assert_eq!(functions[2].description, "Average of numeric values");
    assert_eq!(functions[3].description, "Minimum value");
    assert_eq!(functions[4].description, "Maximum value");
    assert_eq!(functions[5].description, "First non-null value");
    assert_eq!(functions[5].params.len(), 2);
    assert_eq!(functions[5].params[0].name, "expr1");
    assert_eq!(functions[5].params[1].name, "expr2");
    assert_eq!(functions[6].description, "NULL if two values are equal");
    assert_eq!(functions[6].params[0].name, "value1");
    assert_eq!(functions[6].params[1].name, "value2");
    assert_eq!(functions[7].description, "Convert value to another type");
    assert_eq!(functions[8].description, "Concatenate strings");
    assert_eq!(functions[8].params[0].param_type, "string");
    assert_eq!(functions[8].params[1].param_type, "string");
    assert!(functions.iter().all(|entry| entry.dialects.is_none()));
    assert!(functions.iter().all(|entry| entry.returns_value.is_none()));
}

#[test]
fn dialect_aliases_are_case_insensitive_and_exact() {
    assert_eq!(resolve_dialect_family("POSTGRES"), "postgresql");
    assert_eq!(resolve_dialect_family("pg"), "postgresql");
    assert_eq!(resolve_dialect_family("cockroach"), "postgresql");
    assert_eq!(resolve_dialect_family("MariaDB"), "mysql");
    assert_eq!(resolve_dialect_family("tidb"), "mysql");
    assert_eq!(resolve_dialect_family("mssql"), "sqlserver");
    assert_eq!(resolve_dialect_family("SQLite"), "sqlite");
    assert_eq!(DIALECT_ALIASES.len(), 6);
}

#[test]
fn absent_dialect_collects_all_driver_functions_deduped_by_name_and_arity() {
    let registry: DriverFunctionRegistry = vec![
        (
            "postgres".to_owned(),
            meta(None, vec![f("NOW", 0), f("count", 1)]),
        ),
        (
            "mysql".to_owned(),
            meta(None, vec![f("NOW", 0), f("NOW", 1), f("COUNT", 1)]),
        ),
    ];
    let result = get_driver_functions(None, &registry);
    assert_eq!(names(&result), ["NOW", "count", "NOW"]);
    assert_eq!(result[1].params.len(), 1);
}

#[test]
fn direct_canonical_and_normalized_lookups_win() {
    let registry: DriverFunctionRegistry = vec![
        (
            "postgresql".to_owned(),
            meta(None, vec![f("PG_FN", 0)]),
        ),
        ("cockroach".to_owned(), meta(None, vec![f("AliasFn", 0)])),
    ];
    assert_eq!(names(&get_driver_functions(Some("postgres"), &registry)), ["PG_FN"]);
    assert_eq!(names(&get_driver_functions(Some("Cockroach"), &registry)), ["PG_FN"]);

    let normalized_only: DriverFunctionRegistry =
        vec![("cockroach".to_owned(), meta(None, vec![f("AliasFn", 0)]))];
    assert_eq!(
        names(&get_driver_functions(Some("Cockroach"), &normalized_only)),
        ["AliasFn"]
    );
}

#[test]
fn sql_dialect_family_match_is_the_second_resolution_step() {
    let registry: DriverFunctionRegistry = vec![(
        "duckdb".to_owned(),
        meta(Some("PostgreSQL"), vec![f("DuckFn", 0)]),
    )];
    assert_eq!(names(&get_driver_functions(Some("pg"), &registry)), ["DuckFn"]);
}

#[test]
fn empty_driver_functions_fall_through_and_unknown_dialects_yield_nothing() {
    let registry: DriverFunctionRegistry = vec![
        ("postgresql".to_owned(), meta(None, Vec::new())),
        (
            "other".to_owned(),
            meta(Some("postgresql"), vec![f("FamilyFn", 0)]),
        ),
    ];
    assert_eq!(
        names(&get_driver_functions(Some("postgres"), &registry)),
        ["FamilyFn"]
    );
    assert!(get_driver_functions(Some("oracle"), &registry).is_empty());
    assert!(get_driver_functions(None, &Vec::new()).is_empty());
}

#[test]
fn function_entries_are_common_functions_then_driver_functions() {
    let registry: DriverFunctionRegistry = vec![(
        "sqlite".to_owned(),
        meta(Some("sqlite"), vec![f("SQLITE_FN", 0)]),
    )];
    let entries = get_function_entries(Some("sqlite"), &registry);
    assert_eq!(entries.len(), 10);
    assert_eq!(names(&entries)[0], "COUNT");
    assert_eq!(names(&entries)[8], "CONCAT");
    assert_eq!(names(&entries)[9], "SQLITE_FN");
    assert_eq!(get_function_entries(None, &Vec::new()).len(), 9);
}

#[test]
fn build_function_completions_pins_label_type_detail_and_apply() {
    let registry: DriverFunctionRegistry = vec![(
        "mysql".to_owned(),
        meta(Some("mysql"), vec![f("GROUP_CONCAT", 1)]),
    )];
    let completions = build_function_completions(Some("mysql"), &registry);
    assert_eq!(completions.len(), 10);
    assert_eq!(completions[0].label, "COUNT");
    assert_eq!(completions[0].completion_type, "function");
    assert_eq!(completions[0].detail, "Count rows or non-null values");
    assert_eq!(completions[0].apply, "COUNT(");
    assert_eq!(completions[9].label, "GROUP_CONCAT");
    assert_eq!(completions[9].apply, "GROUP_CONCAT(");
    assert_eq!(completions[9].detail, "desc GROUP_CONCAT");
}

#[test]
fn completions_adapt_to_the_existing_completion_seam() {
    let adapted = to_sql_function_completions(build_function_completions(None, &Vec::new()));
    assert_eq!(adapted.len(), 9);
    assert_eq!(adapted[0].label, "COUNT");
    assert_eq!(adapted[0].completion_type, "function");
    assert_eq!(adapted[0].apply, "COUNT(");
}

#[test]
fn crate_root_reexports_the_function_registry_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::FunctionEntry>();
    resolves::<crate::FunctionParam>();
    resolves::<crate::FunctionCompletion>();
    resolves::<crate::DriverFunctionMeta>();
    resolves::<crate::DriverFunctionRegistry>();
    assert_eq!(crate::common_functions().len(), 9);
    assert_eq!(crate::resolve_dialect_family("mssql"), "sqlserver");
    assert_eq!(crate::get_function_entries(None, &Vec::new()).len(), 9);
    assert_eq!(crate::build_function_completions(None, &Vec::new()).len(), 9);
}
