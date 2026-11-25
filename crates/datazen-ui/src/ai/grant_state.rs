//! Editor-session grant lifecycle ported from `packages/frontend/src/ai/grantState.ts`.
//!
//! ```text
//! independent ──grant()──▶ granting ──receipt──▶ granted
//!      ▲                       │                    │
//!      │                     denied              revoke()
//!      │                       ▼                    ▼
//!      └────local expiry──── revoked ◀─────────────┘
//! ```
//!
//! # Faithfulness
//!
//! Every field, phase and transition of the React class has a counterpart:
//! `current`/[`GrantSnapshot`], `grantToken`, `inFlight`, `ttlMs`, both timer
//! handles and the full `grant` / `revoke` / `dispose` / `scheduleTouch` /
//! `runTouch` / `expireLocally` / `clearTimers` / `update` logic. The
//! deprioritised races (revoke or expiry winning over an in-flight grant or
//! touch) are kept via the explicit grant token and receipt-id guards.
//!
//! # Immediate-mode adaptation (honest)
//!
//! The React module injects async `grant`/`revoke`/`touch` and timer functions
//! and fans snapshots out to `subscribe` listeners. Rune has no promises and
//! redraws from state each frame, so:
//!
//! * **No fake async.** Injected promises become emitted [`GrantEffect`]s. The
//!   host executes `CallGrant`/`CallRevoke`/`CallTouch` and reports the outcome
//!   with `on_grant_result`/`on_revoke_result`/`on_touch_result`. Timers become
//!   `Set*Timer`/`ClearTimer` effects with machine-owned handles; the host
//!   fires `fire_touch_timer`/`fire_expiry_timer`. A single `now` closure is
//!   injected instead of `deps.now()`. Nothing fabricates a `Promise`/`Future`.
//! * **State, not observers, is the source of truth.** The host embeds
//!   [`GrantState`] in its state slice and reads [`GrantState::snapshot`] every
//!   frame. A faithful `subscribe`/`unsubscribe` listener facility is kept for
//!   consumers that want change notifications (ids replace TS closures), but
//!   it is optional and never required for rendering.
//! * **No IPC / transport.** `connection_id`, `database` and
//!   `editor_session_id` travel in [`GrantInput`]; resolving the session handle
//!   and client id stays in the host's `CallGrant` executor, exactly as the
//!   React caller resolved them inside `deps.grant`.
//! * **Errors are plain strings.** The host maps a thrown value the way React
//!   `messageOf` does (`Error.message`, otherwise `String(error)`) before
//!   calling the `on_*` methods. `Warn` carries the message plus the original
//!   error text so the host's `onWarn` can log both.

use std::time::{SystemTime, UNIX_EPOCH};

/// `GrantTimerHandle` — the opaque value a timer scheduler returns. The React
/// side uses `ReturnType<typeof setTimeout>`; here the machine allocates its
/// own monotonic `u64` handles so it can mirror "store the handle, pass it to
/// clearTimeout" without depending on a platform type.
#[allow(dead_code)] // T049 foundation API; delete the alias note when the host timer bridge lands.
pub type GrantTimerHandle = u64;

/// Mirrors `AiGrantReceipt` from `ai/targetClaim.ts`.
#[allow(dead_code)] // consumed by the AI chat panel's run-claim wiring; delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiGrantReceipt {
    pub grant_id: String,
    pub connection_id: String,
    pub database: Option<String>,
    pub config_revision: i64,
    pub expires_at_ms: i64,
}

/// `GrantPhase` — the four states of the machine.
#[allow(dead_code)] // T049 foundation API; delete when the chat panel starts reading snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantPhase {
    Independent,
    Granting,
    Granted,
    Revoked,
}

/// `GrantInput` — identifying fields of the editor session to share. The
/// host's opaque session handle and claimed client id are resolved by the
/// caller inside the `CallGrant` executor, keeping this machine transport-free.
#[allow(dead_code)] // passed through by the host's grant executor; delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantInput {
    pub connection_id: String,
    pub database: Option<String>,
    pub editor_session_id: String,
}

/// `GrantSnapshot` — the observable state. `expires_at_ms` is the locally
/// tracked deadline (extended by successful touches); `error` is the last
/// failure, cleared on a new successful grant.
#[allow(dead_code)] // read by the chat panel each frame; delete then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantSnapshot {
    pub phase: GrantPhase,
    pub receipt: Option<AiGrantReceipt>,
    pub expires_at_ms: Option<i64>,
    pub error: Option<String>,
}

impl GrantSnapshot {
    /// The `INDEPENDENT` constant the React module starts from.
    #[allow(dead_code)] // constructor used by the host when creating the machine; delete then.
    pub fn independent() -> Self {
        Self {
            phase: GrantPhase::Independent,
            receipt: None,
            expires_at_ms: None,
            error: None,
        }
    }
}

/// Notification callback for [`GrantState::subscribe`]. In immediate mode this
/// is a convenience; rendering must read [`GrantState::snapshot`] instead.
#[allow(dead_code)] // optional observer API; delete with subscribe/unsubscribe if unused.
pub type GrantListener = Box<dyn FnMut(&GrantSnapshot)>;

/// Work the host must perform on behalf of the machine. This is the immediate-
/// mode stand-in for the React injected async functions and timer functions.
/// The machine never performs I/O itself and never blocks.
#[allow(dead_code)] // the host adapter executes these; delete the note when it lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrantEffect {
    /// Execute the injected `grant(input)`; deliver with `on_grant_result`.
    /// `token` is echoed back so a stale (revoked/disposed) grant is rejected.
    CallGrant { input: GrantInput, token: u64 },
    /// Execute the injected `revoke(grant_id)`; deliver with `on_revoke_result`.
    /// Covers both `revoke()` and the quiet release of an orphan/stale grant.
    CallRevoke { grant_id: String },
    /// Execute the injected `touch(grant_id)`; deliver with `on_touch_result`.
    CallTouch { grant_id: String },
    /// Register the touch timer for `delay_ms`; fire with `fire_touch_timer`.
    SetTouchTimer { handle: GrantTimerHandle, delay_ms: i64 },
    /// Register the local-expiry timer for `delay_ms`; fire with
    /// `fire_expiry_timer`. This is an independent exit: a stuck touch cannot
    /// keep the UI claiming a grant past the deadline.
    SetExpiryTimer { handle: GrantTimerHandle, delay_ms: i64 },
    /// Cancel a previously registered timer (React `clearTimeout`).
    ClearTimer { handle: GrantTimerHandle },
    /// Report a failed quiet revoke through the injected `onWarn`.
    Warn { message: String, error: Option<String> },
}

/// Construction-time configuration. `touch_interval_ratio` is the fraction of
/// the remaining ttl at which a touch is scheduled (React default `0.5`).
/// `now` is the injected clock in milliseconds.
#[allow(dead_code)] // passed to GrantState::new by the host; delete then.
pub struct GrantConfig {
    pub touch_interval_ratio: f64,
    pub now: Box<dyn Fn() -> i64>,
}

impl Default for GrantConfig {
    fn default() -> Self {
        Self {
            touch_interval_ratio: 0.5,
            now: Box::new(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64
            }),
        }
    }
}

/// A `Partial<GrantSnapshot>` patch. `Some(None)` explicitly clears a
/// nullable field (`receipt`, `expiresAtMs`, `error`), matching the TS object
/// literal `{ receipt: null, ... }`; an absent field is left untouched.
#[derive(Default)]
struct GrantPatch {
    phase: Option<GrantPhase>,
    receipt: Option<Option<AiGrantReceipt>>,
    expires_at_ms: Option<Option<i64>>,
    error: Option<Option<String>>,
}

impl GrantPatch {
    /// Full replacement, mirroring a four-field `update({...})` call.
    fn full(
        phase: GrantPhase,
        receipt: Option<AiGrantReceipt>,
        expires_at_ms: Option<i64>,
        error: Option<String>,
    ) -> Self {
        Self {
            phase: Some(phase),
            receipt: Some(receipt),
            expires_at_ms: Some(expires_at_ms),
            error: Some(error),
        }
    }
}

/// The synchronous result of [`GrantState::grant`], mirroring the value the
/// TS `Promise<AiGrantReceipt | null>` resolves to. `joined_in_flight` marks
/// the `granting`-state short circuit (the caller reuses the existing request).
#[allow(dead_code)] // returned to the host's grant button handler; delete then.
#[derive(Debug)]
pub struct GrantStep {
    pub receipt: Option<AiGrantReceipt>,
    pub joined_in_flight: bool,
    pub effects: Vec<GrantEffect>,
}

/// `GrantState` — the ported state machine. Embed in `S`; drive with the
/// `on_*`/`fire_*` methods and execute the returned effects on the host side.
#[allow(dead_code)] // T049; the AI chat panel embeds this in its state slice. Delete then.
pub struct GrantState {
    current: GrantSnapshot,
    listeners: Vec<(u64, GrantListener)>,
    next_listener_id: u64,
    now: Box<dyn Fn() -> i64>,
    touch_interval_ratio: f64,
    touch_timer: Option<GrantTimerHandle>,
    expiry_timer: Option<GrantTimerHandle>,
    next_timer_handle: GrantTimerHandle,
    ttl_ms: i64,
    /// Invalidates an in-flight grant when revoke/dispose wins the race.
    grant_token: u64,
    in_flight: bool,
    disposed: bool,
}

impl GrantState {
    /// Mirrors the React constructor (defaults filled in by `GrantConfig`).
    #[allow(dead_code)] // called by the host when building its state slice; delete then.
    pub fn new(config: GrantConfig) -> Self {
        Self {
            current: GrantSnapshot::independent(),
            listeners: Vec::new(),
            next_listener_id: 1,
            now: config.now,
            touch_interval_ratio: config.touch_interval_ratio,
            touch_timer: None,
            expiry_timer: None,
            next_timer_handle: 1,
            ttl_ms: 0,
            grant_token: 0,
            in_flight: false,
            disposed: false,
        }
    }

    /// `get snapshot` — the immediate-mode source of truth, read each frame.
    #[allow(dead_code)] // read by the panel every frame; delete then.
    pub fn snapshot(&self) -> &GrantSnapshot {
        &self.current
    }

    /// `get isGranted` — granted *and* holding a receipt.
    #[allow(dead_code)] // read by the panel to enable/disable actions; delete then.
    pub fn is_granted(&self) -> bool {
        self.current.phase == GrantPhase::Granted && self.current.receipt.is_some()
    }

    /// `subscribe(listener)` — returns an id; pair with [`Self::unsubscribe`].
    /// Immediate-mode note: ids replace the TS unsubscribe closure because a
    /// closure borrowing the machine cannot be stored beside it in `S`.
    #[allow(dead_code)] // optional observer API; delete with unsubscribe if unused.
    pub fn subscribe(&mut self, listener: GrantListener) -> u64 {
        let id = self.next_listener_id;
        self.next_listener_id += 1;
        self.listeners.push((id, listener));
        id
    }

    /// Remove the listener previously returned by [`Self::subscribe`].
    #[allow(dead_code)] // optional observer API; delete with subscribe if unused.
    pub fn unsubscribe(&mut self, id: u64) {
        self.listeners.retain(|(candidate, _)| *candidate != id);
    }

    /// `grant(input)`: from `granted` returns the existing receipt; from
    /// `granting` joins the in-flight request; from `revoked`/`independent`
    /// starts over. A denied grant leaves the machine `independent` with an
    /// error, delivered later through [`Self::on_grant_result`].
    #[allow(dead_code)] // called by the chat panel's grant button; delete then.
    pub fn grant(&mut self, input: GrantInput) -> GrantStep {
        if self.disposed {
            return GrantStep {
                receipt: None,
                joined_in_flight: false,
                effects: Vec::new(),
            };
        }
        if self.current.phase == GrantPhase::Granted {
            if let Some(receipt) = self.current.receipt.clone() {
                return GrantStep {
                    receipt: Some(receipt),
                    joined_in_flight: false,
                    effects: Vec::new(),
                };
            }
        }
        if self.current.phase == GrantPhase::Granting && self.in_flight {
            return GrantStep {
                receipt: None,
                joined_in_flight: true,
                effects: Vec::new(),
            };
        }

        let mut effects = self.clear_timers();
        self.grant_token += 1;
        let token = self.grant_token;
        self.update(GrantPatch::full(GrantPhase::Granting, None, None, None));
        self.in_flight = true;
        effects.push(GrantEffect::CallGrant { input, token });
        GrantStep {
            receipt: None,
            joined_in_flight: false,
            effects,
        }
    }

    /// Resolution of an injected `grant` call (the `.then`/`.catch`/`.finally`
    /// chain). `token` must be the one emitted with the `CallGrant` effect.
    #[allow(dead_code)] // called by the host's grant executor; delete then.
    pub fn on_grant_result(
        &mut self,
        token: u64,
        result: Result<AiGrantReceipt, String>,
    ) -> Vec<GrantEffect> {
        let mut effects = Vec::new();
        match result {
            Ok(receipt) => {
                if self.disposed || token != self.grant_token {
                    // The panel moved on while the host was granting: release
                    // what we received instead of leaving an orphan grant.
                    effects.extend(self.revoke_quietly(&receipt.grant_id));
                } else {
                    let now = (self.now)();
                    self.ttl_ms = (receipt.expires_at_ms - now).max(0);
                    self.update(GrantPatch::full(
                        GrantPhase::Granted,
                        Some(receipt.clone()),
                        Some(receipt.expires_at_ms),
                        None,
                    ));
                    effects.extend(self.schedule_touch());
                }
            }
            Err(message) => {
                if token != self.grant_token || self.disposed {
                    // `.catch` ignores a result that belongs to a previous run.
                } else {
                    self.update(GrantPatch::full(
                        GrantPhase::Independent,
                        None,
                        None,
                        Some(message),
                    ));
                }
            }
        }
        // `.finally`: only the current request may clear the in-flight flag.
        if token == self.grant_token {
            self.in_flight = false;
        }
        effects
    }

    /// `revoke()`: clear timers, invalidate any in-flight grant and move to
    /// `revoked`. Safe while `granting`; the stale response is released when
    /// it arrives. Returns the effects for the (quiet) revoke of any receipt.
    #[allow(dead_code)] // called by the panel's revoke button and grant races; delete then.
    pub fn revoke(&mut self) -> Vec<GrantEffect> {
        let receipt = self.current.receipt.clone();
        let mut effects = self.clear_timers();
        self.grant_token += 1;
        self.in_flight = false;
        self.update(GrantPatch::full(GrantPhase::Revoked, None, None, None));
        if let Some(receipt) = receipt {
            effects.extend(self.revoke_quietly(&receipt.grant_id));
        }
        effects
    }

    /// `dispose()`: stop timers and release an active grant without emitting
    /// further state. The listener list is cleared like React's
    /// `listeners.clear()`.
    #[allow(dead_code)] // called from the panel's effect cleanup; delete then.
    pub fn dispose(&mut self) -> Vec<GrantEffect> {
        if self.disposed {
            return Vec::new();
        }
        self.disposed = true;
        let mut effects = self.clear_timers();
        self.grant_token += 1;
        self.in_flight = false;
        let receipt = self.current.receipt.clone();
        self.listeners.clear();
        if let Some(receipt) = receipt {
            effects.extend(self.revoke_quietly(&receipt.grant_id));
        }
        effects
    }

    /// Outcome of an injected `revoke` call. Mirrors `revokeQuietly`: a failure
    /// is reported through `onWarn`, a success is silent.
    #[allow(dead_code)] // called by the host's revoke executor; delete then.
    pub fn on_revoke_result(
        &mut self,
        grant_id: &str,
        result: Result<(), String>,
    ) -> Vec<GrantEffect> {
        match result {
            Ok(()) => Vec::new(),
            Err(error) => vec![GrantEffect::Warn {
                message: format!("failed to revoke editor grant {}", grant_id),
                error: Some(error),
            }],
        }
    }

    /// Outcome of an injected `touch` call. Mirrors `runTouch`'s try/catch:
    /// `Ok(false)` becomes the `"touch rejected"` error; a rejection carries
    /// its own message. A revoke/expiry during the await always wins.
    #[allow(dead_code)] // called by the host's touch executor; delete then.
    pub fn on_touch_result(
        &mut self,
        grant_id: &str,
        result: Result<bool, String>,
    ) -> Vec<GrantEffect> {
        if !self.still_granted(grant_id) {
            return Vec::new();
        }
        match result {
            Ok(true) => {
                let now = (self.now)();
                self.update(GrantPatch {
                    expires_at_ms: Some(Some(now + self.ttl_ms)),
                    ..Default::default()
                });
                self.schedule_touch()
            }
            Ok(false) => self.revoke_after_failed_touch("touch rejected"),
            Err(message) => self.revoke_after_failed_touch(&message),
        }
    }

    /// The touch timer fired. Mirrors the `setTimeout` callback clearing
    /// `touchTimer` then calling `runTouch`. A stale handle (cancelled timer)
    /// is ignored, modelling JS `clearTimeout` preventing the callback.
    #[allow(dead_code)] // called by the host timer bridge; delete then.
    pub fn fire_touch_timer(&mut self, handle: GrantTimerHandle) -> Vec<GrantEffect> {
        if self.touch_timer != Some(handle) {
            return Vec::new();
        }
        self.touch_timer = None;
        self.run_touch()
    }

    /// The expiry timer fired. Mirrors its callback clearing `expiryTimer`
    /// then calling `expireLocally`.
    #[allow(dead_code)] // called by the host timer bridge; delete then.
    pub fn fire_expiry_timer(&mut self, handle: GrantTimerHandle) -> Vec<GrantEffect> {
        if self.expiry_timer != Some(handle) {
            return Vec::new();
        }
        self.expiry_timer = None;
        self.expire_locally()
    }

    /// `scheduleTouch`: re-arm both timers. The local expiry is an independent
    /// exit even when the touch request stalls.
    fn schedule_touch(&mut self) -> Vec<GrantEffect> {
        let mut effects = self.clear_timers();
        let Some(receipt) = self.current.receipt.clone() else {
            return effects;
        };
        if self.current.phase != GrantPhase::Granted {
            return effects;
        }
        let now = (self.now)();
        let remaining = (self.current.expires_at_ms.unwrap_or(0) - now).max(0);
        if remaining <= 0 {
            effects.extend(self.expire_locally());
            return effects;
        }
        let touch_delay = ((remaining as f64 * self.touch_interval_ratio).floor() as i64).max(1);

        let touch_handle = self.alloc_timer();
        self.touch_timer = Some(touch_handle);
        effects.push(GrantEffect::SetTouchTimer {
            handle: touch_handle,
            delay_ms: touch_delay,
        });

        let expiry_handle = self.alloc_timer();
        self.expiry_timer = Some(expiry_handle);
        effects.push(GrantEffect::SetExpiryTimer {
            handle: expiry_handle,
            delay_ms: remaining,
        });
        effects
    }

    /// `runTouch`: fire the injected touch unless the grant is gone or the
    /// local deadline has already passed (then expire locally).
    fn run_touch(&mut self) -> Vec<GrantEffect> {
        let Some(receipt) = self.current.receipt.clone() else {
            return Vec::new();
        };
        if self.current.phase != GrantPhase::Granted {
            return Vec::new();
        }
        let now = (self.now)();
        if now >= self.current.expires_at_ms.unwrap_or(0) {
            return self.expire_locally();
        }
        vec![GrantEffect::CallTouch {
            grant_id: receipt.grant_id,
        }]
    }

    /// A failed touch (rejection or `ok === false`) moves to `revoked` with the
    /// error text, but only if the same grant is still active.
    fn revoke_after_failed_touch(&mut self, message: &str) -> Vec<GrantEffect> {
        let mut effects = self.clear_timers();
        self.update(GrantPatch::full(
            GrantPhase::Revoked,
            None,
            None,
            Some(message.to_owned()),
        ));
        effects
    }

    /// `expireLocally`: the local deadline exit back to `independent`.
    fn expire_locally(&mut self) -> Vec<GrantEffect> {
        if self.current.phase != GrantPhase::Granted {
            return Vec::new();
        }
        let mut effects = self.clear_timers();
        self.update(GrantPatch::full(GrantPhase::Independent, None, None, None));
        effects
    }

    /// `revokeQuietly`: issue the revoke; failures are turned into a `Warn`
    /// effect by [`Self::on_revoke_result`].
    fn revoke_quietly(&self, grant_id: &str) -> Vec<GrantEffect> {
        vec![GrantEffect::CallRevoke {
            grant_id: grant_id.to_owned(),
        }]
    }

    /// `still_granted`: the touch guard from `runTouch`'s try/catch — the
    /// machine must still be granted and hold the *same* grant id.
    fn still_granted(&self, grant_id: &str) -> bool {
        self.current.phase == GrantPhase::Granted
            && self
                .current
                .receipt
                .as_ref()
                .is_some_and(|receipt| receipt.grant_id.as_str() == grant_id)
    }

    /// `clearTimers`: cancel and forget both live timer handles.
    fn clear_timers(&mut self) -> Vec<GrantEffect> {
        let mut effects = Vec::new();
        if let Some(handle) = self.touch_timer.take() {
            effects.push(GrantEffect::ClearTimer { handle });
        }
        if let Some(handle) = self.expiry_timer.take() {
            effects.push(GrantEffect::ClearTimer { handle });
        }
        effects
    }

    fn alloc_timer(&mut self) -> GrantTimerHandle {
        let handle = self.next_timer_handle;
        self.next_timer_handle += 1;
        handle
    }

    /// `update(patch)`: merge the patch into `current`, then notify listeners
    /// with the new snapshot (Rust borrows the two fields disjointly).
    fn update(&mut self, patch: GrantPatch) {
        if let Some(phase) = patch.phase {
            self.current.phase = phase;
        }
        if let Some(receipt) = patch.receipt {
            self.current.receipt = receipt;
        }
        if let Some(expires_at_ms) = patch.expires_at_ms {
            self.current.expires_at_ms = expires_at_ms;
        }
        if let Some(error) = patch.error {
            self.current.error = error;
        }
        for (_, listener) in &mut self.listeners {
            listener(&self.current);
        }
    }
}
