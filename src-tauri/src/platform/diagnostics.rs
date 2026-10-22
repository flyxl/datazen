//! SQL diagnostics cross the result boundary only after credential/path redaction.
pub(super) fn safe_message(input: &str) -> String {
    let mut message = datazen_platform_infra::redaction::redact_secrets_for_log(input);
    for pattern in [
        r#"(?i)\b(password|passwd|pwd|token|access_token|api_key|apikey|secret)\s*[=:]\s*(?:'[^']*'|\"[^\"]*\"|[^\s;,]+)"#,
        r#"(?:[A-Za-z]:[\\/]|/)(?:[^\s'\"<>:]+[\\/])+[^\s'\"<>:]*"#,
    ] {
        match regex::Regex::new(pattern) {
            Ok(regex) => message = regex.replace_all(&message, "[redacted]").into_owned(),
            Err(_) => return "Database execution failed".into(),
        }
    }
    message.chars().take(4096).collect()
}
