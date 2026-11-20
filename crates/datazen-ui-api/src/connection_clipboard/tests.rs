use serde_json::json;

use crate::build_connection_url::ConnectionUrlDriverMeta;

use super::parsing::*;
use super::*;

fn meta(
    id: &str,
    schemes: &[&str],
    port: Option<u16>,
    mode: ConnectionMode,
) -> ClipboardDriverMeta {
    ClipboardDriverMeta {
        id: id.to_owned(),
        clipboard_schemes: schemes.iter().map(|item| (*item).to_owned()).collect(),
        default_port: port,
        connection_mode: mode,
        parser: None,
    }
}

fn server(id: &str, schemes: &[&str], port: u16) -> ClipboardDriverMeta {
    meta(id, schemes, Some(port), ConnectionMode::Server)
}

fn url_meta_for(
    _id: &str,
    schemes: &[&str],
    port: Option<u16>,
    mode: ConnectionMode,
) -> ConnectionUrlDriverMeta {
    ConnectionUrlDriverMeta {
        clipboard_schemes: schemes.iter().map(|item| (*item).to_owned()).collect(),
        connection_mode: mode,
        default_port: port,
    }
}

#[test]
fn normalization_strips_bom_quotes_and_space() {
    assert_eq!(normalize_clipboard("  mysql://host  "), "mysql://host");
    assert_eq!(
        normalize_clipboard("\u{feff}'mysql://host'"),
        "mysql://host"
    );
    assert_eq!(normalize_clipboard("\"'mysql://host'\""), "mysql://host");
    assert_eq!(normalize_clipboard("   "), "");
}

#[test]
fn env_prefix_is_stripped_case_insensitively() {
    for prefix in ["DATABASE_URL", "DB_URL", "REDIS_URL", "URL", "database_url"] {
        let raw = format!("export {prefix} =   mysql://host:3306/db ");
        assert_eq!(strip_env_prefix(&raw), "mysql://host:3306/db");
        let raw = format!("{prefix}=mysql://host");
        assert_eq!(strip_env_prefix(&raw), "mysql://host");
    }
    assert_eq!(strip_env_prefix("mysql://host"), "mysql://host");
    assert_eq!(strip_env_prefix("DATABASE_URL="), "DATABASE_URL=");
    assert_eq!(
        strip_env_prefix("export mysql://host"),
        "export mysql://host"
    );
}

#[test]
fn authority_parsing_covers_ipv4_ipv6_and_defaults() {
    assert_eq!(
        parse_authority("localhost:5432", 0),
        Some(("localhost".into(), 5432))
    );
    assert_eq!(
        parse_authority("localhost", 3306),
        Some(("localhost".into(), 3306))
    );
    assert_eq!(parse_authority("localhost", 0), None);
    assert_eq!(
        parse_authority("[::1]:6379", 0),
        Some(("[::1]".into(), 6379))
    );
    assert_eq!(parse_authority("[::1]", 6379), Some(("[::1]".into(), 6379)));
    assert_eq!(parse_authority("[::1]", 0), None);
    assert_eq!(parse_authority("[::1]x", 0), None);
    assert_eq!(parse_authority("host:0", 3306), None);
    assert_eq!(parse_authority("host:70000", 0), None);
    assert_eq!(parse_authority("host:abc", 0), None);
    assert_eq!(parse_authority(" :5432", 0), None);
}

#[test]
fn bare_host_port_rejects_schemes_and_separators() {
    assert_eq!(
        parse_bare_host_port("localhost:5432"),
        Some(("localhost".into(), 5432))
    );
    assert_eq!(parse_bare_host_port("mysql://host"), None);
    assert_eq!(parse_bare_host_port("host:1,other:2"), None);
    assert_eq!(parse_bare_host_port("localhost"), None);
}

#[test]
fn scheme_extraction_handles_jdbc_and_plain_urls() {
    assert_eq!(
        extract_url_scheme("jdbc:postgresql://host").as_deref(),
        Some("postgresql")
    );
    assert_eq!(extract_url_scheme("MYSQL://host").as_deref(), Some("mysql"));
    assert_eq!(
        extract_url_scheme("redis+tcp://host").as_deref(),
        Some("redis+tcp")
    );
    assert_eq!(extract_url_scheme("1bad://host"), None);
    assert_eq!(extract_url_scheme("no-scheme"), None);
}

#[test]
fn ssl_mode_values_map_to_the_enum() {
    for value in ["disable", "FALSE", "0"] {
        assert_eq!(read_ssl_mode(Some(value)), Some(SslMode::Disable));
    }
    assert_eq!(read_ssl_mode(Some("Prefer")), Some(SslMode::Prefer));
    for value in ["require", "true", "1", "verify-ca", "verify-full"] {
        assert_eq!(read_ssl_mode(Some(value)), Some(SslMode::Require));
    }
    assert_eq!(read_ssl_mode(Some("bogus")), None);
    assert_eq!(read_ssl_mode(None), None);
}

#[test]
fn uri_decoding_recovers_bytes_and_rejects_malformed_input() {
    assert_eq!(decode_uri("a%20b%2Fc"), "a b/c");
    assert_eq!(decode_uri("%E4%BD%A0%E5%A5%BD"), "你好");
    assert_eq!(decode_uri("100%"), "100%");
    assert_eq!(decode_uri("%ZZ"), "%ZZ");
    assert_eq!(decode_uri("%FF"), "%FF");
}

#[test]
fn generic_url_parses_credentials_path_and_query() {
    let meta = url_meta_for(
        "postgresql",
        &["postgresql"],
        Some(5432),
        ConnectionMode::Server,
    );
    let fill = parse_generic_connection_url(
        "jdbc:postgresql://alice:s3cret@db.internal:6543/mydb?sslmode=require",
        &meta,
    )
    .unwrap();
    assert_eq!(fill.host.as_deref(), Some("db.internal"));
    assert_eq!(fill.port.as_deref(), Some("6543"));
    assert_eq!(fill.database.as_deref(), Some("mydb"));
    assert_eq!(fill.username.as_deref(), Some("alice"));
    assert_eq!(fill.password.as_deref(), Some("s3cret"));
    assert_eq!(fill.name.as_deref(), Some("db.internal:6543"));
    assert_eq!(fill.ssl_mode, Some(SslMode::Require));
    assert!(fill.expand_advanced);
}

#[test]
fn generic_url_uses_default_port_and_ssl_aliases() {
    let meta = url_meta_for("mysql", &["mysql"], Some(3306), ConnectionMode::Server);
    let fill = parse_generic_connection_url("mysql://db.internal/mydb?ssl=false", &meta).unwrap();
    assert_eq!(fill.port.as_deref(), Some("3306"));
    assert_eq!(fill.name.as_deref(), Some("db.internal:3306"));
    assert_eq!(fill.ssl_mode, Some(SslMode::Disable));
    assert!(fill.expand_advanced);
    let fill = parse_generic_connection_url("mysql://db/internal?sslmode=prefer", &meta).unwrap();
    assert_eq!(fill.ssl_mode, Some(SslMode::Prefer));
    assert!(!fill.expand_advanced);
}

#[test]
fn generic_url_handles_encoded_password_and_username_only() {
    let meta = url_meta_for("redis", &["redis"], Some(6379), ConnectionMode::Server);
    let fill =
        parse_generic_connection_url("redis://alice:p%40ss%2Fword@cache:6380", &meta).unwrap();
    assert_eq!(fill.username.as_deref(), Some("alice"));
    assert_eq!(fill.password.as_deref(), Some("p@ss/word"));
    let fill = parse_generic_connection_url("redis://alice@cache:6380", &meta).unwrap();
    assert_eq!(fill.username.as_deref(), Some("alice"));
    assert_eq!(fill.password, None);
}

#[test]
fn file_mode_uses_path_as_database_and_basename_as_name() {
    let meta = url_meta_for("sqlite", &["sqlite"], None, ConnectionMode::File);
    let fill = parse_generic_connection_url("sqlite:///tmp/my%20db.sqlite", &meta).unwrap();
    assert_eq!(fill.database.as_deref(), Some("/tmp/my db.sqlite"));
    assert_eq!(fill.name.as_deref(), Some("my db.sqlite"));
    let fill = parse_generic_connection_url("sqlite:////tmp/x.db", &meta).unwrap();
    assert_eq!(fill.database.as_deref(), Some("/tmp/x.db"));
    let root = parse_generic_connection_url("sqlite:///", &meta).unwrap();
    assert_eq!(root.database.as_deref(), Some("/"));
    assert_eq!(root.name, None);
}

#[test]
fn generic_url_rejects_malformed_input() {
    let meta = url_meta_for("mysql", &["mysql"], Some(3306), ConnectionMode::Server);
    assert_eq!(parse_generic_connection_url("not a url", &meta), None);
    assert_eq!(parse_generic_connection_url("1mysql://host", &meta), None);
    assert_eq!(
        parse_generic_connection_url("mysql://host:bad", &meta),
        None
    );
    assert_eq!(
        parse_generic_connection_url("DATABASE_URL=mysql://host:3306/db", &meta)
            .unwrap()
            .host
            .as_deref(),
        Some("host")
    );
}

fn redis_parser(text: &str) -> Option<ConnectionClipboardFill> {
    let rest = text.strip_prefix("myredis://")?;
    Some(ConnectionClipboardFill {
        host: Some(rest.to_owned()),
        ..Default::default()
    })
}

fn registry() -> Vec<ClipboardDriverMeta> {
    vec![
        server("mysql", &["mysql", "mysql2"], 3306),
        server("postgresql", &["postgresql", "postgres"], 5432),
        server("redis", &["redis"], 6379),
        ClipboardDriverMeta {
            parser: Some(redis_parser),
            ..meta("mydb", &["myredis"], None, ConnectionMode::Server)
        },
    ]
}

#[test]
fn matcher_returns_none_for_empty_or_unknown_text() {
    let registry = registry();
    assert!(match_connection_clipboard("   ", None, &registry).is_none());
    assert!(match_connection_clipboard("just some text", None, &registry).is_none());
}

#[test]
fn driver_parser_wins_and_respects_allowed_types() {
    let registry = registry();
    let matched = match_connection_clipboard("myredis://cache.internal", None, &registry).unwrap();
    assert_eq!(matched.database_type, "mydb");
    assert_eq!(matched.fill.host.as_deref(), Some("cache.internal"));
    let allowed = vec!["mysql".to_owned()];
    assert!(
        match_connection_clipboard("myredis://cache.internal", Some(&allowed), &registry).is_none()
    );
}

#[test]
fn scheme_matching_picks_the_popular_type_and_parses() {
    let registry = vec![
        server("zzunknown", &["shared"], 1),
        server("mysql", &["shared"], 3306),
    ];
    let matched = match_connection_clipboard("shared://db.internal/db", None, &registry).unwrap();
    assert_eq!(matched.database_type, "mysql");
    assert_eq!(matched.fill.host.as_deref(), Some("db.internal"));
}

#[test]
fn scheme_matching_with_no_parse_returns_none() {
    let registry = vec![server("mysql", &["mysql"], 3306)];
    assert!(match_connection_clipboard("mysql://host:bad", None, &registry).is_none());
    let allowed = vec!["redis".to_owned()];
    assert!(match_connection_clipboard("mysql://host/db", Some(&allowed), &registry).is_none());
}

#[test]
fn bare_host_port_matches_a_unique_default_port() {
    let registry = registry();
    let matched = match_connection_clipboard("localhost:5432", None, &registry).unwrap();
    assert_eq!(matched.database_type, "postgresql");
    assert_eq!(matched.fill.host.as_deref(), Some("localhost"));
    assert_eq!(matched.fill.port.as_deref(), Some("5432"));
    assert_eq!(matched.fill.name.as_deref(), Some("localhost:5432"));
    let ambiguous = vec![server("a", &["a"], 5432), server("b", &["b"], 5432)];
    assert!(match_connection_clipboard("localhost:5432", None, &ambiguous).is_none());
}

#[derive(Default)]
struct MockForm {
    database_type: String,
    host: String,
    port: String,
    database: String,
    username: String,
    password: String,
    schema: String,
    ssl_mode: Option<SslMode>,
    options: Option<Value>,
    show_advanced: bool,
    name: String,
}

impl ConnectionFormOps for MockForm {
    fn database_type(&self) -> &str {
        &self.database_type
    }
    fn set_database_type(&mut self, value: &str) {
        self.database_type = value.to_owned();
    }
    fn set_host(&mut self, value: &str) {
        self.host = value.to_owned();
    }
    fn set_port(&mut self, value: &str) {
        self.port = value.to_owned();
    }
    fn set_database(&mut self, value: &str) {
        self.database = value.to_owned();
    }
    fn set_username(&mut self, value: &str) {
        self.username = value.to_owned();
    }
    fn set_password(&mut self, value: &str) {
        self.password = value.to_owned();
    }
    fn set_schema(&mut self, value: &str) {
        self.schema = value.to_owned();
    }
    fn set_ssl_mode(&mut self, value: SslMode) {
        self.ssl_mode = Some(value);
    }
    fn set_options(&mut self, value: Value) {
        self.options = Some(value);
    }
    fn options(&self) -> Option<&Value> {
        self.options.as_ref()
    }
    fn set_show_advanced(&mut self, value: bool) {
        self.show_advanced = value;
    }
    fn set_name(&mut self, value: &str) {
        self.name = value.to_owned();
    }
    fn name(&self) -> &str {
        &self.name
    }
}

#[test]
fn fill_application_sets_every_field_and_merges_options() {
    let mut form = MockForm {
        options: Some(json!({ "keep": 1, "over": "old" })),
        ..Default::default()
    };
    let fill = ConnectionClipboardFill {
        host: Some("db".into()),
        port: Some("5432".into()),
        database: Some("mydb".into()),
        username: Some("alice".into()),
        password: Some("secret".into()),
        schema: Some("public".into()),
        name: Some("db:5432".into()),
        ssl_mode: Some(SslMode::VerifyCa),
        options: Some(json!({ "over": "new", "added": true })),
        expand_advanced: false,
    };
    apply_connection_clipboard_fill(&mut form, &fill);
    assert_eq!(form.host, "db");
    assert_eq!(form.port, "5432");
    assert_eq!(form.database, "mydb");
    assert_eq!(form.username, "alice");
    assert_eq!(form.password, "secret");
    assert_eq!(form.schema, "public");
    assert_eq!(form.name, "db:5432");
    assert_eq!(form.ssl_mode, Some(SslMode::VerifyCa));
    assert_eq!(
        form.options,
        Some(json!({ "keep": 1, "over": "new", "added": true }))
    );
    assert!(!form.show_advanced);
}

#[test]
fn advanced_expands_for_require_disable_and_explicit_flag() {
    for mode in [SslMode::Require, SslMode::Disable] {
        let mut form = MockForm::default();
        apply_connection_clipboard_fill(
            &mut form,
            &ConnectionClipboardFill {
                ssl_mode: Some(mode),
                ..Default::default()
            },
        );
        assert!(form.show_advanced);
    }
    let mut form = MockForm::default();
    apply_connection_clipboard_fill(
        &mut form,
        &ConnectionClipboardFill {
            expand_advanced: true,
            ..Default::default()
        },
    );
    assert!(form.show_advanced);
}

#[test]
fn name_is_only_set_when_the_form_name_is_blank() {
    let mut form = MockForm {
        name: "   ".to_owned(),
        ..Default::default()
    };
    apply_connection_clipboard_fill(
        &mut form,
        &ConnectionClipboardFill {
            name: Some("generated".into()),
            ..Default::default()
        },
    );
    assert_eq!(form.name, "generated");
    form.name = "mine".to_owned();
    apply_connection_clipboard_fill(
        &mut form,
        &ConnectionClipboardFill {
            name: Some("generated".into()),
            ..Default::default()
        },
    );
    assert_eq!(form.name, "mine");
}

#[test]
fn matched_application_changes_type_only_when_different() {
    let mut form = MockForm {
        database_type: "mysql".to_owned(),
        ..Default::default()
    };
    let same = MatchedConnectionClipboard {
        database_type: "mysql".to_owned(),
        fill: ConnectionClipboardFill {
            host: Some("db".into()),
            ..Default::default()
        },
    };
    apply_matched_clipboard(&mut form, &same);
    assert_eq!(form.database_type, "mysql");
    assert_eq!(form.host, "db");
    let other = MatchedConnectionClipboard {
        database_type: "postgresql".to_owned(),
        fill: ConnectionClipboardFill::default(),
    };
    apply_matched_clipboard(&mut form, &other);
    assert_eq!(form.database_type, "postgresql");
}

#[test]
fn debug_output_never_exposes_the_password() {
    let fill = ConnectionClipboardFill {
        host: Some("db".into()),
        password: Some("super-secret".into()),
        ..Default::default()
    };
    let rendered = format!("{fill:?}");
    assert!(!rendered.contains("super-secret"));
    assert!(rendered.contains("<redacted>"));
    let matched = MatchedConnectionClipboard {
        database_type: "mysql".to_owned(),
        fill,
    };
    assert!(!format!("{matched:?}").contains("super-secret"));
}
