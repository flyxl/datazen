//! Ports of `packages/frontend/src/commands/` — the frontend's typed IPC
//! command wrappers. On the Rust side the IPC boundary collapses into
//! [`DesktopAdapter::execute_driver_command`] (and, for commands with a
//! connection session, the adapter's session methods): one unified entry,
//! typed wrappers above it, exactly the React layering.
//!
//! `adb.ts` (T047) is first; schema-diff (T377), the AI family (T094),
//! backup (T163), the connection family (T190), the context family
//! (T214), the dashboard family (T237), the database family (T258) and
//! the driver gateway (T279), the file family (T298), the history family (T314)
//! have landed; the remaining command
//! families land with their own tasks.

pub mod adb;
/// T130: the artifact export command (one call, the two-arm receipt).
pub mod artifact;
/// T163: the app-data backup/import family (merged-IPC shapers).
pub mod backup;
/// T190: the connection command family + the in-flight connect joiner.
pub mod connection;
/// T214: the AI context-directory family (get/list/read).
pub mod context;
/// T237: the dashboard domain-client family + the two legacy dialog faces.
pub mod dashboard;
/// T258: the database family (target-command shapers, the domain-client
/// rail, the discovery joiner, the legacy privileges face).
pub mod database;
/// T279: the generic driver-command gateway (list / execute / stream).
pub mod driver;
/// T298: the file family (dialog re-exports, the streaming save
/// session, the tables-export stream and its progress event).
pub mod file;
/// T314: the history family (the query-assets purge + the sync domain's
/// migration-run reads).
pub mod history;
/// T356: the query faces — the driver-gateway executes, the
/// query-assets writes (submission-token carrying), the favorites and
/// the transaction natives.
pub mod query;
/// T366: the first-run sample dataset face (the demo_sales seed).
pub mod sample;
/// T388: the settings commands (the kernel/half-port split, the
/// flattenShared transform and the merge order).
pub mod settings;
/// T424: the data-transfer Job command layer (the prepare/apply
/// split's four members + the reads).
pub mod transfer_jobs;
/// T432: the saved-tunnel command family (the CRUD pair, the G9
/// secret-free summary read, the usage read, the round-trip probe).
pub mod tunnel;
/// T440: the workspace-app command family (the list/read pair, the
/// driver command rail, the pick-then-install pair, the storage
/// triple, the file read, the audit entry).
pub mod wapps;
/// T415: the data-transfer legacy command family (the typed wire
/// contracts + the eleven invoke shapers).
pub mod transfer;
/// T406: the Data Sync commands (the dataSync domain rail, the two
/// legacy faces, the selection folding, the poll/apply chains and
/// the plan staging).
pub mod sync;



/// T344: the saved-views menu's command adapter (the three database
/// rail calls plus the two JSON file dialogs).
pub mod named_views;
/// T331: the language-pack family (list/read/inspect/install/remove
/// shapers + the changed-event literal).
pub mod language_packs;
// T377: the schema-diff commands shell (pure helpers, wire folding, the
// job-observer bus and the submit→poll→resolve machine).
pub mod schema_diff;
/// T397: the structure-editor command pair (capabilities + the plan's
/// target-command shapers).
pub mod structure;
// T094: the AI command family (ai_*/mcp_*/prompt_* legacy shapers, the
// workflow domain-client slice, the stream-event edges).
pub mod ai;
