use super::*;

/// Helper: default strict gate (redact credentials + strip result rows).
fn strict_gate() -> AiSafetyGateConfig {
    AiSafetyGateConfig {
        data_egress_level: AiDataEgressLevel::Strict,
        ..Default::default()
    }
}

/// Helper: relaxed gate (redact credentials but keep result rows).
fn relaxed_gate() -> AiSafetyGateConfig {
    AiSafetyGateConfig {
        data_egress_level: AiDataEgressLevel::Unrestricted,
        ..Default::default()
    }
}

/// Helper: sample-masked gate with custom row limit.
fn sample_gate(max_rows: u32) -> AiSafetyGateConfig {
    AiSafetyGateConfig {
        data_egress_level: AiDataEgressLevel::SampleMasked,
        max_sample_rows: max_rows,
        ..Default::default()
    }
}

// ── basic credential redaction ──

#[test]
fn redacts_sensitive_key_assignments() {
    let gate = strict_gate();
    for (input, secret) in [
        ("password=db-secret", "db-secret"),
        ("apiKey=sk-live-123", "sk-live-123"),
        ("Authorization: Bearer bearer-secret", "bearer-secret"),
    ] {
        let output = redact_for_gate(input, &gate);
        assert!(!output.contains(secret), "{output}");
        assert!(output.contains("[REDACTED]"), "{output}");
    }

    let json_output = redact_for_gate("{\"clientSecret\":\"top-secret\"}", &gate);
    assert!(!json_output.contains("top-secret"), "{json_output}");
    assert_eq!(json_output, "{}");
}

#[test]
fn redacts_uri_credentials_and_query_secrets() {
    let gate = strict_gate();
    let input = "url=mysql://user:uri-secret@db.example/app?token=query-secret&api_key=key-secret";
    let output = redact_for_gate(input, &gate);
    for secret in ["uri-secret", "query-secret", "key-secret"] {
        assert!(!output.contains(secret), "{output}");
    }
    assert!(
        output.contains("mysql://[REDACTED]@db.example/app"),
        "{output}"
    );
}

#[test]
fn redacts_bearer_tokens_case_insensitively() {
    let gate = strict_gate();
    let output = redact_for_gate("Authorization: bearer abc.def.ghi", &gate);
    assert!(!output.contains("abc.def.ghi"), "{output}");
    assert!(output.contains("[REDACTED]"), "{output}");

    let standalone = redact_for_gate("prefix bearer abc.def.ghi", &gate);
    assert!(!standalone.contains("abc.def.ghi"), "{standalone}");
    assert!(standalone.contains("bearer [REDACTED]"), "{standalone}");
}

#[test]
fn redacts_sensitive_assignments_and_connection_credentials() {
    let gate = strict_gate();
    let input = "query failed: apiToken=prefix-token; SELECT * FROM t WHERE password = 'db-secret' AND apiToken=token-secret; \
            url=mysql://user:uri-secret@db.example/app?token=query-secret; Bearer bearer-secret";
    let output = redact_for_gate(input, &gate);

    for secret in [
        "db-secret",
        "token-secret",
        "prefix-token",
        "uri-secret",
        "query-secret",
        "bearer-secret",
    ] {
        assert!(!output.contains(secret), "{output}");
    }
    assert!(output.contains("[REDACTED]"), "{output}");
}

// ── JSON structure redaction ──

#[test]
fn removes_nested_json_secrets_and_result_rows_in_strict_mode() {
    let input = serde_json::json!({
        "apiToken": "API_TOKEN_HEAD\"TAIL\\END",
        "nested": {"password": "PASSWORD_SECRET", "safe": "keep"},
        "resultRows": [{"email": "user@example.test"}],
    })
    .to_string();
    let output = redact_for_gate(&input, &strict_gate());
    let parsed: Value = serde_json::from_str(&output).expect("safe JSON");

    assert_eq!(parsed, serde_json::json!({"nested": {"safe": "keep"}}));
    for secret in [
        "API_TOKEN_HEAD",
        "TAIL",
        "END",
        "PASSWORD_SECRET",
        "user@example.test",
    ] {
        assert!(!output.contains(secret), "{output}");
    }
}

#[test]
fn relaxed_egress_keeps_result_rows_but_still_redacts_secrets() {
    let input = serde_json::json!({
        "password": "PASSWORD_SECRET",
        "rows": [{"email": "user@example.test"}],
    })
    .to_string();
    let output = redact_for_gate(&input, &relaxed_gate());
    let parsed: Value = serde_json::from_str(&output).expect("safe JSON");

    assert_eq!(
        parsed,
        serde_json::json!({"rows": [{"email": "user@example.test"}]})
    );
    assert!(!output.contains("PASSWORD_SECRET"), "{output}");
}

#[test]
fn bounds_large_prompt_text_without_panicking_on_utf8() {
    let gate = strict_gate();
    let output = redact_for_gate(&"表".repeat(3_000), &gate);
    assert!(output.len() <= gate.max_context_bytes);
    assert!(output.is_char_boundary(output.len()));
}

// ── whitelist: sensitive-key exemption ──

#[test]
fn whitelist_keyboard_not_flagged() {
    let gate = strict_gate();
    let input = serde_json::json!({
        "keyboard": "mechanical-switches",
    })
    .to_string();
    let output = redact_for_gate(&input, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
    // "keyboard" is whitelisted — should NOT be redacted
    assert_eq!(parsed["keyboard"], "mechanical-switches");
}

#[test]
fn whitelist_monkey_not_flagged() {
    let gate = strict_gate();
    let input = serde_json::json!({
        "monkey": "test-value",
    })
    .to_string();
    let output = redact_for_gate(&input, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
    assert_eq!(parsed["monkey"], "test-value");
}

#[test]
fn whitelist_donkey_not_flagged() {
    let gate = strict_gate();
    let input = serde_json::json!({
        "donkey": "animal",
    })
    .to_string();
    let output = redact_for_gate(&input, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
    assert_eq!(parsed["donkey"], "animal");
}

#[test]
fn whitelist_turnkey_not_flagged() {
    let gate = strict_gate();
    let input = serde_json::json!({
        "turnkey": "solution",
    })
    .to_string();
    let output = redact_for_gate(&input, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
    assert_eq!(parsed["turnkey"], "solution");
}

#[test]
fn whitelist_hockey_not_flagged() {
    let gate = strict_gate();
    let input = serde_json::json!({
        "hockey": "sport",
    })
    .to_string();
    let output = redact_for_gate(&input, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
    assert_eq!(parsed["hockey"], "sport");
}

#[test]
fn non_whitelisted_key_containing_sensitive_word_is_still_flagged() {
    let gate = strict_gate();
    // "monkey_key" — underscore-separated, "key" token triggers
    let input = serde_json::json!({
        "monkey_key": "should-be-redacted",
    })
    .to_string();
    let output = redact_for_gate(&input, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
    assert!(
        parsed.get("monkey_key").is_none(),
        "monkey_key should be removed"
    );

    // "monkeyKey" — camelCase, "key" token triggers
    let input2 = serde_json::json!({
        "monkeyKey": "should-be-redacted",
    })
    .to_string();
    let output2 = redact_for_gate(&input2, &gate);
    let parsed2: Value = serde_json::from_str(&output2).expect("valid JSON");
    assert!(
        parsed2.get("monkeyKey").is_none(),
        "monkeyKey should be removed"
    );
}

// ── result-key prefix regex matching ──

#[test]
fn result_key_prefix_matches_common_patterns() {
    let gate = strict_gate();
    // "data" prefix
    let input = serde_json::json!({"data": [{"id": 1}]}).to_string();
    let output = redact_for_gate(&input, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
    assert!(
        parsed.get("data").is_none(),
        "data key should be removed in strict mode"
    );

    // "rows" prefix
    let input = serde_json::json!({"rows": [{"name": "Alice"}]}).to_string();
    let output = redact_for_gate(&input, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
    assert!(parsed.get("rows").is_none());

    // "resultRows" — canonical = "resultrows", matches "results?"
    let input = serde_json::json!({"resultRows": [{"x": 1}]}).to_string();
    let output = redact_for_gate(&input, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
    assert!(parsed.get("resultRows").is_none());

    // "payload" — direct match
    let input = serde_json::json!({"payload": {"secret": "val"}}).to_string();
    let output = redact_for_gate(&input, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
    assert!(parsed.get("payload").is_none());

    // "sampleData" — canonical = "sampledata", matches "samples?_?"
    let input = serde_json::json!({"sampleData": [{"id": 1}]}).to_string();
    let output = redact_for_gate(&input, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
    assert!(parsed.get("sampleData").is_none());
}

#[test]
fn result_key_prefix_does_not_false_positive() {
    let gate = strict_gate();
    // "description" does NOT match any result-key prefix
    let input = serde_json::json!({"description": "a table of users"}).to_string();
    let output = redact_for_gate(&input, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");
    assert_eq!(parsed["description"], "a table of users");
}

// ── deep nesting / truncation ──

#[test]
fn test_tester_nested_json_sensitive_keys_at_all_depths() {
    let input = serde_json::json!({
        "password": "root-secret",
        "level1": {
            "apiToken": "level1-secret",
            "level2": {
                "clientSecret": "level2-secret",
                "level3": {
                    "refreshToken": "level3-secret",
                    "safe": "keep-me"
                }
            }
        }
    })
    .to_string();

    let output = redact_for_gate(&input, &strict_gate());
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");

    assert_eq!(
        parsed,
        serde_json::json!({"level1": {"level2": {"level3": {"safe": "keep-me"}}}})
    );
    for secret in [
        "root-secret",
        "level1-secret",
        "level2-secret",
        "level3-secret",
    ] {
        assert!(!output.contains(secret), "{output}");
    }
}

#[test]
fn test_tester_deep_json_truncates_objects_at_max_depth() {
    let input = serde_json::json!({
        "a": {"b": {"c": {"d": {"e": "too-deep", "safe": "visible"}}}}
    })
    .to_string();

    let output = redact_for_gate(&input, &relaxed_gate());
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");

    assert_eq!(
        parsed,
        serde_json::json!({"a": {"b": {"c": {"d": "[truncated]"}}}})
    );
    assert!(!output.contains("too-deep"), "{output}");
}

#[test]
fn test_tester_oversized_ascii_payload_truncates_at_byte_boundary() {
    let gate = AiSafetyGateConfig {
        max_context_bytes: MAX_AI_TEXT_BYTES,
        ..Default::default()
    };
    let input = "x".repeat(MAX_AI_TEXT_BYTES + 500);
    let output = redact_for_gate(&input, &gate);

    assert_eq!(output.len(), MAX_AI_TEXT_BYTES);
    assert!(output.is_char_boundary(output.len()));
    assert!(input.starts_with(&output));
}

#[test]
fn test_tester_oversized_multibyte_payload_respects_char_boundaries() {
    let gate = AiSafetyGateConfig {
        max_context_bytes: MAX_AI_TEXT_BYTES,
        ..Default::default()
    };
    let unit = "表";
    let repeat = MAX_AI_TEXT_BYTES / unit.len() + 10;
    let input = unit.repeat(repeat);
    let output = redact_for_gate(&input, &gate);

    assert!(output.len() <= MAX_AI_TEXT_BYTES);
    assert!(output.is_char_boundary(output.len()));
    assert!(input.starts_with(&output));
    assert!(std::str::from_utf8(output.as_bytes()).is_ok());
}

#[test]
fn test_tester_invalid_json_fallback_still_redacts_plain_secrets() {
    let gate = strict_gate();
    let input = r#"{"broken": true, password=leaked-secret"#;
    let output = redact_for_gate(input, &gate);

    assert!(!output.contains("leaked-secret"), "{output}");
    assert!(output.contains("[REDACTED]"), "{output}");
}

// ── egress policy simulation ──

#[test]
fn test_tester_strict_egress_strips_result_rows_like_chat_user_message() {
    let gate = strict_gate();
    let chat_payload = serde_json::json!({
        "question": "summarize",
        "queryResults": [{"id": 1, "email": "alice@corp.test"}],
        "password": "chat-secret"
    })
    .to_string();

    let output = redact_for_gate(&chat_payload, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");

    assert_eq!(parsed, serde_json::json!({"question": "summarize"}));
    assert!(!output.contains("alice@corp.test"), "{output}");
    assert!(!output.contains("chat-secret"), "{output}");
}

#[test]
fn test_tester_strict_egress_strips_tool_result_payload_like_chat_loop() {
    let gate = strict_gate();
    let tool_result = serde_json::json!({
        "rows": [{"name": "Bob"}],
        "meta": {"count": 1}
    })
    .to_string();

    let output = redact_for_gate(&tool_result, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");

    assert_eq!(parsed, serde_json::json!({"meta": {"count": 1}}));
    assert!(!output.contains("Bob"), "{output}");
}

#[test]
fn test_tester_relaxed_egress_keeps_rows_for_generate_natural_language() {
    let gate = relaxed_gate();
    let nl2sql_context = serde_json::json!({
        "password": "nl-secret",
        "resultRows": [{"city": "Shanghai"}],
    })
    .to_string();

    let output = redact_for_gate(&nl2sql_context, &gate);
    let parsed: Value = serde_json::from_str(&output).expect("valid JSON");

    assert_eq!(
        parsed,
        serde_json::json!({"resultRows": [{"city": "Shanghai"}]})
    );
    assert!(!output.contains("nl-secret"), "{output}");
}

// ── strict ↔ relaxed toggle ──

#[test]
fn test_tester_egress_toggle_strict_to_relaxed_preserves_rows_immediately() {
    let payload = serde_json::json!({
        "rows": [{"id": 99}],
        "token": "toggle-secret",
    })
    .to_string();

    let strict = redact_for_gate(&payload, &strict_gate());
    let relaxed = redact_for_gate(&payload, &relaxed_gate());

    let strict_parsed: Value = serde_json::from_str(&strict).expect("strict JSON");
    let relaxed_parsed: Value = serde_json::from_str(&relaxed).expect("relaxed JSON");

    assert_eq!(strict_parsed, serde_json::json!({}));
    assert_eq!(relaxed_parsed, serde_json::json!({"rows": [{"id": 99}]}));
    assert!(!strict.contains("toggle-secret"), "{strict}");
    assert!(!relaxed.contains("toggle-secret"), "{relaxed}");
    assert_ne!(strict, relaxed);
}

#[test]
fn test_tester_egress_toggle_relaxed_to_strict_strips_rows_immediately() {
    let payload = serde_json::json!({
        "queryResultData": [{"score": 10}],
        "safeNote": "reference only",
    })
    .to_string();

    let relaxed = redact_for_gate(&payload, &relaxed_gate());
    let strict = redact_for_gate(&payload, &strict_gate());

    let relaxed_parsed: Value = serde_json::from_str(&relaxed).expect("relaxed JSON");
    let strict_parsed: Value = serde_json::from_str(&strict).expect("strict JSON");

    assert_eq!(
        relaxed_parsed,
        serde_json::json!({
            "queryResultData": [{"score": 10}],
            "safeNote": "reference only",
        })
    );
    assert_eq!(
        strict_parsed,
        serde_json::json!({"safeNote": "reference only"})
    );
    assert_ne!(relaxed, strict);
}

// ── sample-masked gate ──

#[test]
fn test_safety_gate_sample_masked_limits_rows() {
    let gate = sample_gate(2);
    let payload = serde_json::json!({
        "rows": [
            {"id": 1, "name": "user1"},
            {"id": 2, "name": "user2"},
            {"id": 3, "name": "user3"},
            {"id": 4, "name": "user4"},
            {"id": 5, "name": "user5"},
        ],
        "tableName": "users",
    })
    .to_string();

    let result = redact_for_gate(&payload, &gate);
    let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
    assert_eq!(parsed["rows"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["tableName"], "users");
}

#[test]
fn test_safety_gate_unrestricted_allows_rows_but_strips_credentials() {
    let gate = relaxed_gate();
    let payload = serde_json::json!({
        "rows": [{"id": 1, "password": "super-secret"}],
        "token": "secret-token",
        "tableName": "users",
    })
    .to_string();

    let result = redact_for_gate(&payload, &gate);
    let parsed: Value = serde_json::from_str(&result).expect("valid JSON");
    assert_eq!(parsed["rows"].as_array().unwrap().len(), 1);
    assert!(parsed["token"].is_null());
    assert_eq!(parsed["tableName"], "users");
}

// ── max_context_bytes configurable ──

#[test]
fn custom_max_context_bytes_limits_output() {
    let gate = AiSafetyGateConfig {
        max_context_bytes: 50,
        ..Default::default()
    };
    let input = "x".repeat(100);
    let output = redact_for_gate(&input, &gate);
    assert_eq!(output.len(), 50);
}

#[test]
fn global_strict_policy_tightens_an_unconfigured_profile_gate() {
    let mut policy = datazen_modules::settings::ExecutionSettings {
        limit_select_results: false,
        query_result_limit: 5000,
        safe_mode: true,
        connection_pool_size: 10,
        mcp_disabled_tools: vec![],
        mcp_permission_mode: "safe_write".into(),
        mcp_allowed_connection_ids: vec![],
        driver_settings: serde_json::Map::new(),
        ai_strict_egress: true,
    };
    let config = datazen_modules::ai::AiSettingsConfig::default();
    assert_eq!(
        effective_safety_gate(&config, &policy).data_egress_level,
        AiDataEgressLevel::Strict
    );
    policy.ai_strict_egress = false;
    assert_eq!(
        effective_safety_gate(&config, &policy).data_egress_level,
        AiDataEgressLevel::Unrestricted
    );
}
