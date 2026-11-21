use super::*;
use std::collections::BTreeMap;

fn s(v: &str) -> ParamValue {
    ParamValue::Str(v.to_owned())
}

fn vals(pairs: &[(&str, ParamValue)]) -> BTreeMap<String, ParamValue> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

fn named(name: &str, syntax: SqlParamSyntax) -> SqlParam {
    SqlParam {
        name: name.to_owned(),
        kind: SqlParamKind::Named,
        syntax,
        stable_id: named_id(name),
        ordinal: None,
    }
}

fn positional(name: &str, id: &str, syntax: SqlParamSyntax, ordinal: Option<usize>) -> SqlParam {
    SqlParam {
        name: name.to_owned(),
        kind: SqlParamKind::Positional,
        syntax,
        stable_id: id.to_owned(),
        ordinal,
    }
}

fn occ(
    from: usize,
    to: usize,
    id: &str,
    token: &str,
    syntax: SqlParamSyntax,
    name: &str,
    ordinal: Option<usize>,
) -> SqlParamOccurrence {
    SqlParamOccurrence {
        from,
        to,
        id: id.to_owned(),
        token: token.to_owned(),
        syntax,
        name: name.to_owned(),
        ordinal,
    }
}

#[test]
fn the_syntax_and_kind_wire_literals_are_pinned() {
    assert_eq!(SqlParamSyntax::Colon.as_str(), "colon");
    assert_eq!(SqlParamSyntax::At.as_str(), "at");
    assert_eq!(SqlParamSyntax::DollarPositional.as_str(), "dollar-positional");
    assert_eq!(SqlParamSyntax::DollarNamed.as_str(), "dollar-named");
    assert_eq!(SqlParamSyntax::Question.as_str(), "question");
    assert_eq!(SqlParamSyntax::Template.as_str(), "template");
    assert_eq!(SqlParamKind::Named.as_str(), "named");
    assert_eq!(SqlParamKind::Positional.as_str(), "positional");
    assert!(SqlParamSyntax::DollarPositional.is_positional());
    assert!(SqlParamSyntax::Question.is_positional());
    assert!(!SqlParamSyntax::DollarNamed.is_positional());
    assert!(!SqlParamSyntax::Colon.is_positional());
}

#[test]
fn the_default_policy_flags_are_pinned() {
    let p = default_dialect_policy();
    assert_eq!(p.enable_at, Some(true));
    assert_eq!(p.enable_question, Some(true));
    assert_eq!(p.enable_dollar_positional, Some(true));
    assert_eq!(p.enable_dollar_named, Some(false));
    assert_eq!(p.enable_template, Some(true));
    assert!(p.filter_occurrences.is_none());
    assert_eq!(DEFAULT_ENABLE_AT, true);
    assert_eq!(DEFAULT_ENABLE_QUESTION, true);
    assert_eq!(DEFAULT_ENABLE_DOLLAR_POSITIONAL, true);
    assert_eq!(DEFAULT_ENABLE_DOLLAR_NAMED, false);
    assert_eq!(DEFAULT_ENABLE_TEMPLATE, true);
}

#[test]
fn colon_and_dollar_placeholders_keep_exact_spans() {
    let sql = "SELECT * FROM t WHERE id = :uid AND n = $1";
    assert_eq!(
        parse_sql_param_occurrences(sql, None),
        vec![
            occ(27, 31, "named:uid", ":uid", SqlParamSyntax::Colon, "uid", None),
            occ(
                40,
                42,
                "dollar:1",
                "$1",
                SqlParamSyntax::DollarPositional,
                "1",
                None
            ),
        ]
    );
    assert_eq!(
        parse_sql_params(sql, None),
        vec![
            named("uid", SqlParamSyntax::Colon),
            positional("1", "dollar:1", SqlParamSyntax::DollarPositional, None),
        ]
    );
}

#[test]
fn placeholders_inside_strings_comments_and_backticks_are_skipped() {
    assert_eq!(
        parse_sql_params("SELECT ':uid' WHERE x = :uid", None),
        vec![named("uid", SqlParamSyntax::Colon)]
    );
    assert_eq!(
        parse_sql_params("-- :skip\nSELECT :keep /* $9 */", None),
        vec![named("keep", SqlParamSyntax::Colon)]
    );
    assert_eq!(
        parse_sql_params("SELECT `:hid` FROM t WHERE x = :hid", None),
        vec![named("hid", SqlParamSyntax::Colon)]
    );
}

#[test]
fn doubled_single_quotes_stay_inside_the_literal() {
    let sql = "SELECT 'it''s :hidden', :shown";
    assert_eq!(
        parse_sql_params(sql, None),
        vec![named("shown", SqlParamSyntax::Colon)]
    );
}

#[test]
fn casts_and_bare_colons_are_not_placeholders() {
    assert_eq!(
        parse_sql_params("SELECT 1::int, :ok", None),
        vec![named("ok", SqlParamSyntax::Colon)]
    );
    assert!(parse_sql_params("SELECT : , ::", None).is_empty());
}

#[test]
fn repeated_stable_ids_are_deduped_in_first_appearance_order() {
    assert_eq!(
        parse_sql_params("SELECT :id, :id", None),
        vec![named("id", SqlParamSyntax::Colon)]
    );
    assert_eq!(
        parse_sql_params("SELECT :name, @name, ${name}", None)
            .iter()
            .map(|p| p.stable_id.clone())
            .collect::<Vec<_>>(),
        vec!["named:name"]
    );
}

#[test]
fn five_syntax_families_share_named_ids_and_positional_ordinals() {
    let sql = "SELECT :col, @col, ${col}, ?, $1";
    assert_eq!(
        parse_sql_params(sql, None),
        vec![
            named("col", SqlParamSyntax::Colon),
            positional("1", "question:1", SqlParamSyntax::Question, Some(1)),
            positional("1", "dollar:1", SqlParamSyntax::DollarPositional, None),
        ]
    );
    assert_eq!(
        param_fingerprint(&parse_sql_params(sql, None)),
        "named:col,question:1,dollar:1"
    );
    assert_eq!(
        param_fingerprint(&[]),
        ""
    );
}

#[test]
fn all_five_syntaxes_with_explicit_policy_keep_their_own_occurrences() {
    let sql = "SELECT @name, ${name}, ?, $2";
    let policy = SqlParamDialectPolicy {
        enable_at: Some(true),
        enable_question: Some(true),
        enable_template: Some(true),
        enable_dollar_positional: Some(true),
        ..Default::default()
    };
    assert_eq!(
        parse_sql_params(sql, Some(&policy))
            .iter()
            .map(|p| p.stable_id.clone())
            .collect::<Vec<_>>(),
        vec!["named:name", "question:1", "dollar:2"]
    );
    assert_eq!(
        parse_sql_param_occurrences(sql, Some(&policy)),
        vec![
            occ(7, 12, "named:name", "@name", SqlParamSyntax::At, "name", None),
            occ(
                14,
                21,
                "named:name",
                "${name}",
                SqlParamSyntax::Template,
                "name",
                None
            ),
            occ(23, 24, "question:1", "?", SqlParamSyntax::Question, "1", Some(1)),
            occ(
                26,
                28,
                "dollar:2",
                "$2",
                SqlParamSyntax::DollarPositional,
                "2",
                None
            ),
        ]
    );
}

#[test]
fn at_system_variables_and_dialect_local_decisions() {
    let sql = "DECLARE @limit INT; SELECT TOP (@limit) * FROM t WHERE id = @id";
    let policy = SqlParamDialectPolicy {
        enable_at: Some(true),
        ..Default::default()
    };
    assert_eq!(
        parse_sql_param_occurrences(sql, Some(&policy))
            .iter()
            .map(|o| o.name.clone())
            .collect::<Vec<_>>(),
        vec!["limit", "limit", "id"]
    );
    assert_eq!(
        parse_sql_params("SELECT @@version, @user", Some(&policy)),
        vec![named("user", SqlParamSyntax::At)]
    );
}

#[test]
fn dollar_named_placeholders_are_opt_in() {
    let policy = SqlParamDialectPolicy {
        enable_dollar_named: Some(true),
        ..Default::default()
    };
    assert_eq!(
        parse_sql_params("SELECT $customer_id", Some(&policy)),
        vec![named("customer_id", SqlParamSyntax::DollarNamed)]
    );
    assert!(parse_sql_params("SELECT $customer_id", None).is_empty());
}

#[test]
fn question_ordinals_are_consecutive_and_disableable() {
    assert_eq!(
        parse_sql_param_occurrences("SELECT ? , ?", None),
        vec![
            occ(7, 8, "question:1", "?", SqlParamSyntax::Question, "1", Some(1)),
            occ(11, 12, "question:2", "?", SqlParamSyntax::Question, "2", Some(2)),
        ]
    );
    assert_eq!(
        parse_sql_params("SELECT ?, ?, ?, ?", None)
            .iter()
            .map(|p| p.ordinal)
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2), Some(3), Some(4)]
    );
    let off = SqlParamDialectPolicy {
        enable_question: Some(false),
        ..Default::default()
    };
    assert!(parse_sql_param_occurrences("SELECT data ? 'key' , plain ?", Some(&off)).is_empty());
}

#[test]
fn dollar_quoted_bodies_are_skipped() {
    let sql = "SELECT $tag$ :hidden $tag$ , :visible";
    assert_eq!(
        parse_sql_param_occurrences(sql, None),
        vec![occ(
            29,
            37,
            "named:visible",
            ":visible",
            SqlParamSyntax::Colon,
            "visible",
            None
        )]
    );
}

#[test]
fn driver_strategy_filters_by_from_to_and_id() {
    let policy = SqlParamDialectPolicy {
        filter_occurrences: Some(Box::new(|_sql: &str, occs: &[SqlParamOccurrence]| {
            occs.iter()
                .filter(|o| o.name != "skip")
                .cloned()
                .collect::<Vec<_>>()
        })),
        ..Default::default()
    };
    assert_eq!(
        parse_sql_params("SELECT @keep, @skip", Some(&policy)),
        vec![named("keep", SqlParamSyntax::At)]
    );

    let rewrite = SqlParamDialectPolicy {
        filter_occurrences: Some(Box::new(|_sql: &str, occs: &[SqlParamOccurrence]| {
            occs.iter()
                .map(|o| {
                    let mut copy = o.clone();
                    copy.token = "X".to_owned();
                    copy.name = "X".to_owned();
                    copy
                })
                .collect::<Vec<_>>()
        })),
        ..Default::default()
    };
    assert_eq!(
        parse_sql_param_occurrences("SELECT :a, :b", Some(&rewrite)),
        parse_sql_param_occurrences("SELECT :a, :b", None)
    );
}

#[test]
fn coerce_param_value_covers_numbers_bools_and_null() {
    assert_eq!(coerce_param_value(&s("42")), ParamValue::Number(42.0));
    assert_eq!(coerce_param_value(&s(" -3.5 ")), ParamValue::Number(-3.5));
    assert_eq!(coerce_param_value(&s("true")), ParamValue::Bool(true));
    assert_eq!(coerce_param_value(&s("FALSE")), ParamValue::Bool(false));
    assert_eq!(coerce_param_value(&s("null")), ParamValue::Null);
    assert_eq!(coerce_param_value(&s("")), ParamValue::Null);
    assert_eq!(coerce_param_value(&s("  ")), ParamValue::Null);
    assert_eq!(coerce_param_value(&s("O'Brien")), s("O'Brien"));
    assert_eq!(coerce_param_value(&s("  hi  ")), s("  hi  "));
    assert_eq!(coerce_param_value(&ParamValue::Null), ParamValue::Null);
    assert_eq!(coerce_param_value(&ParamValue::Bool(true)), ParamValue::Bool(true));
    assert_eq!(coerce_param_value(&ParamValue::Number(7.0)), ParamValue::Number(7.0));
    assert_eq!(coerce_param_value(&s("1e3")), s("1e3"));
    assert_eq!(coerce_param_value(&s("1.")), s("1."));
    assert_eq!(coerce_param_value(&s("+1")), s("+1"));
}

#[test]
fn params_to_payload_falls_back_from_stable_id_to_name() {
    let params = vec![named("id", SqlParamSyntax::Colon), named("flag", SqlParamSyntax::Colon)];
    let raw = vals(&[("named:id", s("7")), ("flag", s("false"))]);
    assert_eq!(
        params_to_payload(&params, &raw),
        vals(&[("id", ParamValue::Number(7.0)), ("flag", ParamValue::Bool(false))])
    );
    let missing = positional("x", "dollar:1", SqlParamSyntax::DollarPositional, None);
    assert_eq!(
        params_to_payload(&[missing], &vals(&[])),
        vals(&[("x", ParamValue::Null)])
    );
}

#[test]
fn bind_payload_v2_uses_stable_ids_and_keeps_occurrence_spans() {
    let sql = "SELECT :id";
    let payload = build_bind_payload_v2(sql, &vals(&[("named:id", s("42"))]), None);
    assert_eq!(payload.version, 2);
    assert_eq!(payload.values, vals(&[("named:id", ParamValue::Number(42.0))]));
    assert_eq!(
        payload.occurrences,
        vec![SqlParamOccurrenceV2 {
            from: 7,
            to: 10,
            id: "named:id".to_owned(),
            token: ":id".to_owned(),
        }]
    );
    let empty = build_bind_payload_v2("SELECT 1", &vals(&[]), None);
    assert!(empty.values.is_empty());
    assert!(empty.occurrences.is_empty());
}

#[test]
fn get_param_label_returns_original_syntax() {
    assert_eq!(get_param_label(&named("x", SqlParamSyntax::Colon)), ":x");
    assert_eq!(get_param_label(&named("x", SqlParamSyntax::At)), "@x");
    assert_eq!(
        get_param_label(&positional("1", "dollar:1", SqlParamSyntax::DollarPositional, None)),
        "$1"
    );
    assert_eq!(
        get_param_label(&named("x", SqlParamSyntax::DollarNamed)),
        "$x"
    );
    assert_eq!(
        get_param_label(&positional("1", "question:1", SqlParamSyntax::Question, Some(1))),
        "?"
    );
    assert_eq!(get_param_label(&named("x", SqlParamSyntax::Template)), "${x}");
}

#[test]
fn sensitive_param_names_are_pinned() {
    for name in [
        "password",
        "passwd",
        "token",
        "secret",
        "key",
        "credential",
        "api_key",
        "apikey",
        "access_token",
        "auth",
    ] {
        assert!(is_sensitive_param_name(name));
        assert!(sensitive_param_names().contains(name));
    }
    assert_eq!(sensitive_param_names().len(), 10);
    assert!(!is_sensitive_param_name("Password"));
    assert!(!is_sensitive_param_name("user"));
}

#[test]
fn substitute_sql_params_covers_all_five_syntaxes() {
    let sql = "SELECT * FROM t WHERE a = :a AND b = @b AND c = $1 AND d = ? AND e = ${e}";
    let raw = vals(&[
        ("named:a", s("O'Connor")),
        ("named:b", s("100")),
        ("dollar:1", s("true")),
        ("question:1", s("NULL")),
        ("named:e", s("hello")),
    ]);
    assert_eq!(
        substitute_sql_params(sql, &raw, None),
        "SELECT * FROM t WHERE a = 'O''Connor' AND b = 100 AND c = TRUE AND d = NULL AND e = 'hello'"
    );
}

#[test]
fn substitute_sql_params_accepts_non_string_values() {
    let sql = "SELECT * FROM er_orders WHERE order_no = :ord AND er_orders.\"customer_id\" = @usr AND id = $1 AND total_amount > ? AND status = ${status};";
    let raw = vals(&[
        ("named:ord", s("ORD-999")),
        ("named:usr", s("USR-123")),
        ("dollar:1", ParamValue::Number(42.0)),
        ("question:1", ParamValue::Number(99.5)),
        ("named:status", ParamValue::Bool(true)),
    ]);
    assert_eq!(
        substitute_sql_params(sql, &raw, None),
        "SELECT * FROM er_orders WHERE order_no = 'ORD-999' AND er_orders.\"customer_id\" = 'USR-123' AND id = 42 AND total_amount > 99.5 AND status = TRUE;"
    );
}

#[test]
fn substitute_sql_params_keeps_sql_when_there_are_no_placeholders() {
    let sql = "SELECT ':literal' FROM t";
    assert_eq!(substitute_sql_params(sql, &vals(&[]), None), sql);
}

#[test]
fn substitute_sql_params_falls_back_to_name_and_ordinal_keys() {
    let raw = vals(&[("uid", s("7")), ("2", s("nine"))]);
    assert_eq!(
        substitute_sql_params("SELECT :uid, ?, ?", &raw, None),
        "SELECT 7, NULL, 'nine'"
    );
}

#[test]
fn crate_root_reexports_the_bind_param_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::ParamValue>();
    resolves::<crate::SqlParam>();
    resolves::<crate::SqlParamSyntax>();
    resolves::<crate::SqlParamKind>();
    resolves::<crate::SqlParamOccurrence>();
    resolves::<crate::SqlParamOccurrenceV2>();
    resolves::<crate::SqlBindPayloadV2>();
    resolves::<crate::SqlParamDialectPolicy>();
    resolves::<crate::SqlParamStableId>();
    resolves::<crate::SqlParamFilter>();
    assert_eq!(crate::get_param_label(&named("x", SqlParamSyntax::Colon)), ":x");
    assert_eq!(crate::param_fingerprint(&[]), "");
    assert!(crate::is_sensitive_param_name("token"));
    assert_eq!(crate::DEFAULT_ENABLE_DOLLAR_NAMED, false);
    let _: Option<crate::SqlParamDialectPolicy> = None;
}
