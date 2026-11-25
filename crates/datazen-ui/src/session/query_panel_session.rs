//! Port of `packages/frontend/src/lib/session/QueryPanelSession.ts`.
//!
//! Two contracts, both pure logic:
//!
//! * `QueryPanelSessionRegistry` — one lazily created `EditorSessionController`
//!   per pane key. Duplicating a tab creates a *fresh* controller (with the
//!   caller's fresh owner id) and never copies the source runtime handle.
//! * `DatabaseSwitch` — the two-phase database switch from
//!   `switchDatabaseSession`: refresh, transactional confirmation with a
//!   deferred-commit rollback, poll to a terminal execution, setContext with
//!   the latest revision, and the revision-conflict fallback.
//!
//! # Immediate-mode adaptation (no fake async)
//!
//! React awaits injected client calls. Rune emits [`SwitchEffect`]s and the
//! host feeds answers back through [`DatabaseSwitch::step`]; the controller's
//! own [`SessionEffect`]s are produced by the reused T083 controller. No
//! promise, timeout loop, thread or blocking wait exists here — the 50ms poll
//! is a re-emitted `PollExecution` effect, not a `setTimeout`.
//!
//! # Reuse / local stand-ins
//!
//! Every DTO and the controller come from T083 (`super::editor_session_controller`)
//! rather than being duplicated. Two small stand-ins remain, each with a
//! removal stage:
//!
//! * `panel_id_of_pane_key` mirrors `stores/paneKeys.ts` (`::` split) because
//!   that file has no Rune port yet. Remove when it lands and call it there.
//! * `SwitchExecutionView` carries the `state` that T083's `ExecutionView`
//!   omits (it only has `errorCode`), since the switch polls `execution.state`.
//!   Remove when that DTO gains a `state` field.
//! * `ApiErrorCode` has no `ContextConflict` variant; React's string code is
//!   represented as `ApiErrorCode::Other("ContextConflict")`. Remove when the
//!   enum gains the variant.
//!
//! No consumer yet: every item carries a per-item `#[allow(dead_code)]` with
//! the removal stage "drop when the query-panel view wires the registry/switch".

use std::collections::HashMap;

use super::editor_session_controller::{
    ApiErrorCode, CloseMode, EditorSessionController, ExecutionReceipt, ExecutionState,
    ExecutionTarget, NamespaceTarget, OwnerRef, SessionError, SessionEffect, SessionView,
    TransactionState,
};

/// Separator between a panel id and a secondary pane id (`stores/paneKeys.ts`).
const PANE_SEPARATOR: &str = "::";

/// React `panelIdOfPaneKey`: a bare key names the panel's default pane; a
/// `panel::pane` key names a secondary pane of the same panel.
// local stand-in for stores/paneKeys.ts; remove when that file is ported.
#[allow(dead_code)]
pub fn panel_id_of_pane_key(key: &str) -> &str {
    match key.find(PANE_SEPARATOR) {
        Some(at) => &key[..at],
        None => key,
    }
}

/// React `isPaneKeyOfPanel`.
// local stand-in for stores/paneKeys.ts; remove when that file is ported.
#[allow(dead_code)]
pub fn is_pane_key_of_panel(key: &str, panel_id: &str) -> bool {
    panel_id_of_pane_key(key) == panel_id
}

/// The close work emitted by `QueryPanelSessionRegistry::begin_remove`.
#[allow(dead_code)] // host-side teardown; remove with the query-panel view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoveStep {
    /// Identity of the controller being closed. `finish_remove` uses it so a
    /// concurrent replacement can never be removed by a late close.
    pub controller_id: u64,
    pub effects: Vec<SessionEffect>,
}

/// Per-editor lazy session registry, keyed by pane key.
#[allow(dead_code)] // consumer: the query-panel view; remove when it lands.
pub struct QueryPanelSessionRegistry {
    controllers: HashMap<String, EditorSessionController>,
    ids: HashMap<String, u64>,
    next_id: u64,
}

#[allow(dead_code)] // consumer: the query-panel view; remove when it lands.
impl QueryPanelSessionRegistry {
    pub fn new() -> Self {
        Self {
            controllers: HashMap::new(),
            ids: HashMap::new(),
            next_id: 1,
        }
    }

    /// Returns the existing controller for `key`, or lazily creates one with a
    /// fresh owner id — exactly React's `get`.
    pub fn get(
        &mut self,
        key: &str,
        target: &ExecutionTarget,
        owner: &OwnerRef,
    ) -> &mut EditorSessionController {
        let id = *self.ids.entry(key.to_owned()).or_insert_with(|| {
            let id = self.next_id;
            self.next_id += 1;
            id
        });
        self.controllers
            .entry(key.to_owned())
            .or_insert_with(|| EditorSessionController::new(id, target.clone(), owner.clone()))
    }

    /// React `peek`: no creation, no ownership change.
    pub fn peek(&self, key: &str) -> Option<&EditorSessionController> {
        self.controllers.get(key)
    }

    /// Mutable peek for driving the controller at a known pane key.
    pub fn peek_mut(&mut self, key: &str) -> Option<&mut EditorSessionController> {
        self.controllers.get_mut(key)
    }

    /// Every pane key of `panelId` that owns a controller right now. The
    /// controller map — not the exec table — is the source of truth: a pane
    /// that mounted but never executed still holds a runtime session.
    pub fn keys_of_panel(&self, panel_id: &str) -> Vec<String> {
        let mut keys: Vec<String> = self
            .controllers
            .keys()
            .filter(|key| is_pane_key_of_panel(key.as_str(), panel_id))
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    /// Copy semantics: a new owner id and a fresh controller, never the source
    /// runtime handle. A destination that already owns a session is a
    /// `ContextConflict` (React throws `ApiError('ContextConflict', ...)`).
    pub fn duplicate(
        &mut self,
        _source_key: &str,
        target_key: &str,
        target: &ExecutionTarget,
        owner: &OwnerRef,
    ) -> Result<&mut EditorSessionController, ApiErrorCode> {
        if self.controllers.contains_key(target_key) {
            return Err(ApiErrorCode::Other("ContextConflict".to_owned()));
        }
        let id = self.next_id;
        self.next_id += 1;
        self.ids.insert(target_key.to_owned(), id);
        Ok(self
            .controllers
            .entry(target_key.to_owned())
            .or_insert_with(|| EditorSessionController::new(id, target.clone(), owner.clone())))
    }

    /// Begin removal: close the controller with `requireNoTransaction`. A
    /// backend refusal keeps ownership reachable (the controller stays in the
    /// map), so a failed close never orphans the session.
    pub fn begin_remove(&mut self, key: &str) -> Option<RemoveStep> {
        let id = *self.ids.get(key)?;
        let controller = self.controllers.get_mut(key)?;
        Some(RemoveStep {
            controller_id: id,
            effects: controller.close(CloseMode::RequireNoTransaction),
        })
    }

    /// Finish removal only when the same controller still owns the key. A
    /// concurrent replacement that took the key in the meantime survives.
    pub fn finish_remove(&mut self, key: &str, controller_id: u64) -> bool {
        if self.ids.get(key) != Some(&controller_id) {
            return false;
        }
        self.ids.remove(key);
        self.controllers.remove(key).is_some()
    }
}

/// Local stand-in for the `state` of React's `getExecution` result; T083's
/// `ExecutionView` only carries `errorCode`.
// remove when ExecutionView gains a state field.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchExecutionView {
    pub state: ExecutionState,
    pub error_code: Option<String>,
}

/// Work the host performs to drive a database switch.
#[allow(dead_code)] // emitted work; remove with the query-panel view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchEffect {
    /// `await controller.refresh()`.
    Refresh,
    /// Ask the caller whether to leave an active/aborted transaction.
    ConfirmTransaction { session: SessionView },
    /// `controller.execute({ command: 'platform.rollbackTransaction', input:
    /// { defersCommit: true } })` — confirmation is a rollback, never a commit.
    Rollback,
    /// `client.getExecution(receipt.executionId)`; re-emitted until terminal.
    PollExecution { execution_id: String },
    /// `controller.setContext(desired)`.
    SetContext,
    /// Surface the refreshed session to `onRevisionConflict`.
    RevisionConflict { session: Option<SessionView> },
}

/// Host answers fed back into [`DatabaseSwitch::step`].
#[allow(dead_code)] // result feed; remove with the query-panel view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchInput {
    /// `refresh()` result; `None` falls back to the snapshot.
    Refreshed(Option<SessionView>),
    /// The caller's transaction confirmation (absent callback means `false`).
    Confirm(bool),
    /// Rollback execution receipt.
    Rollback(Result<ExecutionReceipt, SessionError>),
    /// One `getExecution` poll.
    Execution(SwitchExecutionView),
    /// `setContext` result.
    Context(Result<SessionView, SessionError>),
    /// The conflict-path `refresh()` result.
    ConflictRefresh(Option<SessionView>),
}

/// A switch failure: React returns `{ status: 'failed', error: unknown }`.
#[allow(dead_code)] // result mirror; remove with the query-panel view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchError {
    pub code: String,
    pub message: String,
}

/// React `DatabaseSwitchResult`.
#[allow(dead_code)] // result mirror; remove with the query-panel view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchResult {
    Switched { session: SessionView },
    Cancelled,
    Conflict { session: Option<SessionView> },
    Failed { error: SwitchError },
}

/// React `DatabaseSwitchResult.status` for tests and the view.
#[allow(dead_code)] // result mirror; remove with the query-panel view.
impl SwitchResult {
    pub fn status(&self) -> &'static str {
        match self {
            Self::Switched { .. } => "switched",
            Self::Cancelled => "cancelled",
            Self::Conflict { .. } => "conflict",
            Self::Failed { .. } => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SwitchPhase {
    AwaitRefresh,
    AwaitConfirm,
    AwaitRollback,
    AwaitExecution,
    AwaitRefreshAfterRollback,
    AwaitSetContext,
    AwaitConflictRefresh,
    Done,
}

/// A step's observable outputs.
#[allow(dead_code)] // step result; remove with the query-panel view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchStep {
    pub effects: Vec<SwitchEffect>,
    pub result: Option<SwitchResult>,
}

/// The two-phase database switch as an explicit machine.
#[allow(dead_code)] // consumer: the query-panel view; remove when it lands.
pub struct DatabaseSwitch {
    desired: NamespaceTarget,
    snapshot: Option<SessionView>,
    current: Option<SessionView>,
    rollback_execution_id: Option<String>,
    phase: SwitchPhase,
}

#[allow(dead_code)] // consumer: the query-panel view; remove when it lands.
impl DatabaseSwitch {
    /// Begin a switch. React's first await is `controller.refresh()`.
    pub fn new(desired: NamespaceTarget, snapshot: Option<SessionView>) -> (Self, SwitchStep) {
        let machine = Self {
            desired,
            snapshot,
            current: None,
            rollback_execution_id: None,
            phase: SwitchPhase::AwaitRefresh,
        };
        (
            machine,
            SwitchStep {
                effects: vec![SwitchEffect::Refresh],
                result: None,
            },
        )
    }

    /// Current phase name, for host observability and tests.
    pub fn phase(&self) -> &'static str {
        match self.phase {
            SwitchPhase::AwaitRefresh => "awaitRefresh",
            SwitchPhase::AwaitConfirm => "awaitConfirm",
            SwitchPhase::AwaitRollback => "awaitRollback",
            SwitchPhase::AwaitExecution => "awaitExecution",
            SwitchPhase::AwaitRefreshAfterRollback => "awaitRefreshAfterRollback",
            SwitchPhase::AwaitSetContext => "awaitSetContext",
            SwitchPhase::AwaitConflictRefresh => "awaitConflictRefresh",
            SwitchPhase::Done => "done",
        }
    }

    /// Advance the machine with one host answer. Inputs that do not match the
    /// current phase are ignored (a stale late result cannot move the switch).
    pub fn step(&mut self, input: SwitchInput) -> SwitchStep {
        match (self.phase, input) {
            (SwitchPhase::AwaitRefresh, SwitchInput::Refreshed(view))
            | (SwitchPhase::AwaitRefreshAfterRollback, SwitchInput::Refreshed(view)) => {
                // React: `(await refresh()) ?? snapshot`.
                let current = view.or_else(|| self.snapshot.clone());
                let after_rollback = self.phase == SwitchPhase::AwaitRefreshAfterRollback;
                self.current = current.clone();
                // Only the first refresh gate is transactional; the refresh
                // after a confirmed rollback goes straight to setContext.
                let transaction_session = if after_rollback {
                    None
                } else {
                    current
                        .as_ref()
                        .filter(|session| {
                            matches!(
                                session.observed_context.transaction_state,
                                TransactionState::Active | TransactionState::Aborted
                            )
                        })
                        .cloned()
                };
                if let Some(session) = transaction_session {
                    self.phase = SwitchPhase::AwaitConfirm;
                    return SwitchStep {
                        effects: vec![SwitchEffect::ConfirmTransaction { session }],
                        result: None,
                    };
                }
                self.phase = SwitchPhase::AwaitSetContext;
                SwitchStep {
                    effects: vec![SwitchEffect::SetContext],
                    result: None,
                }
            }
            (SwitchPhase::AwaitConfirm, SwitchInput::Confirm(false)) => self.finish(SwitchResult::Cancelled),
            (SwitchPhase::AwaitConfirm, SwitchInput::Confirm(true)) => {
                self.phase = SwitchPhase::AwaitRollback;
                SwitchStep {
                    effects: vec![SwitchEffect::Rollback],
                    result: None,
                }
            }
            (SwitchPhase::AwaitRollback, SwitchInput::Rollback(Ok(receipt))) => {
                self.rollback_execution_id = Some(receipt.execution_id.clone());
                self.phase = SwitchPhase::AwaitExecution;
                SwitchStep {
                    effects: vec![SwitchEffect::PollExecution {
                        execution_id: receipt.execution_id,
                    }],
                    result: None,
                }
            }
            (SwitchPhase::AwaitRollback, SwitchInput::Rollback(Err(error))) => {
                self.failed(SwitchError::from_session(&error))
            }
            (SwitchPhase::AwaitExecution, SwitchInput::Execution(view)) => match view.state {
                ExecutionState::Succeeded => {
                    self.phase = SwitchPhase::AwaitRefreshAfterRollback;
                    SwitchStep {
                        effects: vec![SwitchEffect::Refresh],
                        result: None,
                    }
                }
                ExecutionState::Failed | ExecutionState::Cancelled => {
                    let message = view
                        .error_code
                        .unwrap_or_else(|| "Rollback outcome unavailable".to_owned());
                    self.failed(SwitchError {
                        code: message.clone(),
                        message,
                    })
                }
                // React's `await setTimeout(50)` then re-poll: the machine
                // simply emits another poll, without a fake timer.
                _ => {
                    let execution_id = self.rollback_execution_id.clone().unwrap_or_default();
                    SwitchStep {
                        effects: vec![SwitchEffect::PollExecution { execution_id }],
                        result: None,
                    }
                }
            },
            (SwitchPhase::AwaitSetContext, SwitchInput::Context(Ok(session))) => {
                self.finish(SwitchResult::Switched { session })
            }
            (SwitchPhase::AwaitSetContext, SwitchInput::Context(Err(error))) => {
                if is_context_conflict(&error) {
                    self.phase = SwitchPhase::AwaitConflictRefresh;
                    SwitchStep {
                        effects: vec![SwitchEffect::Refresh],
                        result: None,
                    }
                } else {
                    self.failed(SwitchError::from_session(&error))
                }
            }
            (SwitchPhase::AwaitConflictRefresh, SwitchInput::ConflictRefresh(view)) => {
                let session = view.or_else(|| self.current.clone());
                let effects = vec![SwitchEffect::RevisionConflict {
                    session: session.clone(),
                }];
                let mut step = self.finish(SwitchResult::Conflict { session });
                step.effects = effects;
                step
            }
            // Stale or out-of-phase input: no transition, no effect.
            _ => SwitchStep {
                effects: Vec::new(),
                result: None,
            },
        }
    }

    fn finish(&mut self, result: SwitchResult) -> SwitchStep {
        self.phase = SwitchPhase::Done;
        SwitchStep {
            effects: Vec::new(),
            result: Some(result),
        }
    }

    fn failed(&mut self, error: SwitchError) -> SwitchStep {
        self.finish(SwitchResult::Failed { error })
    }
}

impl SwitchError {
    /// Map the T083 error shape into React's `{ code, message }`.
    #[allow(dead_code)] // helper for the query-panel view; remove when it lands.
    pub fn from_session(error: &SessionError) -> Self {
        let code = match &error.code {
            ApiErrorCode::SessionLost => "SessionLost",
            ApiErrorCode::SessionNotFound => "SessionNotFound",
            ApiErrorCode::RuntimeEpochMismatch => "RuntimeEpochMismatch",
            ApiErrorCode::Other(code) => code.as_str(),
        };
        Self {
            code: code.to_owned(),
            message: error.message.clone(),
        }
    }
}

/// React checks `error instanceof ApiError && error.code === 'ContextConflict'`.
/// The Rune error enum carries that string code in `Other`.
#[allow(dead_code)] // helper for the switch; remove when ApiErrorCode gains the variant.
fn is_context_conflict(error: &SessionError) -> bool {
    matches!(&error.code, ApiErrorCode::Other(code) if code == "ContextConflict")
}
