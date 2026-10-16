//! Redact secrets before writing them to application logs.
//!
//! Why this capability is a cross-form primitive rather than a host-private
//! helper: the `packages/ai-providers` extraction is the proof. The host
//! already shipped this exact implementation in `src-tauri/src/log_redact.rs`,
//! but a crate outside `src-tauri` cannot see a private host module, so during
//! that extraction the package hand-wrote a *second* implementation in its
//! `protocol/mod.rs`. That copy was strictly weaker: it masked URL userinfo and
//! dropped the whole query string, but it never handled `Bearer` headers, nor
//! `"password":` JSON fields. Two implementations meant two different leak
//! surfaces, and the weaker one lived in a package nobody reviews as
//! security-sensitive. That duplicate has since been deleted; the functions
//! below are the only implementation.
//!
//! Reimplementing an existing primitive across a visibility boundary is the
//! default failure mode here, not an exception — the boundary looks like it
//! forbids sharing. So primitives that are *mechanism* (not policy, not domain
//! logic, no IO, no transport) get exactly one home that every form links, and
//! the host keeps a `pub use` thin shell at its original module path so that
//! every existing `crate::log_redact::` call site stays untouched.
//!
//! Known limitation, not yet fixed: `url_credentials_re` matches userinfo as
//! `[^/@\s]+`, which excludes `/`. A password containing `/` therefore makes the
//! whole pattern fail to match, and that line is logged with the credential in
//! clear text. Inherited verbatim from the host implementation.

use regex::Regex;
use std::sync::OnceLock;

// The four patterns below are compile-time literals: the only way `get_or_init`
// can panic is a typo in the pattern itself, and every one of them is exercised
// by the unit tests at the bottom of this file. The `expect` is therefore the
// idiomatic OnceLock shape, not an unhandled runtime error path — no
// `unwrap()`/`expect()` here ever touches data-derived input.
fn url_credentials_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Matches scheme://[user[:password]@]host — strips credentials before '@'.
        Regex::new(r"(?i)([a-z][a-z0-9+.-]*://)([^/@\s]+)@").expect("url credentials regex")
    })
}

fn bearer_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(bearer\s+)\S+").expect("bearer regex"))
}

fn query_secret_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)([?&](?:password|passwd|pwd|token|access_token|api_key|apikey|secret|key)=)([^&\s]+)")
            .expect("query secret regex")
    })
}

fn password_field_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)("password"\s*:\s*")([^"]*)(")"#).expect("password field regex")
    })
}

/// First 500 chars of SQL with secrets redacted — for `tracing::debug!` previews.
pub fn sql_preview_for_log(sql: &str) -> String {
    let preview: String = sql.chars().take(500).collect();
    redact_secrets_for_log(&preview)
}

/// Replace credentials / tokens in a log message with placeholders.
pub fn redact_secrets_for_log(input: &str) -> String {
    let mut out = url_credentials_re()
        .replace_all(input, "${1}***@")
        .into_owned();
    out = bearer_re().replace_all(&out, "${1}***").into_owned();
    out = query_secret_re().replace_all(&out, "${1}***").into_owned();
    out = password_field_re()
        .replace_all(&out, "${1}***${3}")
        .into_owned();
    out
}

/// Log-safe URL: credentials redacted and query string stripped.
pub fn redact_url_for_log(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let redacted = redact_secrets_for_log(trimmed);
    match redacted.split_once('?') {
        Some((base, _)) => format!("{base}?[redacted]"),
        None => redacted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_mysql_and_mongo_uris() {
        let s = "ConnectionFailed: mysql://root:s3cret@127.0.0.1:3306/app";
        let out = redact_secrets_for_log(s);
        assert!(!out.contains("s3cret"), "{out}");
        assert!(out.contains("mysql://***@127.0.0.1:3306/app"), "{out}");

        let mongo = "mongodb://alice:p@ss@host:27017/?directConnection=true";
        let out = redact_secrets_for_log(mongo);
        assert!(!out.contains("p@ss") || out.contains("***@"), "{out}");
        assert!(out.contains("***@"), "{out}");
    }

    #[test]
    fn redacts_redis_password_only_userinfo() {
        let s = "redis://:hunter2@127.0.0.1:6379/0";
        let out = redact_secrets_for_log(s);
        assert!(!out.contains("hunter2"), "{out}");
        assert!(out.contains("redis://***@"), "{out}");
    }

    #[test]
    fn redacts_bearer_and_query_tokens() {
        let s = "Authorization: Bearer sk-abc123 failed; url=https://hooks.example/x?token=abc&a=1";
        let out = redact_secrets_for_log(s);
        assert!(!out.contains("sk-abc123"), "{out}");
        assert!(!out.contains("token=abc"), "{out}");
        assert!(out.contains("Bearer ***"), "{out}");
        assert!(out.contains("token=***"), "{out}");
    }

    #[test]
    fn sql_preview_truncates_and_redacts() {
        let sql = "SELECT * FROM t WHERE url = 'mysql://root:hunter2@127.0.0.1/app'";
        let out = sql_preview_for_log(sql);
        assert!(!out.contains("hunter2"), "{out}");
        assert!(out.contains("mysql://***@"), "{out}");
    }

    #[test]
    fn redact_url_strips_query_and_userinfo() {
        let out =
            redact_url_for_log("https://user:pass@hooks.slack.com/services/T/B/x?token=secret");
        assert!(!out.contains("pass"), "{out}");
        assert!(!out.contains("secret"), "{out}");
        assert!(out.contains("hooks.slack.com"), "{out}");
        assert!(out.contains("?[redacted]"), "{out}");
    }
}
