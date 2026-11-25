//! Host result feeds for the editor-session state machine. Split out of
//! `editor_session_controller.rs` to stay under the 800-line ceiling; the
//! inherent methods live on `EditorSessionController` regardless of module.

use super::*;

impl EditorSessionController {
    // --- host result feeds ---------------------------------------------------

    /// Resolution of an `IssueToken` effect.
    #[allow(dead_code)] // host result feed; remove with the host executor.
    pub fn on_token(
        &mut self,
        call_id: u64,
        result: Result<SubmissionToken, SessionError>,
    ) -> Vec<SessionEffect> {
        // The open's token -> request open.
        let is_open_token = self
            .opening
            .as_ref()
            .is_some_and(|opening| opening.call_id == call_id && opening.phase == OpenPhase::AwaitToken);
        if is_open_token {
            return match result {
                Ok(token) => {
                    let id = self.alloc_call_id();
                    if let Some(opening) = self.opening.as_mut() {
                        opening.phase = OpenPhase::AwaitOpen;
                        opening.call_id = id;
                    }
                    let request = OpenSessionRequest {
                        initial_target: self.latest_target.clone(),
                        owner: self.owner.clone(),
                        idempotency_key: token.idempotency_key.clone(),
                    };
                    vec![SessionEffect::Open { call_id: id, request, token }]
                }
                Err(error) => {
                    let resume = self.opening.as_ref().is_some_and(|opening| opening.resume_op);
                    self.opening = None;
                    if resume {
                        self.last_result = Some(Err(error));
                        return self.advance_queue();
                    }
                    Vec::new()
                }
            };
        }

        // The current operation's token -> its main call.
        let op_ctx = self.current.as_ref().and_then(|op| {
            if op.call_id == call_id && op.phase == Phase::IssueToken {
                Some((op.kind, op.session.clone(), op.call.clone(), op.desired.clone()))
            } else {
                None
            }
        });
        if let Some((kind, session, call, desired)) = op_ctx {
            let session = match session {
                Some(session) => session,
                None => return Vec::new(),
            };
            return match result {
                Ok(token) => {
                    let id = self.alloc_call_id();
                    if let Some(op) = self.current.as_mut() {
                        op.phase = Phase::Call;
                        op.call_id = id;
                        // `previousHandle = session.handle` in React setContext:
                        // captured before the call so the replaced handle can be
                        // released after the atomic rebind.
                        if kind == OpKind::SetContext {
                            op.previous_handle = Some(session.handle.clone());
                        }
                    }
                    let key = token.idempotency_key.clone();
                    match kind {
                        OpKind::Execute => vec![SessionEffect::Execute {
                            call_id: id,
                            request: ExecuteRequest {
                                handle: session.handle.clone(),
                                expected_context_revision: session.context_revision.clone(),
                                call: call.unwrap_or(CommandCall { command: String::new(), input: String::new() }),
                                idempotency_key: key,
                            },
                            token,
                        }],
                        OpKind::SetContext => vec![SessionEffect::SetContext {
                            call_id: id,
                            request: SetContextRequest {
                                handle: session.handle.clone(),
                                expected_context_revision: session.context_revision.clone(),
                                desired: desired.unwrap_or_default(),
                                idempotency_key: key,
                            },
                            token,
                        }],
                        OpKind::Close | OpKind::OpenNew => Vec::new(),
                    }
                }
                Err(error) => {
                    // execute wraps the token in try/catch; setContext does not.
                    if kind == OpKind::Execute {
                        self.accept_session_error(&error, Some(&session.handle));
                    }
                    self.last_result = Some(Err(error));
                    self.advance_queue()
                }
            };
        }

        // A public cancel awaiting its token.
        if let Some(PublicCall::CancelAwaitToken { execution_id }) = self.pending_public.get(&call_id).cloned() {
            self.pending_public.remove(&call_id);
            if let Ok(token) = result {
                let id = self.alloc_call_id();
                self.pending_public.insert(id, PublicCall::CancelAwaitResult);
                return vec![SessionEffect::Cancel { call_id: id, execution_id, token }];
            }
        }
        Vec::new()
    }

    /// Resolution of an `Open` effect.
    #[allow(dead_code)] // host result feed; remove with the host executor.
    pub fn on_open(
        &mut self,
        call_id: u64,
        result: Result<OpenSessionReceipt, SessionError>,
    ) -> Vec<SessionEffect> {
        let is_open = self
            .opening
            .as_ref()
            .is_some_and(|opening| opening.call_id == call_id && opening.phase == OpenPhase::AwaitOpen);
        if !is_open {
            return Vec::new();
        }
        let resume = self.opening.as_ref().is_some_and(|opening| opening.resume_op);
        match result {
            Ok(receipt) => {
                self.session = Some(receipt.session.clone());
                self.update_latest_namespace(&receipt.session);
                self.attachment_token = Some(receipt.attachment_token);
                self.opening = None;
                let mut effects = Vec::new();
                if !self.page_closer_armed {
                    self.page_closer_armed = true;
                    effects.push(SessionEffect::RegisterPageCloser { controller_id: self.id });
                }
                self.publish();
                if resume && self.session.is_some() {
                    match self.current.as_ref().map(|op| (op.kind, op.phase)) {
                        // execute / setContext / openNewSession resume their
                        // `await ensureSession()` with the newly opened session.
                        Some((kind, Phase::NeedSession)) => {
                            let session = self.session.clone();
                            let (id, token_effects) = self.token_effects(Self::op_token_name(kind));
                            if let Some(op) = self.current.as_mut() {
                                op.session = session;
                                op.phase = Phase::IssueToken;
                                op.call_id = id;
                            }
                            effects.extend(token_effects);
                        }
                        // close / openNewSession only awaited `opening`; their
                        // own first step now runs.
                        Some((_, Phase::WaitOpening)) => {
                            effects.extend(self.start_current());
                        }
                        _ => {}
                    }
                }
                effects
            }
            Err(error) => {
                self.opening = None;
                if resume {
                    self.last_result = Some(Err(error));
                    return self.advance_queue();
                }
                Vec::new()
            }
        }
    }

    /// Resolution of an `Execute` effect.
    #[allow(dead_code)] // host result feed; remove with the host executor.
    pub fn on_execute(
        &mut self,
        call_id: u64,
        result: Result<ExecutionReceipt, SessionError>,
    ) -> Vec<SessionEffect> {
        let matches = self.current.as_ref().is_some_and(|op| {
            op.kind == OpKind::Execute && op.phase == Phase::Call && op.call_id == call_id
        });
        if !matches {
            return Vec::new();
        }
        let handle = self.current.as_ref().and_then(|op| op.session.as_ref()).map(|session| session.handle.clone());
        let mut effects = Vec::new();
        match result {
            Ok(receipt) => {
                self.execution = Some(receipt.clone());
                self.publish();
                self.last_result = Some(Ok(OpResult::Execution(receipt)));
            }
            Err(error) => {
                self.accept_session_error(&error, handle.as_ref());
                self.last_result = Some(Err(error));
            }
        }
        effects.extend(self.advance_queue());
        effects
    }

    /// Resolution of a `SetContext` effect.
    #[allow(dead_code)] // host result feed; remove with the host executor.
    pub fn on_set_context(
        &mut self,
        call_id: u64,
        result: Result<ContextChangeReceipt, SessionError>,
    ) -> Vec<SessionEffect> {
        let matches = self.current.as_ref().is_some_and(|op| {
            op.kind == OpKind::SetContext && op.phase == Phase::Call && op.call_id == call_id
        });
        if !matches {
            return Vec::new();
        }
        let previous_handle = self.current.as_ref().and_then(|op| op.previous_handle.clone());
        let mut effects = Vec::new();
        match result {
            Ok(receipt) => {
                self.session = Some(receipt.session.clone());
                self.update_latest_namespace(&receipt.session);
                if let Some(token) = receipt.attachment_token {
                    self.attachment_token = Some(token);
                }
                if receipt.replaced_session_id.is_some() {
                    self.execution = None;
                }
                self.publish();
                self.last_result = Some(Ok(OpResult::Session(receipt.session.clone())));
                if let Some(previous) = previous_handle {
                    let same_as_current = self
                        .session
                        .as_ref()
                        .is_some_and(|session| same_handle(Some(&previous), Some(&session.handle)));
                    if !same_as_current {
                        let id = self.alloc_call_id();
                        if let Some(op) = self.current.as_mut() {
                            op.phase = Phase::Release;
                            op.call_id = id;
                        }
                        effects.push(SessionEffect::Close {
                            call_id: id,
                            handle: previous,
                            mode: CloseMode::RequireNoTransaction,
                            purpose: ClosePurpose::ReleaseReplaced,
                        });
                        return effects;
                    }
                }
            }
            Err(error) => {
                // A failed revision/transaction/replacement leaves the old handle bound.
                self.last_result = Some(Err(error));
            }
        }
        effects.extend(self.advance_queue());
        effects
    }

    /// Resolution of a `Close` effect. Routes to the close operation, the
    /// setContext replacement cleanup or the page-unload closer.
    #[allow(dead_code)] // host result feed; remove with the host executor.
    pub fn on_close(
        &mut self,
        call_id: u64,
        result: Result<CloseReceipt, SessionError>,
    ) -> Vec<SessionEffect> {
        let close_op = self.current.as_ref().is_some_and(|op| {
            op.kind == OpKind::Close && op.phase == Phase::Call && op.call_id == call_id
        });
        if close_op {
            let mut effects = Vec::new();
            match result {
                Ok(receipt) => {
                    if let Some(session) = self.session.as_mut() {
                        session.state = SessionState::from_close(receipt.state);
                    }
                    self.attachment_token = None;
                    effects.extend(self.disarm_page_closer());
                    self.publish();
                    self.last_result = Some(Ok(OpResult::Close(receipt)));
                }
                Err(error) => self.last_result = Some(Err(error)),
            }
            effects.extend(self.advance_queue());
            return effects;
        }
        let release = self.current.as_ref().is_some_and(|op| {
            op.kind == OpKind::SetContext && op.phase == Phase::Release && op.call_id == call_id
        });
        if release {
            // Best effort: every outcome is swallowed.
            return self.advance_queue();
        }
        if matches!(self.pending_public.get(&call_id), Some(PublicCall::PageUnload)) {
            self.pending_public.remove(&call_id);
        }
        Vec::new()
    }

    /// Resolution of a `GetExecution` effect.
    #[allow(dead_code)] // host result feed; remove with the host executor.
    pub fn on_get_execution(
        &mut self,
        call_id: u64,
        result: Result<ExecutionView, SessionError>,
    ) -> Vec<SessionEffect> {
        let handle = match self.pending_public.remove(&call_id) {
            Some(PublicCall::GetExecution { handle }) => handle,
            _ => return Vec::new(),
        };
        match result {
            Ok(view) => {
                if view.error_code == Some(ExecutionErrorCode::ResourceLost) {
                    self.mark_session_lost(handle.as_ref());
                }
                self.last_result = Some(Ok(OpResult::ExecutionView(view)));
            }
            Err(error) => {
                self.accept_session_error(&error, handle.as_ref());
                self.last_result = Some(Err(error));
            }
        }
        Vec::new()
    }

    /// Resolution of a `GetSession` effect (`refresh`).
    #[allow(dead_code)] // host result feed; remove with the host executor.
    pub fn on_get_session(
        &mut self,
        call_id: u64,
        result: Result<SessionView, SessionError>,
    ) -> Vec<SessionEffect> {
        let handle = match self.pending_public.remove(&call_id) {
            Some(PublicCall::GetSession { handle }) => handle,
            _ => return Vec::new(),
        };
        match result {
            Ok(view) => {
                self.accept_session(view);
                Vec::new()
            }
            Err(error) => {
                self.accept_session_error(&error, Some(&handle));
                Vec::new()
            }
        }
    }

    /// Resolution of `Detach`/`Attach` — both feed `acceptSession`.
    #[allow(dead_code)] // host result feed; remove with the host executor.
    pub fn on_attachment(
        &mut self,
        call_id: u64,
        result: Result<SessionView, SessionError>,
    ) -> Vec<SessionEffect> {
        let known = matches!(
            self.pending_public.get(&call_id),
            Some(PublicCall::Detach) | Some(PublicCall::Attach)
        );
        if !known {
            return Vec::new();
        }
        self.pending_public.remove(&call_id);
        if let Ok(view) = result {
            self.accept_session(view);
        }
        Vec::new()
    }

    /// Resolution of a `Cancel` effect.
    #[allow(dead_code)] // host result feed; remove with the host executor.
    pub fn on_cancel(
        &mut self,
        call_id: u64,
        result: Result<CancelReceipt, SessionError>,
    ) -> Vec<SessionEffect> {
        if !matches!(self.pending_public.remove(&call_id), Some(PublicCall::CancelAwaitResult)) {
            return Vec::new();
        }
        self.last_result = Some(result.map(OpResult::Cancel));
        Vec::new()
    }
}
