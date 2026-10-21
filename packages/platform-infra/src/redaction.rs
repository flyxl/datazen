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
//! class: `url::Url::parse` tells userinfo apart from host, and the redaction
//! replaces exactly that userinfo, so a password spelled with a percent-encoded
//! `/` (`%2F`, the RFC 3986 form) is redacted like any other character. The
//! previous `[^/@\s]+` pattern could not match a run that carried a `/`
//! before the `@` at all, which is how `scheme://user:pa/ss@host` reached the
//! log in clear text; the fallback below now admits that `/`.
//!
//! What this covers, and what it does not:
//!
//! * The input has to be a *valid* URL. Characters RFC 3986 forbids in userinfo
//!   — `/`, `?`, `#` — must be percent-encoded there (`/` as `%2F`). The
//!   parse step does no intent recovery: `scheme://user:pa/ss@host` is not
//!   parsed as a password containing a slash — read per the RFC, that text is a
//!   host:port followed by a path — so `https://user:pa%2Fss@host/v1` is the
//!   spelling to use, and it is redacted.
//!
//!   That is a statement about the parse step, not about the outcome. A run the
//!   parser rejects is not left alone: the fail-closed fallback below still
//!   redacts its credential-shaped prefix, so `scheme://user:pa/ss@host` comes
//!   out as `scheme://***@host`.
//! * Only absolute `scheme://…` runs are considered. A bare connection string
//!   with no scheme, or a DSN fragment such as `user:password@host:5432/db`, is
//!   not a URL; it is left to the `Bearer` / query / JSON-field patterns, none
//!   of which look at `@`.
//! * The heuristic fallback runs only when parsing *fails* (an illegal port, for
//!   instance), and it rewrites a run only when the userinfo segment carries a
//!   `:`. Without that separator, a valid but credential-free run such as
//!   `https://host/path@x` — or `https://docs.example/page@2.0`, whose path
//!   merely holds an `@` — is left untouched even on the fallback path.
//!
//!   Do not read that separator as protecting `host:port/path@x` shapes: it does
//!   not. Handed `https://host:8080/path@x` or `https://[::1]:8080/path@x`,
//!   the fallback would rewrite them to `https://***@x`, because the `:` before
//!   the port satisfies the guard. Those URLs are safe *today* only because they
//!   parse, so the fallback is never reached — the protection is the parse gate,
//!   not the separator. Anything that loosens the gate (running the fallback
//!   unconditionally, say) would silently rewrite such a valid URL.

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

/// The placeholder that replaces the **directory** part of an absolute path.
const REDACTED_PATH: &str = "***";

/// An absolute filesystem path: a unix root, a Windows drive or a UNC share.
///
/// The first group is the **context** the path sits in: the start of the
/// string or a character that cannot be part of a longer word. That is what
/// keeps a URL's path out of the match — in `https://host/api/v1` the run
/// `/api/v1` follows a word character, and in `https://host//api` it follows
/// a slash, so neither is the start of a path on disk. The `regex` crate has
/// no look-behind, so the context is captured and checked by the pattern
/// itself.
fn absolute_path_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Literal pattern; no data-derived input reaches it.
        Regex::new(r#"(?i)(^|[^\w.:/\\])(?:[a-z]:[\\/]|\\\\|/)((?:[^\s"'<>|]+[\\/])+[^\s"'<>|]*)"#)
            .expect("absolute path regex")
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

/// Replace the **directory** part of every absolute path in `input`.
///
/// This is the path half of the redaction that `AGENTS.md` asks of the IPC
/// error payload ("全局 IPC 错误 Payload 自动脱敏凭据与绝对路径"). It is a
/// **separate** function from [`redact_secrets_for_log`] on purpose: that one
/// is used on log lines all over the host, where a path is exactly what the
/// reader needs, and changing its semantics would rewrite log output
/// everywhere. The IPC payload is the place where a path must not travel.
///
/// The file name survives — it is what the message is about — while the
/// directories are replaced: `/Users/alice/code/app.db` becomes
/// `***/app.db`. The user's home directory, the project layout and the
/// temporary directories stay behind.
///
/// A URL's path is **not** a path on disk and stays intact: `https://host/api/v1`
/// is returned unchanged.
pub fn redact_paths_for_ipc(input: &str) -> String {
    absolute_path_re()
        .replace_all(input, |caps: &regex::Captures<'_>| {
            let context = &caps[1];
            let path = &caps[2];
            // The last segment is the file name; the rest is the layout.
            let file_name = path
                .rsplit(['/', '\\'])
                .find(|segment| !segment.is_empty())
                .unwrap_or("");
            let masked = if file_name.is_empty() {
                REDACTED_PATH.to_string()
            } else {
                format!("{REDACTED_PATH}/{file_name}")
            };
            format!("{context}{masked}")
        })
        .into_owned()
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
        // This input is malformed per RFC 3986 — a raw `/` is not allowed in
        // userinfo — so the parser duly reads the authority as `user:pa` and
        // rejects the non-numeric port `pa`. The previous `[^/@\s]+` pattern
        // matched nothing here, which is how the credential reached the log
        // verbatim; the fallback is deliberately fail-closed and redacts it
        // anyway. The *valid* spelling of this password is the percent-encoded
        // `%2F` form covered by `redacts_percent_encoded_password_through_the_parser`.
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

    /// F3: exercise the *parser* path with real userinfo, so dropping the
    /// password (not merely replacing the user name) is what makes this pass.
    ///
    /// `%2F` and `%40` are RFC 3986's spellings of `/` and `@` inside
    /// userinfo, so both URLs below are valid, parse with genuine userinfo, and
    /// never reach the fallback.
    #[test]
    fn redacts_percent_encoded_password_through_the_parser() {
        let slash = redact_url_for_log("https://user:pa%2Fss@host/v1");
        assert!(!slash.contains("pa%2Fss"), "{slash}");
        assert_eq!(slash, "https://***@host/v1");

        let at = redact_secrets_for_log("https://user:pa%40ss@host/v1");
        assert!(!at.contains("pa%40ss"), "{at}");
        assert_eq!(at, "https://***@host/v1");
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

    /// FKU-D5: the absolute path of an IPC error payload must not travel.
    #[test]
    fn redacts_absolute_unix_paths_for_ipc() {
        let out = redact_paths_for_ipc("cannot open /Users/alice/code/app.db for reading");
        assert!(!out.contains("/Users/alice"), "{out}");
        assert!(!out.contains("code"), "{out}");
        // The file name survives: it is what the message is about.
        assert!(out.contains("app.db"), "{out}");
    }

    #[test]
    fn redacts_absolute_windows_paths_for_ipc() {
        let out = redact_paths_for_ipc(r"cannot open C:\Users\alice\AppData\app.db");
        assert!(!out.contains("Users"), "{out}");
        assert!(!out.contains("AppData"), "{out}");
        assert!(out.contains("app.db"), "{out}");
    }

    #[test]
    fn redacts_unc_paths_for_ipc() {
        let out = redact_paths_for_ipc(r"cannot open \\server\share\app.db");
        assert!(!out.contains("server"), "{out}");
        assert!(out.contains("app.db"), "{out}");
    }

    /// A URL's path is not a path on disk and must survive untouched.
    #[test]
    fn leaves_url_paths_intact_for_ipc() {
        let url = "GET https://api.example.com/v1/users failed with 500";
        assert_eq!(redact_paths_for_ipc(url), url);
    }

    /// A relative path is not absolute and stays as it is.
    #[test]
    fn leaves_relative_paths_intact_for_ipc() {
        let out = redact_paths_for_ipc("config/app.toml not found");
        assert_eq!(out, "config/app.toml not found");
    }

    /// The credential redaction is unchanged: the path step is a **separate**
    /// capability, and the two compose.
    #[test]
    fn path_redaction_does_not_change_credential_redaction() {
        let s = "mysql://root:s3cret@127.0.0.1:3306/app failed on /Users/alice/app.db";
        let secrets = redact_secrets_for_log(s);
        assert!(!secrets.contains("s3cret"), "{secrets}");
        assert!(
            secrets.contains("mysql://***@127.0.0.1:3306/app"),
            "{secrets}"
        );

        let both = redact_paths_for_ipc(&secrets);
        assert!(!both.contains("/Users/alice"), "{both}");
        assert!(both.contains("mysql://***@127.0.0.1:3306/app"), "{both}");
        assert!(both.contains("app.db"), "{both}");
    }
}
