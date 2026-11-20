//! Port of `packages/frontend/src/lib/session/targetCommand.ts`.
//!
//! The React module has two halves: pure target/scope derivation
//! (`metadataTarget`, `databaseDiscoveryTarget`, `readTargetSchema` envelope,
//! `metadataScopeKey`) and an effect-driven command runner
//! (`executeTargetCommand`) with phase hooks, a retrying admission gate and
//! typed result extraction. Both are kept here as pure immediate-mode logic:
//! [`TargetRun`] emits [`TargetCommandEffect`]s and [`TargetCommandInput`]
//! answers drive it. No promise, timeout, thread or wall clock — the 100ms
//! backoff is a re-emitted effect and the 10s bound is checked against a
//! host-supplied time.
//!
//! # Reuse / stand-ins
//!
//! `ExecutionTarget`, `ObjectTarget`, `NamespaceTarget`, `CommandCall`,
//! `SubmissionToken`, `ExecutionReceipt`, `ExecutionView`,
//! `PublishedResultChunk`, `EffectOutcome`, `ExecutionErrorCode` are reused
//! from T083/T122. The React store/backend shapes absent in Rust are local VMs
//! with removal stages:
//!
//! * `TargetConnectionProfile` (connectionId/enabled/configRevision) and
//!   `ActiveConnectionEntry` — replace with the backend-client DTOs when the
//!   metadata identity binding port lands.
//! * `MetadataIdentity` (organizationId/principalId) — T083's `PlatformIdentity`
//!   only carries the client instance id.
//! * `TargetApiError` — a typed stand-in for JS `ApiError`; drop when a shared
//!   error DTO exposes `code` and `message`.
//!
//! No view consumer yet: every item carries a per-item `#[allow(dead_code)]`
//! with the removal stage "drop when the target-command view wires this".

use serde_json::Value;

use crate::session::editor_session_controller::{
    CommandCall, ExecutionErrorCode, ExecutionReceipt, ExecutionState, ExecutionTarget,
    NamespaceTarget, ObjectTarget, SubmissionToken,
};
use crate::session::execution_projection::{
    EffectOutcome, ExecutionView, PublishedResultChunk,
};

/// React's admission retry bound: 10 seconds.
const ADMISSION_BOUND_MS: i64 = 10_000;

/// A profile row as the runner reads it (backend-client `ConnectionProfile`
/// in the Rust port does not carry these fields yet).
#[allow(dead_code)] // local VM; remove with the shared profile DTO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TargetConnectionProfile {
    pub(crate) connection_id: String,
    pub(crate) enabled: bool,
    pub(crate) config_revision: String,
}

/// One entry of React's active-connection store, by dbSessionId.
#[allow(dead_code)] // local VM; remove with the identity binding port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActiveConnectionEntry {
    pub(crate) db_session_id: String,
    pub(crate) connection_id: String,
    pub(crate) current_database: Option<String>,
}

/// Typed stand-in for JS `ApiError`.
#[allow(dead_code)] // stand-in; remove with a shared error DTO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TargetApiError {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[allow(dead_code)] // constructors; remove with the stand-in.
impl TargetApiError {
    pub(crate) fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// React's `getPlatformIdentity()` fields used by `metadataScopeKey`.
#[allow(dead_code)] // local VM; remove when the DTO gains these fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MetadataIdentity {
    pub(crate) organization_id: String,
    pub(crate) principal_id: String,
}

/// The failure messages React throws verbatim.
#[allow(dead_code)] // error mirror; remove with the target-command view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TargetCommandError {
    ProfileUnavailable,
    ProfileUnavailableForScope,
    MetadataOwnerUnavailable,
    Admission(TargetApiError),
    Execution { message: String },
    TypedResultUnavailable,
    InvalidSchemaResult,
    Dispose(TargetApiError),
}

#[allow(dead_code)] // error mirror; remove with the target-command view.
impl TargetCommandError {
    pub(crate) fn message(&self) -> &str {
        match self {
            Self::ProfileUnavailable => "Connection profile unavailable",
            Self::ProfileUnavailableForScope => "Metadata profile unavailable",
            Self::MetadataOwnerUnavailable => "Metadata owner unavailable",
            Self::Admission(error) | Self::Dispose(error) => &error.message,
            Self::Execution { message } => message,
            Self::TypedResultUnavailable => "Typed command result unavailable",
            Self::InvalidSchemaResult => "Invalid schema result",
        }
    }
}

/// How the successful terminal view is turned into a return value.
#[allow(dead_code)] // arg mirror; remove with the target-command view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResponseKind {
    CommandResult,
    Rows,
    Completion,
}

/// React `metadataTarget`: find the owner from an explicit connection id or
/// the active-connection store, then normalize the namespace/object.
#[allow(dead_code)] // helper; remove with the target-command view.
pub(crate) fn metadata_target(
    db_session_id: &str,
    database: Option<&str>,
    schema: Option<&str>,
    table: Option<&str>,
    connection_id: Option<&str>,
    active: &[ActiveConnectionEntry],
) -> Result<ExecutionTarget, TargetCommandError> {
    let entry = if connection_id.is_some() {
        None
    } else {
        active
            .iter()
            .find(|connection| connection.db_session_id == db_session_id)
    };
    let owner = connection_id.or_else(|| entry.map(|connection| connection.connection_id.as_str()));
    let owner = owner.ok_or(TargetCommandError::MetadataOwnerUnavailable)?;
    let resolved_database = database
        .or_else(|| entry.and_then(|connection| connection.current_database.as_deref()));
    Ok(ExecutionTarget {
        connection_id: owner.to_owned(),
        namespace: NamespaceTarget {
            database: resolved_database.map(str::to_owned),
            schema: schema.map(str::to_owned),
            catalog: None,
            path: Vec::new(),
        },
        object: table.map(|name| ObjectTarget {
            kind: "relation".to_owned(),
            name: name.to_owned(),
            signature: None,
        }),
    })
}

/// React `databaseDiscoveryTarget`: metadata target with a null namespace.
#[allow(dead_code)] // helper; remove with the target-command view.
pub(crate) fn database_discovery_target(
    db_session_id: &str,
    connection_id: Option<&str>,
    active: &[ActiveConnectionEntry],
) -> Result<ExecutionTarget, TargetCommandError> {
    let mut target = metadata_target(db_session_id, None, None, None, connection_id, active)?;
    target.namespace = NamespaceTarget::default();
    Ok(target)
}

/// The `get_table_schema` envelope used by `readTargetSchema`.
#[allow(dead_code)] // helper; remove with the target-command view.
pub(crate) fn schema_call(table: &str, database: &str, schema: Option<&str>) -> CommandCall {
    CommandCall {
        command: "get_table_schema".to_owned(),
        input: serde_json::json!({
            "table": table,
            "database": database,
            "schema": schema,
        })
        .to_string(),
    }
}

/// React `readTargetSchema`'s only validation of the returned payload.
#[allow(dead_code)] // helper; remove with the target-command view.
pub(crate) fn schema_from_result(data: Option<&Value>) -> Result<Value, TargetCommandError> {
    let value = data
        .and_then(|value| value.get("schema"))
        .filter(|schema| schema.get("columns").is_some_and(Value::is_array));
    value.cloned().ok_or(TargetCommandError::InvalidSchemaResult)
}

/// React `metadataScopeKey`: a stable JSON array of identity + profile facts.
#[allow(dead_code)] // helper; remove with the target-command view.
pub(crate) fn metadata_scope_key(
    backend_id: &str,
    identity: &MetadataIdentity,
    profile: &TargetConnectionProfile,
) -> Result<String, TargetCommandError> {
    if !profile.enabled {
        return Err(TargetCommandError::ProfileUnavailableForScope);
    }
    Ok(serde_json::json!([
        backend_id,
        identity.organization_id,
        identity.principal_id,
        profile.connection_id,
        profile.config_revision,
    ])
    .to_string())
}

/// The admission request, built from the profile and submission token.
#[allow(dead_code)] // request mirror; remove with the target-command view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TargetExecutionRequest {
    pub(crate) target: ExecutionTarget,
    pub(crate) expected_config_revision: String,
    pub(crate) call: CommandCall,
    pub(crate) idempotency_key: String,
}

/// Work the host performs to drive a target command.
#[allow(dead_code)] // emitted work; remove with the target-command view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TargetCommandEffect {
    ListConnections,
    IssueToken,
    ExecuteAtTarget { request: TargetExecutionRequest },
    Recover,
    Dispose,
}

/// Host answers fed back into [`TargetRun::step`].
#[allow(dead_code)] // result feed; remove with the target-command view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TargetCommandInput {
    Profiles(Vec<TargetConnectionProfile>),
    Token(SubmissionToken),
    /// `now_ms` is the host clock, used only for the 10s admission bound.
    Execute {
        result: Result<ExecutionReceipt, TargetApiError>,
        now_ms: i64,
    },
    Recovered {
        view: Option<ExecutionView>,
        chunks: Vec<PublishedResultChunk>,
    },
    DisposeFailed(TargetApiError),
}

#[allow(dead_code)] // result mirror; remove with the target-command view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TargetCommandOutcome {
    Value(Value),
    Completed,
    Failed(TargetCommandError),
}

#[allow(dead_code)] // step result; remove with the target-command view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TargetCommandStep {
    pub(crate) effects: Vec<TargetCommandEffect>,
    pub(crate) phases: Vec<&'static str>,
    pub(crate) outcome: Option<TargetCommandOutcome>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunPhase {
    Profiles,
    Token,
    Admission,
    Projection,
    Done,
}

/// The target-command flow as an explicit machine.
#[allow(dead_code)] // consumer: the target-command view; remove when it lands.
pub(crate) struct TargetRun {
    target: ExecutionTarget,
    call: CommandCall,
    response_kind: ResponseKind,
    phase: RunPhase,
    deadline_ms: Option<i64>,
    expected_revision: Option<String>,
    last_token: SubmissionToken,
}

#[allow(dead_code)] // consumer: the target-command view; remove when it lands.
impl TargetRun {
    pub(crate) fn new(
        target: ExecutionTarget,
        call: CommandCall,
        response_kind: ResponseKind,
    ) -> (Self, TargetCommandStep) {
        let run = Self {
            target,
            call,
            response_kind,
            phase: RunPhase::Profiles,
            deadline_ms: None,
            expected_revision: None,
            last_token: SubmissionToken {
                idempotency_key: String::new(),
                expires_at: 0,
            },
        };
        let step = TargetCommandStep {
            effects: vec![TargetCommandEffect::ListConnections],
            phases: vec!["profiles"],
            outcome: None,
        };
        (run, step)
    }

    pub(crate) fn phase(&self) -> &'static str {
        match self.phase {
            RunPhase::Profiles => "profiles",
            RunPhase::Token => "submission-token",
            RunPhase::Admission => "target-admission",
            RunPhase::Projection => "projection",
            RunPhase::Done => "done",
        }
    }

    pub(crate) fn step(&mut self, input: TargetCommandInput) -> TargetCommandStep {
        match (self.phase, input) {
            (RunPhase::Profiles, TargetCommandInput::Profiles(profiles)) => {
                let Some(profile) = profiles
                    .iter()
                    .find(|profile| profile.connection_id == self.target.connection_id)
                    .filter(|profile| profile.enabled)
                else {
                    return self.failed(TargetCommandError::ProfileUnavailable);
                };
                // Carry the accepted revision into the admission request.
                self.expected_revision = Some(profile.config_revision.clone());
                self.phase = RunPhase::Token;
                TargetCommandStep {
                    effects: vec![TargetCommandEffect::IssueToken],
                    phases: vec!["submission-token"],
                    outcome: None,
                }
            }
            (RunPhase::Token, TargetCommandInput::Token(token)) => {
                self.phase = RunPhase::Admission;
                self.deadline_ms = None;
                self.last_token = token.clone();
                TargetCommandStep {
                    effects: vec![TargetCommandEffect::ExecuteAtTarget {
                        request: self.admission_request(&token),
                    }],
                    phases: vec!["target-admission"],
                    outcome: None,
                }
            }
            (
                RunPhase::Admission,
                TargetCommandInput::Execute {
                    result: Ok(receipt),
                    ..
                },
            ) => {
                self.phase = RunPhase::Projection;
                TargetCommandStep {
                    effects: vec![TargetCommandEffect::Recover],
                    phases: Vec::new(),
                    outcome: None,
                }
            }
            (
                RunPhase::Admission,
                TargetCommandInput::Execute {
                    result: Err(error),
                    now_ms,
                },
            ) => {
                let deadline = self.deadline_ms.get_or_insert(now_ms + ADMISSION_BOUND_MS);
                if error.code == "ResourceBusy" && now_ms < *deadline {
                    // Retry the exact same request/token before the receipt.
                    let token = self.last_token.clone();
                    TargetCommandStep {
                        effects: vec![TargetCommandEffect::ExecuteAtTarget {
                            request: self.admission_request(&token),
                        }],
                        phases: Vec::new(),
                        outcome: None,
                    }
                } else {
                    self.failed(TargetCommandError::Admission(error))
                }
            }
            (
                RunPhase::Projection,
                TargetCommandInput::Recovered { view, chunks },
            ) => {
                let Some(view) = view else {
                    return TargetCommandStep {
                        effects: vec![TargetCommandEffect::Recover],
                        phases: Vec::new(),
                        outcome: None,
                    };
                };
                if view.state != ExecutionState::Succeeded {
                    let message = format!(
                        "{}; effect: {}",
                        error_code_str(view.error_code),
                        effect_outcome_str(view.effect_outcome)
                    );
                    return self.failed(TargetCommandError::Execution { message });
                }
                let outcome = match self.response_kind {
                    ResponseKind::Completion => TargetCommandOutcome::Completed,
                    ResponseKind::Rows => TargetCommandOutcome::Value(serde_json::Value::Array(
                        chunks
                            .iter()
                            .flat_map(|chunk| {
                                let rows = chunk
                                    .payload
                                    .get("output")
                                    .filter(|output| {
                                        output.get("kind").and_then(|kind| kind.as_str())
                                            == Some("rows")
                                    })
                                    .and_then(|output| output.get("rows"))
                                    .and_then(Value::as_array);
                                rows.cloned().unwrap_or_default()
                            })
                            .collect(),
                    )),
                    ResponseKind::CommandResult => {
                        let data = chunks.iter().find_map(|chunk| {
                            let output = chunk.payload.get("output")?;
                            if output.get("kind").and_then(|kind| kind.as_str())
                                != Some("commandResult")
                            {
                                return None;
                            }
                            output.get("data").cloned()
                        });
                        match data {
                            Some(data) => TargetCommandOutcome::Value(data),
                            None => return self.failed(TargetCommandError::TypedResultUnavailable),
                        }
                    }
                };
                self.phase = RunPhase::Done;
                TargetCommandStep {
                    effects: vec![TargetCommandEffect::Dispose],
                    phases: Vec::new(),
                    outcome: Some(outcome),
                }
            }
            (_, TargetCommandInput::DisposeFailed(error)) => {
                // A failed dispose surfaces its own phase instead of hiding
                // behind whichever phase produced the original outcome.
                self.phase = RunPhase::Done;
                TargetCommandStep {
                    effects: Vec::new(),
                    phases: vec!["dispose"],
                    outcome: Some(TargetCommandOutcome::Failed(TargetCommandError::Dispose(error))),
                }
            }
            _ => TargetCommandStep {
                effects: Vec::new(),
                phases: Vec::new(),
                outcome: None,
            },
        }
    }

    fn admission_request(&self, token: &SubmissionToken) -> TargetExecutionRequest {
        TargetExecutionRequest {
            target: self.target.clone(),
            expected_config_revision: self.expected_revision.clone().unwrap_or_default(),
            call: self.call.clone(),
            idempotency_key: token.idempotency_key.clone(),
        }
    }

    fn failed(&mut self, error: TargetCommandError) -> TargetCommandStep {
        self.phase = RunPhase::Done;
        TargetCommandStep {
            effects: Vec::new(),
            phases: Vec::new(),
            outcome: Some(TargetCommandOutcome::Failed(error)),
        }
    }
}

fn error_code_str(code: Option<ExecutionErrorCode>) -> &'static str {
    match code {
        Some(ExecutionErrorCode::SqlError) => "sqlError",
        Some(ExecutionErrorCode::ProtocolError) => "protocolError",
        Some(ExecutionErrorCode::Cancelled) => "cancelled",
        Some(ExecutionErrorCode::Timeout) => "timeout",
        Some(ExecutionErrorCode::ResourceLost) => "resourceLost",
        Some(ExecutionErrorCode::PipelineAborted) => "pipelineAborted",
        Some(ExecutionErrorCode::HostRejected) => "hostRejected",
        None => "undefined",
    }
}

fn effect_outcome_str(outcome: EffectOutcome) -> &'static str {
    match outcome {
        EffectOutcome::NotStarted => "notStarted",
        EffectOutcome::Completed => "completed",
        EffectOutcome::RolledBack => "rolledBack",
        EffectOutcome::PartiallyApplied => "partiallyApplied",
        EffectOutcome::Unknown => "unknown",
    }
}
