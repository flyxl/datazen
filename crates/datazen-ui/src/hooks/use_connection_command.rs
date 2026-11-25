//! Port of `packages/frontend/src/hooks/useConnectionCommand.ts` (T254)
//! — the connection's driver-command lookup hooks.
//!
//! `useConnectionCommand(dbSessionId, commandId)` resolves ONE command
//! definition (the schema dialogs reach for it); `useConnectionCommands
//! (dbSessionId)` resolves the whole list. Both share the same seam
//! (`driverCommands.getConnectionCommands`, the T279 family's) and the
//! same effect rules:
//!
//! ## What maps where
//!
//! - The `useEffect`'s guard: no session (or no command id, for the
//!   single hook) resolves to empty and never loads.
//! - `cancelled` is the effect's cleanup token: a session or command-id
//!   change retires the in-flight answer — [`ConnectionCommandLookup::
//!   begin`] stamps the epoch, and every answer method
//!   (`note_definitions` / `note_failure`) checks it
//!   ([`Self::is_current`]) before landing, exactly `if (!cancelled)`.
//! - The rejection arm resolves to empty (a failed lookup is "no
//!   commands", not an error surface); `finally` clears loading either
//!   way.
//! - `DriverCommandDefinition` (backend-client's vocabulary) is the
//!   receipt; the schemas ride opaque JSON.
//!
//! Known deltas: none behavioural.

use serde::{Deserialize, Serialize};

/// React `DriverCommandDefinition` — one driver command's declared
/// surface (backend-client's vocabulary, camelCase wire).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverCommandDefinition {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The input's JSON schema (opaque JSON).
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    pub permissions: Vec<String>,
    pub metadata: DriverCommandMetadata,
}

/// React `DriverCommandMetadata` (the display half: where the command
/// shows up).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverCommandMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
}

/// The lookup's state (both hooks' shared shape; the single hook's
/// `definition` is the list filtered to one id).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConnectionCommandLookup {
    /// The definitions the last settled answer delivered (empty on
    /// failure, no-session or nothing-found).
    pub definitions: Vec<DriverCommandDefinition>,
    /// The in-flight flag (true from `begin` until the answer lands).
    pub loading: bool,
    /// The epoch of the latest `begin` (the `cancelled` token's
    /// positive form).
    epoch: u64,
}

impl ConnectionCommandLookup {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `epoch` is still the newest request (the `!cancelled`
    /// check every answer performs).
    pub fn is_current(&self, epoch: u64) -> bool {
        epoch == self.epoch
    }

    /// The effect's start: the no-session guard first (empty, no load),
    /// then loading with a fresh epoch. The `command_id` guard is the
    /// single hook's (pass the id; `None` is the list hook, which loads
    /// without one).
    pub fn begin(
        &mut self,
        db_session_id: Option<&str>,
        command_id: Option<&str>,
    ) -> Option<u64> {
        if db_session_id.is_none_or(str::is_empty)
            || command_id.is_some_and(str::is_empty)
        {
            self.definitions.clear();
            self.loading = false;
            self.epoch += 1;
            return None;
        }
        self.epoch += 1;
        self.loading = true;
        Some(self.epoch)
    }

    /// The `.then` arm: the resolved list lands (the single hook's
    /// `found` filter applied by the caller or here — the single hook
    /// finds by id, the list takes everything).
    pub fn note_definitions(
        &mut self,
        epoch: u64,
        definitions: Vec<DriverCommandDefinition>,
    ) {
        if !self.is_current(epoch) {
            return;
        }
        self.definitions = definitions;
        self.loading = false;
    }

    /// The `.catch` + `.finally` arms on failure: empty, loading off.
    pub fn note_failure(&mut self, epoch: u64) {
        if !self.is_current(epoch) {
            return;
        }
        self.definitions.clear();
        self.loading = false;
    }

    /// The single hook's projection: `definitions.find(id)`.
    pub fn definition(&self, command_id: &str) -> Option<&DriverCommandDefinition> {
        self.definitions.iter().find(|definition| definition.id == command_id)
    }
}
