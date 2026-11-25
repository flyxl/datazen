//! Port of `packages/frontend/src/hooks/useAiEditorGrant.ts`.
//!
//! Owns the panel's editor-session sharing lifecycle: grant, touch,
//! revoke and local expiry. Defaults to `independent`; a denied grant
//! leaves the machine independent with a visible error instead of
//! silently sending without a claim.
//!
//! ## What maps where
//!
//! - The React hook is `useState<GrantSnapshot>` + a `useMemo`'d
//!   `GrantState` + the `useEffect` subscribe/dispose dance. In immediate
//!   mode the hook IS [`AiEditorGrant`]: a `GrantState` plus the
//!   `claimedClientInstanceIdRef`, embedded in the owning state `S`
//!   (the AI chat panel's slice). [`AiEditorGrant::update_params`] is
//!   the `useMemo`/`useEffect` pair: a changed
//!   `(connectionId, editorSessionId)` key disposes the old machine and
//!   creates a new one (the effect cleanup), and a lost key disposes
//!   back to the `INDEPENDENT` snapshot.
//! - The hook's injected `grant` closure (resolve the client instance id
//!   and the pane's session handle, then `aiCommands.grantEditorSession`)
//!   is the `CallGrant` executor contract, documented on
//!   [`AiEditorGrant::set_sharing`]: the host peeks the pane's
//!   `EditorSessionController` (never duplicates it), `ensure_session`s
//!   it — the user explicitly asked to share that session, so binding it
//!   is the requested effect — and resolves the client instance id,
//!   delivering both with [`AiEditorGrant::on_grant_result`].
//!   `commands/ai.ts` is this segment's T094; until it lands the host
//!   executes the effect against its own IPC seam.
//! - React's `claimedClientInstanceIdRef` is written the moment
//!   `queryPanelClientInstanceId()` resolves; here it is written when
//!   the grant result arrives. Unobservable: `claimForSend` requires
//!   the `granted` phase and the fresh receipt, so a stale id can never
//!   reach a claim (the React ref could keep a stale id after a failed
//!   grant too — it is equally unreadable there).
//! - `onWarn`'s `console.warn` rides [`GrantEffect::Warn`] (the machine
//!   emits it; the host logs).
//!
//! Known deltas: none behavioural — `canShare`, `sharing`, `phase`,
//! `error`, `setSharing` and `claimForSend` are the React returns
//! verbatim, and every guard order is kept.

use crate::ai::grant_state::{
    GrantConfig, GrantEffect, GrantInput, GrantPhase, GrantState, GrantTimerHandle,
};
use crate::ai::target_claim::{AiTargetClaim, editor_grant, independent};

/// React `UseAiEditorGrantParams`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UseAiEditorGrantParams {
    pub connection_id: Option<String>,
    pub database: Option<String>,
    /// Stable owner id of the editor pane whose session may be shared.
    pub editor_session_id: Option<String>,
}

/// The hook: a `GrantState` keyed by `(connectionId, editorSessionId)`
/// plus the claimed-client-instance ref. Embed in the owning state `S`
/// and drive through `update_params` / `set_sharing` / the `on_*`
/// reporters; execute the returned effects on the host.
pub struct AiEditorGrant {
    params: UseAiEditorGrantParams,
    machine: Option<GrantState>,
    /// React `claimedClientInstanceIdRef` (written when a grant resolves).
    claimed_client_instance_id: Option<String>,
}

impl AiEditorGrant {
    /// The hook mounted with its params (React's first render).
    pub fn new(params: UseAiEditorGrantParams) -> Self {
        Self::with_config(params, GrantConfig::default())
    }

    /// `new` with an injected clock/touch ratio (the host's seam; tests
    /// drive deterministic time through it).
    pub fn with_config(params: UseAiEditorGrantParams, config: GrantConfig) -> Self {
        let machine = Self::machine_for(&params, config);
        Self { params, machine, claimed_client_instance_id: None }
    }

    /// React's `canShare = Boolean(connectionId && editorSessionId)`.
    pub fn can_share(&self) -> bool {
        self.params.connection_id.is_some() && self.params.editor_session_id.is_some()
    }

    /// The live params (the hook's props; a rebind is [`Self::update_params`]).
    pub fn params(&self) -> &UseAiEditorGrantParams {
        &self.params
    }

    /// React's `sharing = phase === 'granting' || phase === 'granted'`.
    pub fn sharing(&self) -> bool {
        matches!(self.phase(), GrantPhase::Granting | GrantPhase::Granted)
    }

    /// React's `phase` (the `INDEPENDENT` constant when there is no
    /// machine — `state === null` keeps the initial snapshot).
    pub fn phase(&self) -> GrantPhase {
        self.machine.as_ref().map_or(GrantPhase::Independent, |m| m.snapshot().phase)
    }

    /// React's `error`.
    pub fn error(&self) -> Option<&str> {
        self.machine.as_ref().and_then(|m| m.snapshot().error.as_deref())
    }

    /// The live snapshot (independent when there is no machine).
    pub fn snapshot(&self) -> crate::ai::grant_state::GrantSnapshot {
        self.machine
            .as_ref()
            .map_or_else(crate::ai::grant_state::GrantSnapshot::independent, |m| {
                m.snapshot().clone()
            })
    }

    /// The `useMemo`/`useEffect` pair: rebind when the
    /// `(connectionId, editorSessionId)` key changes. A lost key
    /// disposes the machine (the effect cleanup) — its effects must
    /// still be executed. `database` is not part of the key (React's
    /// memo only keys on the identity pair); it only feeds the next
    /// grant input.
    pub fn update_params(&mut self, params: UseAiEditorGrantParams) -> Vec<GrantEffect> {
        self.update_params_with(params, GrantConfig::default())
    }

    /// `update_params` with the config a rebound machine takes (the
    /// injected clock survives a rekey).
    pub fn update_params_with(
        &mut self,
        params: UseAiEditorGrantParams,
        config: GrantConfig,
    ) -> Vec<GrantEffect> {
        let key_now = Self::key(&params);
        let key_before = Self::key(&self.params);
        let mut effects = Vec::new();
        if key_now != key_before {
            if let Some(machine) = self.machine.as_mut() {
                effects.extend(machine.dispose());
            }
            self.machine = Self::machine_for(&params, config);
            self.claimed_client_instance_id = None;
        }
        self.params = params;
        effects
    }

    /// React `setSharing(next)`. `false` revokes; `true` grants with the
    /// hook's params. With no machine (`!state`) both are no-ops, and
    /// the guard order is React's: `if (!state) return;` first.
    ///
    /// The `CallGrant` effect the grant arms is the hook's closure:
    /// resolve `queryPanelClientInstanceId()` and the pane's session
    /// handle (`registry.peek(editorSessionId)` — throw
    /// `Editor session is not ready` when the pane has no controller —
    /// then `ensureSession()`), and call
    /// `aiCommands.grantEditorSession` with
    /// `sessionHandle.dbSessionId`/`runtimeEpoch` stringified. Deliver
    /// with [`Self::on_grant_result`], which also records the claimed
    /// client instance id.
    pub fn set_sharing(&mut self, next: bool) -> Vec<GrantEffect> {
        let Some(machine) = self.machine.as_mut() else {
            return Vec::new();
        };
        if !next {
            return machine.revoke();
        }
        let input = GrantInput {
            connection_id: self.params.connection_id.clone().unwrap_or_default(),
            database: self.params.database.clone(),
            editor_session_id: self.params.editor_session_id.clone().unwrap_or_default(),
        };
        machine.grant(input).effects
    }

    /// The grant executor's report. `Ok` carries the receipt **and** the
    /// client instance id the executor claimed with (React's ref write
    /// inside the closure); `Err` is the closure's rejected promise
    /// (session-not-ready, IPC failure) and leaves the machine
    /// independent with the visible error.
    pub fn on_grant_result(
        &mut self,
        token: u64,
        result: Result<(crate::ai::grant_state::AiGrantReceipt, String), String>,
    ) -> Vec<GrantEffect> {
        let claimed_id = result.as_ref().ok().map(|(_, id)| id.clone());
        let machine_result =
            result.map(|(receipt, _)| receipt);
        let effects = self
            .machine
            .as_mut()
            .map(|m| m.on_grant_result(token, machine_result))
            .unwrap_or_default();
        if let Some(id) = claimed_id {
            self.claimed_client_instance_id = Some(id);
        }
        effects
    }

    /// The revoke executor's report (the `CallRevoke` effect's answer):
    /// `Ok(())` or the rejection message (which surfaces as a `Warn`).
    pub fn on_revoke_result(
        &mut self,
        grant_id: &str,
        result: Result<(), String>,
    ) -> Vec<GrantEffect> {
        self.machine
            .as_mut()
            .map(|m| m.on_revoke_result(grant_id, result))
            .unwrap_or_default()
    }

    /// The touch executor's report (the `CallTouch` effect's answer):
    /// `Ok(true)` extends, `Ok(false)` is the `"touch rejected"` error,
    /// `Err` carries the rejection.
    pub fn on_touch_result(
        &mut self,
        grant_id: &str,
        result: Result<bool, String>,
    ) -> Vec<GrantEffect> {
        self.machine
            .as_mut()
            .map(|m| m.on_touch_result(grant_id, result))
            .unwrap_or_default()
    }

    /// The host's touch-timer fire.
    pub fn fire_touch_timer(&mut self, handle: GrantTimerHandle) -> Vec<GrantEffect> {
        self.machine
            .as_mut()
            .map(|m| m.fire_touch_timer(handle))
            .unwrap_or_default()
    }

    /// The host's local-expiry fire — the independent exit.
    pub fn fire_expiry_timer(&mut self, handle: GrantTimerHandle) -> Vec<GrantEffect> {
        self.machine
            .as_mut()
            .map(|m| m.fire_expiry_timer(handle))
            .unwrap_or_default()
    }

    /// React `claimForSend()`: the claim to attach to the next `ai_chat`
    /// call. Granted + receipt + claimed id + editor session → the
    /// editor-grant claim; anything less → `independent()`.
    pub fn claim_for_send(&self) -> AiTargetClaim {
        let snapshot = self.snapshot();
        if snapshot.phase == GrantPhase::Granted {
            if let (Some(receipt), Some(claimed), Some(editor_session_id)) = (
                snapshot.receipt.as_ref(),
                self.claimed_client_instance_id.as_deref(),
                self.params.editor_session_id.as_deref(),
            ) {
                return editor_grant(receipt, claimed, editor_session_id);
            }
        }
        independent()
    }

    /// The unmount path: dispose the machine (the effect cleanup).
    pub fn dispose(&mut self) -> Vec<GrantEffect> {
        self.machine.as_mut().map_or_else(Vec::new, GrantState::dispose)
    }

    /// The `useMemo` key: `(connectionId, editorSessionId)`.
    fn key(params: &UseAiEditorGrantParams) -> (Option<&str>, Option<&str>) {
        (
            params.connection_id.as_deref(),
            params.editor_session_id.as_deref(),
        )
    }

    /// `useMemo`: a machine exists only when `canShare` (React's
    /// `if (!connectionId || !editorSessionId) return null`).
    fn machine_for(params: &UseAiEditorGrantParams, config: GrantConfig) -> Option<GrantState> {
        if params.connection_id.is_none() || params.editor_session_id.is_none() {
            return None;
        }
        Some(GrantState::new(config))
    }
}
