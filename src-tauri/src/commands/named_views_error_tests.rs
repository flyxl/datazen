//! The named-view error codes on the wire, and the D1 rule (Wave 7 · track B).
//!
//! The front end classifies a `grid.*` code by the **prefix** of the message
//! it receives. Two things have to hold, and neither is visible from the
//! shared layer alone:
//!
//! - the rejection must **reach** the IPC envelope as a `CommandError`, with
//!   the code at the **first** position of the message;
//! - the envelope must serialize to a **plain string** — that is the shape
//!   `CommandError::Serialize` produces, and the shape the desktop transport
//!   has to unwrap.
//!
//! # D1 — a host message says only what the host knows
//!
//! The host's redactor covers the shapes it documents (JSON pairs, query
//! strings, bearer tokens, URL userinfo) and **not** free text. So a host
//! message must never **quote** what the user typed: a view named
//! `password=hunter2` would otherwise travel verbatim into the IPC payload
//! and into the host's log lines.
//!
//! The rule is therefore: the message carries no user-supplied text, and the
//! name stays **structured** (reachable through `NamedViewError::name`).
//! This file is the permanent lock for that rule on the Rust side, at the
//! point where the message becomes the payload that actually leaves the host.
use super::error::CommandError;
use datazen_feature_core::services::named_views::{NamedView, NamedViewError, NamedViewScope};

/// The payload the desktop receives: a plain string, not an object.
fn ipc_payload(error: CommandError) -> String {
    serde_json::to_string(&error).expect("the error serializes")
}

/// The message as the front end sees it: the payload without the JSON quotes.
fn ipc_text(error: CommandError) -> String {
    let payload = ipc_payload(error);
    assert!(
        payload.starts_with('"'),
        "the payload is a string, not an object: {payload}"
    );
    payload.trim_matches('"').to_string()
}

fn scope() -> NamedViewScope {
    NamedViewScope {
        connection_id: "conn-1".into(),
        database: Some("app".into()),
        schema: None,
        table: "orders".into(),
    }
}

fn record(name: &str) -> NamedView {
    NamedView {
        id: String::new(),
        name: name.into(),
        scope: scope(),
        filters: Vec::new(),
        filter_logic: "and".into(),
        sorts: Vec::new(),
        visible_columns: None,
        column_order: None,
        is_default: false,
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

// ── The envelope ────────────────────────────────────────────────────────

#[test]
fn a_duplicate_name_reaches_the_ipc_as_a_plain_string_with_the_code_first() {
    let error = CommandError::from(NamedViewError::NameExists {
        name: "Users".into(),
    });
    let text = ipc_text(error);

    // The code is at the **first** position: a prefix that is not at the
    // start is not a prefix.
    assert!(
        text.starts_with("grid.view.nameExists: "),
        "the code is the prefix: {text}"
    );
    assert_eq!(
        text,
        "grid.view.nameExists: a saved view with this name already exists for this table"
    );
}

#[test]
fn an_invalid_record_reaches_the_ipc_with_the_code_first() {
    let error = CommandError::from(NamedViewError::Invalid(
        "the view name must not be empty".into(),
    ));
    let text = ipc_text(error);
    assert!(
        text.starts_with("grid.view.invalid: "),
        "the code is the prefix: {text}"
    );
}

#[test]
fn a_storage_failure_carries_no_grid_code() {
    // The code space is closed (45 codes): a disk that cannot be written is
    // not one of the grid rules, so the front end shows its raw text.
    let error = CommandError::from(NamedViewError::Storage("disk is on fire".into()));
    let text = ipc_text(error);
    assert!(
        !text.starts_with("grid."),
        "a storage failure is not a grid rule: {text}"
    );
    assert!(text.contains("disk is on fire"), "{text}");
}

// ── D1 · the message never quotes the user ──────────────────────────────

#[test]
fn the_ipc_payload_never_carries_the_user_supplied_name() {
    // The name is whatever the user typed. A name shaped like a credential
    // must not travel — neither the value nor the `key=` shape.
    let error = CommandError::from(NamedViewError::NameExists {
        name: "password=hunter2".into(),
    });
    let text = ipc_text(error);

    assert!(
        text.starts_with("grid.view.nameExists: "),
        "the code stays at the front: {text}"
    );
    assert!(
        !text.contains("hunter2"),
        "the payload must not carry the name: {text}"
    );
    assert!(
        !text.contains("password="),
        "the payload must not carry the name: {text}"
    );
    // And the name is not there in **any** shape: the message is a fixed
    // sentence, so there is no slot a name could slip into.
    assert_eq!(
        text,
        "grid.view.nameExists: a saved view with this name already exists for this table"
    );
}

#[test]
fn the_ipc_payload_never_carries_the_user_supplied_filter_logic() {
    // The second instance of the same hole: the validation reason used to
    // quote the offending value. It is a **host** message, so it says what
    // the host knows and never what the user typed.
    let mut offending = record("Users");
    offending.filter_logic = "password=hunter2".into();
    let reason = offending
        .validate()
        .expect_err("a filter logic must be and or or");

    let text = ipc_text(CommandError::from(NamedViewError::Invalid(reason)));
    assert!(
        text.starts_with("grid.view.invalid: "),
        "the code is the prefix: {text}"
    );
    assert!(
        !text.contains("hunter2"),
        "the payload must not carry the value: {text}"
    );
    assert!(
        !text.contains("password="),
        "the payload must not carry the value: {text}"
    );
    assert!(
        text.contains("the filter logic must be"),
        "the host still says what is wrong: {text}"
    );
}

#[test]
fn a_json_shaped_credential_in_a_name_never_reaches_the_payload() {
    // The redactor covers this shape, but the message no longer carries the
    // name at all — so the name is safe whatever shape it has.
    let error = CommandError::from(NamedViewError::NameExists {
        name: r#"a "password": "hunter2" view"#.into(),
    });
    let text = ipc_text(error);
    assert!(
        !text.contains("hunter2"),
        "the credential did not travel: {text}"
    );
    assert!(
        text.starts_with("grid.view.nameExists: "),
        "the code is still the prefix after the redaction: {text}"
    );
}

#[test]
fn the_code_survives_the_redaction_the_ipc_payload_goes_through() {
    // The payload is scrubbed before it leaves the host. Whatever the
    // scrubber does to the text, it must not cost the **prefix** its
    // position: the front end classifies on the first characters.
    let error = CommandError::from(NamedViewError::NameExists {
        name: "password=hunter2 view".into(),
    });
    let text = ipc_text(error);
    assert!(
        text.starts_with("grid.view.nameExists: "),
        "the code is still the prefix after the redaction: {text}"
    );
}

// ── The structured data behind the message ──────────────────────────────

#[test]
fn the_shared_error_reports_its_own_code() {
    assert_eq!(
        NamedViewError::NameExists { name: "x".into() }.code(),
        Some("grid.view.nameExists")
    );
    assert_eq!(
        NamedViewError::Invalid("x".into()).code(),
        Some("grid.view.invalid")
    );
    assert_eq!(NamedViewError::Storage("x".into()).code(), None);
}

#[test]
fn the_name_stays_reachable_as_structured_data() {
    // The message says only what the host knows; the caller that wants to
    // say *which* name was taken still gets it — as data, not as prose.
    let error = NamedViewError::NameExists {
        name: "password=hunter2".into(),
    };
    assert_eq!(error.name(), Some("password=hunter2"));
    assert_eq!(NamedViewError::Invalid("x".into()).name(), None);
    assert_eq!(NamedViewError::Storage("x".into()).name(), None);
}
