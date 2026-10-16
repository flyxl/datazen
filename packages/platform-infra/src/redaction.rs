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
//! URL userinfo is now read from the URL grammar instead of from a character
//! class. `url::Url::parse` splits the userinfo from the host, so a password
//! containing `/` is just another character of the password — whereas the
//! previous `[^/@\s]+` pattern excluded `/`, failed to match such a URL at
//! all, and logged the credential in clear text. The old pattern survives only
//! as a fallback for `scheme://…` runs the parser rejects.
//!
//! Known limitations, measured against `url` 2:
//!
//! * Only absolute `scheme://…` runs are considered. A bare connection string
//!   with no scheme, or a DSN fragment such as `user:pa/ss@host:5432/db`, is
//!   not a URL; it is left to the `Bearer` / query / JSON-field patterns, none
//!   of which look at `@`.
//! * Inside a run the parser *rejects*, the fallback rewrites only when it finds
//!   a `:` between `://` and the last `@`. That separator is what
//!   distinguishes userinfo from a path that merely precedes an `@`, and
//!   keeping it is what stops `https://host/path@x` from being rewritten even
//!   on the fallback path. Consequence: in an unparseable run a bare user *name*
//!   with no `:` is left as-is; a password cannot be, since `user:password`
//!   always carries the separator.

use regex::Regex;
use std::sync::OnceLock;
use url::Url;

/// Placeholder that replaces the userinfo (user name and password) of a URL.
const REDACTED_USERINFO: &str = "***";

// Why the patterns below may `expect`: each one is a compile-time literal, so
// the only way `Regex::new` fails is a typo in the pattern itself — a bug the
// unit tests at the bottom of this file catch the first time they run. None of
// them ever sees data-derived input, which is the case the panic policy is
// about.
fn url_candidate_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // One `scheme://…` run. Whitespace terminates the run so a URL embedded
    // mid-sentence is isolated from the words around it, and the quote
    // characters are excluded because SQL snippets and log lines wrap URLs in
    // them.
    RE.get_or_init(|| {
        // Literal pattern: the only failure mode is a typo in the pattern, and
        // the tests below exercise every one of these patterns.
        Regex::new(r#"(?i)[a-z][a-z0-9+.-]*://[^\s'"]*"#).expect("url candidate regex")
    })
}

fn url_credentials_fallback_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Fallback for runs the parser rejects — typically `InvalidPort`, which is
    // exactly what a password containing `/` produces: the authority ends at
    // the `/` inside the password, so the parser reads `user:pa` as host+port
    // and chokes on the non-numeric port `pa`. `[^@\s/]*` keeps the
    // scheme-and-first-segment shape, while the `[^@\s]*` after the `:`
    // deliberately admits `/` — that slack is the whole point, since the
    // credential is `user:pa/ss`. The mandatory `:` is what protects
    // `https://host/path@x`.
    RE.get_or_init(|| {
        // Literal pattern, same reasoning as `url_candidate_re` above.
        Regex::new(r"(?i)^([a-z][a-z0-9+.-]*://)([^@\s/]*:[^@\s]*)@")
            .expect("url credentials fallback regex")
    })
}

fn bearer_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Literal pattern; no data-derived input reaches it.
    RE.get_or_init(|| Regex::new(r"(?i)(bearer\s+)\S+").expect("bearer regex"))
}

fn query_secret_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)([?&](?:password|passwd|pwd|token|access_token|api_key|apikey|secret|key)=)([^&\s]+)")
            // Literal pattern; no data-derived input reaches it.
            .expect("query secret regex")
    })
}

fn password_field_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Literal pattern; no data-derived input reaches it.
        Regex::new(r#"(?i)("password"\s*:\s*")([^"]*)(")"#).expect("password field regex")
    })
}

/// Redact the userinfo of one `scheme://…` run.
///
/// The parser is the primary path because it knows where userinfo ends and the
/// host begins, so a `/` inside a password is just another character of the
/// password. The pattern fallback runs only for runs the parser rejects.
fn redact_candidate(candidate: &str) -> String {
    if let Ok(mut parsed) = Url::parse(candidate) {
        if parsed.username().is_empty() && parsed.password().is_none() {
            // Nothing to redact. Return the caller's exact bytes rather than the
            // parser's normalized rendering, which would rewrite `https://host`
            // into `https://host/` in an unrelated log line.
            return candidate.to_string();
        }
        // `set_password(None)` drops the `:` along with the password, so no
        // fragment of the original credential can survive the rebuilt URL.
        if parsed.set_username(REDACTED_USERINFO).is_ok() && parsed.set_password(None).is_ok() {
            return parsed.to_string();
        }
        // A cannot-be-a-base URL has no userinfo slot to rewrite; fall through
        // to the pattern path rather than return the credential untouched.
    }
    let replacement = format!("${{1}}{REDACTED_USERINFO}@");
    url_credentials_fallback_re()
        .replace(candidate, replacement.as_str())
        .into_owned()
}

/// Replace the userinfo of every `scheme://…` run in `input`.
fn redact_url_userinfo(input: &str) -> String {
    url_candidate_re()
        .replace_all(input, |caps: &regex::Captures<'_>| {
            redact_candidate(&caps[0])
        })
        .into_owned()
}

/// First 500 chars of SQL with secrets redacted — for `tracing::debug!` previews.
pub fn sql_preview_for_log(sql: &str) -> String {
    let preview: String = sql.chars().take(500).collect();
    redact_secrets_for_log(&preview)
}

/// Replace credentials / tokens in a log message with placeholders.
pub fn redact_secrets_for_log(input: &str) -> String {
    let mut out = redact_url_userinfo(input);
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

    /// AC2: a password containing `/` reaches the log redacted.
    #[test]
    fn redacts_password_containing_slash() {
        // The regression: the parser reads the authority as `user:pa`, rejects
        // the non-numeric port, and the previous `[^/@\s]+` pattern could not
        // match the run at all — so the credential went to the log verbatim.
        let s = "CustomEndpointFailed: https://user:pa/ss@host/v1";
        let out = redact_secrets_for_log(s);
        assert!(!out.contains("pa/ss"), "{out}");
        assert!(out.contains("https://***@host/v1"), "{out}");

        let url = redact_url_for_log("https://user:pa/ss@host/v1");
        assert!(!url.contains("pa/ss"), "{url}");
        assert!(url.contains("https://***@host/v1"), "{url}");

        // Same shape in front of a port, as a DSN inside a log line.
        let dsn = "mysql://root:pa/ss@127.0.0.1:3306/app";
        let out = redact_secrets_for_log(dsn);
        assert!(!out.contains("pa/ss"), "{out}");
        assert!(out.contains("mysql://***@127.0.0.1:3306/app"), "{out}");
    }

    /// AC3: a valid URL whose path merely contains `@` is not rewritten.
    #[test]
    fn leaves_path_at_sign_untouched() {
        assert_eq!(
            redact_url_for_log("https://host/path@x"),
            "https://host/path@x"
        );
        assert_eq!(
            redact_secrets_for_log("https://host:8080/path@x"),
            "https://host:8080/path@x"
        );
        // No userinfo here either: the parser finds an empty username, so the
        // URL is returned byte-for-byte rather than re-rendered.
        assert_eq!(
            redact_secrets_for_log("https://docs.example/page@2.0"),
            "https://docs.example/page@2.0"
        );
    }

    /// AC1 / AC3: input with no absolute URL — or with a run the parser rejects
    /// and that carries no credential separator — must stay intact and must not
    /// panic.
    #[test]
    fn non_url_input_falls_back_without_panicking() {
        // No `scheme://`: nothing for the URL step to do.
        let bare = "smtp relay user@localhost:587 refused; //user:pass@host/db";
        assert_eq!(redact_secrets_for_log(bare), bare);

        // Unparseable (`InvalidPort`) and no `@` in the run: the fallback runs
        // and changes nothing.
        assert_eq!(
            redact_secrets_for_log("mysql://host:pa/db"),
            "mysql://host:pa/db"
        );

        // Unparseable, has an `@`, but no `:` before it: left alone on purpose.
        assert_eq!(
            redact_secrets_for_log("https://host/pa/ss@x"),
            "https://host/pa/ss@x"
        );

        // Degenerate input must not panic.
        assert_eq!(redact_secrets_for_log("http://"), "http://");
        assert_eq!(redact_secrets_for_log("://@"), "://@");
        assert_eq!(redact_url_for_log("   "), "");
        assert_eq!(
            redact_url_for_log("https://host?token=secret"),
            "https://host?[redacted]"
        );
    }
}
