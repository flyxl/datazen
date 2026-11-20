//! T254 audit: the useConnectionCommand(s) lookup port, verified
//! against the React source re-read line by line (the hooks' only
//! indirect React coverage is CreateSchemaDialog.test.tsx's schema
//! command journey — the effect rules below are re-derived from the
//! hook's own body).
//!
//! Tester-written: every expectation below is re-derived from the React
//! file.

use crate::hooks::use_connection_command::{
    ConnectionCommandLookup, DriverCommandDefinition,
};

fn definition(id: &str, name: &str) -> DriverCommandDefinition {
    DriverCommandDefinition {
        id: id.to_owned(),
        name: name.to_owned(),
        description: None,
        input_schema: serde_json::json!({"type": "object"}),
        output_schema: None,
        permissions: Vec::new(),
        metadata: Default::default(),
    }
}

/// The no-session guard: empty, never loading, and a retired epoch so
/// a stale answer cannot land.
#[test]
fn no_session_resolves_empty_without_loading() {
    let mut lookup = ConnectionCommandLookup::new();
    assert!(lookup.begin(None, Some("create-schema")).is_none());
    assert!(lookup.definitions.is_empty());
    assert!(!lookup.loading);

    // An empty session string is the same guard.
    assert!(lookup.begin(Some(""), Some("create-schema")).is_none());
    assert!(!lookup.loading);
}

/// The single hook's command-id guard: an EMPTY id resolves empty
/// without loading; the LIST hook (no id at all) loads fine — the two
/// hooks differ exactly there.
#[test]
fn an_empty_command_id_resolves_empty_but_the_list_hook_loads() {
    let mut lookup = ConnectionCommandLookup::new();
    // The single hook with an empty command id: no load.
    assert!(lookup.begin(Some("sess-1"), Some("")).is_none());
    assert!(!lookup.loading);
    // The list hook (command id absent): loads.
    let epoch = lookup.begin(Some("sess-1"), None).expect("the list hook loads without an id");
    assert!(lookup.loading);
    assert!(lookup.is_current(epoch));
}

/// The happy path: begin loads, the answer lands, loading clears.
#[test]
fn begin_loads_then_the_answer_lands() {
    let mut lookup = ConnectionCommandLookup::new();
    let epoch = lookup.begin(Some("sess-1"), None).expect("the list hook loads");
    assert!(lookup.loading);
    lookup.note_definitions(
        epoch,
        vec![definition("create-schema", "Create schema"), definition("drop", "Drop")],
    );
    assert!(!lookup.loading);
    assert_eq!(lookup.definitions.len(), 2);
    // The single hook's projection finds by id.
    assert_eq!(
        lookup.definition("create-schema").map(|d| d.name.as_str()),
        Some("Create schema")
    );
    assert!(lookup.definition("missing").is_none());
}

/// The failure arm: empty definitions, loading off — a failed lookup is
/// "no commands", not an error surface.
#[test]
fn a_failure_resolves_empty() {
    let mut lookup = ConnectionCommandLookup::new();
    let epoch = lookup.begin(Some("sess-1"), Some("create-schema")).expect("it loads");
    lookup.note_failure(epoch);
    assert!(!lookup.loading);
    assert!(lookup.definitions.is_empty());
    assert!(lookup.definition("create-schema").is_none());
}

/// The cancelled token: a session or command-id change retires the
/// in-flight answer — a late answer lands nowhere (neither the list nor
/// the loading flag).
#[test]
fn a_superseded_answer_lands_nowhere() {
    let mut lookup = ConnectionCommandLookup::new();
    let first = lookup.begin(Some("sess-1"), None).expect("loads");
    // The session changes before the answer arrives.
    let second = lookup.begin(Some("sess-2"), None).expect("loads again");
    assert!(lookup.loading);

    // The first session's answer arrives late: dropped whole.
    lookup.note_definitions(first, vec![definition("stale", "Stale")]);
    assert!(lookup.loading, "loading never cleared by a stale answer");
    assert!(lookup.definitions.is_empty(), "a stale list lands nowhere");

    // A stale failure is dropped too.
    lookup.note_failure(first);
    assert!(lookup.loading);

    // The current answer lands.
    lookup.note_definitions(second, vec![definition("fresh", "Fresh")]);
    assert!(!lookup.loading);
    assert_eq!(lookup.definitions.len(), 1);
    assert_eq!(lookup.definitions[0].id, "fresh");
}

/// A new begin after a settled answer reloads (the effect's dep change
/// re-runs it).
#[test]
fn a_dep_change_reloads() {
    let mut lookup = ConnectionCommandLookup::new();
    let first = lookup.begin(Some("sess-1"), Some("cmd-a")).expect("loads");
    lookup.note_definitions(first, vec![definition("cmd-a", "A")]);
    assert!(!lookup.loading);

    // The command id changes: a fresh load.
    let second = lookup.begin(Some("sess-1"), Some("cmd-b")).expect("reloads");
    assert!(lookup.loading);
    assert_ne!(first, second, "the epoch advanced");
    lookup.note_definitions(second, vec![definition("cmd-b", "B")]);
    assert_eq!(
        lookup.definition("cmd-b").map(|d| d.name.as_str()),
        Some("B")
    );
}

/// The wire shape decodes (the backend-client vocabulary, camelCase).
#[test]
fn the_definition_decodes_the_wire() {
    let decoded: DriverCommandDefinition = serde_json::from_value(serde_json::json!({
        "id": "create-schema",
        "name": "Create schema",
        "description": "Creates a schema",
        "inputSchema": {"type": "object"},
        "permissions": ["ddl"],
        "metadata": {"icon": "plus", "category": "ddl"}
    }))
    .expect("decodes");
    assert_eq!(decoded.id, "create-schema");
    assert_eq!(decoded.description.as_deref(), Some("Creates a schema"));
    assert_eq!(decoded.permissions, vec!["ddl".to_owned()]);
    assert_eq!(decoded.metadata.icon.as_deref(), Some("plus"));
    assert_eq!(decoded.metadata.category.as_deref(), Some("ddl"));
    // The optionals may be absent.
    let bare: DriverCommandDefinition = serde_json::from_value(serde_json::json!({
        "id": "x", "name": "X", "inputSchema": {}, "permissions": [], "metadata": {}
    }))
    .expect("bare decodes");
    assert!(bare.description.is_none());
    assert!(bare.output_schema.is_none());
}
