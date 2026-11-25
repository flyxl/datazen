//! T318: `tunnelFieldContracts` and `useConnectionClipboardFill` ports.
//!
//! The React tester pins contract completeness, defaults, normalization and the
//! no-op tab fill; those cases are mirrored here alongside every guard branch
//! of the clipboard hook. The hook is exercised through a deterministic fake
//! host — no real clipboard, thread or future is involved.

use std::cell::RefCell;

use datazen_ui::components::connection::ssh_tunnel_fields::{
    SshAuthMethod as PortSshAuthMethod, SshFieldsState,
};
use datazen_ui::components::connection::tunnel_field_contracts::{
    http_proxy_inline_valid, is_positive_port, is_ws_url_valid, noop_tab_fill, ssh_inline_valid,
    websocket_inline_valid, FORM_TRACK_FIELDS, HttpProxyScheme, HttpProxyTunnelFieldsValue,
    SshAuthMethod, SshTunnelFieldsValue, WebSocketTunnelFieldsValue, WsMode,
};
use datazen_ui::components::connection::use_connection_clipboard_fill::{
    handle_event, ClipboardFillEvent, ClipboardFillHost, ClipboardFillOptions,
    ClipboardFillOutcome, ConnectionClipboardFill, MatchedConnectionClipboard,
};
use datazen_ui::components::connection::websocket_tunnel_fields::WsMode as PortWsMode;

// ---- React tester: contract completeness ------------------------------------

#[test]
fn all_three_value_contracts_have_the_pinned_fields_and_defaults() {
    // React's `contractCompleteness` is a type-level assertion; Rust pins the
    // same fact by constructing each value and checking its defaults.
    let ssh = SshTunnelFieldsValue::default();
    let http = HttpProxyTunnelFieldsValue::default();
    let ws = WebSocketTunnelFieldsValue::default();

    assert_eq!(ssh.port.text, "22", "SSH port default matches the form hook");
    assert_eq!(ssh.jump_port.text, "22", "jump port default matches the form hook");
    assert_eq!(ssh.auth_method, SshAuthMethod::Password, "SSH auth default");
    assert_eq!(ssh.jump_auth_method, SshAuthMethod::Password, "jump auth default");
    assert!(!ssh.enabled && !ssh.jump_enabled, "both toggles default off");

    assert_eq!(http.port.text, "8080", "http proxy port default");
    assert_eq!(http.timeout.text, "30", "http proxy timeout default");
    assert_eq!(http.scheme, HttpProxyScheme::Http, "http proxy scheme default");

    assert_eq!(ws.timeout.text, "30", "WebSocket timeout default");
    assert_eq!(ws.mode, WsMode::DatazenV1, "WebSocket mode default");

    // The settings editor must still be able to drive the form-track fields.
    assert_eq!(FORM_TRACK_FIELDS.len(), 12);
    let pinned = [
        "supports_ssh", "enabled", "set_enabled", "jump_enabled", "set_jump_enabled",
        "jump_host", "jump_port", "jump_username", "jump_auth_method", "jump_password",
        "jump_key_path", "jump_passphrase",
    ];
    assert_eq!(FORM_TRACK_FIELDS, pinned, "React FORM_TRACK_FIELDS, snake_cased");
    // The nine value fields really live on the contract (the other three are
    // host ports). Constructing a non-default value proves each is writable.
    let _edited = SshTunnelFieldsValue {
        enabled: true,
        port: rune::TextEdit::new("2222".to_owned()),
        jump_enabled: true,
        jump_port: rune::TextEdit::new("2223".to_owned()),
        jump_host: rune::TextEdit::new("bastion.example".to_owned()),
        jump_username: rune::TextEdit::new("root".to_owned()),
        jump_auth_method: SshAuthMethod::Agent,
        jump_password: rune::TextEdit::new("secret".to_owned()),
        jump_key_path: rune::TextEdit::new("/keys/id_rsa".to_owned()),
        jump_passphrase: rune::TextEdit::new("phrase".to_owned()),
        ..SshTunnelFieldsValue::default()
    };
}

#[test]
fn contract_modules_keep_the_no_escape_hatch_rule() {
    // React's tester scans the four files for `any` / `@ts-ignore` / `unknown`
    // casts. The Rune parity rule is no bare unwrap/expect, no unsafe and no
    // fake async in the pure contract and hook modules.
    for (name, source) in [
        ("tunnel_field_contracts", include_str!("../src/components/connection/tunnel_field_contracts.rs")),
        ("use_connection_clipboard_fill", include_str!("../src/components/connection/use_connection_clipboard_fill.rs")),
    ] {
        for banned in ["unwrap()", "expect(", "unsafe ", "std::thread", "tokio::"] {
            assert!(!source.contains(banned), "{name} must not use {banned}");
        }
    }
}

#[test]
fn field_ports_re_export_the_contract_without_duplicating_it() {
    for (name, source) in [
        ("ssh_tunnel_fields", include_str!("../src/components/connection/ssh_tunnel_fields.rs")),
        ("http_proxy_tunnel_fields", include_str!("../src/components/connection/http_proxy_tunnel_fields.rs")),
        ("websocket_tunnel_fields", include_str!("../src/components/connection/websocket_tunnel_fields.rs")),
    ] {
        assert!(source.contains("tunnel_field_contracts::"), "{name} uses the shared contract");
        assert!(!source.contains("pub enum SshAuthMethod"), "{name} has no local SSH enum");
        assert!(!source.contains("pub enum HttpProxyScheme"), "{name} has no local scheme enum");
        assert!(!source.contains("pub enum WsMode"), "{name} has no local WS enum");
    }
    // The re-exported names are the same types, not look-alikes.
    let from_contract = SshAuthMethod::PrivateKey;
    let from_port: PortSshAuthMethod = from_contract;
    assert_eq!(from_port, PortSshAuthMethod::PrivateKey);
    let ws_from_contract = WsMode::RawBinary;
    let ws_from_port: PortWsMode = ws_from_contract;
    assert_eq!(ws_from_port, PortWsMode::RawBinary);
    let _alias_check: SshFieldsState = SshTunnelFieldsValue::default();
}

// ---- Enum normalization and validation --------------------------------------

#[test]
fn enum_normalization_matches_the_react_onchange_rules() {
    assert_eq!(SshAuthMethod::from_value("private_key"), SshAuthMethod::PrivateKey);
    assert_eq!(SshAuthMethod::from_value("agent"), SshAuthMethod::Agent);
    assert_eq!(SshAuthMethod::from_value("password"), SshAuthMethod::Password);
    assert_eq!(SshAuthMethod::from_value("bogus"), SshAuthMethod::Password);
    for method in [
        SshAuthMethod::Password,
        SshAuthMethod::PrivateKey,
        SshAuthMethod::Agent,
    ] {
        assert_eq!(SshAuthMethod::from_value(method.value()), method);
    }

    assert_eq!(HttpProxyScheme::from_value("https"), HttpProxyScheme::Https);
    assert_eq!(HttpProxyScheme::from_value("http"), HttpProxyScheme::Http);
    assert_eq!(HttpProxyScheme::from_value("bogus"), HttpProxyScheme::Http);

    assert_eq!(WsMode::from_value("raw_binary"), WsMode::RawBinary);
    assert_eq!(WsMode::from_value("bogus"), WsMode::DatazenV1);
    for mode in [WsMode::DatazenV1, WsMode::RawBinary] {
        assert_eq!(WsMode::from_value(mode.value()), mode);
    }
}

#[test]
fn validation_rules_match_the_react_form_hooks() {
    assert!(!is_positive_port(""), "empty port");
    assert!(!is_positive_port("  "), "whitespace port");
    assert!(!is_positive_port("abc"), "non-numeric port");
    assert!(!is_positive_port("0"), "zero port");
    assert!(!is_positive_port("-1"), "negative port");
    assert!(is_positive_port("5432"), "valid port");
    assert!(is_positive_port(" 8080 "), "trimmed valid port");

    assert!(!is_ws_url_valid(""), "blank URL");
    assert!(!is_ws_url_valid("  "), "whitespace URL");
    assert!(is_ws_url_valid("wss://relay.example.com/v1"), "real URL");

    assert!(!ssh_inline_valid(false, "db.example", "root"), "disabled SSH");
    assert!(!ssh_inline_valid(true, "  ", "root"), "blank SSH host");
    assert!(!ssh_inline_valid(true, "db.example", " "), "blank SSH user");
    assert!(ssh_inline_valid(true, "db.example", "root"), "complete SSH");

    assert!(!http_proxy_inline_valid("", "8080"), "blank proxy host");
    assert!(!http_proxy_inline_valid("proxy", "0"), "bad proxy port");
    assert!(http_proxy_inline_valid("proxy", "8080"), "complete proxy");

    assert_eq!(websocket_inline_valid("wss://relay"), is_ws_url_valid("wss://relay"));
}

#[test]
fn noop_tab_fill_is_harmless() {
    let fill = noop_tab_fill();
    let inner = fill.clone();
    inner();
    fill();
}

// ---- Clipboard fill hook -----------------------------------------------------

#[derive(Default)]
struct ClipboardHost {
    name: String,
    password: String,
    applied: bool,
    applied_types: Vec<String>,
    fill_count: usize,
    seen_texts: RefCell<Vec<String>>,
    seen_available: RefCell<Vec<Option<Vec<String>>>>,
    matched_type: String,
    /// When set, the matcher only recognises text containing this needle.
    match_needle: Option<String>,
}

impl ClipboardHost {
    fn accepting() -> Self {
        Self {
            matched_type: "postgres".to_owned(),
            ..Self::default()
        }
    }
}

impl ClipboardFillHost for ClipboardHost {
    fn form_has_input(&self) -> bool {
        !self.name.trim().is_empty() || !self.password.is_empty()
    }

    fn is_applied(&self) -> bool {
        self.applied
    }

    fn match_clipboard(
        &self,
        text: &str,
        available_types: Option<&[String]>,
    ) -> Option<MatchedConnectionClipboard> {
        self.seen_texts.borrow_mut().push(text.to_owned());
        self.seen_available.borrow_mut().push(available_types.map(|v| v.to_vec()));
        if let Some(needle) = &self.match_needle {
            if !text.contains(needle) {
                return None;
            }
        }
        Some(MatchedConnectionClipboard {
            database_type: self.matched_type.clone(),
            fill: ConnectionClipboardFill {
                host: Some("db.example.com".to_owned()),
                port: Some("5432".to_owned()),
                name: Some("db.example.com:5432".to_owned()),
                ..ConnectionClipboardFill::default()
            },
        })
    }

    fn apply_matched(&mut self, matched: &MatchedConnectionClipboard) {
        self.fill_count += 1;
        let _ = matched;
    }

    fn mark_applied(&mut self) {
        self.applied = true;
    }

    fn on_applied(&mut self, database_type: &str) {
        self.applied_types.push(database_type.to_owned());
    }
}

fn paste(text: &str, from_password_field: bool) -> ClipboardFillEvent {
    ClipboardFillEvent::Paste {
        text: text.to_owned(),
        from_password_field,
    }
}

#[test]
fn clipboard_fill_ignores_disabled_and_already_applied_hosts() {
    let mut host = ClipboardHost::accepting();
    let disabled = ClipboardFillOptions::new(false);
    assert_eq!(
        handle_event(&mut host, &disabled, paste("postgres://db", false)),
        ClipboardFillOutcome::Ignored,
        "a disabled hook never matches"
    );
    assert!(host.seen_texts.borrow().is_empty(), "matcher was not called");

    let enabled = ClipboardFillOptions::new(true);
    host.applied = true;
    assert_eq!(
        handle_event(&mut host, &enabled, paste("postgres://db", false)),
        ClipboardFillOutcome::Ignored,
        "the one-shot guard holds"
    );
    assert!(host.seen_texts.borrow().is_empty(), "matcher was not called");
}

#[test]
fn clipboard_fill_never_overwrites_user_input_or_unmatched_text() {
    let enabled = ClipboardFillOptions::new(true);

    let mut named = ClipboardHost::accepting();
    named.name = "my db".to_owned();
    assert_eq!(
        handle_event(&mut named, &enabled, paste("postgres://db", false)),
        ClipboardFillOutcome::Ignored,
        "a typed connection name blocks the fill"
    );

    let mut with_password = ClipboardHost::accepting();
    with_password.password = "hunter2".to_owned();
    assert_eq!(
        handle_event(&mut with_password, &enabled, paste("postgres://db", false)),
        ClipboardFillOutcome::Ignored,
        "a typed password blocks the fill"
    );

    let mut unmatched = ClipboardHost::accepting();
    unmatched.match_needle = Some("postgres://".to_owned());
    assert_eq!(
        handle_event(&mut unmatched, &enabled, paste("hello", false)),
        ClipboardFillOutcome::Ignored,
        "unmatched text is left alone"
    );
    assert!(!unmatched.applied, "nothing was marked applied");
}

#[test]
fn clipboard_fill_enforces_the_password_field_url_rule() {
    let enabled = ClipboardFillOptions::new(true);
    let mut host = ClipboardHost::accepting();
    assert_eq!(
        handle_event(&mut host, &enabled, paste("db.example.com:5432", true)),
        ClipboardFillOutcome::Ignored,
        "a bare host:port pasted into a password field is ignored"
    );
    assert!(host.seen_texts.borrow().iter().any(|t| t == "db.example.com:5432"),
        "the matcher still saw the text");
    assert!(!host.applied);

    let mut with_url = ClipboardHost::accepting();
    assert_eq!(
        handle_event(&mut with_url, &enabled, paste("postgres://db.example.com", true)),
        ClipboardFillOutcome::Applied {
            database_type: "postgres".to_owned(),
            prevent_default: true,
        },
        "a connection URL pasted into a password field applies"
    );
}

#[test]
fn clipboard_fill_paste_applies_once_and_prevents_default() {
    let mut host = ClipboardHost::accepting();
    let options = ClipboardFillOptions::new(true);
    let first = handle_event(&mut host, &options, paste("postgres://db", false));
    assert_eq!(
        first,
        ClipboardFillOutcome::Applied {
            database_type: "postgres".to_owned(),
            prevent_default: true,
        }
    );
    assert_eq!(host.fill_count, 1, "the fill was applied");
    assert!(host.applied, "the one-shot flag is set");
    assert_eq!(host.applied_types, ["postgres"], "on_applied saw the database type");

    let second = handle_event(&mut host, &options, paste("mysql://db", false));
    assert_eq!(second, ClipboardFillOutcome::Ignored, "a second paste is ignored");
    assert_eq!(host.fill_count, 1, "no second fill");
}

#[test]
fn clipboard_fill_auto_read_respects_the_option_and_available_types() {
    let mut no_auto = ClipboardHost::accepting();
    let mut options = ClipboardFillOptions::new(true);
    options.auto_read = false;
    assert_eq!(
        handle_event(&mut no_auto, &options, ClipboardFillEvent::AutoRead("postgres://db".to_owned())),
        ClipboardFillOutcome::Ignored,
        "autoRead=false suppresses the mount-time read"
    );
    assert!(no_auto.seen_texts.borrow().is_empty(), "the matcher was not called");

    let mut with_auto = ClipboardHost::accepting();
    let mut options = ClipboardFillOptions::new(true);
    options.available_types = Some(vec!["postgres".to_owned(), "mysql".to_owned()]);
    assert_eq!(
        handle_event(
            &mut with_auto,
            &options,
            ClipboardFillEvent::AutoRead("postgres://db".to_owned()),
        ),
        ClipboardFillOutcome::Applied {
            database_type: "postgres".to_owned(),
            prevent_default: false,
        },
        "auto read applies without asking for preventDefault"
    );
    assert_eq!(
        with_auto.seen_available.borrow()[0],
        Some(vec!["postgres".to_owned(), "mysql".to_owned()]),
        "availableTypes reaches the matcher"
    );
}
