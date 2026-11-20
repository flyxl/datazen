//! The backend-client query-stream re-exports the aggregate still owed —
//! ported from React `packages/frontend/src/types/index.ts` (tasklist T211).
//!
//! React's `types/index.ts` re-exports eleven names from
//! `@datazen/backend-client`; one (`Value`) is already owned by this
//! crate's `connection_types` (T128's alias of `serde_json::Value`, the
//! untagged JSON image of the cell union) and the aggregate re-exports it
//! from there. The command-catalog half (`CommandCategory`,
//! `CommandAccessLevel`, `DriverSaveDialogSpec`, `DriverCommandMetadata`,
//! `DriverCommandDefinition`) is [`crate::types::driver_commands`], the
//! settings half is [`crate::types::client_settings`] and the workflow half
//! is [`crate::types::workflow`]. This module is the **query-result half**:
//! `ColumnInfo`, `QueryResult`, `StatementResult`, `MultiQueryResult`,
//! `QueryStreamEvent` — the family T128's dependency inventory explicitly
//! deferred ("not referenced by any own interface below; the backend-client
//! query domain ports land as their own module and the facade re-exports
//! them then"). T211 is that landing: the aggregate needs the names, so the
//! port lives here.
//!
//! Source of truth: `packages/backend-client/src/types/driverCommands.ts`
//! — the query DTOs and stream events. The interlocked legs that file
//! references from `types/session.ts` / `types/jobs.ts`
//! (`StatementResultSource`, `RuntimeResultBinding`, `ResultProvenance`,
//! `ResultCompleteness` and their nested shapes) are ported here too,
//! because `StatementResult` and `QueryStreamEvent` embed them; everything
//! else in those domain files (the session/job *envelopes* — `SessionView`,
//! `JobDetails`, …) stays with their own future ports.
//!
//! `QueryResultArtifacts` is declared by driverCommands.ts but **not**
//! re-exported by React's `types/index.ts`, so it is not on the aggregate
//! surface — it rides this module's path only, exactly like its TS twin.
//!
//! The scalar brands (`Id`, `Counter`, `Timestamp`) are the
//! `types/identity.ts` contract: nominal in TS purely to stop revision
//! counters being swapped for session ids. Rust mirrors them as an alias /
//! a dual-decoding newtype / an alias (see each item's docs).
//!
//! Pure contracts — no IO, no state — so the module compiles for wasm32
//! exactly like its `chart_types` sibling.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::connection_types::Value;

// ── The identity scalars (`types/identity.ts`) ──────────────────────────────

/// React `Id` — an opaque, backend-issued identifier; a string on the wire.
///
/// The TS brand is nominal only (it exists so a counter cannot be swapped
/// for a session id at compile time); the Rust mirror is the plain string,
/// the same ruling `connection_types::DatabaseType` made for its union.
pub type Id = String;

/// React `Timestamp` — wall-clock epoch milliseconds.
///
/// The kernel may emit an ISO-8601 string for readability, but adapters
/// normalize on the way in, "so frontend code only ever sees this" — the
/// number. The mirror is that normalized view (`u64`, the family's ms
/// width); a raw ISO payload is the adapter's to normalize before decode.
pub type Timestamp = u64;

/// React `Counter` — a monotonic revision/sequence number, **a decimal
/// string on the wire** (`platform-api` serializes its `u64` with
/// `collect_str` so a count above `2^53` survives the trip; `toCounter`
/// accepts a bare JSON number too, for the hand-written payloads and mock
/// drivers that never had a `u64` to encode).
///
/// The newtype keeps both wire encodings readable and re-emits the kernel's
/// decimal string — the CM-01-safe spelling — while `.0` hands Rust callers
/// the `u64` the TS brand hides behind a normalized number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Counter(pub u64);

impl Serialize for Counter {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // The kernel's own emission: the decimal string (collect_str).
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Counter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // `toCounter`'s dual acceptance: number or non-negative decimal
        // string; anything else is a contract violation, not a zero.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Number(u64),
            String(String),
        }
        match serde::Deserialize::deserialize(deserializer)? {
            Wire::Number(n) => Ok(Self(n)),
            Wire::String(s) => s
                .parse::<u64>()
                .map_err(|_| serde::de::Error::custom(format!("invalid counter: {s:?}")))
                .map(Self),
        }
    }
}

// ── The session-context legs (`types/session.ts`) ─────────────────────────

/// React `NamespaceTarget` — database / catalog / schema resolution,
/// normalized. All three legs are required-but-nullable (`string | null`):
/// `None` serializes as an explicit `null` and the key stays present.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceTarget {
    pub database: Option<String>,
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub path: Vec<String>,
}

/// React `ObjectTarget` — a single addressable object inside a namespace
/// (table, view, …). `signature` is required-but-nullable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectTarget {
    pub kind: String,
    pub name: String,
    pub signature: Option<String>,
}

/// React `ExecutionTarget` — what an execution runs *against*. What is
/// absent is `backendId`: the backend only selects which facade you hold,
/// it is never part of an execution target. `object` is
/// required-but-nullable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionTarget {
    pub connection_id: Id,
    pub namespace: NamespaceTarget,
    pub object: Option<ObjectTarget>,
}

/// React `SessionHandle` — a session paired with the runtime epoch it was
/// bound to; after an epoch change the old handle is dead (why it is a pair
/// and not a bare id). Keeps `dbSessionId` and `connectionId` semantically
/// distinct the way the TS file's own header demands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHandle {
    pub db_session_id: Id,
    pub runtime_epoch: Id,
}

/// React `TransactionState` — `'none' | 'active' | 'aborted' |
/// 'unknown' | 'unsupported'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionState {
    None,
    Active,
    Aborted,
    Unknown,
    Unsupported,
}

impl TransactionState {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Active => "active",
            Self::Aborted => "aborted",
            Self::Unknown => "unknown",
            Self::Unsupported => "unsupported",
        }
    }
}

/// React `SessionContext.confidence`'s inline union `'confirmed' |
/// 'partial' | 'unknown'` — how certainly the driver observed the context.
/// React declares it inline with no exported name; the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionContextConfidence {
    Confirmed,
    Partial,
    Unknown,
}

impl SessionContextConfidence {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Partial => "partial",
            Self::Unknown => "unknown",
        }
    }
}

/// React `SessionContext` — the context an operation actually ran in.
///
/// `confidence` exists because a driver that cannot observe the
/// batch-accurate context reports `partial`/`unknown` rather than guessing;
/// the UI reads it to decide whether a result may be treated as writable.
/// `searchPath`, `effectiveIdentity` and `autocommit` are
/// required-but-nullable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext {
    pub namespace: NamespaceTarget,
    pub search_path: Option<Vec<String>>,
    pub effective_identity: Option<String>,
    pub transaction_state: TransactionState,
    pub autocommit: Option<bool>,
    pub confidence: SessionContextConfidence,
}

/// React `CapabilitySnapshot` — the driver/protocol identity a result's
/// provenance records. `confirmed` is the opaque capability bag
/// (`Record<string, unknown>` → `BTreeMap<String, Value>`, the `ai_types`
/// name-keyed rule).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySnapshot {
    pub driver_id: String,
    pub driver_version: String,
    pub protocol_version: u32,
    pub capability_revision: Counter,
    pub confirmed: BTreeMap<String, Value>,
}

/// React `ResultProvenance` — result provenance for the whole call,
/// before/after, not per statement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultProvenance {
    pub organization_id: Id,
    pub principal_id: Id,
    pub connection_id: Id,
    pub config_revision: Counter,
    pub context_before: SessionContext,
    pub context_after: SessionContext,
    pub requested_target: ExecutionTarget,
    pub capability_snapshot: CapabilitySnapshot,
    pub executed_at: Timestamp,
}

/// React `RuntimeResultBinding` — where a live result is still bound.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeResultBinding {
    pub handle: SessionHandle,
    pub resource_binding_id: Id,
    pub execution_id: Id,
}

/// React `StatementResultSource.writableMapping`'s inline union
/// `'verified' | 'readOnly'` — the confirmed strength of a writable
/// mapping. `ReadOnly` is an honest downgrade, not a failure. React
/// declares it inline with no exported name; the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WritableMapping {
    Verified,
    ReadOnly,
}

impl WritableMapping {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::ReadOnly => "readOnly",
        }
    }
}

/// React `StatementResultSource` — which execution and statement a result
/// came from. `relation` is required-but-nullable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementResultSource {
    pub execution_id: Id,
    pub statement_index: u32,
    pub context: SessionContext,
    pub relation: Option<ExecutionTarget>,
    pub writable_mapping: WritableMapping,
}

// ── The jobs leg (`types/jobs.ts`) ────────────────────────────────────────

/// React `ResultCompleteness` — `'pending' | 'complete' | 'truncated'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultCompleteness {
    Pending,
    Complete,
    Truncated,
}

impl ResultCompleteness {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Complete => "complete",
            Self::Truncated => "truncated",
        }
    }
}

// ── The query DTOs and stream events (`types/driverCommands.ts`) ───────────

/// React `ColumnInfo` — one column of a result's header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

/// React `QueryResult` — one statement's read result: the header, the rows,
/// the wall time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<ColumnInfo>,
    /// Each cell a [`Value`] or `null`.
    pub rows: Vec<Vec<Option<Value>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows_affected: Option<u64>,
    pub execution_time_ms: u64,
}

/// React `StatementResult` — one statement's outcome with its provenance
/// legs.
///
/// The three legs (`source`, `runtimeBinding`, `provenance`) are
/// `?: <T> | null` — null on the wire when set to `None`, absent also
/// accepted — while `rowsAffected`, `truncated` and `pinned` are plain
/// `?:` (absent stays absent).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementResult {
    pub source: Option<StatementResultSource>,
    pub runtime_binding: Option<RuntimeResultBinding>,
    pub provenance: Option<ResultProvenance>,
    pub sql: String,
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<Option<Value>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows_affected: Option<u64>,
    pub execution_time_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
}

/// React `MultiQueryResult` — a script's statements, in order, plus the
/// whole-run wall time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiQueryResult {
    pub results: Vec<StatementResult>,
    pub total_time_ms: u64,
}

/// React `QueryResultArtifacts` — the artifact set an execution published.
///
/// Declared by `driverCommands.ts` but **not** re-exported by React's
/// `types/index.ts`, so it is not on the aggregate surface — it rides this
/// module path only, mirroring its TS reachability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResultArtifacts {
    pub execution_id: Id,
    pub artifact_ids: Vec<String>,
    pub result_completeness: ResultCompleteness,
}

/// React `QueryStreamEvent` — the streaming channel's seven events, tagged
/// by `type` exactly the way the discriminated union reads on the wire.
///
/// The variant-level `rename_all` is what camelCases each variant's fields
/// (the enum-level `rename_all` renames variants only).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum QueryStreamEvent {
    #[serde(rename_all = "camelCase")]
    ExecutionStarted {
        execution_id: Id,
    },
    #[serde(rename_all = "camelCase")]
    PublishedArtifacts {
        artifacts: QueryResultArtifacts,
    },
    #[serde(rename_all = "camelCase")]
    StatementStart {
        index: u32,
        sql: String,
        columns: Vec<ColumnInfo>,
    },
    #[serde(rename_all = "camelCase")]
    Rows {
        index: u32,
        /// Each cell a [`Value`] or `null`.
        rows: Vec<Vec<Option<Value>>>,
    },
    #[serde(rename_all = "camelCase")]
    StatementSnapshot {
        index: u32,
        result: StatementResult,
    },
    #[serde(rename_all = "camelCase")]
    StatementEnd {
        index: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rows_affected: Option<u64>,
        execution_time_ms: u64,
        truncated: bool,
    },
    #[serde(rename_all = "camelCase")]
    Done {
        total_time_ms: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    #[test]
    fn counter_reads_both_wire_encodings_and_reemits_the_kernel_string() {
        // A bare JSON number (the hand-written/mock payloads)…
        let from_number: Counter = decode(serde_json::json!(7));
        assert_eq!(from_number, Counter(7));
        // …and the decimal string `platform-api` actually emits.
        let from_string: Counter = decode(serde_json::json!("18446744073709551615"));
        assert_eq!(from_string, Counter(u64::MAX));
        // The re-emission is the CM-01-safe decimal string, never a number.
        assert_eq!(encode(&from_string), serde_json::json!("18446744073709551615"));
        // A negative or non-decimal string is a contract violation, not a zero.
        assert!(serde_json::from_value::<Counter>(serde_json::json!("-3")).is_err());
        assert!(serde_json::from_value::<Counter>(serde_json::json!("12x")).is_err());
    }

    #[test]
    fn transaction_state_and_confidence_keep_their_literals() {
        let states = [
            (TransactionState::None, "none"),
            (TransactionState::Active, "active"),
            (TransactionState::Aborted, "aborted"),
            (TransactionState::Unknown, "unknown"),
            (TransactionState::Unsupported, "unsupported"),
        ];
        for (value, literal) in states {
            assert_eq!(value.as_str(), literal);
            let parsed: TransactionState = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
        }
        let confidences = [
            (SessionContextConfidence::Confirmed, "confirmed"),
            (SessionContextConfidence::Partial, "partial"),
            (SessionContextConfidence::Unknown, "unknown"),
        ];
        for (value, literal) in confidences {
            assert_eq!(value.as_str(), literal);
            let parsed: SessionContextConfidence = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
        }
        let mappings = [
            (WritableMapping::Verified, "verified"),
            (WritableMapping::ReadOnly, "readOnly"),
        ];
        for (value, literal) in mappings {
            assert_eq!(value.as_str(), literal);
            let parsed: WritableMapping = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
        }
        let completeness = [
            (ResultCompleteness::Pending, "pending"),
            (ResultCompleteness::Complete, "complete"),
            (ResultCompleteness::Truncated, "truncated"),
        ];
        for (value, literal) in completeness {
            assert_eq!(value.as_str(), literal);
            let parsed: ResultCompleteness = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
        }
    }

    #[test]
    fn a_statement_result_round_trips_its_provenance_legs() {
        let context = SessionContext {
            namespace: NamespaceTarget {
                database: Some("app".into()),
                catalog: None,
                schema: Some("public".into()),
                path: vec!["public".into()],
            },
            search_path: None,
            effective_identity: None,
            transaction_state: TransactionState::None,
            autocommit: None,
            confidence: SessionContextConfidence::Confirmed,
        };
        let statement = StatementResult {
            source: Some(StatementResultSource {
                execution_id: "exec-1".into(),
                statement_index: 0,
                context: context.clone(),
                relation: None,
                writable_mapping: WritableMapping::Verified,
            }),
            runtime_binding: None,
            provenance: Some(ResultProvenance {
                organization_id: "org".into(),
                principal_id: "user".into(),
                connection_id: "c1".into(),
                config_revision: Counter(4),
                context_before: context.clone(),
                context_after: context,
                requested_target: ExecutionTarget {
                    connection_id: "c1".into(),
                    namespace: NamespaceTarget {
                        database: Some("app".into()),
                        catalog: None,
                        schema: None,
                        path: Vec::new(),
                    },
                    object: Some(ObjectTarget {
                        kind: "table".into(),
                        name: "users".into(),
                        signature: None,
                    }),
                },
                capability_snapshot: CapabilitySnapshot {
                    driver_id: "postgres".into(),
                    driver_version: "16.2".into(),
                    protocol_version: 1,
                    capability_revision: Counter(0),
                    confirmed: BTreeMap::new(),
                },
                executed_at: 1_732_600_000_000,
            }),
            sql: "SELECT 1".into(),
            columns: vec![ColumnInfo {
                name: "?column?".into(),
                data_type: "integer".into(),
                nullable: false,
            }],
            rows: vec![vec![Some(serde_json::json!(1))]],
            rows_affected: None,
            execution_time_ms: 12,
            truncated: None,
            pinned: None,
        };
        let wire = encode(&statement);
        // `?: T | null` legs keep explicit nulls; plain `?:` legs stay absent.
        assert_eq!(wire["runtimeBinding"], serde_json::Value::Null);
        assert!(wire.get("rowsAffected").is_none());
        assert_eq!(wire["source"]["writableMapping"], "verified");
        // The counter leg re-emits as the decimal string the kernel emits.
        assert_eq!(wire["provenance"]["configRevision"], "4");
        assert_eq!(wire["provenance"]["executedAt"], 1_732_600_000_000i64);
        let back: StatementResult = decode(wire);
        assert_eq!(back, statement);
    }

    #[test]
    fn multi_query_result_round_trips() {
        let multi = MultiQueryResult {
            results: vec![
                StatementResult {
                    source: None,
                    runtime_binding: None,
                    provenance: None,
                    sql: "SELECT 1".into(),
                    columns: Vec::new(),
                    rows: Vec::new(),
                    rows_affected: None,
                    execution_time_ms: 3,
                    truncated: None,
                    pinned: None,
                },
                StatementResult {
                    source: None,
                    runtime_binding: None,
                    provenance: None,
                    sql: "UPDATE t SET x = 1".into(),
                    columns: Vec::new(),
                    rows: Vec::new(),
                    rows_affected: Some(5),
                    execution_time_ms: 9,
                    truncated: None,
                    pinned: None,
                },
            ],
            total_time_ms: 12,
        };
        let wire = encode(&multi);
        assert_eq!(wire["results"][1]["rowsAffected"], 5);
        let back: MultiQueryResult = decode(wire);
        assert_eq!(back, multi);
    }

    #[test]
    fn the_stream_events_tag_their_variants_on_the_wire() {
        let events = vec![
            encode(&QueryStreamEvent::ExecutionStarted { execution_id: "e1".into() }),
            encode(&QueryStreamEvent::PublishedArtifacts {
                artifacts: QueryResultArtifacts {
                    execution_id: "e1".into(),
                    artifact_ids: vec!["a1".into()],
                    result_completeness: ResultCompleteness::Complete,
                },
            }),
            encode(&QueryStreamEvent::StatementStart {
                index: 0,
                sql: "SELECT 1".into(),
                columns: Vec::new(),
            }),
            encode(&QueryStreamEvent::Rows {
                index: 0,
                rows: vec![vec![Some(serde_json::json!("v")), None]],
            }),
            encode(&QueryStreamEvent::StatementSnapshot {
                index: 0,
                result: StatementResult {
                    source: None,
                    runtime_binding: None,
                    provenance: None,
                    sql: "SELECT 1".into(),
                    columns: Vec::new(),
                    rows: Vec::new(),
                    rows_affected: None,
                    execution_time_ms: 1,
                    truncated: None,
                    pinned: None,
                },
            }),
            encode(&QueryStreamEvent::StatementEnd {
                index: 0,
                rows_affected: None,
                execution_time_ms: 1,
                truncated: false,
            }),
            encode(&QueryStreamEvent::Done { total_time_ms: 2 }),
        ];
        assert_eq!(events[0]["type"], "executionStarted");
        assert_eq!(events[1]["type"], "publishedArtifacts");
        assert_eq!(events[1]["artifacts"]["resultCompleteness"], "complete");
        assert_eq!(events[2]["type"], "statementStart");
        assert_eq!(events[3]["type"], "rows");
        assert_eq!(events[4]["type"], "statementSnapshot");
        assert_eq!(events[5]["type"], "statementEnd");
        // `rowsAffected?: number` inside the variant stays absent.
        assert!(events[5].get("rowsAffected").is_none());
        assert_eq!(events[6]["type"], "done");
        // Every event decodes back through the tagged union.
        for wire in events {
            let _: QueryStreamEvent = decode(wire);
        }
    }
}
