// Parameter binding regression tests (included from mod.rs).

#[test]
fn named_params_are_substituted() {
    let sql = apply_params(
        "SELECT * FROM t WHERE id = :uid AND name = :name",
        &json!({ "uid": 42, "name": "O'Brien" }),
    )
    .unwrap();
    assert_eq!(sql, "SELECT * FROM t WHERE id = 42 AND name = 'O''Brien'");
}

#[test]
fn params_inside_strings_are_not_replaced() {
    let sql =
        apply_params("SELECT ':uid' FROM t WHERE id = :uid", &json!({ "uid": 1 })).unwrap();
    assert_eq!(sql, "SELECT ':uid' FROM t WHERE id = 1");
}

#[test]
fn positional_dollar_params() {
    let sql =
        apply_params("SELECT * FROM t WHERE a = $1 AND b = $2", &json!([1, "x"])).unwrap();
    assert_eq!(sql, "SELECT * FROM t WHERE a = 1 AND b = 'x'");
}

#[test]
fn apply_params_handles_null_bool_and_question_marks() {
    assert_eq!(
        apply_params("SELECT :x", &json!(null)).unwrap(),
        "SELECT :x"
    );
    assert_eq!(apply_params("SELECT :x", &json!({})).unwrap(), "SELECT :x");
    let sql = apply_params("SELECT ? , ? , ?", &json!([true, false, {"a": 1}])).unwrap();
    assert_eq!(sql, "SELECT TRUE , FALSE , '{\"a\":1}'");
}

#[test]
fn longer_named_params_win() {
    let sql = apply_params(
        "SELECT :user_id, :user",
        &json!({ "user": "a", "user_id": 9 }),
    )
    .unwrap();
    assert_eq!(sql, "SELECT 9, 'a'");
}

#[test]
fn named_digit_keys_also_replace_dollar() {
    let sql = apply_params("SELECT :1, $1", &json!({ "1": 7 })).unwrap();
    assert_eq!(sql, "SELECT 7, 7");
}

#[test]
fn extra_question_marks_are_left_alone() {
    let sql = apply_params("SELECT ? , ?", &json!([1])).unwrap();
    assert_eq!(sql, "SELECT 1 , ?");
}

#[test]
fn test_harden_backslash_in_literal() {
    let sql = apply_params("SELECT :x", &json!({"x": "a\\b"})).unwrap();
    assert_eq!(sql, "SELECT 'a\\\\b'");
}

#[test]
fn shared_fixture_bind_cases() {
    let raw = include_str!("../../../../src/lib/__tests__/fixtures/sqlBindParamCases.json");
    let fixture: JsonValue = serde_json::from_str(raw).expect("fixture JSON");
    let cases = fixture["bindCases"].as_array().expect("bindCases array");
    for case in cases {
        let id = case["id"].as_str().unwrap_or("unknown");
        let sql = case["sql"].as_str().expect("sql");
        let payload = &case["payload"];
        let expected = case["expected"].as_str().expect("expected");
        let result = apply_params(sql, payload).unwrap_or_else(|e| panic!("case {id}: {e}"));
        assert_eq!(result, expected, "case {id}");
    }
}
