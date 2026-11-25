//! T318 independent audit: tunnel contracts + clipboard-fill step.
//!
//! Probes guard branches the coder's suite does not: whitespace/empty form
//! input, empty available-types passthrough, the matcher/side-effect ordering,
//! numeric parsing on realistic port strings, and source guards that the host
//! port performs no I/O of its own.

use std::cell::RefCell;

use datazen_ui::components::connection::tunnel_field_contracts::{
    http_proxy_inline_valid, is_positive_port, ssh_inline_valid, websocket_inline_valid,
};
use datazen_ui::components::connection::use_connection_clipboard_fill::{
    handle_event, ClipboardFillEvent, ClipboardFillHost, ClipboardFillOptions, ClipboardFillOutcome,
    ConnectionClipboardFill, MatchedConnectionClipboard,
};

#[derive(Default)]
struct Host {
    name: String,
    password: String,
    applied: bool,
    apply_count: usize,
    on_applied: Vec<String>,
    seen: RefCell<Vec<(String, Option<Vec<String>>)>>,
    matched: bool,
    order: RefCell<Vec<&'static str>>,
}

impl ClipboardFillHost for Host {
    fn form_has_input(&self) -> bool {
        !self.name.trim().is_empty() || !self.password.is_empty()
    }
    fn is_applied(&self) -> bool {
        self.applied
    }
    fn match_clipboard(&self, text: &str, types: Option<&[String]>) -> Option<MatchedConnectionClipboard> {
        self.order.borrow_mut().push("match");
        self.seen.borrow_mut().push((text.to_owned(), types.map(|t| t.to_vec())));
        if !self.matched {
            return None;
        }
        Some(MatchedConnectionClipboard {
            database_type: "mysql".to_owned(),
            fill: ConnectionClipboardFill::default(),
        })
    }
    fn apply_matched(&mut self, _matched: &MatchedConnectionClipboard) {
        self.order.borrow_mut().push("apply");
        self.apply_count += 1;
    }
    fn mark_applied(&mut self) {
        self.order.borrow_mut().push("mark");
        self.applied = true;
    }
    fn on_applied(&mut self, database_type: &str) {
        self.order.borrow_mut().push("report");
        self.on_applied.push(database_type.to_owned());
    }
}

fn pasted(text: &str, from_password: bool) -> ClipboardFillEvent {
    ClipboardFillEvent::Paste {
        text: text.to_owned(),
        from_password_field: from_password,
    }
}

/// React's `current.name.trim() || current.password`: a whitespace-only name
/// and an empty password leave the form "empty" so a paste may still fill it.
#[test]
fn whitespace_name_and_empty_password_do_not_block_the_fill() {
    let mut host = Host {
        name: "   ".to_owned(),
        password: String::new(),
        matched: true,
        ..Host::default()
    };
    let outcome = handle_event(&mut host, &ClipboardFillOptions::new(true), pasted("mysql://db", false));
    assert_eq!(
        outcome,
        ClipboardFillOutcome::Applied {
            database_type: "mysql".to_owned(),
            prevent_default: true
        }
    );
    assert_eq!(host.apply_count, 1);
}

/// An empty `availableTypes` list reaches the matcher as `Some([])` (React
/// passes `[] ?? undefined` through unchanged), not as `None`.
#[test]
fn empty_available_types_list_reaches_the_matcher() {
    let mut host = Host {
        matched: true,
        ..Host::default()
    };
    let mut options = ClipboardFillOptions::new(true);
    options.available_types = Some(Vec::new());
    let _ = handle_event(&mut host, &options, pasted("mysql://db", false));
    assert_eq!(host.seen.borrow()[0].1, Some(Vec::new()));
}

/// The hook never asks for `preventDefault` on an auto-read (React only calls
/// it in the paste listener), and the side effects run in React's order.
#[test]
fn auto_read_never_prevents_default_and_side_effects_are_ordered() {
    let mut host = Host {
        matched: true,
        ..Host::default()
    };
    let outcome = handle_event(
        &mut host,
        &ClipboardFillOptions::new(true),
        ClipboardFillEvent::AutoRead("mysql://db".to_owned()),
    );
    assert_eq!(
        outcome,
        ClipboardFillOutcome::Applied {
            database_type: "mysql".to_owned(),
            prevent_default: false
        }
    );
    assert_eq!(*host.order.borrow(), ["match", "apply", "mark", "report"]);
}

/// A password-field paste without `://` is rejected AFTER the matcher runs,
/// and leaves the one-shot flag unset so a later normal paste still works.
#[test]
fn rejected_password_paste_stays_retryable() {
    let mut host = Host {
        matched: true,
        ..Host::default()
    };
    let options = ClipboardFillOptions::new(true);
    assert_eq!(
        handle_event(&mut host, &options, pasted("db.example.com:5432", true)),
        ClipboardFillOutcome::Ignored
    );
    assert!(!host.applied, "one-shot flag not set by a rejected paste");
    assert_eq!(
        handle_event(&mut host, &options, pasted("mysql://db", false)),
        ClipboardFillOutcome::Applied {
            database_type: "mysql".to_owned(),
            prevent_default: true
        }
    );
    assert_eq!(host.apply_count, 1);
}

/// Numeric/edge port strings: trim, scientific notation and finite checks
/// follow JS `Number()` for the forms a port field realistically receives.
#[test]
fn port_validation_covers_realistic_numeric_forms() {
    assert!(is_positive_port("5432"));
    assert!(is_positive_port(" 5432 "));
    assert!(is_positive_port("1e3"));
    assert!(is_positive_port("443.5"));
    assert!(!is_positive_port("0"));
    assert!(!is_positive_port("-1"));
    assert!(!is_positive_port("NaN"));
    assert!(!is_positive_port("Infinity"));
    assert!(!is_positive_port("12abc"));
}

/// The three inline-valid guards use the exact React branch conditions.
#[test]
fn inline_valid_guards_match_react_branches() {
    assert!(ssh_inline_valid(true, "h", "u"));
    assert!(!ssh_inline_valid(false, "h", "u"));
    assert!(!ssh_inline_valid(true, "", "u"));
    assert!(!ssh_inline_valid(true, "h", ""));
    assert!(http_proxy_inline_valid("proxy", "8080"));
    assert!(!http_proxy_inline_valid("proxy", "0"));
    assert!(websocket_inline_valid("wss://relay"));
    assert!(!websocket_inline_valid("   "));
}

/// The clipboard module performs no I/O, threads, futures or fake IPC itself.
#[test]
fn clipboard_module_is_pure_and_fake_free() {
    let src = include_str!("../src/components/connection/use_connection_clipboard_fill.rs");
    for banned in ["std::fs", "std::thread", "tokio::", "invoke(", "TcpStream", "unwrap()", "expect("] {
        assert!(!src.contains(banned), "forbidden in pure step module: {banned}");
    }
    assert!(src.contains("trait ClipboardFillHost"), "all I/O is injected");
}
